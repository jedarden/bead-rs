//! Read-only per-issue audit history. Event detail is intentionally projected
//! to field names: older events may carry private values in their detail JSON.

use crate::cli::HistoryOptions;
use crate::error::{Error, Result};
use crate::{service, store};
use rusqlite::Connection;
use serde::Serialize;
use serde_json::Value;

#[derive(Serialize)]
struct HistoryEvent {
    sequence: i64,
    time: String,
    actor: String,
    kind: String,
    revision: i64,
    changed_fields: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    redacted: Option<&'static str>,
}

struct StoredEvent {
    sequence: i64,
    time: String,
    actor: Option<String>,
    kind: String,
    detail: Value,
}

pub fn run(opts: HistoryOptions) -> Result<()> {
    let config = store::WorkspaceConfig::discover()?
        .ok_or_else(|| Error::workspace("No workspace found. Run `bead init` first."))?;
    let conn = store::open_configured_connection(&config.database_path())
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to open database: {e}")))?;
    let issue = service::get_issue_by_id(&conn, &opts.id)?
        .ok_or_else(|| Error::not_found(format!("Issue {} not found", opts.id)))?;

    let rows = read_events(&conn, &opts.id)?;
    // Revision changes are not stored on older events. Anchor the oldest
    // recognizable mutation to the current revision, then use explicit
    // revisions where newer event detail provides them.
    let increments = rows.iter().filter(|row| increments_revision(row)).count() as i64;
    let mut revision = (issue.revision.unwrap_or(1) - increments).max(0);
    let events: Vec<HistoryEvent> = rows
        .into_iter()
        .map(|row| {
            if increments_revision(&row) {
                revision += 1;
            }
            if let Some(recorded) = row.detail.get("revision").and_then(Value::as_i64) {
                revision = recorded;
            }
            let changed_fields = changed_fields(&row);
            let redacted = (row.kind == "historical_redaction")
                .then_some(crate::model::redaction::REDACTION_MARKER);
            HistoryEvent {
                sequence: row.sequence,
                time: row.time,
                actor: row.actor.unwrap_or_else(|| "system".to_owned()),
                kind: row.kind,
                revision,
                changed_fields,
                redacted,
            }
        })
        .collect();

    if opts.json {
        println!("{}", serde_json::to_string(&events)?);
    } else {
        for event in events {
            let fields = event.changed_fields.join(",");
            let marker = event.redacted.unwrap_or("");
            println!(
                "{} {} {} {} r{} [{}]{}",
                event.sequence, event.time, event.actor, event.kind, event.revision, fields, marker
            );
        }
    }
    Ok(())
}

fn read_events(conn: &Connection, id: &str) -> Result<Vec<StoredEvent>> {
    let mut stmt = conn.prepare(
        "SELECT sequence, time, actor, kind, detail FROM events WHERE issue_id = ?1 ORDER BY sequence ASC",
    )?;
    let rows = stmt
        .query_map([id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(|(sequence, time, actor, kind, detail)| {
            Ok(StoredEvent {
                sequence,
                time,
                actor,
                kind,
                detail: serde_json::from_str(&detail).map_err(|_| {
                    Error::integrity(format!("Invalid detail JSON in event {sequence}"))
                })?,
            })
        })
        .collect()
}

fn increments_revision(row: &StoredEvent) -> bool {
    match row.kind.as_str() {
        "created" | "updated" | "claimed" | "released" | "closed" | "reopened"
        | "assignment_cleared" => true,
        "historical_redaction" => row.detail["record_kind"] == "issue",
        _ => false,
    }
}

fn changed_fields(row: &StoredEvent) -> Vec<String> {
    if let Some(fields) = row.detail.get("changed_fields").and_then(Value::as_array) {
        let safe: Vec<String> = fields
            .iter()
            .filter_map(Value::as_str)
            .filter(|field| is_issue_field(field))
            .map(str::to_owned)
            .collect();
        if !safe.is_empty() {
            return safe;
        }
    }
    let fields: &[&str] = match row.kind.as_str() {
        "created" => &["title", "priority", "issue_type"],
        "updated" => &["unspecified"],
        "claimed" => &["base_status", "assignee", "claim_epoch"],
        "released" => &["base_status", "assignee"],
        "closed" => &["base_status", "closed_at", "close_reason"],
        "reopened" => &["base_status", "assignee", "closed_at", "close_reason"],
        "assignment_cleared" => &["assignee"],
        "historical_redaction" => {
            let field = row.detail["field_path"].as_str().unwrap_or("");
            if is_issue_field(field) {
                return vec![field.to_owned()];
            }
            &["event_detail"]
        }
        _ => &[],
    };
    fields.iter().map(|field| (*field).to_owned()).collect()
}

fn is_issue_field(field: &str) -> bool {
    matches!(
        field,
        "issue"
            | "title"
            | "description"
            | "notes"
            | "priority"
            | "issue_type"
            | "base_status"
            | "manual_blocked"
            | "assignee"
            | "claim_epoch"
            | "closed_at"
            | "close_reason"
            | "event_detail"
    )
}

//! Read-only secret diagnostics across live semantic rows and retained
//! checkpoint generations (ADR-014, secret-rejection-v1 §5).

use crate::error::{Error, Result};
use crate::scan::{self, Field, Finding, Mode, ScanConfig, ScanReport, CONTRACT_IDENTITY};
use crate::store::{open_configured_connection, Store};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::io::BufRead;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
pub struct SecretDiagnosticsReport {
    pub contract_identity: String,
    pub ruleset_version: u32,
    pub effective_mode: String,
    pub live_fields_scanned: usize,
    pub checkpoint_generations_scanned: Vec<String>,
    pub blocking_findings: usize,
    pub advisory_findings: usize,
    pub findings: Vec<Finding>,
}

struct LiveTable {
    name: &'static str,
    identity_fields: &'static [&'static str],
    fields: &'static [&'static str],
}

const LIVE_TABLES: &[LiveTable] = &[
    LiveTable {
        name: "issues",
        identity_fields: &["id"],
        fields: &[
            "title",
            "description",
            "notes",
            "assignee",
            "issue_type",
            "close_reason",
            "source_repo",
        ],
    },
    LiveTable {
        name: "events",
        identity_fields: &["origin_store_uuid", "origin_event_sequence"],
        fields: &["actor", "detail"],
    },
    LiveTable {
        name: "labels",
        identity_fields: &["issue_id", "label"],
        fields: &["label"],
    },
    LiveTable {
        name: "dependencies",
        identity_fields: &["blocked_issue_id", "blocker_issue_id", "kind"],
        fields: &["condition"],
    },
    LiveTable {
        name: "comments",
        identity_fields: &["id"],
        fields: &["author", "body"],
    },
    LiveTable {
        name: "issue_data",
        identity_fields: &["issue_id", "namespace"],
        fields: &["namespace", "schema_ref", "value"],
    },
    LiveTable {
        name: "external_references",
        identity_fields: &["issue_id", "namespace", "key"],
        fields: &["namespace", "key", "value"],
    },
    LiveTable {
        name: "recurrence_templates",
        identity_fields: &["id"],
        fields: &[
            "title",
            "description",
            "base_title_template",
            "base_description",
            "issue_type",
            "labels_json",
        ],
    },
    LiveTable {
        name: "recurrence_materializations",
        identity_fields: &["template_id", "series_sequence"],
        fields: &["actor"],
    },
    LiveTable {
        name: "attempt_outcomes",
        identity_fields: &["receipt_id"],
        fields: &[
            "reason",
            "actor",
            "evidence_refs_json",
            "model",
            "harness",
            "harness_version",
        ],
    },
    LiveTable {
        name: "provenance_receipts",
        identity_fields: &["receipt_id"],
        fields: &["actor"],
    },
];

/// Opaque locator for one live finding.
///
/// The locator deliberately carries database identity values and redacted
/// scanner metadata only. The matched field value is re-read under the
/// redaction transaction and never crosses this boundary.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct LiveFindingLocation {
    pub table: &'static str,
    pub identity_fields: &'static [&'static str],
    pub identity_values: Vec<rusqlite::types::Value>,
    pub field: &'static str,
    pub finding: Finding,
}

/// Scan current semantic state and each retained current/previous checkpoint
/// generation. Workspace enforcement never changes this diagnostic: `off`
/// and `advisory` are reported as the effective mode, but doctor still scans.
pub fn run_secret_diagnostics(store: &impl Store) -> Result<SecretDiagnosticsReport> {
    let workspace = store.get_workspace_config()?;
    let effective = ScanConfig::load_from_workspace_root(&workspace.root)
        .map_err(|error| Error::cli_usage(error.to_string()))?;
    let diagnostic_config = ScanConfig::new(Mode::Advisory);
    let conn = open_configured_connection(&workspace.database_path())?;

    let (live_reports, live_fields_scanned) = scan_live_rows(&conn, &diagnostic_config)?;
    let (checkpoint_reports, checkpoint_generations_scanned) = scan_retained_generations(
        &workspace.root.join(".beads/checkpoint"),
        &diagnostic_config,
    )?;
    let report = ScanReport::merge(live_reports.into_iter().chain(checkpoint_reports));
    let blocking_findings = report
        .findings
        .iter()
        .filter(|finding| finding.is_blocking_match())
        .count();

    Ok(SecretDiagnosticsReport {
        contract_identity: CONTRACT_IDENTITY.to_string(),
        ruleset_version: scan::RULESET_VERSION,
        effective_mode: effective.mode().as_str().to_string(),
        live_fields_scanned,
        checkpoint_generations_scanned,
        blocking_findings,
        advisory_findings: report.findings.len() - blocking_findings,
        findings: report.findings,
    })
}

/// Scan caller-supplied recovery or archaeology input without applying the
/// workspace enforcement mode. Legacy bytes already exist, so this surface is
/// report-only: callers may render the returned redacted findings but must not
/// turn them into a recovery refusal.
pub fn scan_recovery_artifact(path: &Path) -> Result<ScanReport> {
    let config = ScanConfig::new(Mode::Advisory);
    let mut reports = Vec::new();
    if path.is_dir() {
        let mut found_pointer = false;
        for generation in ["current", "previous"] {
            let pointer = path.join(format!("{generation}.json"));
            if pointer.is_file() {
                scan_pointer(&pointer, generation, &config, &mut reports)?;
                found_pointer = true;
            }
        }
        if !found_pointer {
            let forensic = path.join("forensic.jsonl");
            if forensic.is_file() {
                scan_jsonl(&forensic, "recovery", &config, &mut reports)?;
            }
        }
    } else {
        scan_artifact_file(path, "recovery", &config, &mut reports)?;
    }
    Ok(ScanReport::merge(reports))
}

fn scan_live_rows(
    conn: &rusqlite::Connection,
    config: &ScanConfig,
) -> Result<(Vec<ScanReport>, usize)> {
    let mut reports = Vec::new();
    let mut fields_scanned = 0usize;
    for table in LIVE_TABLES {
        let available = table_columns(conn, table.name)?;
        if available.is_empty() {
            continue;
        }
        if table
            .identity_fields
            .iter()
            .any(|field| !available.contains(*field))
        {
            return Err(Error::integrity(format!(
                "{} table is missing a stable identity column",
                table.name
            )));
        }
        let fields: Vec<&str> = table
            .fields
            .iter()
            .copied()
            .filter(|field| available.contains(*field))
            .collect();
        if fields.is_empty() {
            continue;
        }
        let mut selections: Vec<String> = table
            .identity_fields
            .iter()
            .map(|field| format!("CAST({field} AS TEXT)"))
            .collect();
        selections.extend(fields.iter().map(|field| (*field).to_string()));
        let order = table.identity_fields.join(", ");
        let sql = format!(
            "SELECT {} FROM {} ORDER BY {}",
            selections.join(", "),
            table.name,
            order
        );
        let mut statement = conn.prepare(&sql)?;
        let rows = statement.query_map([], |row| {
            let identity = (0..table.identity_fields.len())
                .map(|index| row.get::<_, Option<String>>(index))
                .collect::<rusqlite::Result<Vec<_>>>()?;
            let values = (0..fields.len())
                .map(|index| row.get::<_, Option<String>>(index + table.identity_fields.len()))
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok((identity, values))
        })?;
        for row in rows {
            let (identity, values) = row?;
            let present: Vec<Field<'_>> = fields
                .iter()
                .zip(values.iter())
                .filter_map(|(field, value)| value.as_deref().map(|value| Field::new(field, value)))
                .collect();
            fields_scanned += present.len();
            reports.push(scan::scan(
                config,
                &semantic_selector(table, &identity),
                &present,
            ));
        }
    }
    Ok((reports, fields_scanned))
}

/// Scan only the supplied live database and return redacted findings.
///
/// This is the transaction-facing subset used by historical redaction and
/// callers that already hold the correct workspace connection. It never
/// discovers another workspace or scans retained checkpoint files.
#[allow(dead_code)]
pub fn scan_live_findings(conn: &rusqlite::Connection) -> Result<Vec<Finding>> {
    scan_live_rows(conn, &ScanConfig::new(Mode::Advisory))
        .map(|(reports, _)| ScanReport::merge(reports).findings)
}

/// Locate one scanner fingerprint in current live state without returning
/// the matched bytes.
///
/// BR-T16 uses this once its IMMEDIATE transaction is open. Identity values
/// remain typed so the eventual `UPDATE` addresses the exact row even when a
/// compound identity includes an integer. A duplicate fingerprint is an
/// integrity failure: the fingerprint commits to selector, field and range,
/// so two locations cannot legitimately share it.
#[allow(dead_code)]
pub(crate) fn find_live_finding(
    conn: &rusqlite::Connection,
    fingerprint: &str,
) -> Result<Option<LiveFindingLocation>> {
    let config = ScanConfig::new(Mode::Advisory);
    let mut located = None;
    for table in LIVE_TABLES {
        let available = table_columns(conn, table.name)?;
        if available.is_empty() {
            continue;
        }
        if table
            .identity_fields
            .iter()
            .any(|field| !available.contains(*field))
        {
            return Err(Error::integrity(format!(
                "{} table is missing a stable identity column",
                table.name
            )));
        }
        let fields: Vec<&'static str> = table
            .fields
            .iter()
            .copied()
            .filter(|field| available.contains(*field))
            .collect();
        if fields.is_empty() {
            continue;
        }
        let mut selections: Vec<String> = table
            .identity_fields
            .iter()
            .map(|field| (*field).to_string())
            .collect();
        selections.extend(fields.iter().map(|field| (*field).to_string()));
        let sql = format!(
            "SELECT {} FROM {} ORDER BY {}",
            selections.join(", "),
            table.name,
            table.identity_fields.join(", ")
        );
        let mut statement = conn.prepare(&sql)?;
        let rows = statement.query_map([], |row| {
            let identity_values = (0..table.identity_fields.len())
                .map(|index| row.get::<_, rusqlite::types::Value>(index))
                .collect::<rusqlite::Result<Vec<_>>>()?;
            let values = (0..fields.len())
                .map(|index| row.get::<_, Option<String>>(index + table.identity_fields.len()))
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok((identity_values, values))
        })?;
        for row in rows {
            let (identity_values, values) = row?;
            let selector_values = identity_values
                .iter()
                .map(selector_identity_value)
                .collect::<Result<Vec<_>>>()?;
            let selector = semantic_selector(table, &selector_values);
            for (field, value) in fields.iter().zip(values) {
                let Some(text) = value else {
                    continue;
                };
                let report = scan::scan(&config, &selector, &[Field::new(field, &text)]);
                for finding in report
                    .findings
                    .into_iter()
                    .filter(|finding| finding.fingerprint == fingerprint)
                {
                    if located.is_some() {
                        return Err(Error::integrity(
                            "one secret fingerprint resolved to multiple live locations",
                        ));
                    }
                    located = Some(LiveFindingLocation {
                        table: table.name,
                        identity_fields: table.identity_fields,
                        identity_values: identity_values.clone(),
                        field,
                        finding,
                    });
                }
            }
        }
    }
    Ok(located)
}

#[allow(dead_code)]
fn selector_identity_value(value: &rusqlite::types::Value) -> Result<Option<String>> {
    match value {
        rusqlite::types::Value::Null => Ok(None),
        rusqlite::types::Value::Integer(value) => Ok(Some(value.to_string())),
        rusqlite::types::Value::Real(value) => Ok(Some(value.to_string())),
        rusqlite::types::Value::Text(value) => Ok(Some(value.clone())),
        rusqlite::types::Value::Blob(_) => Err(Error::integrity(
            "live selector identity cannot be a binary value",
        )),
    }
}

/// A live selector commits to the table's durable primary/origin identity,
/// rather than SQLite `rowid`, so it survives restore and table rebuilds. The
/// identity is hashed because some compound keys contain operator text; a
/// diagnostic selector must never become a second disclosure channel.
fn semantic_selector(table: &LiveTable, identity: &[Option<String>]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"bead-rs-secret-live-selector-v1\0");
    hasher.update(table.name.as_bytes());
    hasher.update(b"\0");
    for (field, value) in table.identity_fields.iter().zip(identity) {
        hasher.update(field.as_bytes());
        hasher.update(b"\0");
        match value {
            Some(value) => {
                hasher.update(b"present\0");
                hasher.update((value.len() as u64).to_be_bytes());
                hasher.update(value.as_bytes());
            }
            None => hasher.update(b"null\0"),
        }
        hasher.update(b"\0");
    }
    format!("live:{}:{:x}", table.name, hasher.finalize())
}

fn table_columns(conn: &rusqlite::Connection, table: &str) -> Result<HashSet<String>> {
    let mut statement = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<HashSet<_>>>()?;
    Ok(columns)
}

fn scan_retained_generations(
    checkpoint_dir: &Path,
    config: &ScanConfig,
) -> Result<(Vec<ScanReport>, Vec<String>)> {
    let mut reports = Vec::new();
    let mut generations = Vec::new();
    for name in ["current", "previous"] {
        let pointer_path = checkpoint_dir.join(format!("{name}.json"));
        if !pointer_path.exists() {
            continue;
        }
        scan_pointer(&pointer_path, name, config, &mut reports)?;
        generations.push(name.to_string());
    }
    Ok((reports, generations))
}

fn scan_pointer(
    pointer_path: &Path,
    generation: &str,
    config: &ScanConfig,
    reports: &mut Vec<ScanReport>,
) -> Result<()> {
    let pointer = read_json(pointer_path, "checkpoint pointer")?;
    let mode = pointer
        .get("mode")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::integrity("checkpoint pointer has no mode"))?;
    let root_path = pointer
        .get("active_root")
        .and_then(|root| root.get("path"))
        .and_then(Value::as_str)
        .ok_or_else(|| Error::integrity("checkpoint pointer has no active root path"))?;
    let checkpoint_dir = pointer_path
        .parent()
        .ok_or_else(|| Error::integrity("checkpoint pointer has no parent directory"))?;
    let root_path = confined_path(checkpoint_dir, root_path)?;
    match mode {
        "monolithic" => scan_jsonl(&root_path, generation, config, reports),
        "sharded" => scan_shard_manifest(&root_path, checkpoint_dir, generation, config, reports),
        _ => Err(Error::integrity("checkpoint pointer has unsupported mode")),
    }
}

fn scan_shard_manifest(
    manifest_path: &Path,
    checkpoint_dir: &Path,
    generation: &str,
    config: &ScanConfig,
    reports: &mut Vec<ScanReport>,
) -> Result<()> {
    let manifest = read_json(manifest_path, "checkpoint shard manifest")?;
    for key in [
        "issue_shards",
        "event_shards",
        "receipt_shards",
        "attempt_outcome_shards",
        "redaction_receipt_shards",
    ] {
        if let Some(shards) = manifest.get(key).and_then(Value::as_array) {
            for shard in shards {
                let path = shard
                    .get("path")
                    .and_then(Value::as_str)
                    .ok_or_else(|| Error::integrity("checkpoint manifest shard has no path"))?;
                scan_jsonl(
                    &confined_path(checkpoint_dir, path)?,
                    generation,
                    config,
                    reports,
                )?;
            }
        }
    }
    Ok(())
}

fn scan_artifact_file(
    path: &Path,
    generation: &str,
    config: &ScanConfig,
    reports: &mut Vec<ScanReport>,
) -> Result<()> {
    if path.extension().and_then(|extension| extension.to_str()) == Some("jsonl") {
        return scan_jsonl(path, generation, config, reports);
    }
    let document = read_json(path, "recovery artifact")?;
    if document.get("active_root").is_some() {
        return scan_pointer(path, generation, config, reports);
    }
    if document.get("issue_shards").is_some() || document.get("event_shards").is_some() {
        let parent = path
            .parent()
            .ok_or_else(|| Error::integrity("checkpoint manifest has no parent directory"))?;
        let checkpoint_dir =
            if parent.file_name().and_then(|name| name.to_str()) == Some("manifests") {
                parent.parent().unwrap_or(parent)
            } else {
                parent
            };
        return scan_shard_manifest(path, checkpoint_dir, generation, config, reports);
    }
    scan_json_value(config, "recovery:document", "record", &document, reports);
    Ok(())
}

fn read_json(path: &Path, kind: &str) -> Result<Value> {
    let bytes = std::fs::read(path).map_err(|error| Error::Io {
        path: path.to_path_buf(),
        msg: error,
    })?;
    serde_json::from_slice(&bytes)
        .map_err(|_| Error::integrity(format!("invalid JSON in {kind}: {}", path.display())))
}

fn confined_path(base: &Path, relative: &str) -> Result<PathBuf> {
    let path = Path::new(relative);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
    {
        return Err(Error::integrity(
            "checkpoint pointer contains an unsafe relative path",
        ));
    }
    Ok(base.join(path))
}

fn scan_jsonl(
    path: &Path,
    generation: &str,
    config: &ScanConfig,
    reports: &mut Vec<ScanReport>,
) -> Result<()> {
    let file = std::fs::File::open(path).map_err(|error| Error::Io {
        path: path.to_path_buf(),
        msg: error,
    })?;
    for (line_index, line) in std::io::BufReader::new(file).lines().enumerate() {
        let line = line.map_err(|error| Error::Io {
            path: path.to_path_buf(),
            msg: error,
        })?;
        if line.trim().is_empty() {
            continue;
        }
        let record: Value = serde_json::from_str(&line).map_err(|_| {
            Error::integrity(format!(
                "invalid JSON in {generation} checkpoint record {}",
                line_index + 1
            ))
        })?;
        scan_json_value(
            config,
            &format!("checkpoint:{generation}:record:{}", line_index + 1),
            "record",
            &record,
            reports,
        );
    }
    Ok(())
}

fn scan_json_value(
    config: &ScanConfig,
    selector: &str,
    path: &str,
    value: &Value,
    reports: &mut Vec<ScanReport>,
) {
    match value {
        Value::String(text) if should_scan_path(path) => {
            reports.push(scan::scan(config, selector, &[Field::new(path, text)]));
        }
        Value::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                scan_json_value(
                    config,
                    selector,
                    &format!("{path}[{index}]"),
                    value,
                    reports,
                );
            }
        }
        Value::Object(values) => {
            let mut keys: Vec<&String> = values.keys().collect();
            keys.sort();
            for key in keys {
                let component = safe_path_component(key);
                scan_json_value(
                    config,
                    selector,
                    &format!("{path}.{component}"),
                    &values[key],
                    reports,
                );
            }
        }
        _ => {}
    }
}

fn safe_path_component(component: &str) -> &str {
    let safe = !component.is_empty()
        && component.len() <= 64
        && component.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-' | b'$')
        });
    if safe {
        component
    } else {
        "<field>"
    }
}

fn should_scan_path(path: &str) -> bool {
    let leaf = path
        .rsplit(['.', '['])
        .next()
        .unwrap_or(path)
        .trim_end_matches(']');
    !matches!(
        leaf,
        "$schema"
            | "id"
            | "issue_id"
            | "attempt_id"
            | "receipt_id"
            | "template_id"
            | "occurrence_id"
            | "reply_to_id"
            | "blocked_issue_id"
            | "blocker_issue_id"
            | "origin_store_uuid"
            | "source_store_uuid"
            | "target_store_uuid"
            | "store_uuid"
            | "schema_ref"
            | "fingerprint"
            | "sha256"
            | "event_sha256"
            | "receipt_sha256"
            | "source_root_sha256"
            | "created_at"
            | "updated_at"
            | "closed_at"
            | "materialized_at"
            | "time"
    ) && !leaf.ends_with("_sha256")
        && !leaf.ends_with("_hash")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsafe_record_keys_and_machine_hashes_are_not_reported_as_content() {
        let value = [["AK", "IA"].concat(), "7M4Q9Z2N8C5R3T6V".to_string()].concat();
        let document = serde_json::json!({
            value.clone(): value.clone(),
            "receipt_sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            "description": value.clone()
        });
        let mut reports = Vec::new();
        scan_json_value(
            &ScanConfig::new(Mode::Advisory),
            "checkpoint:current:record:1",
            "record",
            &document,
            &mut reports,
        );
        let report = ScanReport::merge(reports);
        assert_eq!(report.blocking.len(), 2);
        assert!(report
            .findings
            .iter()
            .any(|finding| finding.field_path == "record.<field>"));
        assert!(!serde_json::to_string(&report.findings)
            .unwrap()
            .contains(&value));
    }

    #[test]
    fn checkpoint_paths_cannot_escape_the_checkpoint_directory() {
        let base = Path::new("/tmp/checkpoint");
        assert!(confined_path(base, "objects/a.jsonl").is_ok());
        assert!(confined_path(base, "../outside").is_err());
        assert!(confined_path(base, "/outside").is_err());
    }

    #[test]
    fn live_selector_is_stable_and_does_not_expose_identity_text() {
        let table = LiveTable {
            name: "labels",
            identity_fields: &["issue_id", "label"],
            fields: &["label"],
        };
        let private_identity = [["AK", "IA"].concat(), "7M4Q9Z2N8C5R3T6V".to_string()].concat();
        let identity = vec![Some("issue-a".to_string()), Some(private_identity.clone())];
        let first = semantic_selector(&table, &identity);
        let second = semantic_selector(&table, &identity);
        let moved = semantic_selector(
            &table,
            &[Some("issue-b".to_string()), Some(private_identity.clone())],
        );

        assert_eq!(first, second);
        assert_ne!(first, moved);
        assert!(first.starts_with("live:labels:"));
        assert_eq!(first.rsplit(':').next().unwrap().len(), 64);
        assert!(!first.contains(&private_identity));
    }

    #[test]
    fn recovery_artifact_scan_reports_without_returning_matched_bytes() {
        let directory = tempfile::tempdir().unwrap();
        let artifact = directory.path().join("legacy.jsonl");
        let value = [["AK", "IA"].concat(), "7M4Q9Z2N8C5R3T6V".to_string()].concat();
        std::fs::write(
            &artifact,
            serde_json::to_vec(&serde_json::json!({"description": value.clone()})).unwrap(),
        )
        .unwrap();

        let report = scan_recovery_artifact(&artifact).unwrap();
        assert_eq!(report.blocking.len(), 1);
        assert!(!serde_json::to_string(&report.findings)
            .unwrap()
            .contains(&value));
    }
}

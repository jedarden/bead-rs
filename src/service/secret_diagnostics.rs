//! Read-only secret diagnostics across live semantic rows and retained
//! checkpoint generations (ADR-014, secret-rejection-v1 §5).

use crate::error::{Error, Result};
use crate::scan::{self, Field, Finding, Mode, ScanConfig, ScanReport, CONTRACT_IDENTITY};
use crate::store::{open_configured_connection, Store};
use serde::Serialize;
use serde_json::Value;
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
    fields: &'static [&'static str],
}

const LIVE_TABLES: &[LiveTable] = &[
    LiveTable {
        name: "issues",
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
        fields: &["actor", "detail"],
    },
    LiveTable {
        name: "labels",
        fields: &["label"],
    },
    LiveTable {
        name: "dependencies",
        fields: &["condition"],
    },
    LiveTable {
        name: "comments",
        fields: &["author", "body"],
    },
    LiveTable {
        name: "issue_data",
        fields: &["namespace", "schema_ref", "value"],
    },
    LiveTable {
        name: "external_references",
        fields: &["namespace", "key", "value"],
    },
    LiveTable {
        name: "recurrence_templates",
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
        fields: &["actor"],
    },
    LiveTable {
        name: "attempt_outcomes",
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
        fields: &["actor"],
    },
];

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
        let fields: Vec<&str> = table
            .fields
            .iter()
            .copied()
            .filter(|field| available.contains(*field))
            .collect();
        if fields.is_empty() {
            continue;
        }
        let sql = format!(
            "SELECT rowid, {} FROM {} ORDER BY rowid",
            fields.join(", "),
            table.name
        );
        let mut statement = conn.prepare(&sql)?;
        let rows = statement.query_map([], |row| {
            let rowid: i64 = row.get(0)?;
            let values = (0..fields.len())
                .map(|index| row.get::<_, Option<String>>(index + 1))
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok((rowid, values))
        })?;
        for row in rows {
            let (rowid, values) = row?;
            let present: Vec<Field<'_>> = fields
                .iter()
                .zip(values.iter())
                .filter_map(|(field, value)| value.as_deref().map(|value| Field::new(field, value)))
                .collect();
            fields_scanned += present.len();
            reports.push(scan::scan(
                config,
                &format!("live:{}:{rowid}", table.name),
                &present,
            ));
        }
    }
    Ok((reports, fields_scanned))
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
        let pointer = read_json(&pointer_path, "checkpoint pointer")?;
        let mode = pointer
            .get("mode")
            .and_then(Value::as_str)
            .ok_or_else(|| Error::integrity(format!("{name} checkpoint pointer has no mode")))?;
        let root_path = pointer
            .get("active_root")
            .and_then(|root| root.get("path"))
            .and_then(Value::as_str)
            .ok_or_else(|| {
                Error::integrity(format!("{name} checkpoint pointer has no active root path"))
            })?;
        let root_path = confined_path(checkpoint_dir, root_path)?;
        match mode {
            "monolithic" => scan_jsonl(&root_path, name, config, &mut reports)?,
            "sharded" => {
                let manifest = read_json(&root_path, "checkpoint shard manifest")?;
                for key in [
                    "issue_shards",
                    "event_shards",
                    "receipt_shards",
                    "attempt_outcome_shards",
                    "redaction_receipt_shards",
                ] {
                    if let Some(shards) = manifest.get(key).and_then(Value::as_array) {
                        for shard in shards {
                            let path =
                                shard.get("path").and_then(Value::as_str).ok_or_else(|| {
                                    Error::integrity(format!(
                                        "{name} checkpoint manifest has a shard without a path"
                                    ))
                                })?;
                            scan_jsonl(
                                &confined_path(checkpoint_dir, path)?,
                                name,
                                config,
                                &mut reports,
                            )?;
                        }
                    }
                }
            }
            _ => {
                return Err(Error::integrity(format!(
                    "{name} checkpoint pointer has unsupported mode"
                )))
            }
        }
        generations.push(name.to_string());
    }
    Ok((reports, generations))
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
}

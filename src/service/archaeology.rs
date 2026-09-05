//! Verified, read-only views over retained checkpoint generations (R029).
//!
//! Archaeology deliberately has no activation path. A source is first resolved
//! to a retained generation pointer, then passed through the same verifier used
//! by first-class restore. Only after that complete verification succeeds do we
//! materialize its records into an in-memory SQLite projection for query
//! execution. The verifier runs again after materialization to close the
//! ordinary read/serve time-of-check gap.

use crate::model::Issue;
use crate::service::checkpoint::{self, SerializedEvent};
use crate::service::query::{self, Query};
use anyhow::{anyhow, bail, Context, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

/// Marker attached to every archaeology response. The import service rejects
/// a document carrying this marker, even if a caller writes stdout to a file.
pub const ARCHAEOLOGY_ARTIFACT_KIND: &str = "bead-rs-checkpoint-archaeology-view-v1";

/// Immutable identity and provenance for a materialized generation.
#[derive(Debug, Clone, Serialize)]
pub struct ArchaeologyGeneration {
    pub generation_id: String,
    pub mode: String,
    pub store_uuid: String,
    pub snapshot_sequence: i64,
    pub root_path: String,
    pub root_sha256: String,
}

/// One query response over a verified historical generation.
#[derive(Debug, Clone, Serialize)]
pub struct ArchaeologyQueryReport {
    pub artifact_kind: &'static str,
    pub importable: bool,
    pub generation: ArchaeologyGeneration,
    pub results: Vec<Value>,
}

/// A semantic issue or event change between two historical generations.
#[derive(Debug, Clone, Serialize)]
pub struct SemanticDelta {
    pub identity: String,
    pub change: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<Value>,
}

/// The complete issue- and event-level semantic delta between two views.
#[derive(Debug, Clone, Serialize)]
pub struct ArchaeologyDiffReport {
    pub artifact_kind: &'static str,
    pub importable: bool,
    pub from: ArchaeologyGeneration,
    pub to: ArchaeologyGeneration,
    pub issue_deltas: Vec<SemanticDelta>,
    pub event_deltas: Vec<SemanticDelta>,
}

/// One generation's result in a predicate-driven archaeology series search.
#[derive(Debug, Clone, Serialize)]
pub struct GenerationSearchResult {
    pub generation: ArchaeologyGeneration,
    pub matching_issue_count: usize,
    pub matching_issue_ids: Vec<String>,
}

/// Result of a search over a caller-ordered generation series.
#[derive(Debug, Clone, Serialize)]
pub struct ArchaeologyBisectReport {
    pub artifact_kind: &'static str,
    pub importable: bool,
    pub matches: Vec<GenerationSearchResult>,
}

#[derive(Debug, Clone)]
struct HistoricalView {
    generation: ArchaeologyGeneration,
    issues: Vec<Issue>,
    events: Vec<SerializedEvent>,
}

#[derive(Debug, Deserialize)]
struct PointerDocument {
    generation_id: String,
    mode: String,
    store_uuid: String,
    snapshot_sequence: i64,
    active_root: PointerRoot,
}

#[derive(Debug, Deserialize)]
struct PointerRoot {
    path: String,
    sha256: String,
}

#[derive(Debug)]
struct ResolvedArtifact {
    pointer_path: PathBuf,
    generation_id: String,
}

/// Query one verified checkpoint generation using the ordinary safe query
/// grammar. The query is evaluated only in an in-memory SQLite projection;
/// no source artifact or workspace database is written.
pub fn query_checkpoint(source: &Path, query: &Query) -> Result<ArchaeologyQueryReport> {
    let view = load_verified_view(source)?;
    let issues = execute_historical_query(&view.issues, query)?;
    let results = if let Some(projection) = &query.projection {
        issues
            .iter()
            .map(|issue| query::project_issue(issue, projection).map_err(anyhow::Error::from))
            .collect::<Result<Vec<_>>>()?
    } else {
        issues
            .iter()
            .map(serde_json::to_value)
            .collect::<serde_json::Result<Vec<_>>>()?
    };

    Ok(ArchaeologyQueryReport {
        artifact_kind: ARCHAEOLOGY_ARTIFACT_KIND,
        importable: false,
        generation: view.generation,
        results,
    })
}

/// Produce a deterministic semantic diff over issue records and event records.
pub fn diff_checkpoints(from: &Path, to: &Path) -> Result<ArchaeologyDiffReport> {
    let from = load_verified_view(from)?;
    let to = load_verified_view(to)?;

    Ok(ArchaeologyDiffReport {
        artifact_kind: ARCHAEOLOGY_ARTIFACT_KIND,
        importable: false,
        from: from.generation.clone(),
        to: to.generation.clone(),
        issue_deltas: semantic_deltas(
            from.issues
                .iter()
                .map(|issue| Ok((issue.id.clone(), serde_json::to_value(issue)?)))
                .collect::<Result<BTreeMap<_, _>>>()?,
            to.issues
                .iter()
                .map(|issue| Ok((issue.id.clone(), serde_json::to_value(issue)?)))
                .collect::<Result<BTreeMap<_, _>>>()?,
        ),
        event_deltas: semantic_deltas(
            from.events
                .iter()
                .map(|event| Ok((event_identity(event), serde_json::to_value(event)?)))
                .collect::<Result<BTreeMap<_, _>>>()?,
            to.events
                .iter()
                .map(|event| Ok((event_identity(event), serde_json::to_value(event)?)))
                .collect::<Result<BTreeMap<_, _>>>()?,
        ),
    })
}

/// Search a caller-ordered generation series. This is intentionally a linear
/// scan rather than a binary-search assumption: an arbitrary issue predicate
/// need not be monotonic over time, and reporting every matching generation is
/// more useful for forensic inspection.
pub fn bisect_checkpoints(sources: &[PathBuf], query: &Query) -> Result<ArchaeologyBisectReport> {
    if sources.is_empty() {
        bail!("At least one --checkpoint artifact is required for archaeology bisect");
    }

    let mut seen_generations = BTreeSet::new();
    let mut matches = Vec::new();
    for source in sources {
        let view = load_verified_view(source)?;
        if !seen_generations.insert((
            view.generation.store_uuid.clone(),
            view.generation.generation_id.clone(),
        )) {
            bail!(
                "Archaeology bisect received generation '{}' from store '{}' more than once",
                view.generation.generation_id,
                view.generation.store_uuid
            );
        }
        let issues = execute_historical_query(&view.issues, query)?;
        if !issues.is_empty() {
            matches.push(GenerationSearchResult {
                generation: view.generation,
                matching_issue_count: issues.len(),
                matching_issue_ids: issues.into_iter().map(|issue| issue.id).collect(),
            });
        }
    }

    Ok(ArchaeologyBisectReport {
        artifact_kind: ARCHAEOLOGY_ARTIFACT_KIND,
        importable: false,
        matches,
    })
}

/// Reject a serialized archaeology response before an import command opens or
/// mutates its target. Normal JSONL checkpoints are intentionally not parsed
/// as one JSON document here, so this classification is additive to the
/// existing record-by-record import validation.
pub fn reject_archaeology_input(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("Failed to inspect import input {}", path.display()))?;
    if !metadata.is_file() {
        return Ok(());
    }
    let bytes = fs::read(path)
        .with_context(|| format!("Failed to read import input {}", path.display()))?;
    reject_archaeology_document(&bytes, path)
}

fn semantic_deltas(
    from: BTreeMap<String, Value>,
    to: BTreeMap<String, Value>,
) -> Vec<SemanticDelta> {
    let identities: BTreeSet<_> = from.keys().chain(to.keys()).cloned().collect();
    identities
        .into_iter()
        .filter_map(|identity| match (from.get(&identity), to.get(&identity)) {
            (None, Some(after)) => Some(SemanticDelta {
                identity,
                change: "added".to_string(),
                before: None,
                after: Some(after.clone()),
            }),
            (Some(before), None) => Some(SemanticDelta {
                identity,
                change: "removed".to_string(),
                before: Some(before.clone()),
                after: None,
            }),
            (Some(before), Some(after)) if before != after => Some(SemanticDelta {
                identity,
                change: "changed".to_string(),
                before: Some(before.clone()),
                after: Some(after.clone()),
            }),
            _ => None,
        })
        .collect()
}

fn event_identity(event: &SerializedEvent) -> String {
    format!(
        "{}:{}",
        event.origin_store_uuid, event.origin_event_sequence
    )
}

fn execute_historical_query(issues: &[Issue], query: &Query) -> Result<Vec<Issue>> {
    let conn = Connection::open_in_memory().context("Failed to create archaeology query view")?;
    conn.execute_batch(
        "CREATE TABLE issues (
            id TEXT NOT NULL,
            title TEXT NOT NULL,
            priority INTEGER NOT NULL,
            base_status TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            description TEXT,
            notes TEXT,
            assignee TEXT,
            issue_type TEXT,
            manual_blocked INTEGER,
            closed_at TEXT,
            close_reason TEXT,
            source_repo TEXT,
            profile TEXT,
            schema_ref TEXT,
            claim_epoch INTEGER
        )",
    )?;

    for issue in issues {
        conn.execute(
            "INSERT INTO issues (
                id, title, priority, base_status, created_at, updated_at,
                description, notes, assignee, issue_type, manual_blocked,
                closed_at, close_reason, source_repo, profile, schema_ref, claim_epoch
             ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17
             )",
            params![
                issue.id,
                issue.title,
                issue.priority,
                issue.base_status.as_str(),
                issue.created_at,
                issue.updated_at,
                issue.description,
                issue.notes,
                issue.assignee,
                issue.issue_type,
                issue.manual_blocked.map(i64::from),
                issue.closed_at,
                issue.close_reason,
                issue.source_repo,
                issue.profile,
                issue.schema_ref,
                issue.claim_epoch,
            ],
        )?;
    }

    query::execute_query(&conn, query).map_err(anyhow::Error::from)
}

fn load_verified_view(source: &Path) -> Result<HistoricalView> {
    let resolved = resolve_artifact(source)?;

    // `verify_restore_source` performs the full R036 content-addressed
    // closure, count, ordering, schema, graph, and event-continuity checks.
    // Archaeology intentionally shares it instead of growing a permissive
    // second reader.
    checkpoint::verify_restore_source(&resolved.pointer_path, &resolved.generation_id)?;

    let pointer = read_pointer(&resolved.pointer_path)?;
    let base = resolved
        .pointer_path
        .parent()
        .ok_or_else(|| anyhow!("Checkpoint pointer has no parent directory"))?;
    let (issues, events) = match pointer.mode.as_str() {
        "monolithic" => {
            let root = checked_relative_path(base, &pointer.active_root.path, "objects", "jsonl")?;
            read_records(&root)?
        }
        "sharded" => {
            let manifest =
                checked_relative_path(base, &pointer.active_root.path, "manifests", "json")?;
            read_sharded_records(base, &manifest)?
        }
        other => bail!("Verified checkpoint has unsupported mode '{other}'"),
    };

    // A source may have changed while the in-memory projection was being
    // materialized. Re-running the shared verifier means no such view is
    // served as verified.
    checkpoint::verify_restore_source(&resolved.pointer_path, &resolved.generation_id)?;

    Ok(HistoricalView {
        generation: ArchaeologyGeneration {
            generation_id: pointer.generation_id,
            mode: pointer.mode,
            store_uuid: pointer.store_uuid,
            snapshot_sequence: pointer.snapshot_sequence,
            root_path: pointer.active_root.path,
            root_sha256: pointer.active_root.sha256,
        },
        issues,
        events,
    })
}

fn resolve_artifact(source: &Path) -> Result<ResolvedArtifact> {
    let metadata = fs::symlink_metadata(source).with_context(|| {
        format!(
            "Checkpoint archaeology artifact is missing or unreadable: {}",
            source.display()
        )
    })?;
    if metadata.file_type().is_symlink() {
        bail!(
            "Checkpoint archaeology artifact {} must not be a symlink",
            source.display()
        );
    }
    if metadata.is_dir() {
        let pointer_path = source.join("current.json");
        let pointer = read_pointer(&pointer_path)?;
        return Ok(ResolvedArtifact {
            pointer_path,
            generation_id: pointer.generation_id,
        });
    }
    if !metadata.is_file() {
        bail!(
            "Checkpoint archaeology artifact {} is neither a pointer, manifest, nor monolithic object",
            source.display()
        );
    }

    if let Ok(pointer) = read_pointer(source) {
        return Ok(ResolvedArtifact {
            pointer_path: source.to_path_buf(),
            generation_id: pointer.generation_id,
        });
    }

    let parent = source.parent().ok_or_else(|| {
        anyhow!(
            "Checkpoint archaeology artifact {} has no parent directory",
            source.display()
        )
    })?;
    let collection = parent.file_name().and_then(|part| part.to_str());
    let expected_extension = source.extension().and_then(|part| part.to_str());
    let expected_collection = match (collection, expected_extension) {
        (Some("manifests"), Some("json")) => "manifests",
        (Some("objects"), Some("jsonl")) => "objects",
        _ => bail!(
            "Checkpoint archaeology artifact {} is not a generation pointer, manifests/*.json, or objects/*.jsonl artifact",
            source.display()
        ),
    };
    let checkpoint_base = parent.parent().ok_or_else(|| {
        anyhow!(
            "Checkpoint archaeology artifact {} has no checkpoint-set directory",
            source.display()
        )
    })?;
    let filename = source
        .file_name()
        .and_then(|part| part.to_str())
        .ok_or_else(|| anyhow!("Checkpoint archaeology artifact has a non-UTF-8 filename"))?;
    let relative = format!("{expected_collection}/{filename}");

    for pointer_name in ["current.json", "previous.json"] {
        let pointer_path = checkpoint_base.join(pointer_name);
        let Ok(pointer) = read_pointer(&pointer_path) else {
            continue;
        };
        if pointer.active_root.path == relative {
            return Ok(ResolvedArtifact {
                pointer_path,
                generation_id: pointer.generation_id,
            });
        }
    }

    bail!(
        "Checkpoint archaeology artifact {} is not selected by current.json or previous.json; manifests and objects are served only through a retained, verifiable generation pointer",
        source.display()
    )
}

fn read_pointer(path: &Path) -> Result<PointerDocument> {
    let bytes = fs::read(path).with_context(|| {
        format!(
            "Failed to read checkpoint generation pointer {}",
            path.display()
        )
    })?;
    reject_archaeology_document(&bytes, path)?;
    serde_json::from_slice(&bytes).map_err(|error| {
        anyhow!(
            "Checkpoint archaeology requires a generation pointer at {}: {}",
            path.display(),
            error
        )
    })
}

fn reject_archaeology_document(bytes: &[u8], path: &Path) -> Result<()> {
    let Ok(value) = serde_json::from_slice::<Value>(bytes) else {
        return Ok(());
    };
    let Some(object) = value.as_object() else {
        return Ok(());
    };
    let marked = ["artifact_kind", "kind", "$schema"]
        .iter()
        .filter_map(|key| object.get(*key).and_then(Value::as_str))
        .any(|value| value.to_ascii_lowercase().contains("archaeology"));
    if marked || object.get("importable") == Some(&Value::Bool(false)) {
        bail!(
            "Refusing R029 checkpoint archaeology view {} as an input: archaeology outputs are explicitly non-importable",
            path.display()
        );
    }
    Ok(())
}

fn checked_relative_path(
    base: &Path,
    relative: &str,
    directory: &str,
    extension: &str,
) -> Result<PathBuf> {
    let path = Path::new(relative);
    if path.is_absolute()
        || path.components().count() != 2
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        || path
            .components()
            .next()
            .and_then(|component| component.as_os_str().to_str())
            != Some(directory)
        || path.extension().and_then(|value| value.to_str()) != Some(extension)
    {
        bail!("Invalid checkpoint-relative {directory} path '{relative}'");
    }
    Ok(base.join(path))
}

fn read_sharded_records(
    base: &Path,
    manifest_path: &Path,
) -> Result<(Vec<Issue>, Vec<SerializedEvent>)> {
    let bytes = fs::read(manifest_path).with_context(|| {
        format!(
            "Failed to read verified manifest {}",
            manifest_path.display()
        )
    })?;
    let manifest: Value = serde_json::from_slice(&bytes).with_context(|| {
        format!(
            "Verified manifest {} is no longer valid JSON",
            manifest_path.display()
        )
    })?;
    let mut issues = Vec::new();
    let mut events = Vec::new();
    for field in ["issue_shards", "event_shards", "receipt_shards"] {
        let shards = manifest
            .get(field)
            .and_then(Value::as_array)
            .ok_or_else(|| anyhow!("Verified manifest is missing {field}"))?;
        for shard in shards {
            let relative = shard
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("Verified manifest has a {field} entry without path"))?;
            let path = checked_relative_path(base, relative, "objects", "jsonl")?;
            let (mut shard_issues, mut shard_events) = read_records(&path)?;
            issues.append(&mut shard_issues);
            events.append(&mut shard_events);
        }
    }
    Ok((issues, events))
}

fn read_records(path: &Path) -> Result<(Vec<Issue>, Vec<SerializedEvent>)> {
    let data = fs::read_to_string(path).with_context(|| {
        format!(
            "Failed to read verified checkpoint record file {}",
            path.display()
        )
    })?;
    let mut issues = Vec::new();
    let mut events = Vec::new();
    for (index, line) in data.lines().enumerate() {
        let record: Value = serde_json::from_str(line).with_context(|| {
            format!(
                "Verified checkpoint record file {} became malformed at line {}",
                path.display(),
                index + 1
            )
        })?;
        match record.get("record_type").and_then(Value::as_str) {
            Some("issue") => {
                let issue = record.get("issue").ok_or_else(|| {
                    anyhow!(
                        "Issue record at {}:{} has no issue",
                        path.display(),
                        index + 1
                    )
                })?;
                issues.push(serde_json::from_value(issue.clone()).with_context(|| {
                    format!(
                        "Issue record at {}:{} is no longer valid",
                        path.display(),
                        index + 1
                    )
                })?);
            }
            Some("event") => {
                let event = record.get("event").ok_or_else(|| {
                    anyhow!(
                        "Event record at {}:{} has no event",
                        path.display(),
                        index + 1
                    )
                })?;
                events.push(serde_json::from_value(event.clone()).with_context(|| {
                    format!(
                        "Event record at {}:{} is no longer valid",
                        path.display(),
                        index + 1
                    )
                })?);
            }
            Some("provenance_receipt") => {}
            other => bail!(
                "Verified checkpoint record file {} has unsupported record type {:?} at line {}",
                path.display(),
                other,
                index + 1
            ),
        }
    }
    Ok((issues, events))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_delta_ignores_object_key_order() {
        let from = BTreeMap::from([(
            "issue-1".to_string(),
            serde_json::json!({"title": "one", "priority": 1}),
        )]);
        let to = BTreeMap::from([(
            "issue-1".to_string(),
            serde_json::json!({"priority": 1, "title": "one"}),
        )]);
        assert!(semantic_deltas(from, to).is_empty());
    }

    #[test]
    fn semantic_delta_is_stably_ordered() {
        let from = BTreeMap::from([("b".to_string(), serde_json::json!(1))]);
        let to = BTreeMap::from([
            ("a".to_string(), serde_json::json!(1)),
            ("b".to_string(), serde_json::json!(2)),
        ]);
        let deltas = semantic_deltas(from, to);
        assert_eq!(
            deltas
                .iter()
                .map(|delta| &delta.identity)
                .collect::<Vec<_>>(),
            ["a", "b"]
        );
        assert_eq!(deltas[0].change, "added");
        assert_eq!(deltas[1].change, "changed");
    }
}

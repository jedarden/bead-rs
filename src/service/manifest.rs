//! Atomic bulk transaction manifests (R033).
//!
//! A manifest is a versioned JSON document composed strictly of existing
//! command primitives — creates, updates, labels, dependencies, closes —
//! with `$name` local references for beads earlier operations created.
//! `manifest_dry_run` reports the full semantic delta without mutation by
//! executing the manifest inside one transaction that is always rolled
//! back; `manifest_commit` applies every operation in one transaction so
//! the R026 post-commit chokepoint publishes at most one checkpoint
//! generation for the whole manifest. Version 1 refuses any semantics a
//! single existing command does not already have; the normative contract
//! is `research/specs/bulk-manifests-v1.md`.

use crate::error::{Error, Result};
use crate::model::Issue;
use crate::scan::{self, Field, ScanConfig, ScanReport};
use crate::store::WorkspaceConfig;
use rusqlite::{Connection, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;

/// The only manifest version this implementation accepts.
pub const MANIFEST_VERSION: i64 = 1;

/// Maximum number of bytes of one manifest file.
pub const MAX_MANIFEST_BYTES: u64 = 8 * 1024 * 1024;

/// Maximum length of one `local_id` in UTF-8 bytes.
pub const MAX_LOCAL_ID_BYTES: usize = 64;

/// One parsed manifest: a version and an ordered operation list.
#[derive(Debug, Clone)]
pub struct Manifest {
    pub version: i64,
    pub operations: Vec<ManifestOp>,
}

/// The v1 operation set: each variant is exactly one existing command.
#[derive(Debug, Clone)]
pub enum ManifestOp {
    Create(CreateOp),
    Update(UpdateOp),
    LabelAdd(LabelOp),
    LabelRemove(LabelOp),
    DepAdd(DepAddOp),
    DepRemove(DepRemoveOp),
    Close(CloseOp),
}

impl ManifestOp {
    /// The command kind name used in reports and error context.
    pub fn kind(&self) -> &'static str {
        match self {
            ManifestOp::Create(_) => "create",
            ManifestOp::Update(_) => "update",
            ManifestOp::LabelAdd(_) => "label_add",
            ManifestOp::LabelRemove(_) => "label_remove",
            ManifestOp::DepAdd(_) => "dep_add",
            ManifestOp::DepRemove(_) => "dep_remove",
            ManifestOp::Close(_) => "close",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateOp {
    pub local_id: Option<String>,
    pub title: String,
    pub description: Option<String>,
    #[serde(default = "default_priority")]
    pub priority: i64,
    pub issue_type: Option<String>,
    pub assignee: Option<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub resource_keys: Vec<String>,
    pub unique_ref: Option<String>,
}

fn default_priority() -> i64 {
    2
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateOp {
    pub id: String,
    pub status: Option<String>,
    pub assignee: Option<String>,
    #[serde(default)]
    pub clear_assignee: bool,
    pub notes: Option<String>,
    pub if_revision: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LabelOp {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DepAddOp {
    pub blocked: String,
    pub blocker: String,
    #[serde(default = "default_dep_kind")]
    pub kind: String,
}

fn default_dep_kind() -> String {
    "blocks".to_string()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DepRemoveOp {
    pub blocked: String,
    pub blocker: String,
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CloseOp {
    pub id: String,
    pub reason: String,
    pub if_revision: Option<i64>,
}

/// Report shared by dry-run and commit: the per-operation result map plus
/// manifest-level totals.
#[derive(Debug, Clone, Serialize)]
pub struct ManifestReport {
    pub manifest_version: i64,
    pub committed: bool,
    pub dry_run: bool,
    pub operations: usize,
    pub semantic_changes: usize,
    pub workspace_sequence: i64,
    pub results: Vec<Value>,
}

/// Read and parse a manifest file, applying document validation only.
///
/// Document validation covers JSON shape, the closed v1 operation schema,
/// `local_id` syntax and uniqueness, the update-supplies-a-field rule, and
/// static local-reference resolution (every `$name` must match the
/// `local_id` of an earlier create). Nothing here touches the workspace.
pub fn load_manifest(path: &Path) -> Result<Manifest> {
    let metadata = std::fs::metadata(path).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => {
            Error::not_found(format!("manifest file not found: {}", path.display()))
        }
        _ => Error::Io {
            path: path.to_path_buf(),
            msg: error,
        },
    })?;
    if metadata.len() > MAX_MANIFEST_BYTES {
        return Err(Error::integrity(format!(
            "manifest file exceeds {} bytes: {}",
            MAX_MANIFEST_BYTES,
            path.display()
        )));
    }

    let content = std::fs::read_to_string(path).map_err(|error| Error::Io {
        path: path.to_path_buf(),
        msg: error,
    })?;
    parse_manifest(&content)
}

/// Parse and document-validate manifest text.
pub fn parse_manifest(content: &str) -> Result<Manifest> {
    let document: Value = serde_json::from_str(content)
        .map_err(|error| Error::integrity(format!("manifest is not valid JSON: {}", error)))?;

    let root = document
        .as_object()
        .ok_or_else(|| Error::integrity("manifest must be a JSON object"))?;

    if root.len() != 2 {
        return Err(Error::integrity(
            "manifest must contain exactly the keys 'manifest_version' and 'operations'",
        ));
    }

    let version = root
        .get("manifest_version")
        .ok_or_else(|| Error::integrity("manifest is missing 'manifest_version'"))?
        .as_i64()
        .ok_or_else(|| Error::integrity("'manifest_version' must be the integer 1".to_string()))?;
    if version != MANIFEST_VERSION {
        return Err(Error::integrity(format!(
            "unsupported manifest_version {} (this bead-rs understands version {})",
            version, MANIFEST_VERSION
        )));
    }

    let operations_value = root
        .get("operations")
        .ok_or_else(|| Error::integrity("manifest is missing 'operations'"))?;
    let operations_array = operations_value
        .as_array()
        .ok_or_else(|| Error::integrity("'operations' must be an array".to_string()))?;

    let mut operations = Vec::with_capacity(operations_array.len());
    for (index, op_value) in operations_array.iter().enumerate() {
        operations.push(parse_operation(index, op_value)?);
    }

    validate_local_ids(&operations)?;
    validate_references(&operations)?;
    validate_updates_supply_a_field(&operations)?;

    Ok(Manifest {
        version,
        operations,
    })
}

/// Scan the complete canonical text of every operation before the manifest's
/// single semantic transaction opens. Each operation has its own stable,
/// non-secret selector; the merged report provides one all-or-none verdict.
pub fn scan_manifest(config: &ScanConfig, manifest: &Manifest) -> ScanReport {
    ScanReport::merge(
        manifest
            .operations
            .iter()
            .enumerate()
            .map(|(index, operation)| {
                let selector = format!("manifest:operation:{index}");
                let fields = manifest_fields(operation);
                scan::scan(config, &selector, &fields)
            }),
    )
}

fn manifest_fields(operation: &ManifestOp) -> Vec<Field<'_>> {
    let mut fields = Vec::new();
    match operation {
        ManifestOp::Create(op) => {
            push_optional(&mut fields, "local_id", op.local_id.as_deref());
            fields.push(Field::new("title", &op.title));
            push_optional(&mut fields, "description", op.description.as_deref());
            push_optional(&mut fields, "issue_type", op.issue_type.as_deref());
            push_optional(&mut fields, "assignee", op.assignee.as_deref());
            fields.extend(op.labels.iter().map(|value| Field::new("labels[]", value)));
            fields.extend(
                op.resource_keys
                    .iter()
                    .map(|value| Field::new("resource_keys[]", value)),
            );
            push_optional(&mut fields, "unique_ref", op.unique_ref.as_deref());
        }
        ManifestOp::Update(op) => {
            fields.push(Field::new("id", &op.id));
            push_optional(&mut fields, "status", op.status.as_deref());
            push_optional(&mut fields, "assignee", op.assignee.as_deref());
            push_optional(&mut fields, "notes", op.notes.as_deref());
        }
        ManifestOp::LabelAdd(op) | ManifestOp::LabelRemove(op) => {
            fields.push(Field::new("id", &op.id));
            fields.push(Field::new("label", &op.label));
        }
        ManifestOp::DepAdd(op) => {
            fields.push(Field::new("blocked_issue_id", &op.blocked));
            fields.push(Field::new("blocker_issue_id", &op.blocker));
            fields.push(Field::new("kind", &op.kind));
        }
        ManifestOp::DepRemove(op) => {
            fields.push(Field::new("blocked_issue_id", &op.blocked));
            fields.push(Field::new("blocker_issue_id", &op.blocker));
            push_optional(&mut fields, "kind", op.kind.as_deref());
        }
        ManifestOp::Close(op) => {
            fields.push(Field::new("id", &op.id));
            fields.push(Field::new("close_reason", &op.reason));
        }
    }
    fields
}

fn push_optional<'a>(fields: &mut Vec<Field<'a>>, path: &'a str, value: Option<&'a str>) {
    if let Some(value) = value {
        fields.push(Field::new(path, value));
    }
}

/// Closed-schema parse of one operation object.
fn parse_operation(index: usize, value: &Value) -> Result<ManifestOp> {
    let malformed = |message: String| Error::integrity(format!("operation {index}: {message}"));

    let object = value
        .as_object()
        .ok_or_else(|| malformed("each operation must be a JSON object".to_string()))?;

    let kind = object
        .get("op")
        .ok_or_else(|| malformed("operation is missing 'op'".to_string()))?
        .as_str()
        .ok_or_else(|| malformed("'op' must be a string".to_string()))?;

    // The `op` discriminator has been consumed by the match below; strip it
    // so each closed-schema struct rejects unknown fields without rejecting
    // its own kind tag.
    let mut fields = object.clone();
    fields.remove("op");
    let body = Value::Object(fields);

    let parsed = match kind {
        "create" => ManifestOp::Create(
            serde_json::from_value(body.clone()).map_err(|error| malformed(error.to_string()))?,
        ),
        "update" => ManifestOp::Update(
            serde_json::from_value(body.clone()).map_err(|error| malformed(error.to_string()))?,
        ),
        "label_add" => ManifestOp::LabelAdd(
            serde_json::from_value(body.clone()).map_err(|error| malformed(error.to_string()))?,
        ),
        "label_remove" => ManifestOp::LabelRemove(
            serde_json::from_value(body.clone()).map_err(|error| malformed(error.to_string()))?,
        ),
        "dep_add" => ManifestOp::DepAdd(
            serde_json::from_value(body.clone()).map_err(|error| malformed(error.to_string()))?,
        ),
        "dep_remove" => ManifestOp::DepRemove(
            serde_json::from_value(body.clone()).map_err(|error| malformed(error.to_string()))?,
        ),
        "close" => ManifestOp::Close(
            serde_json::from_value(body.clone()).map_err(|error| malformed(error.to_string()))?,
        ),
        other => {
            return Err(malformed(format!(
                "unknown op '{other}' (v1 allows create, update, label_add, label_remove, \
                 dep_add, dep_remove, close)"
            )));
        }
    };

    Ok(parsed)
}

/// `local_id` syntax and uniqueness.
fn validate_local_ids(operations: &[ManifestOp]) -> Result<()> {
    let mut seen = HashMap::new();
    for (index, op) in operations.iter().enumerate() {
        if let ManifestOp::Create(create) = op {
            if let Some(local_id) = &create.local_id {
                if local_id.is_empty()
                    || local_id.len() > MAX_LOCAL_ID_BYTES
                    || !local_id
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-')
                    || local_id.contains('$')
                {
                    return Err(Error::integrity(format!(
                        "operation {index}: local_id must be 1-{} bytes of letters, digits, \
                         '_', '.', or '-' and must not contain '$'",
                        MAX_LOCAL_ID_BYTES
                    )));
                }
                if seen.contains_key(local_id) {
                    return Err(Error::integrity(format!(
                        "operation {index}: local_id '{local_id}' is already defined"
                    )));
                }
                seen.insert(local_id.clone(), index);
            }
        }
    }
    Ok(())
}

/// Every issue-naming field of an operation, as raw strings.
fn target_fields(op: &ManifestOp) -> Vec<&str> {
    match op {
        ManifestOp::Create(_) => vec![],
        ManifestOp::Update(op) => vec![op.id.as_str()],
        ManifestOp::LabelAdd(op) | ManifestOp::LabelRemove(op) => vec![op.id.as_str()],
        ManifestOp::DepAdd(op) => vec![op.blocked.as_str(), op.blocker.as_str()],
        ManifestOp::DepRemove(op) => vec![op.blocked.as_str(), op.blocker.as_str()],
        ManifestOp::Close(op) => vec![op.id.as_str()],
    }
}

/// Static local-reference resolution: a `$name` must name the `local_id` of
/// an earlier create, so every reference is resolvable before execution.
fn validate_references(operations: &[ManifestOp]) -> Result<()> {
    let mut defined: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for (index, op) in operations.iter().enumerate() {
        for field in target_fields(op) {
            if let Some(name) = field.strip_prefix('$') {
                if !defined.contains(name) {
                    return Err(Error::integrity(format!(
                        "operation {index} ({}): local reference '{field}' does not name the \
                         local_id of an earlier create",
                        op.kind()
                    )));
                }
            }
        }
        if let ManifestOp::Create(create) = op {
            if let Some(local_id) = &create.local_id {
                defined.insert(local_id.as_str());
            }
        }
    }
    Ok(())
}

/// An update that supplies nothing to change is malformed, matching the
/// command's refusal of a no-field update at the earliest possible layer.
fn validate_updates_supply_a_field(operations: &[ManifestOp]) -> Result<()> {
    for (index, op) in operations.iter().enumerate() {
        if let ManifestOp::Update(update) = op {
            let supplied = update.status.is_some()
                || update.assignee.is_some()
                || update.clear_assignee
                || update.notes.is_some();
            if !supplied {
                return Err(Error::integrity(format!(
                    "operation {index}: update supplies none of 'status', 'assignee', \
                     'clear_assignee', 'notes'"
                )));
            }
        }
    }
    Ok(())
}

/// Execute a whole manifest inside the caller's transaction, in order.
///
/// Every operation runs its command's own in-transaction code path, so
/// validation, guards, events, and idempotence are identical to running
/// the commands one at a time — the only difference is that exactly one
/// transaction wraps them all. Returns the per-operation result values and
/// the count of semantic changes.
fn execute_manifest_in_tx(
    tx: &mut Transaction,
    config: &WorkspaceConfig,
    manifest: &Manifest,
) -> Result<(Vec<Value>, usize)> {
    let mut locals: HashMap<String, String> = HashMap::new();
    let mut results = Vec::with_capacity(manifest.operations.len());
    let mut semantic_changes = 0usize;

    for (index, op) in manifest.operations.iter().enumerate() {
        let result = execute_operation_in_tx(tx, config, index, op, &mut locals)?;
        if result.get("semantic_change").and_then(Value::as_bool) == Some(true) {
            semantic_changes += 1;
        }
        results.push(result);
    }

    Ok((results, semantic_changes))
}

/// Execute one operation; see `execute_manifest_in_tx`.
fn execute_operation_in_tx(
    tx: &mut Transaction,
    config: &WorkspaceConfig,
    index: usize,
    op: &ManifestOp,
    locals: &mut HashMap<String, String>,
) -> Result<Value> {
    let context = |error: Error| with_operation_context(index, op.kind(), error);

    let mut result = match op {
        ManifestOp::Create(create) => {
            let created = crate::service::issues::create_issue_with_unique_ref(
                tx,
                config,
                create.title.clone(),
                create.description.clone(),
                create.priority,
                create.issue_type.clone(),
                create.assignee.clone(),
                create.labels.clone(),
                create.resource_keys.clone(),
                create.unique_ref.as_deref(),
            )
            .map_err(context)?;

            if let Some(local_id) = &create.local_id {
                locals.insert(local_id.clone(), created.issue.id.clone());
            }

            let outcome = match created.outcome {
                crate::service::CreateOutcome::Created => "created",
                crate::service::CreateOutcome::Existing { closed: false } => "existing",
                crate::service::CreateOutcome::Existing { closed: true } => "existing_closed",
            };
            json!({
                "index": index,
                "op": "create",
                "local_id": create.local_id,
                "issue_id": created.issue.id,
                "outcome": outcome,
                "semantic_change": outcome == "created",
                "issue": issue_projection(tx, &created.issue).map_err(context)?,
            })
        }
        ManifestOp::Update(update) => {
            let id = resolve_target(&update.id, locals).map_err(context)?;
            let before = issue_snapshot(tx, &id).map_err(context)?;
            crate::service::lifecycle::update_issue_in_tx(
                tx,
                &id,
                update.status.as_deref(),
                update.assignee.as_deref(),
                update.clear_assignee,
                update.notes.as_deref(),
                update.if_revision,
                None,
            )
            .map_err(context)?;
            state_change_result(index, "update", &id, before.as_ref(), tx).map_err(context)?
        }
        ManifestOp::Close(close) => {
            let id = resolve_target(&close.id, locals).map_err(context)?;
            let before = issue_snapshot(tx, &id).map_err(context)?;
            crate::service::lifecycle::close_issue_in_tx(
                tx,
                &id,
                &close.reason,
                close.if_revision,
                None,
            )
            .map_err(context)?;
            state_change_result(index, "close", &id, before.as_ref(), tx).map_err(context)?
        }
        ManifestOp::LabelAdd(label) => {
            let id = resolve_target(&label.id, locals).map_err(context)?;
            let added = crate::service::dependencies::add_label_in_tx(tx, &id, &label.label)
                .map_err(context)?;
            json!({
                "index": index,
                "op": "label_add",
                "id": id,
                "label": label.label,
                "outcome": if added { "added" } else { "no-op" },
                "semantic_change": added,
            })
        }
        ManifestOp::LabelRemove(label) => {
            let id = resolve_target(&label.id, locals).map_err(context)?;
            let removed = crate::service::dependencies::remove_label_in_tx(tx, &id, &label.label)
                .map_err(context)?;
            json!({
                "index": index,
                "op": "label_remove",
                "id": id,
                "label": label.label,
                "outcome": if removed { "removed" } else { "no-op" },
                "semantic_change": removed,
            })
        }
        ManifestOp::DepAdd(dep) => {
            let blocked = resolve_target(&dep.blocked, locals).map_err(context)?;
            let blocker = resolve_target(&dep.blocker, locals).map_err(context)?;
            let added = crate::service::dependencies::add_dependency_in_tx(
                tx, &blocked, &blocker, &dep.kind, None,
            )
            .map_err(context)?;
            json!({
                "index": index,
                "op": "dep_add",
                "blocked": blocked,
                "blocker": blocker,
                "kind": dep.kind,
                "outcome": if added { "added" } else { "no-op" },
                "semantic_change": added,
            })
        }
        ManifestOp::DepRemove(dep) => {
            let blocked = resolve_target(&dep.blocked, locals).map_err(context)?;
            let blocker = resolve_target(&dep.blocker, locals).map_err(context)?;
            let removed = crate::service::dependencies::remove_dependency_in_tx(
                tx,
                &blocked,
                &blocker,
                dep.kind.as_deref(),
            )
            .map_err(context)?;
            json!({
                "index": index,
                "op": "dep_remove",
                "blocked": blocked,
                "blocker": blocker,
                "kind": dep.kind,
                "outcome": if removed { "removed" } else { "no-op" },
                "semantic_change": removed,
            })
        }
    };

    if result.get("index").is_none() {
        result["index"] = json!(index);
    }
    Ok(result)
}

/// The before-snapshot fields a state operation's delta reports.
struct IssueSnapshot {
    base_status: String,
    assignee: Option<String>,
    manual_blocked: Option<bool>,
    close_reason: Option<String>,
    revision: i64,
    labels: Vec<String>,
    notes: Option<String>,
}

fn snapshot_of(conn: &Connection, issue: &Issue) -> Result<IssueSnapshot> {
    Ok(IssueSnapshot {
        base_status: issue.base_status.to_string(),
        assignee: issue.assignee.clone(),
        manual_blocked: issue.manual_blocked,
        close_reason: issue.close_reason.clone(),
        revision: issue.revision.unwrap_or(1),
        labels: labels_of(conn, &issue.id)?,
        notes: issue.notes.clone(),
    })
}

/// Labels attached to an issue, sorted for stable comparison.
fn labels_of(conn: &Connection, id: &str) -> Result<Vec<String>> {
    let mut stmt =
        conn.prepare_cached("SELECT label FROM labels WHERE issue_id = ?1 ORDER BY label")?;
    let labels = stmt
        .query_map([&id], |row| row.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(labels)
}

fn issue_snapshot(conn: &Connection, id: &str) -> Result<Option<IssueSnapshot>> {
    crate::service::issues::get_issue_by_id(conn, id)?
        .as_ref()
        .map(|issue| snapshot_of(conn, issue))
        .transpose()
}

/// Build the delta result for update/close: outcome, semantic_change, and a
/// before/after `changes` object over the fields the commands can move.
///
/// Every real `update` and `close` mutation bumps the revision, so a revision
/// delta is exactly the commands' own notion of a semantic change; an
/// idempotent no-op leaves the revision alone.
fn state_change_result(
    index: usize,
    kind: &str,
    id: &str,
    before: Option<&IssueSnapshot>,
    tx: &Transaction,
) -> Result<Value> {
    // The operation's own validation guarantees the issue exists, so this
    // re-read cannot miss unless integrity already failed.
    let after = issue_snapshot(tx, id)?
        .ok_or_else(|| Error::integrity(format!("issue {id} vanished inside the transaction")))?;

    let changes = before.map(|before| diff_snapshots(before, &after));
    let semantic_change = before
        .map(|before| before.revision != after.revision)
        .unwrap_or(true);
    let outcome = match (kind, semantic_change) {
        ("update", true) => "updated",
        ("close", true) => "closed",
        (_, false) => "no-op",
        _ => kind,
    };

    Ok(json!({
        "index": index,
        "op": kind,
        "id": id,
        "outcome": outcome,
        "semantic_change": semantic_change,
        "changes": changes.unwrap_or_default(),
    }))
}

/// Before/after delta of the fields `update` and `close` can change.
fn diff_snapshots(before: &IssueSnapshot, after: &IssueSnapshot) -> Value {
    let mut changes = serde_json::Map::new();
    let mut field = |name: &str, before_value: Value, after_value: Value| {
        if before_value != after_value {
            changes.insert(
                name.to_string(),
                json!({"before": before_value, "after": after_value}),
            );
        }
    };
    field(
        "base_status",
        json!(before.base_status),
        json!(after.base_status),
    );
    field("assignee", json!(before.assignee), json!(after.assignee));
    field(
        "manual_blocked",
        json!(before.manual_blocked),
        json!(after.manual_blocked),
    );
    field(
        "close_reason",
        json!(before.close_reason),
        json!(after.close_reason),
    );
    field("revision", json!(before.revision), json!(after.revision));
    field("labels", json!(before.labels), json!(after.labels));
    if before.notes != after.notes {
        changes.insert("notes_changed".to_string(), json!(true));
    }
    Value::Object(changes)
}

/// The public projection of a created issue in a result entry.
fn issue_projection(conn: &Connection, issue: &Issue) -> Result<Value> {
    Ok(json!({
        "id": issue.id,
        "title": issue.title,
        "priority": issue.priority,
        "issue_type": issue.issue_type,
        "assignee": issue.assignee,
        "base_status": issue.base_status.to_string(),
        "labels": labels_of(conn, &issue.id)?,
        "resource_keys": issue.extensions
            .get(crate::service::resource_locks::RESOURCE_KEYS_EXTENSION)
            .cloned()
            .unwrap_or_else(|| json!([])),
    }))
}

/// Resolve one issue-naming field to a real ID.
fn resolve_target(raw: &str, locals: &HashMap<String, String>) -> Result<String> {
    match raw.strip_prefix('$') {
        Some(name) => locals.get(name).cloned().ok_or_else(|| {
            // Document validation already rejected unresolved references,
            // so reaching here means an earlier create in this manifest
            // failed to record its local_id — an integrity failure.
            Error::integrity(format!("local reference '{raw}' was not recorded"))
        }),
        None => Ok(raw.to_string()),
    }
}

/// Prepend the failing operation's position and kind to a primitive's
/// error while preserving the primitive's own variant, so the exit code
/// stays the command's exit code.
fn with_operation_context(index: usize, kind: &str, error: Error) -> Error {
    let message = format!("operation {index} ({kind}): {error}");
    match error {
        Error::CliUsage(_) | Error::Model(_) => Error::cli_usage(message),
        Error::Workspace(_) => Error::workspace(message),
        Error::Conflict(_) => Error::conflict(message),
        // ValidationError maps to exit 4, the same as Conflict.
        Error::Validation(_) => Error::conflict(message),
        Error::LeaseExpired(_) => Error::LeaseExpired(message),
        Error::LeaseConflict(_) => Error::LeaseConflict(message),
        Error::ClaimRefused { .. } => Error::conflict(message),
        Error::Integrity(_) => Error::integrity(message),
        Error::DatabaseBusy(_) => Error::DatabaseBusy(message),
        Error::PostCommitPublicationFailed { source } => {
            Error::PostCommitPublicationFailed { source }
        }
        Error::RedactionPublicationFailed { receipt_id, source } => {
            Error::RedactionPublicationFailed { receipt_id, source }
        }
        Error::Redaction(error) => Error::Redaction(error),
        Error::Internal(_) => Error::Internal(anyhow::anyhow!(message)),
        Error::Sqlite(_) => Error::Internal(anyhow::anyhow!(message)),
        Error::Json(_) => Error::Internal(anyhow::anyhow!(message)),
        Error::Io { path, msg } => Error::Io { path, msg },
    }
}

fn read_sequence(conn: &Connection) -> Result<i64> {
    Ok(
        conn.query_row("SELECT COALESCE(MAX(sequence), 0) FROM events", [], |row| {
            row.get(0)
        })?,
    )
}

/// Execute the manifest and roll everything back, reporting the delta a
/// commit would produce. Created IDs in the report are provisional: they
/// are generated per execution and commit generates fresh ones.
pub fn manifest_dry_run(
    conn: &Connection,
    config: &WorkspaceConfig,
    manifest: &Manifest,
) -> Result<ManifestReport> {
    let sequence_before = read_sequence(conn)?;
    let mut tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    let (results, semantic_changes) = execute_manifest_in_tx(&mut tx, config, manifest)?;
    tx.rollback()?;
    Ok(ManifestReport {
        manifest_version: manifest.version,
        committed: false,
        dry_run: true,
        operations: manifest.operations.len(),
        semantic_changes,
        workspace_sequence: sequence_before,
        results,
    })
}

/// Execute the whole manifest in one transaction and commit once. Any
/// failure rolls back every operation; the R026 chokepoint then publishes
/// at most one generation for the committed span.
pub fn manifest_commit(
    conn: &Connection,
    config: &WorkspaceConfig,
    manifest: &Manifest,
) -> Result<ManifestReport> {
    let mut tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    let (results, semantic_changes) = execute_manifest_in_tx(&mut tx, config, manifest)?;
    let sequence_after = read_sequence(&tx)?;
    tx.commit()?;
    Ok(ManifestReport {
        manifest_version: manifest.version,
        committed: true,
        dry_run: false,
        operations: manifest.operations.len(),
        semantic_changes,
        workspace_sequence: sequence_after,
        results,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_minimal_v1_manifest() {
        let manifest = parse_manifest(
            r#"{"manifest_version": 1, "operations": [
                {"op": "create", "local_id": "a", "title": "one"},
                {"op": "dep_add", "blocked": "$a", "blocker": "bead-00000000"}
            ]}"#,
        )
        .unwrap();
        assert_eq!(manifest.version, 1);
        assert_eq!(manifest.operations.len(), 2);
        assert_eq!(manifest.operations[0].kind(), "create");
        assert_eq!(manifest.operations[1].kind(), "dep_add");
    }

    #[test]
    fn scans_every_operation_before_the_manifest_transaction() {
        let value = [["AK", "IA"].concat(), "7M4Q9Z2N8C5R3T6V".to_string()].concat();
        let document = serde_json::json!({
            "manifest_version": 1,
            "operations": [
                {"op": "create", "local_id": "a", "title": "one", "description": value},
                {"op": "update", "id": "$a", "notes": "safe"},
                {"op": "label_add", "id": "$a", "label": "safe"},
                {"op": "dep_add", "blocked": "$a", "blocker": "bead-00000000"},
                {"op": "close", "id": "$a", "reason": "safe"}
            ]
        });
        let manifest = parse_manifest(&document.to_string()).unwrap();
        let report = scan_manifest(&ScanConfig::enforce(), &manifest);
        assert_eq!(report.blocking.len(), 1);
        assert_eq!(report.blocking[0].selector, "manifest:operation:0");
        assert_eq!(report.blocking[0].field_path, "description");
        assert!(!serde_json::to_string(&report.blocking[0])
            .unwrap()
            .contains(&value));
    }

    #[test]
    fn refuses_unknown_version_op_and_fields() {
        assert!(parse_manifest(r#"{"manifest_version": 2, "operations": []}"#).is_err());
        assert!(parse_manifest(
            r#"{"manifest_version": 1, "operations": [
            {"op": "reopen", "id": "bead-x"}]}"#
        )
        .is_err());
        assert!(parse_manifest(
            r#"{"manifest_version": 1, "operations": [
            {"op": "create", "title": "t", "mystery": true}]}"#
        )
        .is_err());
    }

    #[test]
    fn refuses_forward_and_undefined_references() {
        assert!(parse_manifest(
            r#"{"manifest_version": 1, "operations": [
            {"op": "close", "id": "$a", "reason": "r"},
            {"op": "create", "local_id": "a", "title": "t"}]}"#
        )
        .is_err());
        assert!(parse_manifest(
            r#"{"manifest_version": 1, "operations": [
            {"op": "label_add", "id": "$nope", "label": "x"}]}"#
        )
        .is_err());
    }

    #[test]
    fn refuses_duplicate_local_ids_and_fieldless_updates() {
        assert!(parse_manifest(
            r#"{"manifest_version": 1, "operations": [
            {"op": "create", "local_id": "a", "title": "t"},
            {"op": "create", "local_id": "a", "title": "u"}]}"#
        )
        .is_err());
        assert!(parse_manifest(
            r#"{"manifest_version": 1, "operations": [
            {"op": "update", "id": "bead-x"}]}"#
        )
        .is_err());
    }

    #[test]
    fn accepts_an_empty_manifest() {
        let manifest = parse_manifest(r#"{"manifest_version": 1, "operations": []}"#).unwrap();
        assert!(manifest.operations.is_empty());
    }
}

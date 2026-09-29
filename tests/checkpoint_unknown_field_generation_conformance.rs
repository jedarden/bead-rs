//! Unknown-field conformance at every checkpoint object level, across
//! generations, through the recovery paths that rewrite history.
//!
//! AGENTS.md's compatibility contract: "Unknown JSON fields must survive
//! import/export round trips." The other unknown-field suites pin single
//! levels or single legs; this one walks a payload through every level the
//! checkpoint format can carry -- issue, event, dependency edge, external
//! reference, structured data envelope, resource key, attempt outcome,
//! provenance receipt, and the `current.json` pointer itself -- across
//! three published generations connected by the native recovery path
//! (`sync import-only --restore-into-empty`), and then through the two
//! paths that rewrite historical records: the stale-event merge replay
//! (`--merge` of a log the destination already holds) and historical
//! redaction of a rewritten event.
//!
//! Unknown fields are seeded where no CLI produces them, exactly the way the
//! other suites seed theirs: direct writes to the store tables and pointer
//! a newer producer would have written. Each generation is diffed against
//! the first: every seeded payload must come back parse-equal -- shapes
//! intact, never dropped, never rewrapped -- and the identity fields around
//! them must be stable, so a silent reinterpretation cannot hide as a
//! payload change.
//!
//! The merge replay is exercised across all three outcomes the contract
//! allows for events carrying unknown extensions: replaying the log the
//! destination already holds must land exactly where it started
//! (idempotent); a newer producer's wider map -- the seeded value grown,
//! plus an additive key beside it -- must replace the stored map outright
//! (widen); and a replay stripped of additive fields, the rendering of an
//! older producer who never knew the keys, must never erase what earlier
//! generations stored (strip/narrow). Redaction is exercised on the
//! extension-bearing event itself: the rewrite replaces the field's bytes
//! and its integrity hash, and the unknown fields riding beside them must
//! survive untouched into the next published generation.
//!
//! The comparison is only worth what it catches, so a closing negative
//! test deliberately drops each level's seeded extension from a published
//! generation and requires the comparison to trip naming that level -- the
//! preservation assertions are proven able to fail, not merely able to
//! pass.

use assert_cmd::Command;
use bead_rs::model::redaction::REDACTION_MARKER;
use bead_rs::service::secret_diagnostics::scan_live_findings;
use predicates::prelude::*;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// Every key an issue object may carry that is NOT an unknown extension:
/// the serialized fields of `Issue` plus the projected collections the
/// checkpoint layer embeds and routes to their own tables.
const KNOWN_ISSUE_KEYS: [&str; 24] = [
    "id",
    "title",
    "revision",
    "description",
    "notes",
    "priority",
    "base_status",
    "manual_blocked",
    "assignee",
    "claim_epoch",
    "issue_type",
    "created_at",
    "updated_at",
    "closed_at",
    "close_reason",
    "source_repo",
    "profile",
    "schema_ref",
    "data",
    "dependencies",
    "labels",
    "comments",
    "external_references",
    "resource_keys",
];

/// Every key a serialized event record may carry beside its additive
/// extensions (src/service/checkpoint.rs `EventRecord`).
const KNOWN_EVENT_KEYS: [&str; 8] = [
    "$schema",
    "origin_store_uuid",
    "origin_event_sequence",
    "issue_id",
    "kind",
    "actor",
    "time",
    "detail",
];

/// Every key a serialized provenance receipt may carry beside its additive
/// extensions (`ProvenanceReceipt`).
const KNOWN_RECEIPT_KEYS: [&str; 12] = [
    "$schema",
    "receipt_id",
    "kind",
    "source_store_uuid",
    "target_store_uuid",
    "source_root_sha256",
    "actor",
    "created_at",
    "counts",
    "result",
    "summary_event_identity",
    "receipt_sha256",
];

/// Every key a projected dependency edge entry may carry beside its
/// additive extensions.
const KNOWN_DEPENDENCY_ENTRY_KEYS: [&str; 2] = ["blocker", "kind"];

/// Every key a projected external-reference entry may carry beside its
/// additive extensions.
const KNOWN_REFERENCE_ENTRY_KEYS: [&str; 4] = ["namespace", "key", "value", "unique_ref"];

/// Every key a structured-data envelope may carry beside its additive
/// extensions.
const KNOWN_DATA_ENVELOPE_KEYS: [&str; 2] = ["schema_ref", "value"];

/// Every key a projected resource-key entry may carry beside its additive
/// extensions.
const KNOWN_RESOURCE_KEY_ENTRY_KEYS: [&str; 1] = ["resource_key"];

/// Every key a serialized attempt-outcome record may carry beside its
/// additive extensions (`AttemptOutcomeRecord`).
const KNOWN_ATTEMPT_OUTCOME_KEYS: [&str; 17] = [
    "$schema",
    "attempt_id",
    "issue_id",
    "outcome",
    "action",
    "reason",
    "canonical_request_hash",
    "resulting_issue_revision",
    "resulting_state",
    "resulting_attempt_tier",
    "receipt_id",
    "actor",
    "created_at",
    "evidence_refs",
    "model",
    "harness",
    "harness_version",
];

/// Every top-level key a generation pointer may carry beside its additive
/// extensions (src/service/checkpoint.rs `POINTER_KNOWN_KEYS`).
const KNOWN_POINTER_KEYS: [&str; 19] = [
    "schema_version",
    "generation_id",
    "mode",
    "store_uuid",
    "snapshot_sequence",
    "active_root",
    "added_paths",
    "replaced_paths",
    "deleted_paths",
    "issue_count",
    "event_count",
    "receipt_count",
    "attempt_outcome_count",
    "redaction_record_count",
    "total_record_count",
    "created_at",
    "redaction_epoch_id",
    "previous_generation_reset",
    "superseded_generations",
];

/// The pointer key a newer producer would add, seeded into `current.json`
/// by hand and required to ride every later generation.
const POINTER_EXTENSION_KEY: &str = "future_pointer_key";

/// The additive event key the widened merge replay writes beside the seeded
/// one, and which the stripped replay must not cause to erase. Widening is
/// not only additive: `widened_event_payload` also grows the seeded key's
/// own value, and that wider value must replace the stored one outright.
const EVENT_WIDEN_KEY: &str = "future_tier_replayed";

fn issue_payload() -> Value {
    json!({
        "future_priority_signal": { "weight": 7, "tags": ["alpha", null] },
        "future_note": "carry me across generations",
    })
}

fn event_payload() -> Value {
    json!({ "future_tier": { "level": 3, "path": ["x", "y"] } })
}

/// The exact map a newer producer replays for the seeded event: the seeded
/// key carrying a wider value -- nested shapes only a later writer would
/// add -- plus the additive `EVENT_WIDEN_KEY` beside it. It is both the
/// widen replay's input and the map the store must hold afterwards: the
/// wider value replaces the older one outright, and the stripped replay
/// must not roll either key back.
fn widened_event_payload() -> Value {
    let mut payload = event_payload();
    payload["future_tier"]["widened"] = json!({ "by": "replay-ufk", "gates": [true, null] });
    payload[EVENT_WIDEN_KEY] = json!("widened by replay");
    payload
}

/// Like the record corpus's payloads, every store-record payload below
/// nests an object and an array -- the shapes a projection can flatten,
/// stringify, or reorder -- so scalar-only survival cannot pass for
/// preservation. Keys stay stable across edits: the deliberate-drop test
/// removes them by name.
fn dependency_payload() -> Value {
    json!({ "future_weight": { "lanes": [1.5, 2.5], "gate": { "soft": true } } })
}

fn reference_payload() -> Value {
    json!({ "future_visibility": { "audiences": ["audit", "ops"], "gate": { "internal": true } } })
}

fn data_payload() -> Value {
    json!({ "future_etag": { "digests": ["etag-11", "etag-12"], "gate": { "weak": false } } })
}

fn resource_key_payload() -> Value {
    json!({ "future_exclusive_until": { "windows": ["2026-12-01"], "gate": { "hard": true } } })
}

fn attempt_outcome_payload() -> Value {
    json!({ "future_telemetry": { "gpus": ["0", "1"], "gate": { "sampled": false } } })
}

fn receipt_payload() -> Value {
    json!({ "future_signed_by": { "keys": ["pk-11"], "gate": { "sealed": true } } })
}

fn pointer_extension_value() -> Value {
    json!({ "v": 1 })
}

fn bead(dir: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::cargo_bin("bead").unwrap();
    cmd.args(args).current_dir(dir);
    cmd
}

fn init_workspace(workspace: &Path) {
    bead(
        workspace,
        &[
            "init",
            "--prefix",
            "ufk",
            "--no-auto-flush",
            "--skip-foreign-workspace",
        ],
    )
    .assert()
    .success();
    suppress_auto_flush(workspace);
}

/// Pin publication to the explicit `sync flush-only` so each generation is
/// observed exactly once, by the test, with no auto publisher interleaving
/// (the same suppression the corpus suite applies; the init flag alone does
/// not stop mutation-triggered publication).
fn suppress_auto_flush(workspace: &Path) {
    let config_path = workspace.join(".beads/config.json");
    let mut config: Value =
        serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
    config.as_object_mut().unwrap().insert(
        "checkpoint".to_string(),
        serde_json::json!({ "auto_flush": false }),
    );
    fs::write(&config_path, serde_json::to_string(&config).unwrap()).unwrap();
}

fn extract_id(output: &str) -> String {
    output
        .split_whitespace()
        .find(|token| token.starts_with("ufk-"))
        .unwrap_or_else(|| panic!("no issue id in output: {output}"))
        .to_string()
}

fn create_issue(workspace: &Path, title: &str) -> String {
    let output = bead(workspace, &["create", "--title", title])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    extract_id(&String::from_utf8_lossy(&output))
}

fn flush(workspace: &Path) {
    bead(workspace, &["sync", "flush-only"])
        .assert()
        .success()
        .stderr(predicate::str::contains("Flushed forensic checkpoint:"));
}

fn restore_into_empty(checkpoint_dir: &Path, workspace: &Path, actor: &str, issues: usize) {
    bead(
        workspace,
        &[
            "sync",
            "import-only",
            "--input",
            checkpoint_dir.to_str().unwrap(),
            "--restore-into-empty",
            "--actor",
            actor,
        ],
    )
    .assert()
    .success()
    .stderr(predicate::str::contains(format!(
        "Restored {issues} issues"
    )));
}

/// Replay a forensic log into a workspace that already holds it, and
/// require the merge to have inserted nothing: replay refreshes additive
/// fields, it never duplicates records.
fn merge_replay(workspace: &Path, log: &Path, actor: &str) {
    let output = bead(
        workspace,
        &[
            "sync",
            "import-only",
            "--input",
            log.to_str().unwrap(),
            "--merge",
            "--actor",
            actor,
        ],
    )
    .assert()
    .success()
    .get_output()
    .stderr
    .clone();
    assert!(
        String::from_utf8_lossy(&output).contains("0 inserted"),
        "a replay of an already-held log must insert nothing: {}",
        String::from_utf8_lossy(&output)
    );
}

fn open_db(workspace: &Path) -> rusqlite::Connection {
    rusqlite::Connection::open(workspace.join(".beads/beads.db")).unwrap()
}

/// Seed the issue-level unknown fields the way every other suite does:
/// rows in `issue_extensions`, the storage the importer itself uses.
fn insert_issue_extensions(workspace: &Path, issue_id: &str, payload: &Value) {
    let conn = open_db(workspace);
    for (key, value) in payload.as_object().unwrap() {
        conn.execute(
            "INSERT INTO issue_extensions (issue_id, key, value, profile)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                issue_id,
                key,
                serde_json::to_string(value).unwrap(),
                "native-v1"
            ],
        )
        .unwrap();
    }
}

/// Seed the event-level unknown fields on the store's earliest event. The
/// exported identity is captured from generation 1, never re-derived here.
fn set_first_event_extension(workspace: &Path, payload: &Value) {
    let conn = open_db(workspace);
    let sequence: i64 = conn
        .query_row(
            "SELECT sequence FROM events ORDER BY sequence ASC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    conn.execute(
        "UPDATE events SET extensions_json = ?1 WHERE sequence = ?2",
        rusqlite::params![serde_json::to_string(payload).unwrap(), sequence],
    )
    .unwrap();
}

fn set_dependency_extension(workspace: &Path, blocked: &str, blocker: &str, payload: &Value) {
    let conn = open_db(workspace);
    let changed = conn
        .execute(
            "UPDATE dependencies SET extensions_json = ?1
             WHERE blocked_issue_id = ?2 AND blocker_issue_id = ?3",
            rusqlite::params![serde_json::to_string(payload).unwrap(), blocked, blocker],
        )
        .unwrap();
    assert_eq!(changed, 1, "the dependency edge must exist to be seeded");
}

fn insert_external_reference(workspace: &Path, issue_id: &str, payload: &Value) {
    let conn = open_db(workspace);
    conn.execute(
        "INSERT INTO external_references (issue_id, namespace, key, value, extensions_json)
         VALUES (?1, 'future-tracker', 'src-id', 'src-value-1', ?2)",
        rusqlite::params![issue_id, serde_json::to_string(payload).unwrap()],
    )
    .unwrap();
}

fn insert_issue_data(workspace: &Path, issue_id: &str, payload: &Value) {
    let conn = open_db(workspace);
    conn.execute(
        "INSERT INTO issue_data (issue_id, namespace, schema_ref, value, extensions_json)
         VALUES (?1, 'future-config', 'urn:test:future', '{\"region\":\"iad\"}', ?2)",
        rusqlite::params![issue_id, serde_json::to_string(payload).unwrap()],
    )
    .unwrap();
}

fn set_resource_key_extension(workspace: &Path, issue_id: &str, payload: &Value) {
    let conn = open_db(workspace);
    let changed = conn
        .execute(
            "UPDATE issue_resource_keys SET extensions_json = ?1
             WHERE issue_id = ?2 AND resource_key = 'gpu:0'",
            rusqlite::params![serde_json::to_string(payload).unwrap(), issue_id],
        )
        .unwrap();
    assert_eq!(changed, 1, "the resource key must exist to be seeded");
}

/// Seed an attempt-outcome row carrying unknown fields, and return the
/// attempt id whose exported record must carry them.
fn insert_extended_attempt_outcome(workspace: &Path, issue_id: &str, payload: &Value) -> String {
    let attempt_id = String::from("urn:needle:attempt:generations-001");
    let conn = open_db(workspace);
    conn.execute(
        "INSERT INTO attempt_outcomes (
            receipt_id, attempt_id, issue_id, outcome, action, reason,
            canonical_request_hash, prior_attempt_tier, resulting_attempt_tier,
            resulting_issue_revision, actor, created_at, evidence_refs_json,
            model, harness, harness_version, resulting_state, extensions_json
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
        rusqlite::params![
            "ao-generations-001",
            &attempt_id,
            issue_id,
            "verified_success",
            "none",
            "generation conformance attempt",
            "generations-canonical-request-hash",
            0i64,
            0i64,
            0i64,
            "ufk-worker",
            "2026-09-29T00:00:00Z",
            r#"["s3:logs/generations.tar.gz"]"#,
            "glm-5.3-flash",
            "needle",
            "1.0.0",
            "open",
            serde_json::to_string(payload).unwrap(),
        ],
    )
    .unwrap();
    attempt_id
}

/// Seed the provenance receipt's unknown fields and return the receipt ID
/// whose exported record must carry them.
fn extend_single_receipt(workspace: &Path, payload: &Value) -> String {
    let conn = open_db(workspace);
    let receipt_id: String = conn
        .query_row("SELECT receipt_id FROM provenance_receipts", [], |row| {
            row.get(0)
        })
        .unwrap();
    conn.execute(
        "UPDATE provenance_receipts SET extensions_json = ?1 WHERE receipt_id = ?2",
        rusqlite::params![serde_json::to_string(payload).unwrap(), &receipt_id],
    )
    .unwrap();
    receipt_id
}

/// A stored per-record extension map, decoded. `Null` when the column is
/// empty, so a missing seed fails loudly at the comparison.
fn stored_extensions(workspace: &Path, query: &str, params: &[&str]) -> Value {
    let conn = open_db(workspace);
    let raw: Option<String> = conn
        .query_row(query, rusqlite::params_from_iter(params.iter()), |row| {
            row.get(0)
        })
        .unwrap();
    match raw {
        Some(raw) => serde_json::from_str(&raw).unwrap(),
        None => Value::Null,
    }
}

/// The seeded event's stored extension map: the row the restore wrote and
/// each replay outcome below rewrites. Exactly one extension-bearing row
/// exists for the origin store, as the corpus seeds it.
fn stored_event_extensions(workspace: &Path, identity: &(String, i64)) -> Value {
    stored_extensions(
        workspace,
        "SELECT extensions_json FROM events
         WHERE origin_store_uuid = ?1 AND extensions_json IS NOT NULL",
        &[&identity.0],
    )
}

/// Add the unknown top-level pointer key to a workspace's published
/// `current.json`, the way a newer producer's republish would have left it.
fn seed_pointer_extension(workspace: &Path) {
    let path = workspace.join(".beads/checkpoint/current.json");
    let mut pointer: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    pointer
        .as_object_mut()
        .unwrap()
        .insert(POINTER_EXTENSION_KEY.to_string(), pointer_extension_value());
    fs::write(&path, serde_json::to_string_pretty(&pointer).unwrap()).unwrap();
}

fn read_pointer(workspace: &Path) -> Value {
    serde_json::from_str(
        &fs::read_to_string(workspace.join(".beads/checkpoint/current.json")).unwrap(),
    )
    .unwrap()
}

fn read_jsonl(path: &Path) -> Vec<Value> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

/// Every record of a workspace's active published generation, any layout.
fn active_generation_records(workspace: &Path) -> Vec<Value> {
    let checkpoint_dir = workspace.join(".beads/checkpoint");
    let pointer = read_pointer(workspace);
    let root = checkpoint_dir.join(pointer["active_root"]["path"].as_str().unwrap());

    if pointer["mode"] == "sharded" {
        let manifest: Value = serde_json::from_str(&fs::read_to_string(&root).unwrap()).unwrap();
        let mut records = Vec::new();
        let extend_shards = |key: &str, records: &mut Vec<Value>| {
            for shard in manifest[key].as_array().unwrap() {
                records.extend(read_jsonl(
                    &checkpoint_dir.join(shard["path"].as_str().unwrap()),
                ));
            }
        };
        extend_shards("issue_shards", &mut records);
        extend_shards("event_shards", &mut records);
        extend_shards("receipt_shards", &mut records);
        extend_shards("attempt_outcome_shards", &mut records);
        records
    } else {
        read_jsonl(&root)
    }
}

fn records_of_type<'a>(records: &'a [Value], record_type: &str) -> Vec<&'a Value> {
    records
        .iter()
        .filter(|record| record["record_type"] == record_type)
        .collect()
}

/// The unknown members of one exported object: every key beyond the known
/// native ones. Parsed-JSON equality of this map is the preservation
/// assertion -- loss, rewrapping, and reinterpretation all fail it.
fn unknown_members(value: &Value, known: &[&str]) -> serde_json::Map<String, Value> {
    value
        .as_object()
        .unwrap()
        .iter()
        .filter(|(key, _)| !known.contains(&key.as_str()))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

fn object_of(payload: &Value) -> serde_json::Map<String, Value> {
    payload.as_object().unwrap().clone()
}

/// The exported issue record carrying the seeded payloads.
fn issue_record<'a>(records: &'a [Value], issue_id: &str) -> &'a Value {
    records_of_type(records, "issue")
        .into_iter()
        .map(|record| &record["issue"])
        .find(|issue| issue["id"] == issue_id)
        .unwrap_or_else(|| panic!("issue {issue_id} missing from generation"))
}

/// The exported event record carrying the seeded event payload.
fn extended_event(records: &[Value]) -> &Value {
    let carriers: Vec<&Value> = records_of_type(records, "event")
        .into_iter()
        .map(|record| &record["event"])
        .filter(|event| event.get("future_tier").is_some())
        .collect();
    assert_eq!(
        carriers.len(),
        1,
        "exactly one event must carry the seeded payload"
    );
    carriers[0]
}

/// The exported attempt-outcome record carrying the seeded payload.
fn extended_attempt_outcome<'a>(records: &'a [Value], attempt_id: &str) -> &'a Value {
    records_of_type(records, "attempt_outcome")
        .into_iter()
        .map(|record| &record["attempt_outcome"])
        .find(|outcome| outcome["attempt_id"].as_str() == Some(attempt_id))
        .unwrap_or_else(|| panic!("attempt outcome {attempt_id} missing from generation"))
}

/// Identity of an exported event: the two fields every later generation
/// must reproduce for the rewritten records to stay the same record.
fn event_identity(event: &Value) -> (String, i64) {
    (
        event["origin_store_uuid"].as_str().unwrap().to_string(),
        event["origin_event_sequence"].as_i64().unwrap(),
    )
}

/// Copy a forensic log, mapping every event record through `edit`.
fn edited_forensic_log(source: &Path, destination: &Path, edit: impl Fn(&mut Value)) {
    let content = fs::read_to_string(source).unwrap();
    let mut edited = String::new();
    for line in content.lines() {
        if line.trim().is_empty() {
            edited.push_str(line);
            edited.push('\n');
            continue;
        }
        let mut record: Value = serde_json::from_str(line).unwrap();
        if let Some(event) = record.as_object_mut().unwrap().get_mut("event") {
            edit(event);
        }
        edited.push_str(&serde_json::to_string(&record).unwrap());
        edited.push('\n');
    }
    fs::write(destination, edited).unwrap();
}

/// The three-workspace generation chain plus everything the assertions need
/// to locate the seeded records in each generation.
struct Generations {
    _workspaces: Vec<TempDir>,
    generation_1: Vec<Value>,
    generation_2: Vec<Value>,
    generation_3: Vec<Value>,
    issue_a: String,
    issue_b: String,
    attempt_id: String,
    receipt_with_extensions: String,
    extended_event_identity: (String, i64),
}

impl Generations {
    fn workspace(&self, index: usize) -> &Path {
        self._workspaces[index].path()
    }
}

/// Build the full generation chain:
///
/// 1. source workspace: real lifecycle, unknown fields seeded at every
///    record level, generation 1; then the pointer seeded and a republish
///    to prove the same workspace re-projects the pointer key.
/// 2. restored workspace: restore-into-empty of the source's checkpoint
///    directory (pointer staging, so the pointer key rides), the restore
///    receipt seeded, the stale-event merge replay exercised through all
///    three outcomes -- idempotent, widen, strip/narrow; generation 2.
/// 3. round-tripped workspace: restore-into-empty one hop further;
///    generation 3.
fn build_generations() -> Generations {
    // -- source workspace ----------------------------------------------------
    let source = tempfile::Builder::new()
        .prefix("ufk-source-")
        .tempdir_in("/var/tmp")
        .unwrap();
    init_workspace(source.path());

    let issue_a = create_issue(source.path(), "generation payload A — every level");
    let issue_b = create_issue(source.path(), "generation payload B — edge target");
    let issue_c = create_issue(source.path(), "generation payload C — closed");
    bead(
        source.path(),
        &["update", &issue_a, "--notes", "lifecycle notes"],
    )
    .assert()
    .success();
    bead(source.path(), &["label", "add", "--label", "ufk", &issue_a])
        .assert()
        .success();
    bead(
        source.path(),
        &["dep", "add", &issue_a, &issue_b, "--kind", "blocks"],
    )
    .assert()
    .success();
    bead(
        source.path(),
        &["close", &issue_c, "--reason", "ufk-complete"],
    )
    .assert()
    .success();
    bead(
        source.path(),
        &["resource", "add", &issue_a, "--key", "gpu:0"],
    )
    .assert()
    .success();

    insert_issue_extensions(source.path(), &issue_a, &issue_payload());
    set_first_event_extension(source.path(), &event_payload());
    set_dependency_extension(source.path(), &issue_a, &issue_b, &dependency_payload());
    insert_external_reference(source.path(), &issue_a, &reference_payload());
    insert_issue_data(source.path(), &issue_a, &data_payload());
    set_resource_key_extension(source.path(), &issue_a, &resource_key_payload());
    let attempt_id =
        insert_extended_attempt_outcome(source.path(), &issue_a, &attempt_outcome_payload());

    flush(source.path());
    let generation_1 = active_generation_records(source.path());
    let extended_event_identity = event_identity(extended_event(&generation_1));

    // Seed the pointer key and republish: the outgoing pointer's unknown
    // fields must ride the next generation produced by the same workspace.
    seed_pointer_extension(source.path());
    create_issue(source.path(), "generation probe 1 — forces the republish");
    flush(source.path());
    let source_checkpoint = source.path().join(".beads/checkpoint");

    // -- restored workspace ---------------------------------------------------
    let restored = tempfile::Builder::new()
        .prefix("ufk-restored-")
        .tempdir_in("/var/tmp")
        .unwrap();
    init_workspace(restored.path());
    restore_into_empty(&source_checkpoint, restored.path(), "ufk-restore", 4);

    // Every record-level seed must have landed in the restored store's own
    // columns, and the pointer key in checkpoint_state.
    let event_extensions = stored_event_extensions(restored.path(), &extended_event_identity);
    assert_eq!(
        event_extensions.get("future_tier"),
        Some(&event_payload()["future_tier"]),
        "the restored event must carry its seeded extension map: {event_extensions}"
    );
    assert_eq!(
        stored_extensions(
            restored.path(),
            "SELECT extensions_json FROM dependencies
             WHERE blocked_issue_id = ?1 AND blocker_issue_id = ?2",
            &[&issue_a, &issue_b],
        ),
        dependency_payload()
    );
    assert_eq!(
        stored_extensions(
            restored.path(),
            "SELECT extensions_json FROM external_references WHERE issue_id = ?1",
            &[&issue_a],
        ),
        reference_payload()
    );
    assert_eq!(
        stored_extensions(
            restored.path(),
            "SELECT extensions_json FROM issue_data WHERE issue_id = ?1",
            &[&issue_a],
        ),
        data_payload()
    );
    assert_eq!(
        stored_extensions(
            restored.path(),
            "SELECT extensions_json FROM issue_resource_keys WHERE issue_id = ?1",
            &[&issue_a],
        ),
        resource_key_payload()
    );
    assert_eq!(
        stored_extensions(
            restored.path(),
            "SELECT extensions_json FROM attempt_outcomes WHERE attempt_id = ?1",
            &[attempt_id.as_str()],
        ),
        attempt_outcome_payload()
    );
    let pointer_state = stored_extensions(
        restored.path(),
        "SELECT pointer_extensions_json FROM checkpoint_state WHERE id = 1",
        &[],
    );
    assert_eq!(
        pointer_state[POINTER_EXTENSION_KEY],
        pointer_extension_value(),
        "restore-into-empty must stage the pointer key for the next publication"
    );

    let receipt_with_extensions = extend_single_receipt(restored.path(), &receipt_payload());

    // -- stale-event merge replay: all three replay outcomes ------------------
    let replay_log = source_checkpoint.join("forensic.jsonl");

    // Idempotent: the destination replays the log it restored from, whose
    // producer value is exactly what it stores. The replay's event map is
    // non-empty, so the refresh branch runs and must land precisely where
    // it started -- asserted immediately, because the widen and strip
    // replays below rewrite the same map and would mask an idempotent
    // replay that erased instead of refreshed.
    merge_replay(restored.path(), &replay_log, "ufk-replay");
    let after_idempotent = stored_event_extensions(restored.path(), &extended_event_identity);
    assert_eq!(
        after_idempotent,
        event_payload(),
        "an idempotent replay must leave the stored extension map exactly as stored: {after_idempotent}"
    );

    // Widen: a newer producer replays the extension-bearing event with a
    // wider map -- the seeded key's value grown by shapes only a later
    // writer would add, plus an additive key beside it. The replay replaces
    // the stored map wholesale, so the wider value must replace the older
    // one outright: never a merge that keeps the stale value, never an
    // erasure of the key, never an error.
    let widened = tempfile::Builder::new()
        .prefix("ufk-widened-")
        .suffix(".jsonl")
        .tempfile_in("/var/tmp")
        .unwrap();
    edited_forensic_log(&replay_log, widened.path(), |event| {
        let object = event.as_object_mut().unwrap();
        if !object.contains_key("future_tier") {
            return; // a newer producer re-writes only the extension-bearing event
        }
        for (key, value) in widened_event_payload().as_object().unwrap() {
            object.insert(key.clone(), value.clone());
        }
    });
    merge_replay(restored.path(), widened.path(), "ufk-replay-widen");
    let after_widen = stored_event_extensions(restored.path(), &extended_event_identity);
    assert_eq!(
        after_widen,
        widened_event_payload(),
        "a newer producer's wider extension value must replace the stored one: {after_widen}"
    );

    // Strip/narrow: the same log as an older producer would have written it
    // -- every additive field stripped, because that writer never knew the
    // keys. Its event maps are empty, so the replay must skip the refresh
    // entirely and leave both generations' fields exactly as the widen left
    // them; a would-be erasure of either key, or any value change, fails
    // this comparison.
    let stripped = tempfile::Builder::new()
        .prefix("ufk-stripped-")
        .suffix(".jsonl")
        .tempfile_in("/var/tmp")
        .unwrap();
    edited_forensic_log(&replay_log, stripped.path(), |event| {
        let unknown: Vec<String> = event
            .as_object()
            .unwrap()
            .keys()
            .filter(|key| !KNOWN_EVENT_KEYS.contains(&key.as_str()))
            .cloned()
            .collect();
        for key in unknown {
            event.as_object_mut().unwrap().remove(&key);
        }
    });
    merge_replay(restored.path(), stripped.path(), "ufk-replay-strip");
    let after_strip = stored_event_extensions(restored.path(), &extended_event_identity);
    assert_eq!(
        after_strip, after_widen,
        "a stripped replay must not erase what earlier generations stored: {after_strip}"
    );

    create_issue(restored.path(), "generation probe 2 — forces the export");
    flush(restored.path());
    let generation_2 = active_generation_records(restored.path());
    let restored_checkpoint = restored.path().join(".beads/checkpoint");

    // -- round-tripped workspace ----------------------------------------------
    let round_tripped = tempfile::Builder::new()
        .prefix("ufk-round-trip-")
        .tempdir_in("/var/tmp")
        .unwrap();
    init_workspace(round_tripped.path());
    restore_into_empty(
        &restored_checkpoint,
        round_tripped.path(),
        "ufk-round-trip",
        5,
    );
    create_issue(
        round_tripped.path(),
        "generation probe 3 — forces the re-export",
    );
    flush(round_tripped.path());
    let generation_3 = active_generation_records(round_tripped.path());

    Generations {
        _workspaces: vec![source, restored, round_tripped],
        generation_1,
        generation_2,
        generation_3,
        issue_a,
        issue_b,
        attempt_id,
        receipt_with_extensions,
        extended_event_identity,
    }
}

/// Every seeded unknown field must appear parse-equal at its own level in
/// the generation, and nowhere else: the exact map, no strays.
fn assert_levels_preserved(generation: &[Value], first: &Generations, event_widened: bool) {
    let issue = issue_record(generation, &first.issue_a);

    let issue_unknown = unknown_members(issue, &KNOWN_ISSUE_KEYS);
    assert_eq!(
        issue_unknown,
        object_of(&issue_payload()),
        "issue-level unknown fields must survive parse-equal"
    );

    let dependencies = issue["dependencies"].as_array().unwrap();
    let edge = dependencies
        .iter()
        .find(|entry| entry["blocker"] == first.issue_b.as_str())
        .unwrap_or_else(|| panic!("dependency edge missing from generation"));
    assert_eq!(
        unknown_members(edge, &KNOWN_DEPENDENCY_ENTRY_KEYS),
        object_of(&dependency_payload()),
        "dependency-entry unknown fields must survive parse-equal"
    );

    let references = issue["external_references"].as_array().unwrap();
    let reference = references
        .iter()
        .find(|entry| entry["key"] == "src-id")
        .unwrap_or_else(|| panic!("external reference missing from generation"));
    assert_eq!(
        unknown_members(reference, &KNOWN_REFERENCE_ENTRY_KEYS),
        object_of(&reference_payload()),
        "external-reference unknown fields must survive parse-equal"
    );

    let envelope = &issue["data"]["future-config"];
    assert_eq!(
        unknown_members(envelope, &KNOWN_DATA_ENVELOPE_KEYS),
        object_of(&data_payload()),
        "structured-data envelope unknown fields must survive parse-equal"
    );

    let resource_keys = issue["resource_keys"].as_array().unwrap();
    let resource_key = resource_keys
        .iter()
        .find(|entry| entry["resource_key"] == "gpu:0")
        .unwrap_or_else(|| panic!("resource key entry missing from generation"));
    assert_eq!(
        unknown_members(resource_key, &KNOWN_RESOURCE_KEY_ENTRY_KEYS),
        object_of(&resource_key_payload()),
        "resource-key unknown fields must survive parse-equal"
    );

    let outcome = extended_attempt_outcome(generation, &first.attempt_id);
    assert_eq!(
        unknown_members(outcome, &KNOWN_ATTEMPT_OUTCOME_KEYS),
        object_of(&attempt_outcome_payload()),
        "attempt-outcome unknown fields must survive parse-equal"
    );

    let event = extended_event(generation);
    assert_eq!(
        event_identity(event),
        first.extended_event_identity,
        "the extension-bearing event must keep its wire identity across generations"
    );
    let expected_event = if event_widened {
        object_of(&widened_event_payload())
    } else {
        object_of(&event_payload())
    };
    assert_eq!(
        unknown_members(event, &KNOWN_EVENT_KEYS),
        expected_event,
        "event-level unknown fields must survive parse-equal (and keep the replay's widened map)"
    );
}

/// A generation pointer must re-project the seeded top-level key.
fn assert_pointer_extension(pointer: &Value, label: &str) {
    let unknown = unknown_members(pointer, &KNOWN_POINTER_KEYS);
    assert_eq!(
        unknown.get(POINTER_EXTENSION_KEY),
        Some(&pointer_extension_value()),
        "{label} pointer must re-project {POINTER_EXTENSION_KEY} verbatim"
    );
}

/// The provenance receipt carrying seeded unknown fields must keep them
/// parse-equal in every generation that exports receipts.
fn assert_receipt_extension(generation: &[Value], receipt_id: &str) {
    let receipt = records_of_type(generation, "provenance_receipt")
        .into_iter()
        .map(|record| &record["provenance_receipt"])
        .find(|receipt| receipt["receipt_id"] == receipt_id)
        .unwrap_or_else(|| panic!("receipt {receipt_id} missing from generation"));
    assert_eq!(
        unknown_members(receipt, &KNOWN_RECEIPT_KEYS),
        object_of(&receipt_payload()),
        "receipt-level unknown fields must survive parse-equal"
    );
}

#[test]
fn unknown_fields_survive_every_level_across_generations() {
    let generations = build_generations();

    assert_levels_preserved(&generations.generation_1, &generations, false);
    assert_levels_preserved(&generations.generation_2, &generations, true);
    assert_levels_preserved(&generations.generation_3, &generations, true);

    assert_receipt_extension(
        &generations.generation_2,
        &generations.receipt_with_extensions,
    );
    assert_receipt_extension(
        &generations.generation_3,
        &generations.receipt_with_extensions,
    );

    assert_pointer_extension(&read_pointer(generations.workspace(0)), "source republish");
    assert_pointer_extension(
        &read_pointer(generations.workspace(1)),
        "restored republish",
    );
    assert_pointer_extension(
        &read_pointer(generations.workspace(2)),
        "round-tripped republish",
    );
}

/// Historical redaction rewrites an event's `detail` in place -- the one
/// path besides merge replay that mutates checkpoint-carried history after
/// the fact. The rewrite replaces the field bytes and the integrity hash;
/// the unknown fields riding beside them must reach the next generation
/// untouched, on the very record that was rewritten.
#[test]
fn redaction_rewrite_preserves_unknown_fields_on_the_rewritten_event() {
    let generations = build_generations();
    let round_tripped = generations.workspace(2);
    let (origin_uuid, origin_sequence) = &generations.extended_event_identity;

    // Seed a secret into the extension-bearing event's detail, so the
    // redaction target IS the record whose unknown fields are under test.
    let secret = ["AK", "IA", "7Q9W2E4R6T8Y1U3I"].concat();
    {
        let conn = open_db(round_tripped);
        conn.execute(
            "UPDATE events SET detail = ?1
             WHERE origin_store_uuid = ?2 AND origin_event_sequence = ?3",
            rusqlite::params![
                format!(r#"{{"credential":"{secret}"}}"#),
                origin_uuid,
                origin_sequence
            ],
        )
        .unwrap();
    }

    let fingerprint = {
        let conn = open_db(round_tripped);
        let findings = scan_live_findings(&conn)
            .unwrap()
            .into_iter()
            .filter(|finding| {
                finding.selector.starts_with("live:events:")
                    && finding.field_path == "detail"
                    && finding.rule_id == "aws-access-key-id"
                    && finding.is_blocking_match()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            findings.len(),
            1,
            "the seeded secret must yield one finding"
        );
        findings[0].fingerprint.clone()
    };

    bead(
        round_tripped,
        &[
            "redact",
            "--finding",
            &fingerprint,
            "--actor",
            "ufk-redaction",
            "--reason",
            "strip the seeded secret",
            "--json",
        ],
    )
    .assert()
    .success();

    create_issue(
        round_tripped,
        "generation probe 4 — forces the post-redaction export",
    );
    flush(round_tripped);
    let generation_4 = active_generation_records(round_tripped);

    assert_levels_preserved(&generation_4, &generations, true);
    assert_receipt_extension(&generation_4, &generations.receipt_with_extensions);

    let rewritten = extended_event(&generation_4);
    // The documented marker semantics (historical-redaction-v1 §3): the
    // matched value is replaced with the fixed marker and no other byte of
    // the field changes -- the JSON envelope around the secret survives
    // intact, just as the extension keys beside it did above.
    assert_eq!(
        rewritten["detail"],
        json!({ "credential": REDACTION_MARKER }),
        "redaction must replace the value and change no other byte: {}",
        rewritten["detail"]
    );
    assert!(
        !serde_json::to_string(&generation_4)
            .unwrap()
            .contains(&secret),
        "the redacted bytes must not survive anywhere in the published generation"
    );
}

/// Remove one member from an object, refusing to continue when the member
/// was never there: a mutation that removes nothing would let a vacuous
/// failure masquerade as coverage.
fn remove_member(object: &mut Value, key: &str) {
    if object
        .as_object_mut()
        .expect("the mutated position must be a JSON object")
        .remove(key)
        .is_none()
    {
        panic!("extension key {key} was absent; the deliberate drop would be vacuous");
    }
}

/// The mutable issue record carrying the seeded payloads.
fn issue_record_mut<'a>(records: &'a mut [Value], issue_id: &str) -> &'a mut Value {
    let record = records
        .iter_mut()
        .find(|record| record["record_type"] == "issue" && record["issue"]["id"] == issue_id)
        .unwrap_or_else(|| panic!("issue {issue_id} missing from the generation under mutation"));
    &mut record["issue"]
}

fn drop_issue_extension(records: &mut [Value], issue_id: &str, key: &str) {
    remove_member(issue_record_mut(records, issue_id), key);
}

fn drop_dependency_extension(records: &mut [Value], blocked: &str, blocker: &str, key: &str) {
    let edge = issue_record_mut(records, blocked)["dependencies"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entry| entry["blocker"] == blocker)
        .unwrap_or_else(|| panic!("the dependency edge to {blocker} is missing"));
    remove_member(edge, key);
}

fn drop_reference_extension(records: &mut [Value], issue_id: &str, key: &str) {
    let reference = issue_record_mut(records, issue_id)["external_references"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entry| entry["key"] == "src-id")
        .unwrap_or_else(|| panic!("the seeded external reference is missing"));
    remove_member(reference, key);
}

fn drop_data_extension(records: &mut [Value], issue_id: &str, key: &str) {
    let envelope = &mut issue_record_mut(records, issue_id)["data"]["future-config"];
    remove_member(envelope, key);
}

fn drop_resource_key_extension(records: &mut [Value], issue_id: &str, key: &str) {
    let entry = issue_record_mut(records, issue_id)["resource_keys"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entry| entry["resource_key"] == "gpu:0")
        .unwrap_or_else(|| panic!("the seeded resource key entry is missing"));
    remove_member(entry, key);
}

/// The mutable exported event record carrying the seeded payload.
fn extended_event_mut<'a>(records: &'a mut [Value], identity: &(String, i64)) -> &'a mut Value {
    records
        .iter_mut()
        .find(|record| {
            record["record_type"] == "event"
                && record["event"]["origin_store_uuid"] == identity.0.as_str()
                && record["event"]["origin_event_sequence"] == identity.1
        })
        .map(|record| &mut record["event"])
        .unwrap_or_else(|| panic!("the seeded event is missing from the generation under mutation"))
}

fn drop_event_extension(records: &mut [Value], identity: &(String, i64), key: &str) {
    remove_member(extended_event_mut(records, identity), key);
}

/// Overwrite one member of the extension-bearing event in place: the
/// narrowing a stale replay would have performed -- a wider value rolled
/// back to an older producer's -- without removing the key.
fn narrow_event_extension(
    records: &mut [Value],
    identity: &(String, i64),
    key: &str,
    value: Value,
) {
    extended_event_mut(records, identity)
        .as_object_mut()
        .unwrap()
        .insert(key.to_string(), value);
}

fn drop_attempt_outcome_extension(records: &mut [Value], attempt_id: &str, key: &str) {
    let outcome = records
        .iter_mut()
        .find(|record| {
            record["record_type"] == "attempt_outcome"
                && record["attempt_outcome"]["attempt_id"].as_str() == Some(attempt_id)
        })
        .unwrap_or_else(|| {
            panic!("attempt outcome {attempt_id} missing from the generation under mutation")
        });
    remove_member(&mut outcome["attempt_outcome"], key);
}

fn drop_receipt_extension(records: &mut [Value], receipt_id: &str, key: &str) {
    let receipt = records
        .iter_mut()
        .find(|record| {
            record["record_type"] == "provenance_receipt"
                && record["provenance_receipt"]["receipt_id"] == receipt_id
        })
        .unwrap_or_else(|| {
            panic!("receipt {receipt_id} missing from the generation under mutation")
        });
    remove_member(&mut receipt["provenance_receipt"], key);
}

fn panic_payload_message(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else if let Some(message) = payload.downcast_ref::<&'static str>() {
        (*message).to_string()
    } else {
        "non-string panic payload".to_string()
    }
}

/// Run one preservation assertion over a generation whose extension was
/// deliberately dropped or narrowed, and require that mutation -- with the
/// assertion naming its level -- to be what tripped it. The panic hook is
/// muted only across the expected panic; a wrongly-shaped failure still
/// fails the test through the message comparison.
fn assert_mutation_trips_assertion(label: &str, expected_message: &str, check: impl FnOnce()) {
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(check));
    std::panic::set_hook(previous_hook);

    let message = match outcome {
        Err(payload) => panic_payload_message(payload),
        Ok(()) => panic!("mutating the {label} extension must fail the preservation assertion"),
    };
    assert!(
        message.contains(expected_message),
        "dropping the {label} extension must fail with {expected_message:?}, got: {message}"
    );
}

/// The negative half of the contract: for every object level the corpus
/// seeds, remove that level's extension from a published generation and
/// require the comparison to panic naming the loss; for the replayed event,
/// also narrow the widened value back to the older producer's, since that
/// is the erasure a stale replay would actually perform. The pristine
/// generations clear the same assertions just before, so the mutation is
/// the only possible trigger -- a future edit that silently neuters an
/// assertion fails here.
#[test]
fn a_dropped_unknown_field_fails_the_preservation_assertions() {
    let generations = build_generations();

    assert_levels_preserved(&generations.generation_1, &generations, false);
    assert_levels_preserved(&generations.generation_2, &generations, true);
    assert_receipt_extension(
        &generations.generation_2,
        &generations.receipt_with_extensions,
    );
    assert_pointer_extension(&read_pointer(generations.workspace(0)), "pristine");

    let mut mutated = generations.generation_1.clone();
    drop_issue_extension(&mut mutated, &generations.issue_a, "future_priority_signal");
    assert_mutation_trips_assertion("issue", "issue-level unknown fields", || {
        assert_levels_preserved(&mutated, &generations, false)
    });

    let mut mutated = generations.generation_1.clone();
    drop_dependency_extension(
        &mut mutated,
        &generations.issue_a,
        &generations.issue_b,
        "future_weight",
    );
    assert_mutation_trips_assertion("dependency", "dependency-entry unknown fields", || {
        assert_levels_preserved(&mutated, &generations, false)
    });

    let mut mutated = generations.generation_1.clone();
    drop_reference_extension(&mut mutated, &generations.issue_a, "future_visibility");
    assert_mutation_trips_assertion(
        "external-reference",
        "external-reference unknown fields",
        || assert_levels_preserved(&mutated, &generations, false),
    );

    let mut mutated = generations.generation_1.clone();
    drop_data_extension(&mut mutated, &generations.issue_a, "future_etag");
    assert_mutation_trips_assertion(
        "structured-data",
        "structured-data envelope unknown fields",
        || assert_levels_preserved(&mutated, &generations, false),
    );

    let mut mutated = generations.generation_1.clone();
    drop_resource_key_extension(&mut mutated, &generations.issue_a, "future_exclusive_until");
    assert_mutation_trips_assertion("resource-key", "resource-key unknown fields", || {
        assert_levels_preserved(&mutated, &generations, false)
    });

    let mut mutated = generations.generation_1.clone();
    drop_attempt_outcome_extension(&mut mutated, &generations.attempt_id, "future_telemetry");
    assert_mutation_trips_assertion("attempt-outcome", "attempt-outcome unknown fields", || {
        assert_levels_preserved(&mutated, &generations, false)
    });

    // Dropping the seeded map itself leaves no carrier event at all, so the
    // locator trips first.
    let mut mutated = generations.generation_1.clone();
    drop_event_extension(
        &mut mutated,
        &generations.extended_event_identity,
        "future_tier",
    );
    assert_mutation_trips_assertion(
        "event",
        "exactly one event must carry the seeded payload",
        || assert_levels_preserved(&mutated, &generations, false),
    );

    // Dropping only the replay widening keeps the event a carrier, so the
    // drop surfaces where the contract cares: a parse-equal mismatch on the
    // surviving map.
    let mut mutated = generations.generation_2.clone();
    drop_event_extension(
        &mut mutated,
        &generations.extended_event_identity,
        EVENT_WIDEN_KEY,
    );
    assert_mutation_trips_assertion(
        "event",
        "event-level unknown fields must survive parse-equal",
        || assert_levels_preserved(&mutated, &generations, true),
    );

    // The other would-be erasure the replay contract forbids is not a
    // missing key but the widened value narrowed back to the older
    // producer's seeded one. The event still carries the seeded key, so the
    // locator passes and the loss must surface the same way: a parse-equal
    // mismatch on the surviving map.
    let mut mutated = generations.generation_2.clone();
    narrow_event_extension(
        &mut mutated,
        &generations.extended_event_identity,
        "future_tier",
        event_payload()["future_tier"].clone(),
    );
    assert_mutation_trips_assertion(
        "event",
        "event-level unknown fields must survive parse-equal",
        || assert_levels_preserved(&mutated, &generations, true),
    );

    let mut mutated = generations.generation_2.clone();
    drop_receipt_extension(
        &mut mutated,
        &generations.receipt_with_extensions,
        "future_signed_by",
    );
    assert_mutation_trips_assertion("receipt", "receipt-level unknown fields", || {
        assert_receipt_extension(&mutated, &generations.receipt_with_extensions)
    });

    let mut pointer = read_pointer(generations.workspace(0));
    assert_eq!(
        pointer
            .as_object_mut()
            .expect("the generation pointer must be a JSON object")
            .remove(POINTER_EXTENSION_KEY),
        Some(pointer_extension_value()),
        "the pointer extension must be present to be dropped"
    );
    assert_mutation_trips_assertion("pointer", "pointer must re-project", || {
        assert_pointer_extension(&pointer, "deliberately dropped")
    });
}

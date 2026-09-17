//! Unknown-field checkpoint round-trip conformance
//!
//! Issue records may carry JSON keys that are neither part of the native-v1
//! issue schema nor one of the projected collections. The `Issue` model
//! captures those keys through the `#[serde(flatten)]` extensions catch-all,
//! the import stores them in `issue_extensions`, and the export re-projects
//! them into the issue object. These tests pin that preservation contract
//! against the committed fixtures under `tests/fixtures/extensions/`, which
//! carry 15 unknown fields chosen to cover the shapes a preservation
//! contract can get wrong: empty objects/arrays, null, the empty-string
//! key, non-ASCII keys and values, integers vs floats, and deep nesting.
//!
//! Covered here:
//!
//! * structural validation of both fixture layouts (pointer ↔ generation ↔
//!   content addressing), and equality of the payload across the two layouts
//! * restore fidelity: both fixtures into empty workspaces, unknown fields
//!   diffed exactly against the fixture records, known projections proven to
//!   land in their own tables and never in `issue_extensions`
//! * flush fidelity: a restored workspace re-published monolithically and
//!   shardedly, the new generations parsed, unknown fields diffed exactly
//! * the full export×import chain: fixture → restore → monolithic flush →
//!   restore → sharded flush → restore, payload intact at the far end
//! * merge semantics: insert into a fresh workspace, replace when the
//!   incoming content is newer (mutation, addition, and deletion of unknown
//!   fields all land), retain when it is not
//! * the boundary of the contract: `resource_keys` looks like an extension
//!   but is a known projection, so a malformed value is rejected rather
//!   than preserved blindly
//!
//! What counts as "unknown" here is derived from the native field set below.
//! When the native schema grows, the fixtures must be regenerated (see
//! `tests/fixtures/extensions/README.md`); these tests fail loudly until
//! that happens rather than silently narrowing the contract.

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Every key an issue object may carry that is NOT an unknown extension:
/// the serialized fields of `Issue` (src/model.rs) plus the projected
/// collections the checkpoint layer embeds and routes to their own tables.
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

/// The unknown-field payload the fixtures carry. If this number changes, the
/// fixtures were regenerated with a different payload -- update the inventory
/// in tests/fixtures/extensions/README.md alongside.
const EXPECTED_UNKNOWN_FIELD_COUNT: usize = 15;

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/extensions")
}

fn bead(dir: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::cargo_bin("bead").unwrap();
    cmd.args(args).current_dir(dir);
    cmd
}

fn init_workspace(workspace: &Path) {
    bead(
        workspace,
        &["init", "--prefix", "bead", "--skip-foreign-workspace"],
    )
    .assert()
    .success();
}

/// Replace the `checkpoint` section of the workspace config. Publication is
/// pinned to the explicit `sync flush-only` (auto_flush off) so the test
/// observes exactly one publisher, with an optional forced mode.
fn set_checkpoint_config(workspace: &Path, checkpoint: Value) {
    let config_path = workspace.join(".beads/config.json");
    let mut config: Value =
        serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
    config
        .as_object_mut()
        .unwrap()
        .insert("checkpoint".to_string(), checkpoint);
    fs::write(&config_path, serde_json::to_string(&config).unwrap()).unwrap();
}

fn read_jsonl(path: &Path) -> Vec<Value> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn read_pointer(path: impl AsRef<Path>) -> Value {
    serde_json::from_str(&fs::read_to_string(path.as_ref()).unwrap()).unwrap()
}

/// The issue records of the committed monolithic fixture.
fn fixture_issue_records() -> Vec<Value> {
    read_jsonl(&fixture_dir().join("checkpoint.jsonl"))
        .into_iter()
        .filter(|record| record["record_type"] == "issue")
        .collect()
}

/// The unknown fields of each fixture issue, keyed by issue ID: every key
/// the issue object carries beyond the known native fields and projections.
fn expected_extensions(records: &[Value]) -> HashMap<String, serde_json::Map<String, Value>> {
    let mut extensions = HashMap::new();
    for record in records {
        let issue = &record["issue"];
        let mut fields = serde_json::Map::new();
        for (key, value) in issue.as_object().unwrap() {
            if !KNOWN_ISSUE_KEYS.contains(&key.as_str()) {
                fields.insert(key.clone(), value.clone());
            }
        }
        extensions.insert(issue["id"].as_str().unwrap().to_string(), fields);
    }
    extensions
}

/// Restore a checkpoint source (a monolithic generation file or a sharded
/// pointer) into a fresh workspace.
fn restore(source: &Path) -> TempDir {
    let workspace = TempDir::new().unwrap();
    init_workspace(workspace.path());
    bead(
        workspace.path(),
        &[
            "sync",
            "import-only",
            "--input",
            source.to_str().unwrap(),
            "--restore-into-empty",
            "--actor",
            "unknown-field-conformance",
        ],
    )
    .assert()
    .success()
    .stderr(predicate::str::contains("Restored 3 issues"));
    workspace
}

/// The `issue_extensions` rows of a workspace: issue ID -> key -> parsed
/// JSON value.
fn read_extensions(workspace: &Path) -> HashMap<String, serde_json::Map<String, Value>> {
    let db = workspace.join(".beads/beads.db");
    let conn = rusqlite::Connection::open(&db).unwrap();
    let mut stmt = conn
        .prepare("SELECT issue_id, key, value FROM issue_extensions")
        .unwrap();
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .unwrap();
    let mut extensions: HashMap<String, serde_json::Map<String, Value>> = HashMap::new();
    for row in rows {
        let (issue_id, key, value) = row.unwrap();
        let parsed: Value = serde_json::from_str(&value).unwrap();
        extensions.entry(issue_id).or_default().insert(key, parsed);
    }
    extensions
}

/// Assert a workspace's `issue_extensions` rows equal the expected unknown
/// fields exactly: every field present, nothing extra, no known projection
/// leaked in.
fn assert_extensions_match(
    workspace: &Path,
    expected: &HashMap<String, serde_json::Map<String, Value>>,
    label: &str,
) {
    let actual = read_extensions(workspace);
    assert_eq!(
        actual.len(),
        expected.len(),
        "{label}: every fixture issue carries unknown fields"
    );
    for (issue_id, expected_fields) in expected {
        let actual_fields = actual
            .get(issue_id)
            .unwrap_or_else(|| panic!("{label}: issue {issue_id} has no issue_extensions rows"));
        assert_eq!(
            actual_fields, expected_fields,
            "{label}: unknown fields of {issue_id} not preserved"
        );
        let leaked: Vec<_> = actual_fields
            .keys()
            .filter(|key| KNOWN_ISSUE_KEYS.contains(&key.as_str()))
            .collect();
        assert!(
            leaked.is_empty(),
            "{label}: known projections leaked into issue_extensions for \
             {issue_id}: {leaked:?}"
        );
    }
}

/// Assert the projected collections of the fixture landed in their own
/// tables rather than in `issue_extensions`.
fn assert_projections_restored(workspace: &Path) {
    let db = workspace.join(".beads/beads.db");
    let conn = rusqlite::Connection::open(&db).unwrap();

    let mut expected_labels: HashSet<(String, String)> = HashSet::new();
    let mut expected_deps: HashSet<(String, String, String)> = HashSet::new();
    let mut expected_refs: HashSet<(String, String, String, String)> = HashSet::new();
    let mut expected_data: HashSet<(String, String, String, String)> = HashSet::new();
    let mut expected_comments: HashMap<String, usize> = HashMap::new();
    for record in fixture_issue_records() {
        let issue = &record["issue"];
        let id = issue["id"].as_str().unwrap().to_string();
        if let Some(labels) = issue["labels"].as_array() {
            for label in labels {
                expected_labels.insert((id.clone(), label.as_str().unwrap().to_string()));
            }
        }
        if let Some(deps) = issue["dependencies"].as_array() {
            for dep in deps {
                expected_deps.insert((
                    id.clone(),
                    dep["blocker"].as_str().unwrap().to_string(),
                    dep["kind"].as_str().unwrap().to_string(),
                ));
            }
        }
        if let Some(references) = issue["external_references"].as_array() {
            for reference in references {
                expected_refs.insert((
                    id.clone(),
                    reference["namespace"].as_str().unwrap().to_string(),
                    reference["key"].as_str().unwrap().to_string(),
                    reference["value"].as_str().unwrap().to_string(),
                ));
            }
        }
        if let Some(data) = issue["data"].as_object() {
            for (namespace, entry) in data {
                expected_data.insert((
                    id.clone(),
                    namespace.clone(),
                    entry["schema_ref"].as_str().unwrap().to_string(),
                    entry["value"].to_string(),
                ));
            }
        }
        if let Some(comments) = issue["comments"].as_array() {
            // The export projects an empty comments array for issues with no
            // rows; absence and emptiness are the same state in the table.
            if !comments.is_empty() {
                expected_comments.insert(id, comments.len());
            }
        }
    }

    let mut stmt = conn.prepare("SELECT issue_id, label FROM labels").unwrap();
    let labels: HashSet<_> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(|row| row.unwrap())
        .collect();
    assert_eq!(labels, expected_labels, "labels projection");

    let mut stmt = conn
        .prepare("SELECT blocked_issue_id, blocker_issue_id, kind FROM dependencies")
        .unwrap();
    let deps: HashSet<_> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap()
        .map(|row| row.unwrap())
        .collect();
    assert_eq!(deps, expected_deps, "dependencies projection");

    let mut stmt = conn
        .prepare("SELECT issue_id, namespace, key, value FROM external_references")
        .unwrap();
    let references: HashSet<_> = stmt
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .unwrap()
        .map(|row| row.unwrap())
        .collect();
    assert_eq!(references, expected_refs, "external_references projection");

    let mut stmt = conn
        .prepare("SELECT issue_id, namespace, schema_ref, value FROM issue_data")
        .unwrap();
    let data: HashSet<_> = stmt
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .unwrap()
        .map(|row| row.unwrap())
        .collect();
    assert_eq!(data, expected_data, "issue_data projection");

    let mut stmt = conn
        .prepare("SELECT issue_id, count(*) FROM comments GROUP BY issue_id")
        .unwrap();
    let comments: HashMap<_, _> = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as usize))
        })
        .unwrap()
        .map(|row| row.unwrap())
        .collect();
    assert_eq!(comments, expected_comments, "comments projection");
}

/// Parse the issue records out of a workspace's active checkpoint generation
/// (whatever layout the pointer publishes) into issue ID -> full issue
/// object.
fn active_generation_issues(workspace: &Path) -> HashMap<String, Value> {
    let checkpoint_dir = workspace.join(".beads/checkpoint");
    let pointer = read_pointer(checkpoint_dir.join("current.json"));
    let root = checkpoint_dir.join(pointer["active_root"]["path"].as_str().unwrap());

    let records = if pointer["mode"] == "sharded" {
        let manifest = read_pointer(&root);
        let mut records = Vec::new();
        for shard in manifest["issue_shards"].as_array().unwrap() {
            records.extend(read_jsonl(
                &checkpoint_dir.join(shard["path"].as_str().unwrap()),
            ));
        }
        records
    } else {
        read_jsonl(&root)
    };

    records
        .into_iter()
        .filter(|record| record["record_type"] == "issue")
        .map(|record| {
            let id = record["issue"]["id"].as_str().unwrap().to_string();
            (id, record["issue"].clone())
        })
        .collect()
}

/// Assert the active generation of a workspace re-projects the expected
/// unknown fields, and that it was published in the requested layout.
fn assert_flush_reprojects(
    workspace: &Path,
    expected: &HashMap<String, serde_json::Map<String, Value>>,
    mode: &str,
) {
    let pointer = read_pointer(workspace.join(".beads/checkpoint/current.json"));
    assert_eq!(pointer["mode"], mode, "published layout");

    for (issue_id, expected_fields) in expected {
        let issue = active_generation_issues(workspace)
            .remove(issue_id)
            .unwrap_or_else(|| panic!("{mode} generation is missing issue {issue_id}"));
        let mut fields = serde_json::Map::new();
        for (key, value) in issue.as_object().unwrap() {
            if !KNOWN_ISSUE_KEYS.contains(&key.as_str()) {
                fields.insert(key.clone(), value.clone());
            }
        }
        assert_eq!(
            &fields, expected_fields,
            "{mode} generation did not re-project the unknown fields of {issue_id}"
        );
    }
}

/// Copy the committed monolithic fixture into a scratch directory, apply a
/// mutation to its records, and re-address the pointer. The result is a
/// valid checkpoint a merge or restore will accept.
fn staged_variant(mutate: impl Fn(&mut Value)) -> (TempDir, PathBuf) {
    let dir = TempDir::new().unwrap();
    let mut records = read_jsonl(&fixture_dir().join("checkpoint.jsonl"));
    for record in &mut records {
        mutate(record);
    }
    let body = records
        .iter()
        .map(|record| serde_json::to_string(record).unwrap())
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    fs::write(dir.path().join("checkpoint.jsonl"), &body).unwrap();

    let mut pointer = read_pointer(fixture_dir().join("current.json"));
    pointer["active_root"]["sha256"] = json!(sha256_hex(body.as_bytes()));
    fs::write(
        dir.path().join("current.json"),
        serde_json::to_string_pretty(&pointer).unwrap(),
    )
    .unwrap();
    let source = dir.path().join("checkpoint.jsonl");
    (dir, source)
}

// ---- structural validation -------------------------------------------------

#[test]
fn monolithic_fixture_pointer_reconciles_with_its_generation() {
    let pointer = read_pointer(fixture_dir().join("current.json"));
    assert_eq!(pointer["mode"], "monolithic");

    let generation = fixture_dir().join(pointer["active_root"]["path"].as_str().unwrap());
    let body = fs::read(&generation).unwrap();
    assert_eq!(
        sha256_hex(&body),
        pointer["active_root"]["sha256"],
        "active_root must hash the exact generation bytes"
    );

    let records = read_jsonl(&generation);
    let issues = records
        .iter()
        .filter(|record| record["record_type"] == "issue")
        .count();
    let events = records
        .iter()
        .filter(|record| record["record_type"] == "event")
        .count();
    assert_eq!(issues as u64, pointer["issue_count"].as_u64().unwrap());
    assert_eq!(events as u64, pointer["event_count"].as_u64().unwrap());
    assert_eq!(
        records.len() as u64,
        pointer["total_record_count"].as_u64().unwrap()
    );
}

#[test]
fn sharded_fixture_is_fully_content_addressed() {
    let pointer = read_pointer(fixture_dir().join("sharded/current.json"));
    assert_eq!(pointer["mode"], "sharded");

    let manifest_path = fixture_dir()
        .join("sharded")
        .join(pointer["active_root"]["path"].as_str().unwrap());
    let manifest_bytes = fs::read(&manifest_path).unwrap();
    assert_eq!(
        sha256_hex(&manifest_bytes),
        pointer["active_root"]["sha256"],
        "the manifest is content-addressed by its own hash"
    );
    assert_eq!(
        manifest_path.file_stem().unwrap().to_str().unwrap(),
        pointer["active_root"]["sha256"],
        "the manifest file name is its hash"
    );

    let manifest: Value = serde_json::from_slice(&manifest_bytes).unwrap();
    assert_eq!(manifest["format"], "checkpoint-set-v1");
    for (count_key, shard_key) in [
        ("issue_count", "issue_shards"),
        ("event_count", "event_shards"),
        ("receipt_count", "receipt_shards"),
        ("attempt_outcome_count", "attempt_outcome_shards"),
        ("redaction_record_count", "redaction_shards"),
    ] {
        assert_eq!(
            manifest[count_key], pointer[count_key],
            "{count_key} agrees"
        );
        let mut records = 0;
        for shard in manifest[shard_key].as_array().unwrap() {
            let object_path = fixture_dir()
                .join("sharded")
                .join(shard["path"].as_str().unwrap());
            let bytes = fs::read(&object_path).unwrap();
            assert_eq!(
                sha256_hex(&bytes),
                shard["sha256"],
                "{} content addressing",
                shard["path"]
            );
            assert_eq!(
                object_path.file_stem().unwrap().to_str().unwrap(),
                shard["sha256"],
                "{} file name is its hash",
                shard["path"]
            );
            assert_eq!(
                bytes.len() as u64,
                shard["byte_length"].as_u64().unwrap(),
                "{} byte length",
                shard["path"]
            );
            let lines = read_jsonl(&object_path).len() as u64;
            assert_eq!(
                lines,
                shard["record_count"].as_u64().unwrap(),
                "{} record count",
                shard["path"]
            );
            records += lines;
        }
        assert_eq!(
            records,
            manifest[count_key].as_u64().unwrap(),
            "{count_key} reconciles with its shards"
        );
    }
}

#[test]
fn both_fixture_layouts_carry_the_same_unknown_field_payload() {
    let monolithic = expected_extensions(&fixture_issue_records());
    let sharded_records: Vec<Value> = {
        let mut records = Vec::new();
        let manifest = read_pointer(
            fixture_dir().join("sharded").join(
                read_pointer(fixture_dir().join("sharded/current.json"))["active_root"]["path"]
                    .as_str()
                    .unwrap(),
            ),
        );
        for shard in manifest["issue_shards"].as_array().unwrap() {
            records.extend(read_jsonl(
                &fixture_dir()
                    .join("sharded")
                    .join(shard["path"].as_str().unwrap()),
            ));
        }
        records
    };
    let sharded = expected_extensions(&sharded_records);
    assert_eq!(monolithic, sharded);

    let total: usize = monolithic.values().map(serde_json::Map::len).sum();
    assert_eq!(
        total, EXPECTED_UNKNOWN_FIELD_COUNT,
        "the payload inventory drifted from the documented 15 fields"
    );
}

// ---- restore fidelity ------------------------------------------------------

#[test]
fn monolithic_fixture_restores_unknown_fields_into_an_empty_workspace() {
    let workspace = restore(&fixture_dir().join("checkpoint.jsonl"));
    let expected = expected_extensions(&fixture_issue_records());
    assert_extensions_match(workspace.path(), &expected, "monolithic restore");
    assert_projections_restored(workspace.path());
}

#[test]
fn sharded_fixture_restores_unknown_fields_into_an_empty_workspace() {
    let workspace = restore(&fixture_dir().join("sharded/current.json"));
    let expected = expected_extensions(&fixture_issue_records());
    assert_extensions_match(workspace.path(), &expected, "sharded restore");
    assert_projections_restored(workspace.path());
}

/// The ID of the fixture's open issue: the safe target for the probing
/// mutation that makes a restored workspace dirty.
fn open_issue_id() -> String {
    fixture_issue_records()
        .iter()
        .find(|record| record["issue"]["base_status"] == "open")
        .map(|record| record["issue"]["id"].as_str().unwrap().to_string())
        .unwrap()
}

/// Restore leaves the restored generation materialized, so an immediate
/// flush is an idempotent no-op. A real CLI mutation makes the live store
/// genuinely dirty, and the next flush a genuine export.
fn make_dirty(workspace: &Path, marker: &str) {
    bead(
        workspace,
        &[
            "update",
            &open_issue_id(),
            "--notes",
            &format!("dirty for flush: {marker}"),
        ],
    )
    .assert()
    .success();
}

// ---- flush fidelity --------------------------------------------------------

#[test]
fn monolithic_flush_reprojects_unknown_fields() {
    let workspace = restore(&fixture_dir().join("checkpoint.jsonl"));
    set_checkpoint_config(workspace.path(), json!({ "auto_flush": false }));
    make_dirty(workspace.path(), "monolithic");

    bead(workspace.path(), &["sync", "flush-only"])
        .assert()
        .success()
        .stderr(predicate::str::contains("Flushed forensic checkpoint:"));

    let expected = expected_extensions(&fixture_issue_records());
    assert_flush_reprojects(workspace.path(), &expected, "monolithic");
}

#[test]
fn sharded_flush_reprojects_unknown_fields() {
    let workspace = restore(&fixture_dir().join("checkpoint.jsonl"));
    set_checkpoint_config(
        workspace.path(),
        json!({ "auto_flush": false, "mode": "sharded" }),
    );
    make_dirty(workspace.path(), "sharded");

    bead(workspace.path(), &["sync", "flush-only"])
        .assert()
        .success()
        .stderr(predicate::str::contains("Flushed forensic checkpoint:"));

    let expected = expected_extensions(&fixture_issue_records());
    assert_flush_reprojects(workspace.path(), &expected, "sharded");
}

// ---- the full export x import chain ---------------------------------------

#[test]
fn unknown_fields_survive_alternating_export_and_import_generations() {
    let expected = expected_extensions(&fixture_issue_records());

    // Generation 1: restore the fixture, republish monolithically.
    let first = restore(&fixture_dir().join("checkpoint.jsonl"));
    set_checkpoint_config(first.path(), json!({ "auto_flush": false }));
    bead(first.path(), &["sync", "flush-only"])
        .assert()
        .success();

    // Generation 2: restore that monolithic generation, republish shardedly.
    let pointer = read_pointer(first.path().join(".beads/checkpoint/current.json"));
    let generation_two_source = first
        .path()
        .join(".beads/checkpoint")
        .join(pointer["active_root"]["path"].as_str().unwrap());
    let second = restore(&generation_two_source);
    set_checkpoint_config(
        second.path(),
        json!({ "auto_flush": false, "mode": "sharded" }),
    );
    bead(second.path(), &["sync", "flush-only"])
        .assert()
        .success();

    // Generation 3: restore that sharded set. The payload must be intact
    // after every hop.
    let pointer = read_pointer(second.path().join(".beads/checkpoint/current.json"));
    let generation_three_source = second
        .path()
        .join(".beads/checkpoint")
        .join(pointer["active_root"]["path"].as_str().unwrap());
    let third = restore(&generation_three_source);
    assert_extensions_match(third.path(), &expected, "chained generation 3");
    assert_projections_restored(third.path());
}

// ---- merge semantics -------------------------------------------------------

#[test]
fn merge_inserts_unknown_fields_into_a_fresh_workspace() {
    let workspace = TempDir::new().unwrap();
    init_workspace(workspace.path());

    bead(
        workspace.path(),
        &[
            "sync",
            "import-only",
            "--input",
            fixture_dir().join("checkpoint.jsonl").to_str().unwrap(),
            "--merge",
            "--actor",
            "unknown-field-conformance",
        ],
    )
    .assert()
    .success()
    .stderr(predicate::str::contains(
        "3 inserted, 0 updated, 0 retained",
    ));

    let expected = expected_extensions(&fixture_issue_records());
    assert_extensions_match(workspace.path(), &expected, "merge insert");
}

#[test]
fn merge_replaces_unknown_fields_when_incoming_content_is_newer() {
    let workspace = restore(&fixture_dir().join("checkpoint.jsonl"));

    // Newer content for issue A: its unknown fields mutate, gain a key, and
    // lose a key. The other issues stay untouched by timestamp.
    let (_variant_dir, variant_source) = staged_variant(|record| {
        let issue = match record.get_mut("issue") {
            Some(issue)
                if issue["title"]
                    .as_str()
                    .unwrap_or_default()
                    .starts_with("Unknown-field fixture A") =>
            {
                issue
            }
            _ => return,
        };
        issue["updated_at"] = json!("2027-01-01T00:00:00.000000000Z");
        let object = issue.as_object_mut().unwrap();
        object.remove("weighting");
        object.insert("ratio".to_string(), json!(0.25));
        object.insert("added_in_transit".to_string(), json!({ "arrived": true }));
    });

    bead(
        workspace.path(),
        &[
            "sync",
            "import-only",
            "--input",
            variant_source.to_str().unwrap(),
            "--merge",
            "--actor",
            "unknown-field-conformance",
        ],
    )
    .assert()
    .success()
    .stderr(predicate::str::contains(
        "0 inserted, 1 updated, 2 retained",
    ));

    let mut expected = expected_extensions(&fixture_issue_records());
    let issue_a = expected
        .values_mut()
        .find(|fields| fields.get("x-obsidian-fidelity").is_some())
        .unwrap();
    issue_a.remove("weighting");
    issue_a.insert("ratio".to_string(), json!(0.25));
    issue_a.insert("added_in_transit".to_string(), json!({ "arrived": true }));

    assert_extensions_match(workspace.path(), &expected, "merge replace");
}

#[test]
fn merge_retains_unknown_fields_when_incoming_content_is_not_newer() {
    let workspace = restore(&fixture_dir().join("checkpoint.jsonl"));

    bead(
        workspace.path(),
        &[
            "sync",
            "import-only",
            "--input",
            fixture_dir().join("checkpoint.jsonl").to_str().unwrap(),
            "--merge",
            "--actor",
            "unknown-field-conformance",
        ],
    )
    .assert()
    .success()
    .stderr(predicate::str::contains(
        "0 inserted, 0 updated, 3 retained",
    ));

    let expected = expected_extensions(&fixture_issue_records());
    assert_extensions_match(workspace.path(), &expected, "merge retain");
}

// ---- the boundary of the contract ------------------------------------------

#[test]
fn malformed_resource_keys_projection_is_rejected_not_preserved() {
    // `resource_keys` rides in the issue object like an extension, but it is
    // a known projection: a malformed value must fail the import rather than
    // be preserved blindly the way an unknown field would.
    let (_variant_dir, variant_source) = staged_variant(|record| {
        let Some(issue) = record.get_mut("issue") else {
            return;
        };
        if issue["title"]
            .as_str()
            .unwrap_or_default()
            .starts_with("Unknown-field fixture A")
        {
            issue["resource_keys"] = json!({ "not": "an array" });
        }
    });

    let workspace = TempDir::new().unwrap();
    init_workspace(workspace.path());
    bead(
        workspace.path(),
        &[
            "sync",
            "import-only",
            "--input",
            variant_source.to_str().unwrap(),
            "--restore-into-empty",
            "--actor",
            "unknown-field-conformance",
        ],
    )
    .assert()
    .failure()
    .stderr(predicate::str::contains("resource_keys"));
}

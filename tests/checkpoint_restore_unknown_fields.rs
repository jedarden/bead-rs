//! Unknown-field conformance for the R036 verified restore command
//!
//! `bead restore` selects one named, verified checkpoint generation,
//! verifies the complete content-addressed closure, and rebuilds the target
//! store from the verified bytes. That is the last native surface the
//! unknown-field payload had not crossed:
//! `checkpoint_unknown_field_round_trip.rs` pins the forensic generation
//! chain (restore-by-import, flush re-projection, merge semantics) and
//! `unknown_field_side_export.rs` pins the bare issue-only side export, but
//! neither drives the explicit `bead restore` recovery command, whose
//! verification-and-rebuild path never parses records through the merge or
//! bare-record readers.
//!
//! These tests run the same committed fixtures under
//! `tests/fixtures/extensions/` through `bead restore` and diff the
//! restored `issue_extensions` rows against the fixture records. The
//! payload covers the shapes a preservation contract can get wrong —
//! deep nesting (two- and three-level objects, heterogeneous arrays),
//! JSON null, the empty-string key, non-ASCII keys, empty containers, and
//! exact scalar types — and every key is by construction a future-version
//! field: a key no current schema version defines, of the kind a newer
//! writer will one day emit into a checkpoint this version must carry
//! forward losslessly.
//!
//! * restore of the monolithic generation lands all 15 unknown fields of
//!   all 3 issues exactly, with the report pinning generation, root hash,
//!   and counts
//! * restore of the sharded generation (a different on-disk layout and a
//!   different code path through the manifest) preserves the same payload
//! * the `--allow-non-empty` override atomically replaces prior semantic
//!   state and lands the fixture payload, so recovery over an existing
//!   store neither merges nor drops the unknown fields
//! * the boundary of the surface: a generation object at a bare top-level
//!   path is refused as an unverified source — even for a payload-correct
//!   checkpoint-set — and nothing is restored

use assert_cmd::Command;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
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

fn bead(dir: &Path) -> Command {
    let mut command = Command::cargo_bin("bead").unwrap();
    command.current_dir(dir);
    command.arg("--skip-foreign-workspace");
    command
}

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/extensions")
}

fn read_jsonl(path: &Path) -> Vec<Value> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn read_pointer(path: impl AsRef<Path>) -> Value {
    serde_json::from_str(&fs::read_to_string(path.as_ref()).unwrap()).unwrap()
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// The committed monolithic fixture restaged into the layout the restore
/// verifier requires: the generation object content-addressed under
/// `objects/`, with the pointer's `active_root` repointed at it. The
/// committed fixture keeps its root at a bare top-level `checkpoint.jsonl`
/// for fixture-layout stability — a shape `sync import-only` accepts but
/// `bead restore` refuses (pinned by the refusal test below) — so the
/// monolithic restore test stages the same records the way a current
/// monolithic publication writes them.
fn staged_monolithic_set() -> TempDir {
    let dir = TempDir::new().unwrap();
    let records = read_jsonl(&fixture_dir().join("checkpoint.jsonl"));
    let body = records
        .iter()
        .map(|record| serde_json::to_string(record).unwrap())
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    let sha = sha256_hex(body.as_bytes());
    fs::create_dir(dir.path().join("objects")).unwrap();
    fs::write(
        dir.path().join("objects").join(format!("{sha}.jsonl")),
        &body,
    )
    .unwrap();

    let mut pointer = read_pointer(fixture_dir().join("current.json"));
    pointer["active_root"]["path"] = json!(format!("objects/{sha}.jsonl"));
    pointer["active_root"]["sha256"] = json!(sha);
    fs::write(
        dir.path().join("current.json"),
        serde_json::to_string_pretty(&pointer).unwrap(),
    )
    .unwrap();
    dir
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

/// Run `bead restore` of the generation named by `source`'s current.json
/// into `workspace` and return the parsed JSON report.
fn restore_into(workspace: &Path, source: &Path, extra: &[&str]) -> Value {
    let pointer = read_pointer(source.join("current.json"));
    let generation = pointer["generation_id"].as_str().unwrap();
    let output = bead(workspace)
        .args([
            "restore",
            "--source",
            source.to_str().unwrap(),
            "--generation",
            generation,
            "--actor",
            "unknown-field-restore",
            "--format",
            "json",
        ])
        .args(extra)
        .assert()
        .success()
        .get_output()
        .clone();
    serde_json::from_slice(&output.stdout).unwrap()
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
/// fields exactly: every field present, nothing extra, exact JSON types.
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
    }
}

#[test]
fn verified_restore_of_the_monolithic_generation_preserves_unknown_fields() {
    let source = staged_monolithic_set();
    let workspace = TempDir::new().unwrap();
    let report = restore_into(workspace.path(), source.path(), &[]);
    let pointer = read_pointer(source.path().join("current.json"));

    assert_eq!(report["generation_id"], pointer["generation_id"]);
    assert_eq!(
        report["source_root_sha256"],
        pointer["active_root"]["sha256"]
    );
    assert_eq!(report["actor"], "unknown-field-restore");
    assert_eq!(report["issues_restored"], pointer["issue_count"]);
    assert_eq!(report["events_restored"], pointer["event_count"]);
    assert_eq!(report["provenance_receipts_restored"], 0);
    assert_eq!(report["non_empty_override"], false);
    assert!(report["restore_receipt_id"]
        .as_str()
        .unwrap()
        .starts_with("restore-"));

    let expected = expected_extensions(&fixture_issue_records());
    assert_extensions_match(workspace.path(), &expected, "monolithic verified restore");
}

#[test]
fn verified_restore_of_the_sharded_generation_preserves_unknown_fields() {
    let source = fixture_dir().join("sharded");
    let workspace = TempDir::new().unwrap();
    let report = restore_into(workspace.path(), &source, &[]);
    let pointer = read_pointer(source.join("current.json"));

    assert_eq!(report["generation_id"], pointer["generation_id"]);
    assert_eq!(
        report["source_root_sha256"],
        pointer["active_root"]["sha256"]
    );
    assert_eq!(report["issues_restored"], pointer["issue_count"]);
    assert_eq!(report["events_restored"], pointer["event_count"]);
    // The sharded fixture was republished by generate.sh, whose operation
    // rode along as a provenance receipt the restore must also carry.
    assert_eq!(
        report["provenance_receipts_restored"],
        pointer["receipt_count"]
    );

    let expected = expected_extensions(&fixture_issue_records());
    assert_extensions_match(workspace.path(), &expected, "sharded verified restore");
}

#[test]
fn allow_non_empty_override_replaces_semantic_state_and_lands_unknown_fields() {
    let target = TempDir::new().unwrap();
    bead(target.path())
        .args(["init", "--prefix", "bead"])
        .assert()
        .success();
    let straggler = String::from_utf8(
        bead(target.path())
            .args(["create", "--title", "pre-restore straggler"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone(),
    )
    .unwrap()
    .trim()
    .to_string();
    assert!(straggler.starts_with("bead-"), "create prints the issue ID");

    let source = staged_monolithic_set();
    let report = restore_into(target.path(), source.path(), &["--allow-non-empty"]);
    let pointer = read_pointer(source.path().join("current.json"));

    assert_eq!(report["non_empty_override"], true);
    assert_eq!(report["issues_restored"], pointer["issue_count"]);

    let expected = expected_extensions(&fixture_issue_records());
    assert_extensions_match(target.path(), &expected, "override verified restore");

    // The override replaced semantic state wholesale: only the fixture's
    // issues remain, and the pre-restore straggler is gone.
    let conn = rusqlite::Connection::open(target.path().join(".beads/beads.db")).unwrap();
    let total: i64 = conn
        .query_row("SELECT COUNT(*) FROM issues", [], |row| row.get(0))
        .unwrap();
    assert_eq!(total, 3, "override leaves exactly the fixture issues");
    let stragglers: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM issues WHERE id = ?1",
            [&straggler],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(stragglers, 0, "pre-restore state must not survive");
}

#[test]
fn restore_refuses_a_bare_top_level_generation_object() {
    let source = fixture_dir();
    let pointer = read_pointer(source.join("current.json"));
    let workspace = TempDir::new().unwrap();
    bead(workspace.path())
        .args([
            "restore",
            "--source",
            source.to_str().unwrap(),
            "--generation",
            pointer["generation_id"].as_str().unwrap(),
            "--actor",
            "unknown-field-restore",
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "Unverified restore source: invalid checkpoint-relative path 'checkpoint.jsonl'",
        ));
    // Source verification precedes target initialization: the refusal must
    // not have left a half-restored store behind.
    assert!(
        !workspace.path().join(".beads/beads.db").exists(),
        "a refused restore must not initialize the target"
    );
}

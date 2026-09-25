//! Unknown-field conformance for the native issue-only side export
//!
//! `sync flush-only --output PATH` writes the other native export format:
//! bare `Issue` objects, one JSON line per issue, re-projected straight from
//! the live store. Unlike the forensic generation writer, whose issue
//! records are canonicalized through a `serde_json::to_value` hop (sorted
//! keys), the side exporter serializes the `Issue` struct directly, so the
//! `#[serde(flatten)]` extensions catch-all is what carries the unknown
//! fields onto the wire. `sync import-only` reads that shape back through
//! its backward-compatible bare-record path.
//!
//! These tests pin the unknown-field contract across that pair against the
//! committed fixtures under `tests/fixtures/extensions/`, complementing
//! `checkpoint_unknown_field_round_trip.rs`, which pins the forensic
//! generation chain over the same payload:
//!
//! * the side export re-projects every unknown field at the top level of
//!   the issue object -- flattened, never under an "extensions" wrapper --
//!   with nested values, JSON null, the empty-string key, non-ASCII keys,
//!   and exact JSON scalar types intact
//! * repeated serialization agrees on content: exports of the same payload
//!   from separate processes parse to equal issue objects (key ORDER across
//!   those serializations is deliberately not pinned here -- the extensions
//!   map is a `HashMap` and the side exporter does not canonicalize key
//!   order; the generation path is where byte-identical republication is
//!   contracted)
//! * the bare-record import preserves unknown fields exactly into
//!   `issue_extensions`, while the projections that ride the flattened map
//!   (`comments`, `external_references`) are routed to their own tables and
//!   never stored as if they were unknown -- recognized, not interpreted
//! * serializing the round-tripped store again reproduces the original
//!   export's issue objects exactly: the chain export -> import -> export
//!   neither drifts nor reinterprets the payload

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Every key an issue object may carry that is NOT an unknown extension:
/// the serialized fields of `Issue` (src/model.rs) plus the projections the
/// read path embeds into the flattened map and the import routes to their
/// own tables.
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

fn read_jsonl(path: &Path) -> Vec<Value> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
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

/// Restore a checkpoint source into a fresh workspace.
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
            "unknown-field-side-export",
        ],
    )
    .assert()
    .success()
    .stderr(predicate::str::contains("Restored 3 issues"));
    workspace
}

/// Write the issue-only side export of a workspace to `output`, which must
/// not already exist.
fn side_export(workspace: &Path, output: &Path) {
    bead(
        workspace,
        &["sync", "flush-only", "--output", output.to_str().unwrap()],
    )
    .assert()
    .success()
    .stderr(predicate::str::contains("Exported issue-only checkpoint:"));
}

/// Parse an issue-only side export into issue ID -> full issue object.
fn exported_issues(path: &Path) -> HashMap<String, Value> {
    read_jsonl(path)
        .into_iter()
        .map(|issue| {
            let id = issue["id"].as_str().unwrap().to_string();
            (id, issue)
        })
        .collect()
}

/// The unknown fields of one exported issue object.
fn unknown_fields_of(issue: &Value) -> serde_json::Map<String, Value> {
    issue
        .as_object()
        .unwrap()
        .iter()
        .filter(|(key, _)| !KNOWN_ISSUE_KEYS.contains(&key.as_str()))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

/// The exported issue whose title carries `marker` (the fixtures are named
/// "Unknown-field fixture A/B/C: ...").
fn exported_issue_by_marker<'a>(exported: &'a HashMap<String, Value>, marker: &str) -> &'a Value {
    exported
        .values()
        .find(|issue| issue["title"].as_str().unwrap_or_default().contains(marker))
        .unwrap_or_else(|| panic!("no exported issue titled like {marker:?}"))
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

/// Export a freshly restored fixture workspace and return the workspace, the
/// export file, and the parsed export.
fn restored_and_exported() -> (TempDir, PathBuf, HashMap<String, Value>) {
    let workspace = restore(&fixture_dir().join("checkpoint.jsonl"));
    let output = workspace.path().join("side-export.jsonl");
    side_export(workspace.path(), &output);
    let exported = exported_issues(&output);
    (workspace, output, exported)
}

/// Import a bare issue-only JSONL file into a fresh workspace with `--merge`.
fn merge_import(export_path: &Path) -> TempDir {
    let workspace = TempDir::new().unwrap();
    init_workspace(workspace.path());
    bead(
        workspace.path(),
        &[
            "sync",
            "import-only",
            "--input",
            export_path.to_str().unwrap(),
            "--merge",
            "--actor",
            "unknown-field-side-export",
        ],
    )
    .assert()
    .success()
    .stderr(predicate::str::contains("3 inserted"));
    workspace
}

// ---- the export's re-projection ---------------------------------------------

#[test]
fn side_export_reprojects_unknown_fields_at_the_top_level() {
    let (_workspace, _export_path, exported) = restored_and_exported();

    for (issue_id, expected_fields) in expected_extensions(&fixture_issue_records()) {
        let issue = exported
            .get(&issue_id)
            .unwrap_or_else(|| panic!("side export is missing issue {issue_id}"));

        // Flattened, not wrapped: the unknown fields sit on the issue object
        // itself, exactly where the bare-record import expects to find them.
        assert!(
            !issue.as_object().unwrap().contains_key("extensions"),
            "{issue_id}: unknown fields must be re-projected flat, not nested \
             under an extensions wrapper"
        );
        assert_eq!(
            &unknown_fields_of(issue),
            &expected_fields,
            "{issue_id}: the side export did not preserve the unknown fields"
        );
    }
}

#[test]
fn side_export_preserves_awkward_value_shapes_and_scalar_types() {
    let (_workspace, _export_path, exported) = restored_and_exported();

    // Issue B carries the shapes a serializer is tempted to interpret:
    // an empty-string key, an empty object, an empty array, JSON null, a
    // heterogeneous array, and two-level nesting.
    let awkward = exported_issue_by_marker(&exported, "fixture B");
    assert_eq!(
        awkward[""],
        json!("empty-string key survives round trips"),
        "the empty-string key must survive as a first-class field"
    );
    assert_eq!(awkward["hollow"], json!({}), "empty object");
    assert_eq!(awkward["void"], json!([]), "empty array");
    assert_eq!(awkward["ghost"], json!(null), "JSON null must stay null");
    assert_eq!(
        awkward["cluster"],
        json!([1, "two", [3], { "four": 4 }]),
        "heterogeneous array preserved element-for-element"
    );
    assert_eq!(
        awkward["trace_context"],
        json!({
            "spans": [{ "depth": 2, "id": "s1" }],
            "traceparent": "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
        }),
        "nested object with an array of objects preserved"
    );

    // Scalar types are part of the payload: an integer must not arrive as a
    // float, a float must not be mangled, zero and negatives must survive.
    let pinned = exported_issue_by_marker(&exported, "fixture C")["pinned_numbers"]
        .as_array()
        .unwrap();
    assert!(
        pinned[0].is_i64() && pinned[0] == 0,
        "zero stays an integer"
    );
    assert!(
        pinned[1].is_i64() && pinned[1] == -1,
        "negative stays an integer"
    );
    assert!(
        pinned[2].is_f64() && pinned[2] == 2.5,
        "float stays a float"
    );
    assert!(
        pinned[3].is_f64() && pinned[3] == -0.5,
        "negative float stays a float"
    );

    let open_issue = exported_issue_by_marker(&exported, "fixture A");
    assert!(
        open_issue["weighting"].is_i64(),
        "a plain integer must not be re-serialized as a float"
    );
    assert_eq!(open_issue["ratio"], json!(0.125), "exact float preserved");
    assert_eq!(
        exported_issue_by_marker(&exported, "fixture C")["deeply"],
        json!({ "nested": { "structure": { "leaf": [true, false, null] } } }),
        "three-level nesting ending in a mixed leaf array"
    );
    assert_eq!(
        exported_issue_by_marker(&exported, "fixture C")["ünkoded_key"],
        json!("vàlue-é"),
        "non-ASCII key and value preserved verbatim"
    );
}

// ---- repeated serialization -------------------------------------------------

/// The same payload serialized twice -- by two separate processes, so two
/// independent `HashMap` seeds behind the flattened extensions -- must parse
/// to equal issue objects. Key order inside the serialized objects is not
/// pinned (the side exporter does not canonicalize it); the CONTENT must be
/// identical, with no field gained, lost, or reinterpreted between runs.
#[test]
fn repeated_side_exports_of_one_payload_agree_on_content() {
    let (_first, _first_path, first_export) = restored_and_exported();
    let (_second, _second_path, second_export) = restored_and_exported();

    assert_eq!(
        first_export, second_export,
        "two side exports of the same restored payload disagree on content"
    );
}

// ---- the bare-record import round trip --------------------------------------

#[test]
fn bare_record_import_preserves_unknown_fields_and_routes_projections() {
    let (_source_workspace, export_path, exported) = restored_and_exported();
    let workspace = merge_import(&export_path);

    // Every unknown field landed in issue_extensions, byte-for-byte the
    // value the export carried.
    let expected = expected_extensions(&fixture_issue_records());
    let actual = read_extensions(workspace.path());
    assert_eq!(
        actual.len(),
        expected.len(),
        "every imported issue carries unknown fields"
    );
    for (issue_id, expected_fields) in &expected {
        assert_eq!(
            actual.get(issue_id).unwrap_or_else(|| panic!(
                "issue {issue_id} has no issue_extensions rows after the bare-record import"
            )),
            expected_fields,
            "{issue_id}: unknown fields not preserved through the bare-record import"
        );
    }

    // The projections that rode the flattened map were recognized as
    // projections, not interpreted as unknown fields: they live in their own
    // tables and never in issue_extensions.
    let conn = rusqlite::Connection::open(workspace.path().join(".beads/beads.db")).unwrap();
    let mut expected_comments: HashSet<(String, String, String, String)> = HashSet::new();
    let mut expected_refs: HashSet<(String, String, String, String)> = HashSet::new();
    for issue in exported.values() {
        let id = issue["id"].as_str().unwrap();
        for comment in issue["comments"].as_array().unwrap_or(&vec![]) {
            expected_comments.insert((
                id.to_string(),
                comment["id"].as_str().unwrap().to_string(),
                comment["author"].as_str().unwrap().to_string(),
                comment["body"].as_str().unwrap().to_string(),
            ));
        }
        for reference in issue["external_references"].as_array().unwrap_or(&vec![]) {
            expected_refs.insert((
                id.to_string(),
                reference["namespace"].as_str().unwrap().to_string(),
                reference["key"].as_str().unwrap().to_string(),
                reference["value"].as_str().unwrap().to_string(),
            ));
        }
    }

    let mut stmt = conn
        .prepare("SELECT issue_id, id, author, body FROM comments")
        .unwrap();
    let comments: HashSet<_> = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .unwrap()
        .map(|row| row.unwrap())
        .collect();
    assert_eq!(comments, expected_comments, "comments projection");

    let mut stmt = conn
        .prepare("SELECT issue_id, namespace, key, value FROM external_references")
        .unwrap();
    let references: HashSet<_> = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .unwrap()
        .map(|row| row.unwrap())
        .collect();
    assert_eq!(references, expected_refs, "external_references projection");

    for (issue_id, fields) in &actual {
        let leaked: Vec<_> = fields
            .keys()
            .filter(|key| matches!(key.as_str(), "comments" | "external_references"))
            .collect();
        assert!(
            leaked.is_empty(),
            "{issue_id}: projections leaked into issue_extensions as if they \
             were unknown fields: {leaked:?}"
        );
    }
}

// ---- repeated serialization across the full chain ---------------------------

/// Serializing the round-tripped store again must reproduce the original
/// export's issue objects exactly. This is the repeated-serialization half
/// of the contract: export -> import -> export may not drift, drop, or
/// reinterpret anything, including the nested values and the projections
/// the side exporter embeds.
#[test]
fn reexporting_a_round_tripped_store_reproduces_the_original_export() {
    let (_source_workspace, export_path, first_export) = restored_and_exported();
    let workspace = merge_import(&export_path);

    let reexport_path = workspace.path().join("reexport.jsonl");
    side_export(workspace.path(), &reexport_path);
    assert_eq!(
        first_export,
        exported_issues(&reexport_path),
        "export -> import -> export changed the payload"
    );
}

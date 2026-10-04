//! Restore conformance for the reusable unknown-field checkpoint corpus.
//!
//! The fixture is deliberately data-driven: `expected.json` names each
//! sentinel and its selector, so adding another checkpoint object level does
//! not hide coverage in a hand-written assertion. The test validates the
//! authored checkpoint, restores it through the native empty-store path, and
//! checks the newly published generation. Generated IDs, timestamps, and
//! hashes are native publication details and are intentionally not compared.

use assert_cmd::Command;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

const ISSUE_KEYS: &[&str] = &[
    "$schema",
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
    "labels",
    "comments",
    "dependencies",
    "external_references",
    "data",
    "resource_keys",
];
const EVENT_KEYS: &[&str] = &[
    "$schema",
    "origin_store_uuid",
    "origin_event_sequence",
    "issue_id",
    "kind",
    "actor",
    "time",
    "detail",
];
const DEPENDENCY_KEYS: &[&str] = &["blocker", "kind"];
const REFERENCE_KEYS: &[&str] = &["namespace", "key", "value", "unique_ref"];
const DATA_KEYS: &[&str] = &["schema_ref", "value"];
const RESOURCE_KEY_KEYS: &[&str] = &["resource_key"];
const RECEIPT_KEYS: &[&str] = &[
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
const POINTER_KEYS: &[&str] = &[
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
];

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/checkpoint-unknown-fields-v1")
}

fn read_json(path: &Path) -> Value {
    serde_json::from_str(
        &fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display())),
    )
    .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn read_records(path: &Path) -> Vec<Value> {
    fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn fixture_pointer() -> Value {
    read_json(&fixture_dir().join("current.json"))
}

fn fixture_records() -> Vec<Value> {
    read_records(&fixture_dir().join("objects/unknown-fields.jsonl"))
}

fn object<'a>(value: &'a Value, context: &str) -> &'a Map<String, Value> {
    value
        .as_object()
        .unwrap_or_else(|| panic!("{context}: expected a JSON object"))
}

fn issue<'a>(records: &'a [Value], id: &str) -> &'a Value {
    records
        .iter()
        .filter(|record| record["record_type"] == "issue")
        .map(|record| &record["issue"])
        .find(|candidate| candidate["id"] == id)
        .unwrap_or_else(|| panic!("issue {id} is missing"))
}

fn selected_record<'a>(records: &'a [Value], selector: &Value) -> &'a Value {
    let selector = object(selector, "selector");
    match selector["record_type"].as_str().unwrap() {
        "issue" => issue(records, selector["id"].as_str().unwrap()),
        "event" => records
            .iter()
            .filter(|record| record["record_type"] == "event")
            .map(|record| &record["event"])
            .find(|event| {
                event["origin_store_uuid"] == selector["origin_store_uuid"]
                    && event["origin_event_sequence"] == selector["origin_event_sequence"]
            })
            .unwrap_or_else(|| panic!("event selector did not match")),
        "provenance_receipt" => records
            .iter()
            .filter(|record| record["record_type"] == "provenance_receipt")
            .map(|record| &record["provenance_receipt"])
            .find(|receipt| receipt["receipt_id"] == selector["receipt_id"])
            .unwrap_or_else(|| panic!("receipt selector did not match")),
        other => panic!("unsupported record selector {other:?}"),
    }
}

fn nested_selected_record<'a>(records: &'a [Value], selector: &Value, level: &str) -> &'a Value {
    let selector = object(selector, "selector");
    let issue = issue(records, selector["id"].as_str().unwrap());
    let issue = object(issue, "issue");
    match level {
        "dependency" => issue["dependencies"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["blocker"] == selector["dependency_blocker"])
            .unwrap_or_else(|| panic!("dependency selector did not match")),
        "external reference" => issue["external_references"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["key"] == selector["reference_key"])
            .unwrap_or_else(|| panic!("external-reference selector did not match")),
        "structured data" => issue["data"]
            .get(selector["data_namespace"].as_str().unwrap())
            .unwrap_or_else(|| panic!("structured-data selector did not match")),
        "resource key" => issue["resource_keys"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["resource_key"] == selector["resource_key"])
            .unwrap_or_else(|| panic!("resource-key selector did not match")),
        other => panic!("unsupported nested level {other:?}"),
    }
}

fn known_keys(level: &str) -> &'static [&'static str] {
    match level {
        "issue" => ISSUE_KEYS,
        "event" => EVENT_KEYS,
        "dependency" => DEPENDENCY_KEYS,
        "external reference" => REFERENCE_KEYS,
        "structured data" => DATA_KEYS,
        "resource key" => RESOURCE_KEY_KEYS,
        "receipt" => RECEIPT_KEYS,
        "pointer" => POINTER_KEYS,
        other => panic!("unknown fixture level {other:?}"),
    }
}

fn assert_sentinels(records: &[Value], pointer: &Value, expected: &Value, phase: &str) {
    let sentinels = expected["sentinels"].as_array().unwrap();
    let mut fields = HashSet::new();
    for sentinel in sentinels {
        let level = sentinel["level"].as_str().unwrap();
        let field = sentinel["field"].as_str().unwrap();
        assert!(fields.insert(field), "duplicate sentinel field {field}");
        assert!(
            !known_keys(level).contains(&field),
            "{level} sentinel {field} must remain outside the native schema"
        );

        if level == "pointer" {
            assert_eq!(
                pointer.get(field),
                Some(&sentinel["value"]),
                "{phase}: pointer sentinel {field} changed"
            );
            continue;
        }

        let selected = match level {
            "issue" | "event" | "receipt" => selected_record(records, &sentinel["selector"]),
            "dependency" | "external reference" | "structured data" | "resource key" => {
                nested_selected_record(records, &sentinel["selector"], level)
            }
            _ => unreachable!(),
        };
        assert_eq!(
            object(selected, level).get(field),
            Some(&sentinel["value"]),
            "{phase}: {level} sentinel {field} changed"
        );
    }
    assert_eq!(
        fields.len(),
        8,
        "the fixture must make all eight required checkpoint levels evident"
    );
}

fn bead(workspace: &Path, args: &[&str]) -> Command {
    let mut command = Command::cargo_bin("bead").unwrap();
    command.current_dir(workspace).args(args);
    command
}

fn active_records(workspace: &Path, pointer: &Value) -> Vec<Value> {
    let root = workspace
        .join(".beads/checkpoint")
        .join(pointer["active_root"]["path"].as_str().unwrap());
    read_records(&root)
}

#[test]
fn unknown_field_fixture_restores_and_republishes_all_required_levels() {
    let fixture = fixture_dir();
    let expected = read_json(&fixture.join("expected.json"));
    let pointer = fixture_pointer();
    let root_path = fixture.join("objects/unknown-fields.jsonl");
    let root_bytes = fs::read(&root_path).unwrap();
    let root_hash = format!("{:x}", Sha256::digest(root_bytes));

    assert_eq!(expected["fixture_format"], 1);
    assert_eq!(expected["profile"], "native-v1");
    assert_eq!(pointer["mode"], expected["input"]["mode"]);
    assert_eq!(pointer["active_root"]["path"], expected["input"]["root"]);
    assert_eq!(pointer["active_root"]["sha256"], root_hash);
    let records = fixture_records();
    assert_eq!(records.len(), 4);
    assert_eq!(pointer["total_record_count"], records.len());
    assert_sentinels(&records, &pointer, &expected, "fixture input");

    let target = TempDir::new().unwrap();
    bead(
        target.path(),
        &[
            "init",
            "--prefix",
            "ufx",
            "--no-auto-flush",
            "--skip-foreign-workspace",
        ],
    )
    .assert()
    .success();
    bead(
        target.path(),
        &[
            "sync",
            "import-only",
            "--input",
            fixture.to_str().unwrap(),
            "--restore-into-empty",
            "--actor",
            "unknown-field-fixture-test",
        ],
    )
    .assert()
    .success();
    bead(target.path(), &["sync", "flush-only"])
        .assert()
        .success();

    let restored_pointer = read_json(&target.path().join(".beads/checkpoint/current.json"));
    let restored_records = active_records(target.path(), &restored_pointer);
    assert_sentinels(
        &restored_records,
        &restored_pointer,
        &expected,
        "restored publication",
    );
}

#[test]
fn unknown_field_fixture_declares_only_documented_native_rewrites() {
    let expected = read_json(&fixture_dir().join("expected.json"));
    let rewrites = expected["allowed_native_rewrites"].as_array().unwrap();
    assert!(!rewrites.is_empty());
    for rewrite in rewrites {
        assert!(
            rewrite
                .as_str()
                .is_some_and(|value| !value.trim().is_empty()),
            "native rewrite documentation must contain non-empty strings"
        );
    }
}

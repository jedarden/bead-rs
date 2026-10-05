//! Restore/export conformance for the independent unknown-fields-v1 corpus.
//!
//! The corpus is deliberately supplied as a checkpoint-set directory rather
//! than as hand-built SQL. This test therefore exercises the public
//! `sync import-only --restore-into-empty` activation path, then publishes two
//! native generations and compares their semantic records.

use assert_cmd::Command;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::{Builder, TempDir};

fn bead(dir: &Path, args: &[&str]) -> Command {
    let mut command = Command::cargo_bin("bead").unwrap();
    command.current_dir(dir).args(args);
    command
}

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("research/fixtures/unknown-fields-v1")
}

fn fresh_workspace() -> TempDir {
    Builder::new()
        .prefix("restore-export-unknown-fields-")
        .tempdir_in("/var/tmp")
        .unwrap()
}

fn read_json(path: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

fn read_jsonl(path: &Path) -> Vec<Value> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn pointer(workspace: &Path) -> Value {
    read_json(&workspace.join(".beads/checkpoint/current.json"))
}

fn active_records(workspace: &Path) -> Vec<Value> {
    let checkpoint = workspace.join(".beads/checkpoint");
    let current = pointer(workspace);
    let root = checkpoint.join(current["active_root"]["path"].as_str().unwrap());
    read_jsonl(&root)
}

fn record_key(record: &Value) -> String {
    match record["record_type"].as_str().unwrap() {
        "issue" => format!("issue/{}", record["issue"]["id"].as_str().unwrap()),
        "event" => format!(
            "event/{}/{}",
            record["event"]["origin_store_uuid"].as_str().unwrap(),
            record["event"]["origin_event_sequence"].as_i64().unwrap()
        ),
        "provenance_receipt" => format!(
            "receipt/{}",
            record["provenance_receipt"]["receipt_id"].as_str().unwrap()
        ),
        record_type => panic!("unexpected record type {record_type:?}"),
    }
}

fn semantic_records(records: &[Value]) -> BTreeMap<String, Value> {
    records
        .iter()
        .map(|record| (record_key(record), record.clone()))
        .collect()
}

fn record_with_id<'a>(records: &'a [Value], record_type: &str, id: &str) -> &'a Value {
    records
        .iter()
        .find(|record| {
            record["record_type"] == record_type
                && match record_type {
                    "issue" => record["issue"]["id"] == id,
                    "provenance_receipt" => record["provenance_receipt"]["receipt_id"] == id,
                    _ => false,
                }
        })
        .unwrap_or_else(|| panic!("missing {record_type} record {id}"))
}

fn record_with_event_sequence(records: &[Value], sequence: i64) -> &Value {
    records
        .iter()
        .find(|record| {
            record["record_type"] == "event" && record["event"]["origin_event_sequence"] == sequence
        })
        .unwrap_or_else(|| panic!("missing event sequence {sequence}"))
}

fn assert_corpus_unknown_fields(records: &[Value], current: &Value, expected: &Value) {
    let unknown_members = expected["unknown_members"].as_object().unwrap();
    assert_eq!(
        unknown_members.len(),
        10,
        "the corpus must cover every probe"
    );

    for (path, expected_value) in unknown_members {
        if path.starts_with("/pointer/current.json/") {
            let field = path.rsplit('/').next().unwrap();
            assert_eq!(&current[field], expected_value, "pointer field {path}");
            continue;
        }
        if path.starts_with("/resource_key/") {
            // Native issues intentionally represent resource keys as sorted
            // strings, so this companion probe cannot be imported as an
            // object-level extension without changing the native shape.
            let resource_key = read_json(&corpus_dir().join("resource-key.json"));
            let field = path.rsplit('/').next().unwrap();
            assert_eq!(
                &resource_key["resource_key"][field], expected_value,
                "resource-key companion field {path}"
            );
            continue;
        }

        let (object, field) = if let Some(rest) = path.strip_prefix("/issue/") {
            let (id, tail) = rest.split_once('/').unwrap();
            let issue = &record_with_id(records, "issue", id)["issue"];
            let (container, field) = if let Some(data_path) = tail.strip_prefix("data/") {
                let (namespace, field) = data_path.rsplit_once('/').unwrap();
                (&issue["data"][namespace], field)
            } else if let Some(dependency_path) = tail.strip_prefix("dependencies/0/") {
                (&issue["dependencies"][0], dependency_path)
            } else if let Some(reference_path) = tail.strip_prefix("external_references/0/") {
                (&issue["external_references"][0], reference_path)
            } else {
                (issue, tail)
            };
            (container, field)
        } else if let Some(rest) = path.strip_prefix("/event/") {
            let (sequence, field) = rest.rsplit_once('/').unwrap();
            (
                &record_with_event_sequence(records, sequence.parse().unwrap())["event"],
                field,
            )
        } else if let Some(rest) = path.strip_prefix("/provenance_receipt/") {
            let (id, field) = rest.rsplit_once('/').unwrap();
            (
                &record_with_id(records, "provenance_receipt", id)["provenance_receipt"],
                field,
            )
        } else {
            panic!("unhandled corpus pointer {path}");
        };

        assert_eq!(&object[field], expected_value, "unknown field {path}");
    }
}

fn assert_resource_key_known_semantics(records: &[Value], expected: &Value) {
    let expected_key = expected["known_semantics"]["resource_key_probe"]["key"]
        .as_str()
        .unwrap();
    let issue = record_with_id(records, "issue", "bead-unknown-a");
    assert_eq!(
        issue["issue"]["resource_keys"],
        Value::Array(vec![Value::String(expected_key.to_string())]),
        "the native resource-key projection must survive the restore/export path"
    );
}

#[test]
fn restore_into_empty_then_two_exports_preserve_corpus_unknown_fields() {
    let corpus = corpus_dir();
    let expected = read_json(&corpus.join("expected.json"));
    let source_records = read_jsonl(&corpus.join("checkpoint.jsonl"));
    let source_pointer = read_json(&corpus.join("current.json"));
    assert_eq!(source_records.len(), 5);
    assert_eq!(
        source_pointer["x-fixture-pointer"],
        expected["unknown_members"]["/pointer/current.json/x-fixture-pointer"]
    );

    let workspace = fresh_workspace();
    bead(
        workspace.path(),
        &[
            "init",
            "--skip-foreign-workspace",
            "--prefix",
            "uf",
            "--no-auto-flush",
        ],
    )
    .assert()
    .success();

    bead(
        workspace.path(),
        &[
            "sync",
            "import-only",
            "--input",
            corpus.to_str().unwrap(),
            "--restore-into-empty",
            "--actor",
            "unknown-fields-conformance",
            "--no-auto-flush",
        ],
    )
    .assert()
    .success()
    .stderr(predicates::str::contains("Restored 2 issues, 2 events"));

    bead(
        workspace.path(),
        &["sync", "flush-only", "--skip-foreign-workspace"],
    )
    .assert()
    .success();
    let first_pointer = pointer(workspace.path());
    let first_records = active_records(workspace.path());
    assert_eq!(first_pointer["issue_count"], 2);
    assert_eq!(first_pointer["event_count"], 2);
    assert_eq!(first_pointer["receipt_count"], 2);
    assert_corpus_unknown_fields(&first_records, &first_pointer, &expected);
    assert_resource_key_known_semantics(&first_records, &expected);

    bead(
        workspace.path(),
        &["create", "--title", "second generation", "--no-auto-flush"],
    )
    .assert()
    .success();
    bead(
        workspace.path(),
        &["sync", "flush-only", "--skip-foreign-workspace"],
    )
    .assert()
    .success();
    let second_pointer = pointer(workspace.path());
    let second_records = active_records(workspace.path());
    assert_ne!(
        first_pointer["generation_id"], second_pointer["generation_id"],
        "the mutation must publish a second generation"
    );
    assert_eq!(
        read_json(&workspace.path().join(".beads/checkpoint/previous.json"))["generation_id"],
        first_pointer["generation_id"]
    );
    assert_corpus_unknown_fields(&second_records, &second_pointer, &expected);
    assert_resource_key_known_semantics(&second_records, &expected);

    let first_semantics = semantic_records(&first_records);
    let second_semantics = semantic_records(&second_records);
    for (key, record) in &first_semantics {
        assert_eq!(
            second_semantics.get(key),
            Some(record),
            "semantic record {key} changed between generations"
        );
    }
    let added: Vec<_> = second_semantics
        .keys()
        .filter(|key| !first_semantics.contains_key(*key))
        .collect();
    assert_eq!(
        added.len(),
        2,
        "the second generation adds one issue and event"
    );
    assert!(added.iter().any(|key| key.starts_with("issue/uf-")));
    assert!(added.iter().any(|key| key.starts_with("event/")));

    let expected_counts = expected["record_counts"].as_object().unwrap();
    assert_eq!(
        first_records.len(),
        6,
        "source records plus restore receipt"
    );
    assert_eq!(
        second_records.len(),
        8,
        "second generation adds issue and event"
    );
    assert_eq!(expected_counts["total"], 5);
}

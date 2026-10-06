//! Restore/export conformance for the independent unknown-fields-v1 corpus.
//!
//! The corpus is deliberately supplied as a checkpoint-set directory rather
//! than as hand-built SQL. This test therefore exercises the public
//! `sync import-only --restore-into-empty` activation path, then publishes three
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
    serde_json::from_str(
        &fs::read_to_string(path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display())),
    )
    .unwrap_or_else(|error| panic!("parse {}: {error}", path.display()))
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
    // Normalize only the documented checkpoint ordering and stable record
    // identities. Inherited values, including timestamps and opaque
    // extensions, remain exact; newly generated restore metadata is additive
    // and is not compared against an earlier generation.
    records
        .iter()
        .map(|record| (record_key(record), record.clone()))
        .collect()
}

fn assert_json_semantics(expected: &Value, actual: &Value, path: &str) {
    match (expected, actual) {
        (Value::Object(expected), Value::Object(actual)) => {
            for (key, expected_value) in expected {
                let member_path = format!("{path}/{key}");
                let Some(actual_value) = actual.get(key) else {
                    // Native serialization omits some optional null members,
                    // empty collections, and empty objects, which are
                    // semantically equivalent to their explicit forms.
                    // Unknown sentinels are checked separately and do not use
                    // this exception.
                    assert!(
                        expected_value.is_null()
                            || expected_value.as_array().is_some_and(Vec::is_empty)
                            || expected_value
                                .as_object()
                                .is_some_and(serde_json::Map::is_empty),
                        "{member_path}: known member was dropped"
                    );
                    continue;
                };
                assert_json_semantics(expected_value, actual_value, &member_path);
            }
        }
        (Value::Array(expected), Value::Array(actual)) => {
            assert_eq!(
                actual.len(),
                expected.len(),
                "{path}: semantic array length changed"
            );
            for (index, (expected_value, actual_value)) in expected.iter().zip(actual).enumerate() {
                assert_json_semantics(expected_value, actual_value, &format!("{path}/{index}"));
            }
        }
        _ => assert_eq!(expected, actual, "{path}: semantic value changed"),
    }
}

fn assert_records_contain_semantics(
    expected_records: &[Value],
    actual_records: &[Value],
    label: &str,
) {
    let expected = semantic_records(expected_records);
    let actual = semantic_records(actual_records);

    for (key, record) in expected {
        let actual_record = actual
            .get(&key)
            .unwrap_or_else(|| panic!("{label}: semantic record {key} was dropped"));
        assert_json_semantics(&record, actual_record, &format!("{label}/{key}"));
    }
}

struct PublishedGeneration {
    workspace: TempDir,
    source_generation_id: String,
    pointer: Value,
    records: Vec<Value>,
}

fn restore_and_publish(source: &Path, actor: &str) -> PublishedGeneration {
    let source_generation_id = read_json(&source.join("current.json"))["generation_id"]
        .as_str()
        .unwrap()
        .to_string();
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
            source.to_str().unwrap(),
            "--restore-into-empty",
            "--actor",
            actor,
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

    PublishedGeneration {
        pointer: pointer(workspace.path()),
        records: active_records(workspace.path()),
        source_generation_id,
        workspace,
    }
}

fn assert_semantic_preservation(
    previous: &PublishedGeneration,
    current: &PublishedGeneration,
    label: &str,
) {
    let previous_semantics = semantic_records(&previous.records);
    let current_semantics = semantic_records(&current.records);
    for (key, record) in &previous_semantics {
        assert_eq!(
            current_semantics.get(key),
            Some(record),
            "{label}: semantic record {key} changed across restore/export"
        );
    }
}

fn remove_unknown_member(records: &mut [Value], issue_id: &str, field: &str) {
    let issue = records
        .iter_mut()
        .find(|record| record["record_type"] == "issue" && record["issue"]["id"] == issue_id)
        .unwrap_or_else(|| panic!("missing issue {issue_id} in deliberate-drop mutation"));
    assert!(
        issue["issue"]
            .as_object_mut()
            .unwrap()
            .remove(field)
            .is_some(),
        "deliberate-drop mutation must remove an existing field"
    );
}

fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<String>() {
        return message.clone();
    }
    if let Some(message) = payload.downcast_ref::<&str>() {
        return (*message).to_string();
    }
    "unknown panic payload".to_string()
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
fn native_restore_of_fixture_generation_preserves_corpus_unknown_fields() {
    let source = corpus_dir();
    let expected = read_json(&source.join("expected.json"));
    let source_pointer = read_json(&source.join("current.json"));
    let generation = source_pointer["generation_id"].as_str().unwrap();
    let workspace = fresh_workspace();

    let output = bead(
        workspace.path(),
        &[
            "restore",
            "--source",
            source.to_str().unwrap(),
            "--generation",
            generation,
            "--actor",
            "unknown-fields-native-restore",
            "--format",
            "json",
        ],
    )
    .assert()
    .success()
    .get_output()
    .stdout
    .clone();
    let report: Value = serde_json::from_slice(&output).unwrap();
    let restored_pointer = pointer(workspace.path());
    let restored_records = active_records(workspace.path());

    assert_eq!(report["generation_id"], generation);
    assert_eq!(report["issues_restored"], source_pointer["issue_count"]);
    assert_eq!(report["events_restored"], source_pointer["event_count"]);
    assert_eq!(
        report["provenance_receipts_restored"],
        source_pointer["receipt_count"]
    );
    assert_eq!(restored_records.len(), 7);
    assert_corpus_unknown_fields(&restored_records, &restored_pointer, &expected);
    assert_resource_key_known_semantics(&restored_records, &expected);
}

fn build_three_generations() -> Vec<PublishedGeneration> {
    let corpus = corpus_dir();
    let first = restore_and_publish(&corpus, "unknown-fields-generation-1");
    let second_source = first.workspace.path().join(".beads/checkpoint");
    let second = restore_and_publish(&second_source, "unknown-fields-generation-2");
    let third_source = second.workspace.path().join(".beads/checkpoint");
    let third = restore_and_publish(&third_source, "unknown-fields-generation-3");
    vec![first, second, third]
}

#[test]
fn restore_into_empty_then_three_connected_exports_preserve_corpus_unknown_fields() {
    let corpus = corpus_dir();
    let expected = read_json(&corpus.join("expected.json"));
    let source_records = read_jsonl(&corpus.join("checkpoint.jsonl"));
    let source_pointer = read_json(&corpus.join("current.json"));
    assert_eq!(source_records.len(), 5);
    assert_eq!(
        source_pointer["x-fixture-pointer"],
        expected["unknown_members"]["/pointer/current.json/x-fixture-pointer"]
    );

    let generations = build_three_generations();
    for (index, generation) in generations.iter().enumerate() {
        let label = format!("generation {}", index + 1);
        assert_eq!(generation.pointer["issue_count"], 2, "{label}: issue count");
        assert_eq!(generation.pointer["event_count"], 2, "{label}: event count");
        assert_eq!(
            generation.pointer["receipt_count"],
            (index + 2) as i64,
            "{label}: receipt count"
        );
        assert_eq!(generation.records.len(), 6 + index, "{label}: record count");

        // Match records by their stable wire identities before comparing the
        // complete JSON values. `serde_json::Value` compares object members by
        // key, so this assertion is independent of object insertion order,
        // while still requiring every known field and nested extension from
        // the fixture to survive the restore/export hop.
        assert_records_contain_semantics(&source_records, &generation.records, &label);
        assert_corpus_unknown_fields(&generation.records, &generation.pointer, &expected);
        assert_resource_key_known_semantics(&generation.records, &expected);
        assert_ne!(
            generation.pointer["generation_id"].as_str().unwrap(),
            generation.source_generation_id,
            "{label}: restore/export must publish a new generation"
        );

        if let Some(previous) = index.checked_sub(1) {
            assert_eq!(
                generation.source_generation_id,
                generations[previous].pointer["generation_id"]
                    .as_str()
                    .unwrap(),
                "{label}: restore input must be the connected predecessor"
            );
            assert_semantic_preservation(
                &generations[previous],
                generation,
                &format!("{label} from predecessor"),
            );
        }
    }

    assert_semantic_preservation(&generations[0], &generations[2], "generation 3 from source");
    assert_eq!(expected["record_counts"]["total"], 5);
}

#[test]
fn dropped_corpus_unknown_field_fails_preservation_assertion() {
    let expected = read_json(&corpus_dir().join("expected.json"));
    let generations = build_three_generations();
    assert_corpus_unknown_fields(&generations[0].records, &generations[0].pointer, &expected);

    let mut mutated = generations[1].records.clone();
    remove_unknown_member(&mut mutated, "bead-unknown-a", "x-fixture-issue");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        assert_corpus_unknown_fields(&mutated, &generations[1].pointer, &expected);
    }));
    let message = panic_message(
        result.expect_err("a deliberate dropped corpus field must fail preservation assertions"),
    );
    assert!(
        message.contains("/issue/bead-unknown-a/x-fixture-issue"),
        "the failed assertion must identify the dropped field: {message}"
    );
}

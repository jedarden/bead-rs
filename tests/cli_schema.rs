use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use std::collections::HashSet;

#[test]
fn schema_list_is_workspace_independent_and_deterministic() {
    let first = Command::cargo_bin("bead")
        .unwrap()
        .args(["schema", "list", "--format", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let second = Command::cargo_bin("bead")
        .unwrap()
        .args(["schema", "list"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert_eq!(first, second);

    let entries: Vec<Value> = serde_json::from_slice(&first).unwrap();
    assert!(!entries.is_empty());
    let refs: Vec<&str> = entries
        .iter()
        .map(|entry| entry["schema_ref"].as_str().unwrap())
        .collect();
    let mut sorted = refs.clone();
    sorted.sort_unstable();
    assert_eq!(refs, sorted);
    assert_eq!(refs.len(), refs.iter().collect::<HashSet<_>>().len());

    for entry in entries {
        for member in [
            "schema_ref",
            "document_kind",
            "readable",
            "writable",
            "validate",
            "consume",
            "emit",
        ] {
            assert!(entry.get(member).is_some(), "missing {member}: {entry}");
        }
    }
}

#[test]
fn schema_list_matches_capabilities_catalog() {
    let list: Value = serde_json::from_slice(
        &Command::cargo_bin("bead")
            .unwrap()
            .args(["schema", "list"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    let capabilities: Value = serde_json::from_slice(
        &Command::cargo_bin("bead")
            .unwrap()
            .arg("capabilities")
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    assert_eq!(list, capabilities["schemas"]);
}

#[test]
fn schema_list_rejects_unsupported_format_as_usage_error() {
    Command::cargo_bin("bead")
        .unwrap()
        .args(["schema", "list", "--format", "yaml"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("invalid value"));
}

#[test]
fn schema_show_resolves_every_catalog_identity() {
    let catalog: Vec<Value> = serde_json::from_slice(
        &Command::cargo_bin("bead")
            .unwrap()
            .args(["schema", "list"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    for entry in catalog {
        let schema_ref = entry["schema_ref"].as_str().unwrap();
        let output = Command::cargo_bin("bead")
            .unwrap()
            .args(["schema", "show", schema_ref, "--format", "json"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let schema: Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(schema["$id"], schema_ref);
        assert_eq!(
            schema["$schema"],
            "https://json-schema.org/draft/2020-12/schema"
        );
    }
}

#[test]
fn schema_explain_json_and_markdown_are_deterministic() {
    let schema_ref = "urn:bead-rs:schema:issue:native-v1";
    let json_output = Command::cargo_bin("bead")
        .unwrap()
        .args(["schema", "explain", schema_ref, "--format", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let explanation: Value = serde_json::from_slice(&json_output).unwrap();
    assert_eq!(explanation["guide_version"], 1);
    assert!(explanation["describes_schema_refs"]
        .as_array()
        .unwrap()
        .iter()
        .any(|value| value == schema_ref));
    assert!(!explanation["fields"].as_array().unwrap().is_empty());
    let documents = explanation["documents"].as_array().unwrap();
    assert_eq!(documents.len(), 5);
    let expected_fields: usize = documents
        .iter()
        .map(|document| document["members"].as_array().unwrap().len())
        .sum();
    assert_eq!(
        explanation["fields"].as_array().unwrap().len(),
        expected_fields
    );
    assert!(explanation["fields"]
        .as_array()
        .unwrap()
        .iter()
        .all(|field| field["json_type"] != "any"));

    Command::cargo_bin("bead")
        .unwrap()
        .args(["schema", "explain", schema_ref, "--format", "markdown"])
        .assert()
        .success()
        .stdout(predicate::str::contains("# Native field guide v1"))
        .stdout(predicate::str::contains("checkpoint_issue.priority"));
}

#[test]
fn schema_show_and_explain_reject_unknown_identity() {
    for operation in ["show", "explain"] {
        Command::cargo_bin("bead")
            .unwrap()
            .args(["schema", operation, "urn:unknown:schema"])
            .assert()
            .code(2)
            .stderr(predicate::str::contains("Unsupported schema identity"));
    }
}

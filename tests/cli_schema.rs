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

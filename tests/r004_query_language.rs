//! R004 Safe Query Language integration tests
//!
//! Comprehensive tests for R004's safe query language and saved views functionality.

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use serial_test::serial;

#[test]
#[serial]
fn test_query_basic_predicate() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Create test issues
    Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "High Priority Task", "--priority", "0"])
        .assert()
        .success();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Low Priority Task", "--priority", "4"])
        .assert()
        .success();

    // Query for high priority issues
    let query_json = r#"{
        "version": "v1",
        "predicates": [
            {"field": "priority", "operator": "less_than_or_equal", "value": 1}
        ],
        "sort": [
            {"field": "priority", "direction": "asc"}
        ]
    }"#;

    Command::cargo_bin("bead")
        .unwrap()
        .args(["query", "--json", query_json])
        .assert()
        .success()
        .stdout(predicates::str::contains("High Priority Task"));
}

#[test]
#[serial]
fn test_query_invalid_version() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Try to use invalid version
    let query_json = r#"{
        "version": "v2",
        "predicates": [],
        "sort": []
    }"#;

    Command::cargo_bin("bead")
        .unwrap()
        .args(["query", "--json", query_json])
        .assert()
        .failure();
}

#[test]
#[serial]
fn test_query_string_operators() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Create test issues
    Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Authentication Bug"])
        .assert()
        .success();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "UI Enhancement"])
        .assert()
        .success();

    // Query for issues containing "Bug"
    let query_json = r#"{
        "version": "v1",
        "predicates": [
            {"field": "title", "operator": "contains", "value": "Bug"}
        ],
        "sort": []
    }"#;

    Command::cargo_bin("bead")
        .unwrap()
        .args(["query", "--json", query_json])
        .assert()
        .success()
        .stdout(predicates::str::contains("Authentication Bug"))
        .stdout(predicates::str::contains("UI Enhancement").not());
}

#[test]
#[serial]
fn test_save_and_execute_view() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Create test issues
    Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Task 1", "--priority", "1"])
        .assert()
        .success();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Task 2", "--priority", "3"])
        .assert()
        .success();

    // Save a query as a view
    let query_json = r#"{
        "version": "v1",
        "predicates": [
            {"field": "priority", "operator": "greater_than_or_equal", "value": 2}
        ],
        "sort": []
    }"#;

    Command::cargo_bin("bead")
        .unwrap()
        .args(["query", "--json", query_json, "--save-as", "high-priority"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Saved view: high-priority"));

    // List views
    Command::cargo_bin("bead")
        .unwrap()
        .args(["query", "--list-views"])
        .assert()
        .success()
        .stdout(predicates::str::contains("high-priority"));

    // Execute the saved view
    Command::cargo_bin("bead")
        .unwrap()
        .args(["query", "--view", "high-priority"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Task 2"));
}

#[test]
#[serial]
fn test_delete_view() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Create a test issue
    Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Task 1"])
        .assert()
        .success();

    // Save a view
    let query_json = r#"{
        "version": "v1",
        "predicates": [],
        "sort": []
    }"#;

    Command::cargo_bin("bead")
        .unwrap()
        .args(["query", "--json", query_json, "--save-as", "test-view"])
        .assert()
        .success();

    // Delete the view
    Command::cargo_bin("bead")
        .unwrap()
        .args(["query", "--delete-view", "test-view"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Deleted view: test-view"));

    // Try to execute deleted view (should fail)
    Command::cargo_bin("bead")
        .unwrap()
        .args(["query", "--view", "test-view"])
        .assert()
        .failure();
}

#[test]
#[serial]
fn test_query_with_projection() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Create a test issue
    Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Test Task", "--priority", "2"])
        .assert()
        .success();

    // Query with projection
    let query_json = r#"{
        "version": "v1",
        "predicates": [],
        "sort": [],
        "projection": {
            "fields": ["title", "priority"]
        }
    }"#;

    Command::cargo_bin("bead")
        .unwrap()
        .args(["query", "--json", query_json])
        .assert()
        .success()
        .stdout(predicates::str::contains("Title"))
        .stdout(predicates::str::contains("Priority"));
}

#[test]
#[serial]
fn test_query_limit() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Create multiple test issues
    for i in 1..=5 {
        Command::cargo_bin("bead")
            .unwrap()
            .args(["create", "--title", &format!("Task {}", i)])
            .assert()
            .success();
    }

    // Query with limit
    let query_json = r#"{
        "version": "v1",
        "predicates": [],
        "sort": [],
        "limit": 2
    }"#;

    Command::cargo_bin("bead")
        .unwrap()
        .args(["query", "--json", query_json])
        .assert()
        .success()
        .stdout(predicates::str::contains("Found 2 issues"));
}

#[test]
#[serial]
fn test_query_without_workspace() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Try to query without initializing workspace
    let query_json = r#"{
        "version": "v1",
        "predicates": []
    }"#;

    Command::cargo_bin("bead")
        .unwrap()
        .args(["query", "--json", query_json])
        .assert()
        .failure()
        .stderr(predicates::str::contains("No bead workspace found"));
}

#[test]
#[serial]
fn test_query_empty_result() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Query with no matching issues
    let query_json = r#"{
        "version": "v1",
        "predicates": [
            {"field": "title", "operator": "contains", "value": "NonExistent"}
        ],
        "sort": []
    }"#;

    Command::cargo_bin("bead")
        .unwrap()
        .args(["query", "--json", query_json])
        .assert()
        .success()
        .stdout(predicates::str::contains("Found 0 issues"));
}

#[test]
#[serial]
fn test_query_file_input() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Create a test issue
    Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "File Test Task"])
        .assert()
        .success();

    // Create query file
    let query_file = temp.path().join("query.json");
    std::fs::write(
        &query_file,
        r#"{
        "version": "v1",
        "predicates": [
            {"field": "title", "operator": "contains", "value": "File"}
        ],
        "sort": []
    }"#,
    )
    .unwrap();

    // Query using file
    Command::cargo_bin("bead")
        .unwrap()
        .args(["query", "--file", query_file.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicates::str::contains("File Test Task"));
}

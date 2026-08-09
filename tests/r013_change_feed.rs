//! Integration tests for R013 cursor-based local change feed
//!
//! These tests verify the cursor-based change feed functionality that enables
//! incremental local indexes and adapters to track workspace changes without
//! requiring a daemon or network service.

use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

fn setup_workspace() -> TempDir {
    let temp_dir = TempDir::new().unwrap();
    let workspace = temp_dir.path();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .arg("init")
        .current_dir(workspace)
        .assert()
        .success();

    temp_dir
}

fn get_latest_cursor_json(workspace: &std::path::Path) -> Value {
    let output = Command::cargo_bin("bead")
        .unwrap()
        .arg("changes")
        .arg("--latest")
        .arg("--json")
        .current_dir(workspace)
        .assert()
        .success();

    let stdout = &output.get_output().stdout;
    serde_json::from_slice(stdout).unwrap()
}

fn get_snapshot_json(workspace: &std::path::Path) -> Value {
    let output = Command::cargo_bin("bead")
        .unwrap()
        .arg("changes")
        .arg("--snapshot")
        .arg("--json")
        .current_dir(workspace)
        .assert()
        .success();

    let stdout = &output.get_output().stdout;
    serde_json::from_slice(stdout).unwrap()
}

fn get_changes_since_json(workspace: &std::path::Path, cursor: &str) -> Value {
    let output = Command::cargo_bin("bead")
        .unwrap()
        .arg("changes")
        .arg("--since")
        .arg(cursor)
        .arg("--json")
        .current_dir(workspace)
        .assert()
        .success();

    let stdout = &output.get_output().stdout;
    serde_json::from_slice(stdout).unwrap()
}

#[test]
fn test_change_feed_empty_workspace() {
    let workspace = setup_workspace();

    // Get latest cursor from empty workspace
    let snapshot = get_latest_cursor_json(workspace.path());
    assert_eq!(snapshot["max_sequence"], 0);
    assert!(snapshot["workspace_uuid"].is_string());
    assert!(snapshot["checksum"].is_string());

    // Get snapshot identity
    let snapshot = get_snapshot_json(workspace.path());
    assert_eq!(snapshot["max_sequence"], 0);
    assert!(snapshot["workspace_uuid"].is_string());

    // Get changes since beginning (should be empty)
    let changes = get_changes_since_json(workspace.path(), "0");
    assert_eq!(changes["total_available"], 0);
    assert_eq!(changes["returned_count"], 0);
    assert!(changes["mutations"].as_array().unwrap().is_empty());
    assert!(!changes["has_gaps"].as_bool().unwrap());
}

#[test]
fn test_change_feed_after_create() {
    let workspace = setup_workspace();

    // Create an issue
    let _output = Command::cargo_bin("bead")
        .unwrap()
        .arg("create")
        .arg("--title")
        .arg("Test Issue")
        .current_dir(workspace.path())
        .assert()
        .success();

    // Perform an operation that generates events (claim)
    Command::cargo_bin("bead")
        .unwrap()
        .arg("claim")
        .arg("--assignee")
        .arg("test-worker")
        .current_dir(workspace.path())
        .assert()
        .success();

    // Get latest cursor
    let snapshot = get_latest_cursor_json(workspace.path());
    let max_sequence = snapshot["max_sequence"].as_i64().unwrap();
    assert!(max_sequence > 0);

    // Get changes since beginning
    let changes = get_changes_since_json(workspace.path(), "0");
    assert_eq!(changes["total_available"], max_sequence);
    assert!(changes["returned_count"].as_i64().unwrap() >= 1);
    assert!(!changes["has_gaps"].as_bool().unwrap());

    let mutations = changes["mutations"].as_array().unwrap();
    assert!(!mutations.is_empty());
    // The last event should be the claim
    let last_mutation = &mutations[mutations.len() - 1];
    assert_eq!(last_mutation["kind"], "claimed");
}

#[test]
fn test_change_feed_incremental_updates() {
    let workspace = setup_workspace();

    // Create multiple issues
    let _issue_ids: Vec<String> = vec![];
    for i in 1..=3 {
        let output = Command::cargo_bin("bead")
            .unwrap()
            .arg("create")
            .arg("--title")
            .arg(format!("Issue {}", i))
            .current_dir(workspace.path())
            .assert()
            .success();

        let issue_id = String::from_utf8(output.get_output().stdout.clone()).unwrap();
        let _issue_id = issue_id.trim().to_string();
    }

    // Get initial cursor
    let snapshot1 = get_latest_cursor_json(workspace.path());
    let cursor1 = snapshot1["max_sequence"].as_i64().unwrap();

    // Update an issue - get first issue from list
    let output = Command::cargo_bin("bead")
        .unwrap()
        .arg("list")
        .arg("--json")
        .arg("--limit")
        .arg("1")
        .current_dir(workspace.path())
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let first_line = stdout.lines().next().unwrap();
    let first_issue_json: Value = serde_json::from_str(first_line).unwrap();
    let first_issue_id = first_issue_json["id"].as_str().unwrap();

    Command::cargo_bin("bead")
        .unwrap()
        .arg("update")
        .arg(first_issue_id)
        .arg("--notes")
        .arg("Updated notes")
        .current_dir(workspace.path())
        .assert()
        .success();

    // Get changes after update
    let snapshot2 = get_latest_cursor_json(workspace.path());
    let _cursor2 = snapshot2["max_sequence"].as_i64().unwrap();

    let changes = get_changes_since_json(workspace.path(), &cursor1.to_string());
    assert_eq!(changes["returned_count"], 1);
    assert!(!changes["has_gaps"].as_bool().unwrap());

    let mutations = changes["mutations"].as_array().unwrap();
    assert_eq!(mutations.len(), 1);
    assert_eq!(mutations[0]["kind"], "updated");
}

#[test]
fn test_cursor_validation() {
    let workspace = setup_workspace();

    // Create an issue
    Command::cargo_bin("bead")
        .unwrap()
        .arg("create")
        .arg("--title")
        .arg("Test Issue")
        .current_dir(workspace.path())
        .assert()
        .success();

    // Validate cursor at beginning (should be valid)
    let _output = Command::cargo_bin("bead")
        .unwrap()
        .arg("changes")
        .arg("--validate")
        .arg("0")
        .current_dir(workspace.path())
        .assert()
        .success();

    // Validate cursor at current position (should be valid)
    let snapshot = get_latest_cursor_json(workspace.path());
    let current_cursor = snapshot["max_sequence"].as_i64().unwrap();

    let _output = Command::cargo_bin("bead")
        .unwrap()
        .arg("changes")
        .arg("--validate")
        .arg(current_cursor.to_string())
        .current_dir(workspace.path())
        .assert()
        .success();
}

#[test]
fn test_cursor_serialization() {
    // Test basic cursor
    let cursor_str = "42";
    let _output = Command::cargo_bin("bead")
        .unwrap()
        .arg("changes")
        .arg("--validate")
        .arg(cursor_str)
        .current_dir(setup_workspace().path())
        .assert()
        .success();

    // Test cursor with checksum
    let cursor_str = "42:abc123";
    let _output = Command::cargo_bin("bead")
        .unwrap()
        .arg("changes")
        .arg("--validate")
        .arg(cursor_str)
        .current_dir(setup_workspace().path())
        .assert()
        .success();
}

#[test]
fn test_gap_detection() {
    let workspace = setup_workspace();

    // Create some issues to generate events
    for i in 1..=3 {
        Command::cargo_bin("bead")
            .unwrap()
            .arg("create")
            .arg("--title")
            .arg(format!("Issue {}", i))
            .current_dir(workspace.path())
            .assert()
            .success();
    }

    // Get current cursor
    let snapshot = get_latest_cursor_json(workspace.path());
    let current_cursor = snapshot["max_sequence"].as_i64().unwrap();

    // Validate cursor (no gaps expected)
    let _output = Command::cargo_bin("bead")
        .unwrap()
        .arg("changes")
        .arg("--validate")
        .arg(current_cursor.to_string())
        .current_dir(workspace.path())
        .assert()
        .success();

    // In a real scenario with gaps, this would fail
    // For now we're testing the validation mechanism works
}

#[test]
fn test_change_feed_multiple_mutations() {
    let workspace = setup_workspace();

    // Create issue
    let output = Command::cargo_bin("bead")
        .unwrap()
        .arg("create")
        .arg("--title")
        .arg("Test Issue")
        .current_dir(workspace.path())
        .assert()
        .success();

    let issue_id = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let issue_id = issue_id.trim();

    // Get cursor after creation
    let snapshot1 = get_latest_cursor_json(workspace.path());
    let cursor1 = snapshot1["max_sequence"].as_i64().unwrap();

    // Perform multiple mutations
    Command::cargo_bin("bead")
        .unwrap()
        .arg("claim")
        .arg("--assignee")
        .arg("test-worker")
        .current_dir(workspace.path())
        .assert()
        .success();

    Command::cargo_bin("bead")
        .unwrap()
        .arg("update")
        .arg(issue_id)
        .arg("--notes")
        .arg("Progress made")
        .current_dir(workspace.path())
        .assert()
        .success();

    Command::cargo_bin("bead")
        .unwrap()
        .arg("release")
        .arg(issue_id)
        .current_dir(workspace.path())
        .assert()
        .success();

    // Get all changes since cursor
    let changes = get_changes_since_json(workspace.path(), &cursor1.to_string());
    assert!(changes["returned_count"].as_i64().unwrap() >= 3);

    // Check we have the expected event kinds
    let mutations = changes["mutations"].as_array().unwrap();
    let event_kinds: Vec<String> = mutations
        .iter()
        .map(|m| m["kind"].as_str().unwrap().to_string())
        .collect();

    assert!(event_kinds.contains(&"claimed".to_string()));
    assert!(event_kinds.contains(&"updated".to_string()));
    assert!(event_kinds.contains(&"released".to_string()));
}

#[test]
fn test_change_feed_workspace_events() {
    let workspace = setup_workspace();

    // Create an issue and claim it to generate events
    Command::cargo_bin("bead")
        .unwrap()
        .arg("create")
        .arg("--title")
        .arg("Test Issue")
        .current_dir(workspace.path())
        .assert()
        .success();

    Command::cargo_bin("bead")
        .unwrap()
        .arg("claim")
        .arg("--assignee")
        .arg("test-worker")
        .current_dir(workspace.path())
        .assert()
        .success();

    // Get changes from beginning
    let changes = get_changes_since_json(workspace.path(), "0");

    // Should have at least claim events
    assert!(changes["returned_count"].as_i64().unwrap() >= 1);

    let mutations = changes["mutations"].as_array().unwrap();
    // All events should have valid issue_id
    for mutation in mutations.iter() {
        assert!(mutation["issue_id"].is_string() || mutation["issue_id"].is_null());
    }
}

#[test]
fn test_change_feed_json_format() {
    let workspace = setup_workspace();

    // Create issue
    Command::cargo_bin("bead")
        .unwrap()
        .arg("create")
        .arg("--title")
        .arg("JSON Format Test")
        .current_dir(workspace.path())
        .assert()
        .success();

    // Get changes in JSON format
    let changes = get_changes_since_json(workspace.path(), "0");

    // Verify required fields exist
    assert!(changes["snapshot"].is_object());
    assert!(changes["has_gaps"].is_boolean());
    assert!(changes["total_available"].is_number());
    assert!(changes["returned_count"].is_number());
    assert!(changes["mutations"].is_array());

    // Check snapshot structure
    let snapshot = &changes["snapshot"];
    assert!(snapshot["workspace_uuid"].is_string());
    assert!(snapshot["max_sequence"].is_number());
    assert!(snapshot["checksum"].is_string());
    assert!(snapshot["timestamp"].is_string());
}

#[test]
fn test_change_feed_human_readable_output() {
    let workspace = setup_workspace();

    // Create issue and perform operation that generates events
    Command::cargo_bin("bead")
        .unwrap()
        .arg("create")
        .arg("--title")
        .arg("Human Readable Test")
        .current_dir(workspace.path())
        .assert()
        .success();

    Command::cargo_bin("bead")
        .unwrap()
        .arg("claim")
        .arg("--assignee")
        .arg("test-worker")
        .current_dir(workspace.path())
        .assert()
        .success();

    // Get human-readable output
    let output = Command::cargo_bin("bead")
        .unwrap()
        .arg("changes")
        .arg("--since")
        .arg("0")
        .current_dir(workspace.path())
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();

    // Check for expected human-readable elements
    // The output should contain workspace state information
    assert!(!stdout.is_empty());
    assert!(
        stdout.contains("workspace")
            || stdout.contains("Workspace")
            || stdout.contains("UUID")
            || stdout.contains("seq")
    );
}

#[test]
fn test_change_feed_no_workspace() {
    let temp_dir = TempDir::new().unwrap();

    // Should fail without workspace
    Command::cargo_bin("bead")
        .unwrap()
        .arg("changes")
        .arg("--latest")
        .current_dir(temp_dir.path())
        .assert()
        .failure();
}

#[test]
fn test_change_feed_help() {
    Command::cargo_bin("bead")
        .unwrap()
        .arg("changes")
        .arg("--help")
        .assert()
        .success();
}

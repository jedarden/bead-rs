//! Integration tests for `bead create` command

use assert_cmd::Command;
use serial_test::serial;

#[test]
#[serial]
fn test_create_basic() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Create an issue
    let result = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Test Issue"])
        .assert()
        .success();

    let output = std::str::from_utf8(&result.get_output().stdout).unwrap();
    let issue_id = output.trim();

    // Verify ID format: <prefix>-<8 hex chars>
    assert!(issue_id.starts_with("test-"));
    assert_eq!(issue_id.len(), 13); // "test-" + 8 hex chars
}

#[test]
#[serial]
fn test_create_with_description() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Create an issue with description
    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "create",
            "--title",
            "Test Issue",
            "--description",
            "This is a test description",
        ])
        .assert()
        .success();
}

#[test]
#[serial]
fn test_create_with_priority() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Create an issue with priority
    Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Urgent Issue", "--priority", "0"])
        .assert()
        .success();
}

#[test]
#[serial]
fn test_create_with_labels() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Create an issue with labels
    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "create",
            "--title",
            "Labeled Issue",
            "--label",
            "bug",
            "--label",
            "urgent",
        ])
        .assert()
        .success();
}

#[test]
#[serial]
fn test_create_without_workspace() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Try to create without workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Test Issue"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("No workspace found"));
}

#[test]
#[serial]
fn test_create_invalid_priority() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Create with invalid priority
    Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Test Issue", "--priority", "10"])
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "Priority must be between 0 and 4",
        ));
}

#[test]
#[serial]
fn test_create_empty_title() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Create with empty title
    Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", ""])
        .assert()
        .failure()
        .stderr(predicates::str::contains("Title cannot be empty"));
}

#[test]
#[serial]
fn test_create_failure_rolls_back_issue_and_created_event() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // A duplicate label violates the labels primary key mid-transaction, after
    // both the issue row and its "created" audit event have been inserted. The
    // whole transaction must roll back together: a surviving event would make
    // the live event sequence (the R013 dirtiness signal) report a change that
    // never committed.
    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "create",
            "--title",
            "Rolled Back",
            "--label",
            "dup",
            "--label",
            "dup",
        ])
        .assert()
        .failure();

    // Neither the issue nor its event survived the rollback
    let conn = rusqlite::Connection::open(".beads/beads.db").unwrap();
    let issue_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM issues", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        issue_count, 0,
        "failed create must not leave the issue behind"
    );

    let event_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        event_count, 0,
        "created event must roll back with the issue it belongs to"
    );

    // The change feed still reports an empty sequence
    let output = Command::cargo_bin("bead")
        .unwrap()
        .args(["changes", "--latest"])
        .assert()
        .success();

    let stdout = std::str::from_utf8(&output.get_output().stdout).unwrap();
    assert!(stdout.contains("Latest cursor: 0"), "got: {}", stdout);
}

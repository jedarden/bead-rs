//! Integration tests for `bead create` command

use assert_cmd::Command;

#[test]
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

    // Verify ID format: <prefix>-<16 hex chars>
    assert!(issue_id.starts_with("test-"));
    assert_eq!(issue_id.len(), 21); // "test-" + 16 hex chars
}

#[test]
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

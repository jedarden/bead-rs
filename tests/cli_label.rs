//! Integration tests for label commands

use assert_cmd::Command;
use predicates::prelude::*;
use serial_test::serial;

#[test]
#[serial]
fn test_label_add_basic() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Create an issue
    Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Test Issue"])
        .assert()
        .success()
        .stdout(predicate::str::is_match("test-[a-f0-9]{8}").unwrap());

    let issue_id = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Another Issue"])
        .assert()
        .success()
        .stdout(predicate::str::is_match("test-[a-f0-9]{8}").unwrap());

    let issue_id = String::from_utf8(issue_id.get_output().clone().stdout)
        .unwrap()
        .trim()
        .to_string();

    // Add a label
    Command::cargo_bin("bead")
        .unwrap()
        .args(["label", "add", &issue_id, "--label", "bug"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Added label 'bug'"));

    // Add another label
    Command::cargo_bin("bead")
        .unwrap()
        .args(["label", "add", &issue_id, "--label", "urgent"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Added label 'urgent'"));
}

#[test]
#[serial]
fn test_label_add_idempotent() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Create an issue
    let issue_id = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Test Issue"])
        .assert()
        .success()
        .stdout(predicate::str::is_match("test-[a-f0-9]{8}").unwrap());

    let issue_id = String::from_utf8(issue_id.get_output().clone().stdout)
        .unwrap()
        .trim()
        .to_string();

    // Add a label twice
    Command::cargo_bin("bead")
        .unwrap()
        .args(["label", "add", &issue_id, "--label", "bug"])
        .assert()
        .success();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["label", "add", &issue_id, "--label", "bug"])
        .assert()
        .success(); // Should succeed idempotently
}

#[test]
#[serial]
fn test_label_add_nonexistent_issue() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Try to add label to nonexistent issue
    Command::cargo_bin("bead")
        .unwrap()
        .args(["label", "add", "nonexistent", "--label", "bug"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Issue nonexistent"));
}

#[test]
#[serial]
fn test_label_remove_basic() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Create an issue
    let issue_id = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Test Issue"])
        .assert()
        .success()
        .stdout(predicate::str::is_match("test-[a-f0-9]{8}").unwrap());

    let issue_id = String::from_utf8(issue_id.get_output().clone().stdout)
        .unwrap()
        .trim()
        .to_string();

    // Add labels
    Command::cargo_bin("bead")
        .unwrap()
        .args(["label", "add", &issue_id, "--label", "bug"])
        .assert()
        .success();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["label", "add", &issue_id, "--label", "urgent"])
        .assert()
        .success();

    // Remove one label
    Command::cargo_bin("bead")
        .unwrap()
        .args(["label", "remove", &issue_id, "--label", "bug"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Removed label 'bug'"));
}

#[test]
#[serial]
fn test_label_remove_idempotent() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Create an issue
    let issue_id = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Test Issue"])
        .assert()
        .success()
        .stdout(predicate::str::is_match("test-[a-f0-9]{8}").unwrap());

    let issue_id = String::from_utf8(issue_id.get_output().clone().stdout)
        .unwrap()
        .trim()
        .to_string();

    // Add a label
    Command::cargo_bin("bead")
        .unwrap()
        .args(["label", "add", &issue_id, "--label", "bug"])
        .assert()
        .success();

    // Remove it twice
    Command::cargo_bin("bead")
        .unwrap()
        .args(["label", "remove", &issue_id, "--label", "bug"])
        .assert()
        .success();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["label", "remove", &issue_id, "--label", "bug"])
        .assert()
        .success(); // Should succeed idempotently
}

#[test]
#[serial]
fn test_label_remove_nonexistent_issue() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Try to remove label from nonexistent issue
    Command::cargo_bin("bead")
        .unwrap()
        .args(["label", "remove", "nonexistent", "--label", "bug"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Issue nonexistent"));
}

#[test]
#[serial]
fn test_label_without_workspace() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Try to add label without workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["label", "add", "issue-1", "--label", "bug"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("No workspace found"));

    // Try to remove label without workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["label", "remove", "issue-1", "--label", "bug"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("No workspace found"));
}

//! Integration tests for `bead list` command

use assert_cmd::Command;
use serial_test::serial;

#[test]
#[serial]
fn test_list_empty_workspace() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // List issues (should be empty)
    Command::cargo_bin("bead")
        .unwrap()
        .args(["list"])
        .assert()
        .success()
        .stdout(predicates::str::contains("No issues found"));
}

#[test]
#[serial]
fn test_list_with_issues() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Create some issues
    Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "First Issue"])
        .assert()
        .success();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Second Issue"])
        .assert()
        .success();

    // List issues
    Command::cargo_bin("bead")
        .unwrap()
        .args(["list"])
        .assert()
        .success()
        .stdout(predicates::str::contains("First Issue"))
        .stdout(predicates::str::contains("Second Issue"));
}

#[test]
#[serial]
fn test_list_json() {
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
        .args(["create", "--title", "JSON Test"])
        .assert()
        .success();

    // List with JSON output
    Command::cargo_bin("bead")
        .unwrap()
        .args(["list", "--json"])
        .assert()
        .success()
        .stdout(predicates::str::contains("\"id\""))
        .stdout(predicates::str::contains("\"title\""))
        .stdout(predicates::str::contains("JSON Test"));
}

#[test]
#[serial]
fn test_list_with_limit() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Create multiple issues
    for i in 1..=5 {
        Command::cargo_bin("bead")
            .unwrap()
            .args(["create", "--title", &format!("Issue {}", i)])
            .assert()
            .success();
    }

    // List with limit
    Command::cargo_bin("bead")
        .unwrap()
        .args(["list", "--limit", "3"])
        .assert()
        .success();
}

#[test]
#[serial]
fn test_list_invalid_limit() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // List with invalid limit
    Command::cargo_bin("bead")
        .unwrap()
        .args(["list", "--limit", "1000000"])
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "Limit must be between 0 and 999999",
        ));
}

#[test]
#[serial]
fn test_list_invalid_comments() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // List with invalid comments option
    Command::cargo_bin("bead")
        .unwrap()
        .args(["list", "--comments", "invalid"])
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "Comments must be one of: none, unresolved, all",
        ));
}

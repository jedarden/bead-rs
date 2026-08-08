//! Integration tests for `bead show` command

use assert_cmd::Command;
use serial_test::serial;

#[test]
#[serial]
fn test_show_existing_issue() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Create an issue
    let create_result = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Show Test"])
        .assert()
        .success();

    let issue_id = std::str::from_utf8(&create_result.get_output().stdout)
        .unwrap()
        .trim();

    // Show the issue
    Command::cargo_bin("bead")
        .unwrap()
        .args(["show", issue_id])
        .assert()
        .success()
        .stdout(predicates::str::contains("Show Test"))
        .stdout(predicates::str::contains("ID:"))
        .stdout(predicates::str::contains("Status:"))
        .stdout(predicates::str::contains("Priority:"));
}

#[test]
#[serial]
fn test_show_json() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Create an issue
    let create_result = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "JSON Show Test"])
        .assert()
        .success();

    let issue_id = std::str::from_utf8(&create_result.get_output().stdout)
        .unwrap()
        .trim();

    // Show with JSON output
    Command::cargo_bin("bead")
        .unwrap()
        .args(["show", issue_id, "--json"])
        .assert()
        .success()
        .stdout(predicates::str::contains("["))
        .stdout(predicates::str::contains("\"id\""))
        .stdout(predicates::str::contains("\"title\""))
        .stdout(predicates::str::contains("JSON Show Test"));
}

#[test]
#[serial]
fn test_show_nonexistent_issue() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Try to show nonexistent issue
    Command::cargo_bin("bead")
        .unwrap()
        .args(["show", "test-nonexistent"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("Issue not found"));
}

#[test]
#[serial]
fn test_show_with_description() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Create an issue with description
    let create_result = Command::cargo_bin("bead")
        .unwrap()
        .args([
            "create",
            "--title",
            "Described Issue",
            "--description",
            "This is a detailed description",
        ])
        .assert()
        .success();

    let issue_id = std::str::from_utf8(&create_result.get_output().stdout)
        .unwrap()
        .trim();

    // Show the issue
    Command::cargo_bin("bead")
        .unwrap()
        .args(["show", issue_id])
        .assert()
        .success()
        .stdout(predicates::str::contains("This is a detailed description"));
}

#[test]
#[serial]
fn test_show_invalid_comments() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Create an issue
    let create_result = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Test"])
        .assert()
        .success();

    let issue_id = std::str::from_utf8(&create_result.get_output().stdout)
        .unwrap()
        .trim();

    // Try to show with invalid comments option
    Command::cargo_bin("bead")
        .unwrap()
        .args(["show", issue_id, "--comments", "invalid"])
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "Comments must be one of: none, unresolved, all",
        ));
}

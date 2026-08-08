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

#[test]
#[serial]
fn test_list_ready_excludes_blocked_issues() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Create three issues: A, B, and C
    let task_a_output = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Task A"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let task_a = String::from_utf8_lossy(&task_a_output).trim().to_string();

    let task_b_output = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Task B"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let task_b = String::from_utf8_lossy(&task_b_output).trim().to_string();

    let task_c_output = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Task C"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let task_c = String::from_utf8_lossy(&task_c_output).trim().to_string();

    // Add blocking dependency: Task B blocks Task C
    Command::cargo_bin("bead")
        .unwrap()
        .args(["dep", "add", &task_c, &task_b])
        .assert()
        .success();

    // List ready issues - should only include Task A and Task B (not Task C)
    let ready_output = Command::cargo_bin("bead")
        .unwrap()
        .args(["list", "--ready", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let ready_str = String::from_utf8_lossy(&ready_output);

    // Task A and Task B should be present, Task C should NOT be present
    assert!(ready_str.contains(&format!("\"id\":\"{}\"", task_a)));
    assert!(ready_str.contains(&format!("\"id\":\"{}\"", task_b)));
    assert!(!ready_str.contains(&format!("\"id\":\"{}\"", task_c)));
}

#[test]
#[serial]
fn test_list_ready_includes_after_blocker_closed() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Create two issues: A and B
    let task_a_output = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Task A"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let task_a = String::from_utf8_lossy(&task_a_output).trim().to_string();

    let task_b_output = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Task B"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let task_b = String::from_utf8_lossy(&task_b_output).trim().to_string();

    // Add blocking dependency: Task A blocks Task B
    Command::cargo_bin("bead")
        .unwrap()
        .args(["dep", "add", &task_b, &task_a])
        .assert()
        .success();

    // List ready issues - should only include Task A (not Task B)
    let ready_output = Command::cargo_bin("bead")
        .unwrap()
        .args(["list", "--ready", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let ready_str = String::from_utf8_lossy(&ready_output);

    // Task A should be present, Task B should NOT be ready
    assert!(ready_str.contains(&format!("\"id\":\"{}\"", task_a)));
    assert!(!ready_str.contains(&format!("\"id\":\"{}\"", task_b)));

    // Close Task A (the blocker)
    Command::cargo_bin("bead")
        .unwrap()
        .args(["close", &task_a, "--reason", "Completed"])
        .assert()
        .success();

    // Now Task B should be ready since its blocker is closed
    let ready_after_close = Command::cargo_bin("bead")
        .unwrap()
        .args(["list", "--ready", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let ready_after_str = String::from_utf8_lossy(&ready_after_close);

    // Task B should now be ready
    assert!(ready_after_str.contains(&format!("\"id\":\"{}\"", task_b)));
}

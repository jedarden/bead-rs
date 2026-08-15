//! Integration tests for dependency commands

use assert_cmd::Command;
use predicates::prelude::*;
use serial_test::serial;

#[test]
#[serial]
fn test_dep_add_basic() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Create issues
    let blocked_id = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Blocked Issue"])
        .assert()
        .success()
        .stdout(predicate::str::is_match("test-[a-f0-9]{8}").unwrap());

    let blocked_id = String::from_utf8(blocked_id.get_output().clone().stdout)
        .unwrap()
        .trim()
        .to_string();

    let blocker_id = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Blocker Issue"])
        .assert()
        .success()
        .stdout(predicate::str::is_match("test-[a-f0-9]{8}").unwrap());

    let blocker_id = String::from_utf8(blocker_id.get_output().clone().stdout)
        .unwrap()
        .trim()
        .to_string();

    // Add a dependency
    Command::cargo_bin("bead")
        .unwrap()
        .args(["dep", "add", &blocked_id, &blocker_id])
        .assert()
        .success()
        .stdout(predicate::str::contains("blocked by"));
}

#[test]
#[serial]
fn test_dep_add_with_kind() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Create issues
    let issue1_id = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Issue 1"])
        .assert()
        .success()
        .stdout(predicate::str::is_match("test-[a-f0-9]{8}").unwrap());

    let issue1_id = String::from_utf8(issue1_id.get_output().clone().stdout)
        .unwrap()
        .trim()
        .to_string();

    let issue2_id = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Issue 2"])
        .assert()
        .success()
        .stdout(predicate::str::is_match("test-[a-f0-9]{8}").unwrap());

    let issue2_id = String::from_utf8(issue2_id.get_output().clone().stdout)
        .unwrap()
        .trim()
        .to_string();

    // Add a relates_to dependency
    Command::cargo_bin("bead")
        .unwrap()
        .args(["dep", "add", &issue1_id, &issue2_id, "--kind", "relates_to"])
        .assert()
        .success()
        .stdout(predicate::str::contains("related to"));
}

#[test]
#[serial]
fn test_dep_add_invalid_kind_returns_exit_code_4() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "dep",
            "add",
            "test-blocked",
            "test-blocker",
            "--kind",
            "parent-child",
        ])
        .assert()
        .code(4)
        .stderr(predicate::str::contains("parent-child"));
}

#[test]
#[serial]
fn test_dep_add_idempotent() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Create issues
    let blocked_id = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Blocked Issue"])
        .assert()
        .success()
        .stdout(predicate::str::is_match("test-[a-f0-9]{8}").unwrap());

    let blocked_id = String::from_utf8(blocked_id.get_output().clone().stdout)
        .unwrap()
        .trim()
        .to_string();

    let blocker_id = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Blocker Issue"])
        .assert()
        .success()
        .stdout(predicate::str::is_match("test-[a-f0-9]{8}").unwrap());

    let blocker_id = String::from_utf8(blocker_id.get_output().clone().stdout)
        .unwrap()
        .trim()
        .to_string();

    // Add the same dependency twice
    Command::cargo_bin("bead")
        .unwrap()
        .args(["dep", "add", &blocked_id, &blocker_id])
        .assert()
        .success();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["dep", "add", &blocked_id, &blocker_id])
        .assert()
        .success(); // Should succeed idempotently
}

#[test]
#[serial]
fn test_dep_add_self_edge() {
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
        .args(["create", "--title", "Self Issue"])
        .assert()
        .success()
        .stdout(predicate::str::is_match("test-[a-f0-9]{8}").unwrap());

    let issue_id = String::from_utf8(issue_id.get_output().clone().stdout)
        .unwrap()
        .trim()
        .to_string();

    // Try to add self-edge
    Command::cargo_bin("bead")
        .unwrap()
        .args(["dep", "add", &issue_id, &issue_id])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Self-edge"));
}

#[test]
#[serial]
fn test_dep_add_creates_cycle() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Create issues
    let issue1_id = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Issue 1"])
        .assert()
        .success()
        .stdout(predicate::str::is_match("test-[a-f0-9]{8}").unwrap());

    let issue1_id = String::from_utf8(issue1_id.get_output().clone().stdout)
        .unwrap()
        .trim()
        .to_string();

    let issue2_id = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Issue 2"])
        .assert()
        .success()
        .stdout(predicate::str::is_match("test-[a-f0-9]{8}").unwrap());

    let issue2_id = String::from_utf8(issue2_id.get_output().clone().stdout)
        .unwrap()
        .trim()
        .to_string();

    let issue3_id = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Issue 3"])
        .assert()
        .success()
        .stdout(predicate::str::is_match("test-[a-f0-9]{8}").unwrap());

    let issue3_id = String::from_utf8(issue3_id.get_output().clone().stdout)
        .unwrap()
        .trim()
        .to_string();

    // Create chain: issue1 -> issue2 -> issue3
    Command::cargo_bin("bead")
        .unwrap()
        .args(["dep", "add", &issue2_id, &issue3_id])
        .assert()
        .success();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["dep", "add", &issue1_id, &issue2_id])
        .assert()
        .success();

    // Try to create cycle: issue3 -> issue1
    Command::cargo_bin("bead")
        .unwrap()
        .args(["dep", "add", &issue3_id, &issue1_id])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cycle"));
}

#[test]
#[serial]
fn test_relates_to_allows_cycles() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Create issues
    let issue1_id = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Issue 1"])
        .assert()
        .success()
        .stdout(predicate::str::is_match("test-[a-f0-9]{8}").unwrap());

    let issue1_id = String::from_utf8(issue1_id.get_output().clone().stdout)
        .unwrap()
        .trim()
        .to_string();

    let issue2_id = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Issue 2"])
        .assert()
        .success()
        .stdout(predicate::str::is_match("test-[a-f0-9]{8}").unwrap());

    let issue2_id = String::from_utf8(issue2_id.get_output().clone().stdout)
        .unwrap()
        .trim()
        .to_string();

    // Create cycle with relates_to: issue1 -> issue2 -> issue1
    Command::cargo_bin("bead")
        .unwrap()
        .args(["dep", "add", &issue1_id, &issue2_id, "--kind", "relates_to"])
        .assert()
        .success();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["dep", "add", &issue2_id, &issue1_id, "--kind", "relates_to"])
        .assert()
        .success(); // Should succeed - relates_to cycles are allowed
}

#[test]
#[serial]
fn test_dep_add_nonexistent_blocked() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Create blocker issue
    let blocker_id = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Blocker Issue"])
        .assert()
        .success()
        .stdout(predicate::str::is_match("test-[a-f0-9]{8}").unwrap());

    let blocker_id = String::from_utf8(blocker_id.get_output().clone().stdout)
        .unwrap()
        .trim()
        .to_string();

    // Try to add dependency with nonexistent blocked issue
    Command::cargo_bin("bead")
        .unwrap()
        .args(["dep", "add", "nonexistent", &blocker_id])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Issue nonexistent"));
}

#[test]
#[serial]
fn test_dep_add_nonexistent_blocker() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Create blocked issue
    let blocked_id = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Blocked Issue"])
        .assert()
        .success()
        .stdout(predicate::str::is_match("test-[a-f0-9]{8}").unwrap());

    let blocked_id = String::from_utf8(blocked_id.get_output().clone().stdout)
        .unwrap()
        .trim()
        .to_string();

    // Try to add dependency with nonexistent blocker issue
    Command::cargo_bin("bead")
        .unwrap()
        .args(["dep", "add", &blocked_id, "nonexistent"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Issue nonexistent"));
}

#[test]
#[serial]
fn test_dep_remove_basic() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Create issues
    let blocked_id = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Blocked Issue"])
        .assert()
        .success()
        .stdout(predicate::str::is_match("test-[a-f0-9]{8}").unwrap());

    let blocked_id = String::from_utf8(blocked_id.get_output().clone().stdout)
        .unwrap()
        .trim()
        .to_string();

    let blocker_id = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Blocker Issue"])
        .assert()
        .success()
        .stdout(predicate::str::is_match("test-[a-f0-9]{8}").unwrap());

    let blocker_id = String::from_utf8(blocker_id.get_output().clone().stdout)
        .unwrap()
        .trim()
        .to_string();

    // Add a dependency
    Command::cargo_bin("bead")
        .unwrap()
        .args(["dep", "add", &blocked_id, &blocker_id])
        .assert()
        .success();

    // Remove the dependency
    Command::cargo_bin("bead")
        .unwrap()
        .args(["dep", "remove", &blocked_id, &blocker_id])
        .assert()
        .success()
        .stdout(predicate::str::contains("Removed dependency"));
}

#[test]
#[serial]
fn test_dep_remove_with_kind() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Create issues
    let issue1_id = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Issue 1"])
        .assert()
        .success()
        .stdout(predicate::str::is_match("test-[a-f0-9]{8}").unwrap());

    let issue1_id = String::from_utf8(issue1_id.get_output().clone().stdout)
        .unwrap()
        .trim()
        .to_string();

    let issue2_id = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Issue 2"])
        .assert()
        .success()
        .stdout(predicate::str::is_match("test-[a-f0-9]{8}").unwrap());

    let issue2_id = String::from_utf8(issue2_id.get_output().clone().stdout)
        .unwrap()
        .trim()
        .to_string();

    // Add both blocks and relates_to dependencies
    Command::cargo_bin("bead")
        .unwrap()
        .args(["dep", "add", &issue1_id, &issue2_id])
        .assert()
        .success();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["dep", "add", &issue1_id, &issue2_id, "--kind", "relates_to"])
        .assert()
        .success();

    // Remove only the blocks dependency
    Command::cargo_bin("bead")
        .unwrap()
        .args(["dep", "remove", &issue1_id, &issue2_id, "--kind", "blocks"])
        .assert()
        .success();
}

#[test]
#[serial]
fn test_dep_remove_without_kind() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Create issues
    let issue1_id = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Issue 1"])
        .assert()
        .success()
        .stdout(predicate::str::is_match("test-[a-f0-9]{8}").unwrap());

    let issue1_id = String::from_utf8(issue1_id.get_output().clone().stdout)
        .unwrap()
        .trim()
        .to_string();

    let issue2_id = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Issue 2"])
        .assert()
        .success()
        .stdout(predicate::str::is_match("test-[a-f0-9]{8}").unwrap());

    let issue2_id = String::from_utf8(issue2_id.get_output().clone().stdout)
        .unwrap()
        .trim()
        .to_string();

    // Add both blocks and relates_to dependencies
    Command::cargo_bin("bead")
        .unwrap()
        .args(["dep", "add", &issue1_id, &issue2_id])
        .assert()
        .success();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["dep", "add", &issue1_id, &issue2_id, "--kind", "relates_to"])
        .assert()
        .success();

    // Remove all dependencies between these issues
    Command::cargo_bin("bead")
        .unwrap()
        .args(["dep", "remove", &issue1_id, &issue2_id])
        .assert()
        .success();
}

#[test]
#[serial]
fn test_dep_remove_idempotent() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Create issues
    let blocked_id = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Blocked Issue"])
        .assert()
        .success()
        .stdout(predicate::str::is_match("test-[a-f0-9]{8}").unwrap());

    let blocked_id = String::from_utf8(blocked_id.get_output().clone().stdout)
        .unwrap()
        .trim()
        .to_string();

    let blocker_id = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Blocker Issue"])
        .assert()
        .success()
        .stdout(predicate::str::is_match("test-[a-f0-9]{8}").unwrap());

    let blocker_id = String::from_utf8(blocker_id.get_output().clone().stdout)
        .unwrap()
        .trim()
        .to_string();

    // Add a dependency
    Command::cargo_bin("bead")
        .unwrap()
        .args(["dep", "add", &blocked_id, &blocker_id])
        .assert()
        .success();

    // Remove it twice
    Command::cargo_bin("bead")
        .unwrap()
        .args(["dep", "remove", &blocked_id, &blocker_id])
        .assert()
        .success();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["dep", "remove", &blocked_id, &blocker_id])
        .assert()
        .success(); // Should succeed idempotently
}

#[test]
#[serial]
fn test_dep_without_workspace() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Try to add dependency without workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["dep", "add", "issue-1", "issue-2"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("No workspace found"));

    // Try to remove dependency without workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["dep", "remove", "issue-1", "issue-2"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("No workspace found"));
}

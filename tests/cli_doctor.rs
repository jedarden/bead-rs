//! Integration tests for `bead doctor` command

use assert_cmd::Command;
use serial_test::serial;

#[test]
#[serial]
fn test_doctor_no_workspace() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Doctor should fail when there's no workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["doctor"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("No workspace found"));
}

#[test]
#[serial]
fn test_doctor_basic() {
    let temp = tempfile::tempdir().unwrap();
    let temp_dir = temp.path();
    let original_home = std::env::var("HOME").ok();

    unsafe { std::env::set_var("HOME", temp_dir); }
    std::env::set_current_dir(temp_dir).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Run doctor diagnostics
    Command::cargo_bin("bead")
        .unwrap()
        .args(["doctor"])
        .assert()
        .success()
        .stderr(predicates::str::contains("OK"))
        .stderr(predicates::str::contains("workspace_config"))
        .stderr(predicates::str::contains("database_integrity"))
        .stderr(predicates::str::contains("checkpoint_freshness")) // R016: checkpoint_state replaced with checkpoint_freshness
        .stderr(predicates::str::contains("temporary_files"));

    // Cleanup
    if let Some(home) = original_home {
        unsafe { std::env::set_var("HOME", home); }
    } else {
        unsafe { std::env::remove_var("HOME"); }
    }
}

#[test]
#[serial]
fn test_doctor_with_dirty_checkpoint() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Create an issue to make checkpoint dirty
    Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Test Issue"])
        .assert()
        .success();

    // Run doctor diagnostics - should show checkpoint info
    Command::cargo_bin("bead")
        .unwrap()
        .args(["doctor"])
        .assert()
        .success()
        .stderr(predicates::str::contains("checkpoint_freshness")); // R016: checkpoint_state replaced with checkpoint_freshness
}

#[test]
#[serial]
fn test_doctor_repair_no_repairs_needed() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Run doctor with repair flag when no repairs needed
    Command::cargo_bin("bead")
        .unwrap()
        .args(["doctor", "--repair"])
        .assert()
        .success()
        .stderr(predicates::str::contains("Attempting repairs"))
        .stderr(predicates::str::contains("No repairs needed"));
}

#[test]
#[serial]
fn test_doctor_repair_temp_files() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    std::env::set_current_dir(root).unwrap();

    // Save original directory to restore later
    let original_dir = std::env::current_dir().unwrap().canonicalize().unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Create a temporary file in .beads directory
    let temp_file = root.join(".beads/test.tmp");
    std::fs::write(&temp_file, "test content").unwrap();

    // Run doctor without repair - should warn about temp files
    Command::cargo_bin("bead")
        .unwrap()
        .args(["doctor"])
        .assert()
        .success()
        .stderr(predicates::str::contains("WARN"))
        .stderr(predicates::str::contains("temporary_files"));

    // Run doctor with repair - should clean up temp file
    Command::cargo_bin("bead")
        .unwrap()
        .args(["doctor", "--repair"])
        .assert()
        .success()
        .stderr(predicates::str::contains("FIXED"))
        .stderr(predicates::str::contains("removed_temp_file"));

    // Verify temp file was removed
    assert!(!temp_file.exists());

    // Restore original directory before dropping temp
    std::env::set_current_dir(original_dir).unwrap();
}

#[test]
#[serial]
fn test_doctor_after_flush() {
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
        .success();

    // Flush checkpoint
    Command::cargo_bin("bead")
        .unwrap()
        .args(["sync", "flush-only"])
        .assert()
        .success();

    // Run doctor diagnostics - should pass all checks
    Command::cargo_bin("bead")
        .unwrap()
        .args(["doctor"])
        .assert()
        .success()
        .stderr(predicates::str::contains("OK"));
}

#[test]
#[serial]
fn test_doctor_rejects_inconsistent_close_metadata() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();
    let output = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Corrupt close metadata probe"])
        .output()
        .unwrap();
    let id = String::from_utf8(output.stdout).unwrap();

    let conn = rusqlite::Connection::open(temp.path().join(".beads/beads.db")).unwrap();
    conn.execute(
        "UPDATE issues SET base_status = 'closed', closed_at = NULL, close_reason = NULL WHERE id = ?1",
        [id.trim()],
    )
    .unwrap();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["doctor"])
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "inconsistent closed status metadata",
        ));
}

#[test]
#[serial]
fn test_doctor_reports_open_issue_held_by_assignee() {
    let temp = tempfile::tempdir().unwrap();
    let temp_dir = temp.path();
    let original_home = std::env::var("HOME").ok();

    unsafe { std::env::set_var("HOME", temp_dir); }
    std::env::set_current_dir(temp_dir).unwrap();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["init"])
        .assert()
        .success();
    let output = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Held by an assignee"])
        .output()
        .unwrap();
    let id = String::from_utf8(output.stdout).unwrap();
    let id = id.trim();

    // A clean workspace reports the frontier as healthy.
    Command::cargo_bin("bead")
        .unwrap()
        .args(["doctor"])
        .assert()
        .stderr(predicates::str::contains("Ready frontier OK"));

    // Assigning an issue while it stays open takes it off the ready frontier
    // without changing its status, which is the shape doctor must surface.
    Command::cargo_bin("bead")
        .unwrap()
        .args(["update", id, "--assignee", "worker-1"])
        .assert()
        .success();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["doctor"])
        .assert()
        .stderr(predicates::str::contains("Ready frontier warning"))
        .stderr(predicates::str::contains(id))
        .stderr(predicates::str::contains("--clear-assignee"));

    // Clearing the assignee returns it to the frontier and silences the warning.
    Command::cargo_bin("bead")
        .unwrap()
        .args(["update", id, "--clear-assignee"])
        .assert()
        .success();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["doctor"])
        .assert()
        .stderr(predicates::str::contains("Ready frontier OK"));
}

#[test]
#[serial]
fn test_ready_frontier_emits_r001_reason_codes() {
    let temp = tempfile::tempdir().unwrap();
    let temp_dir = temp.path();

    // Store original directory and HOME to restore later
    let original_dir = std::env::current_dir().unwrap();
    let original_home = std::env::var("HOME").ok();

    // Set HOME to the temp directory to avoid interfering with user's actual workspace
    unsafe { std::env::set_var("HOME", temp_dir); }

    // Change to the temporary directory
    std::env::set_current_dir(temp_dir).unwrap();

    // Ensure we're in a clean directory without any existing .beads
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test-r035"])
        .assert()
        .success();

    // Create multiple held issues
    Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "First held issue"])
        .assert()
        .success();
    let output = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Second held issue"])
        .output()
        .unwrap();
    let id2 = String::from_utf8(output.stdout).unwrap();
    let id2 = id2.trim();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Third held issue"])
        .assert()
        .success();

    // Assign all issues to take them off the ready frontier
    Command::cargo_bin("bead")
        .unwrap()
        .args(["list", "--json"])
        .assert()
        .success();
    let list_output = Command::cargo_bin("bead")
        .unwrap()
        .args(["list", "--json"])
        .output()
        .unwrap();
    let list_json: serde_json::Value = serde_json::from_slice(&list_output.stdout).unwrap();

    if let Some(issues) = list_json.as_array() {
        for issue in issues {
            if let Some(id) = issue.get("id").and_then(|i| i.as_str()) {
                Command::cargo_bin("bead")
                    .unwrap()
                    .args(["update", id, "--assignee", "worker-1"])
                    .assert()
                    .success();
            }
        }
    }

    // Run doctor with JSON output and validate structured reason codes
    let output = Command::cargo_bin("bead")
        .unwrap()
        .args(["doctor", "--json"])
        .output()
        .unwrap();

    assert!(output.status.success());

    let doctor_json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let checks = doctor_json
        .get("checks")
        .and_then(|c| c.as_array())
        .unwrap();

    // Find the ready_frontier check
    let frontier_check = checks
        .iter()
        .find(|c| c.get("name").and_then(|n| n.as_str()) == Some("ready_frontier"))
        .expect("ready_frontier check should be present");

    // Validate the structured output
    let details = frontier_check.get("details").unwrap().as_object().unwrap();

    // Check that held_ids is a machine-readable array, not embedded in prose
    let held_ids = details.get("held_ids").and_then(|h| h.as_array()).unwrap();
    assert!(held_ids.len() >= 2, "Should have at least 2 held IDs");

    // Verify the specific ID we tracked is in the list
    assert!(
        held_ids.iter().any(|id| id.as_str() == Some(id2)),
        "Created issue ID should be in held_ids list"
    );

    // Check that held_count matches the array length
    let held_count = details.get("held_count").and_then(|c| c.as_u64()).unwrap();
    assert_eq!(
        held_count as usize,
        held_ids.len(),
        "held_count should match held_ids length"
    );

    // Validate R001 reason codes are present
    let reason_codes = details
        .get("reason_codes")
        .and_then(|r| r.as_array())
        .unwrap();
    assert!(
        reason_codes.len() >= 1,
        "Should have at least one reason code"
    );

    // Check for the specific R035 reason code
    assert!(
        reason_codes
            .iter()
            .any(|rc| rc.as_str() == Some("open_issue_held_by_assignee")),
        "Should include open_issue_held_by_assignee reason code"
    );

    // Validate remedy is provided
    let remedy = details.get("remedy").and_then(|r| r.as_str()).unwrap();
    assert!(
        remedy.contains("--clear-assignee"),
        "Remedy should mention --clear-assignee"
    );

    // Validate human-readable message still contains sample
    let message = frontier_check
        .get("message")
        .and_then(|m| m.as_str())
        .unwrap();
    assert!(
        message.contains("open issue(s) are assigned"),
        "Message should explain the condition in prose"
    );

    // Cleanup: restore original HOME and directory
    if let Some(home) = original_home {
        unsafe { std::env::set_var("HOME", home); }
    } else {
        unsafe { std::env::remove_var("HOME"); }
    }
    std::env::set_current_dir(original_dir).unwrap();
}

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
    std::env::set_current_dir(temp.path()).unwrap();

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
        .stderr(predicates::str::contains("checkpoint_freshness"))  // R016: checkpoint_state replaced with checkpoint_freshness
        .stderr(predicates::str::contains("temporary_files"));
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
        .stderr(predicates::str::contains("checkpoint_freshness"));  // R016: checkpoint_state replaced with checkpoint_freshness
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

//! R003 logical revision guards tests
//!
//! These tests verify the implementation of logical revision guards for
//! optimistic concurrency control, preventing silent lost updates across
//! concurrent operations.

use assert_cmd::Command;

fn create_workspace() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_bead"));
    cmd.current_dir(dir.path())
        .args(["init", "--skip-foreign-workspace"])
        .assert()
        .success();
    dir
}

#[test]
fn test_revision_initialization() {
    let workspace = create_workspace();

    // Create an issue and verify it starts at revision 1
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_bead"));
    cmd.current_dir(workspace.path())
        .args(["create", "--title", "Test issue"])
        .assert()
        .success();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_bead"))
        .current_dir(workspace.path())
        .args(["list", "--json", "--limit", "1"])
        .output()
        .unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("\"revision\":1"));
}

#[test]
fn test_revision_increment_on_update() {
    let workspace = create_workspace();

    // Create an issue
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_bead"));
    let create_result = cmd
        .current_dir(workspace.path())
        .args(["create", "--title", "Test issue"])
        .output()
        .unwrap();

    let issue_id = String::from_utf8(create_result.stdout)
        .unwrap()
        .trim()
        .to_string();

    // Update the issue
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_bead"));
    cmd.current_dir(workspace.path())
        .args(["update", &issue_id, "--status", "in_progress"])
        .assert()
        .success();

    // Verify revision is now 2
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_bead"))
        .current_dir(workspace.path())
        .args(["show", &issue_id, "--json"])
        .output()
        .unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("\"revision\":2"));
}

#[test]
fn test_revision_increment_on_close() {
    let workspace = create_workspace();

    // Create an issue
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_bead"));
    let create_result = cmd
        .current_dir(workspace.path())
        .args(["create", "--title", "Test issue"])
        .output()
        .unwrap();

    let issue_id = String::from_utf8(create_result.stdout)
        .unwrap()
        .trim()
        .to_string();

    // Close the issue
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_bead"));
    cmd.current_dir(workspace.path())
        .args(["close", &issue_id, "--reason", "Done"])
        .assert()
        .success();

    // Verify revision is now 2
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_bead"))
        .current_dir(workspace.path())
        .args(["show", &issue_id, "--json"])
        .output()
        .unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("\"revision\":2"));
}

#[test]
fn test_revision_increment_on_reopen() {
    let workspace = create_workspace();

    // Create and close an issue
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_bead"));
    let create_result = cmd
        .current_dir(workspace.path())
        .args(["create", "--title", "Test issue"])
        .output()
        .unwrap();

    let issue_id = String::from_utf8(create_result.stdout)
        .unwrap()
        .trim()
        .to_string();

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_bead"));
    cmd.current_dir(workspace.path())
        .args(["close", &issue_id, "--reason", "Done"])
        .assert()
        .success();

    // Reopen the issue
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_bead"));
    cmd.current_dir(workspace.path())
        .args(["reopen", &issue_id])
        .assert()
        .success();

    // Verify revision is now 3 (create->close->reopen)
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_bead"))
        .current_dir(workspace.path())
        .args(["show", &issue_id, "--json"])
        .output()
        .unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("\"revision\":3"));
}

#[test]
fn test_revision_increment_on_release() {
    let workspace = create_workspace();

    // Create and claim an issue
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_bead"));
    let create_result = cmd
        .current_dir(workspace.path())
        .args(["create", "--title", "Test issue"])
        .output()
        .unwrap();

    let issue_id = String::from_utf8(create_result.stdout)
        .unwrap()
        .trim()
        .to_string();

    // Claim the issue (moves to in_progress and assigns). The claim's epoch
    // comes back in the --json projection and is the credential the release
    // below must present now that release is a claimant-owned mutation.
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_bead"));
    let claim_result = cmd
        .current_dir(workspace.path())
        .args(["claim", "--assignee", "worker-1", "--json"])
        .output()
        .unwrap();
    let claim: serde_json::Value = serde_json::from_slice(&claim_result.stdout).unwrap();
    let epoch = claim["claim_epoch"].as_i64().unwrap().to_string();

    // Release the issue with the claim's epoch credential
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_bead"));
    cmd.current_dir(workspace.path())
        .args(["release", &issue_id, "--fencing-token", &epoch])
        .assert()
        .success();

    // Verify revision is now 3 (create->assign->release)
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_bead"))
        .current_dir(workspace.path())
        .args(["show", &issue_id, "--json"])
        .output()
        .unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("\"revision\":3"));
}

#[test]
fn test_revision_guard_success() {
    let workspace = create_workspace();

    // Create an issue
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_bead"));
    let create_result = cmd
        .current_dir(workspace.path())
        .args(["create", "--title", "Test issue"])
        .output()
        .unwrap();

    let issue_id = String::from_utf8(create_result.stdout)
        .unwrap()
        .trim()
        .to_string();

    // Update with correct revision guard
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_bead"));
    cmd.current_dir(workspace.path())
        .args([
            "update",
            &issue_id,
            "--status",
            "in_progress",
            "--if-revision",
            "1",
        ])
        .assert()
        .success();
}

#[test]
fn test_revision_guard_conflict() {
    let workspace = create_workspace();

    // Create an issue
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_bead"));
    let create_result = cmd
        .current_dir(workspace.path())
        .args(["create", "--title", "Test issue"])
        .output()
        .unwrap();

    let issue_id = String::from_utf8(create_result.stdout)
        .unwrap()
        .trim()
        .to_string();

    // First update to revision 2
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_bead"));
    cmd.current_dir(workspace.path())
        .args(["update", &issue_id, "--status", "in_progress"])
        .assert()
        .success();

    // Try to update with old revision guard (should fail)
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_bead"));
    cmd.current_dir(workspace.path())
        .args([
            "update",
            &issue_id,
            "--status",
            "deferred",
            "--if-revision",
            "1",
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("Revision mismatch"));
}

#[test]
fn test_revision_guard_on_close() {
    let workspace = create_workspace();

    // Create an issue
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_bead"));
    let create_result = cmd
        .current_dir(workspace.path())
        .args(["create", "--title", "Test issue"])
        .output()
        .unwrap();

    let issue_id = String::from_utf8(create_result.stdout)
        .unwrap()
        .trim()
        .to_string();

    // Close with correct revision guard
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_bead"));
    cmd.current_dir(workspace.path())
        .args(["close", &issue_id, "--reason", "Done", "--if-revision", "1"])
        .assert()
        .success();
}

#[test]
fn test_revision_guard_on_close_conflict() {
    let workspace = create_workspace();

    // Create and update an issue to increment revision
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_bead"));
    let create_result = cmd
        .current_dir(workspace.path())
        .args(["create", "--title", "Test issue"])
        .output()
        .unwrap();

    let issue_id = String::from_utf8(create_result.stdout)
        .unwrap()
        .trim()
        .to_string();

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_bead"));
    cmd.current_dir(workspace.path())
        .args(["update", &issue_id, "--assignee", "worker"])
        .assert()
        .success();

    // Try to close with old revision guard (should fail)
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_bead"));
    cmd.current_dir(workspace.path())
        .args(["close", &issue_id, "--reason", "Done", "--if-revision", "1"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("Revision mismatch"));
}

#[test]
fn test_revision_guard_on_reopen() {
    let workspace = create_workspace();

    // Create and close an issue
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_bead"));
    let create_result = cmd
        .current_dir(workspace.path())
        .args(["create", "--title", "Test issue"])
        .output()
        .unwrap();

    let issue_id = String::from_utf8(create_result.stdout)
        .unwrap()
        .trim()
        .to_string();

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_bead"));
    cmd.current_dir(workspace.path())
        .args(["close", &issue_id, "--reason", "Done"])
        .assert()
        .success();

    // Reopen with correct revision guard (revision 2 after close)
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_bead"));
    cmd.current_dir(workspace.path())
        .args(["reopen", &issue_id, "--if-revision", "2"])
        .assert()
        .success();
}

#[test]
fn test_revision_guard_on_release() {
    let workspace = create_workspace();

    // Create and assign an issue
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_bead"));
    let create_result = cmd
        .current_dir(workspace.path())
        .args(["create", "--title", "Test issue"])
        .output()
        .unwrap();

    let issue_id = String::from_utf8(create_result.stdout)
        .unwrap()
        .trim()
        .to_string();

    // Claim the issue (moves to in_progress and assigns)
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_bead"));
    let claim_result = cmd
        .current_dir(workspace.path())
        .args(["claim", "--assignee", "worker", "--json"])
        .output()
        .unwrap();
    let claim: serde_json::Value = serde_json::from_slice(&claim_result.stdout).unwrap();
    let epoch = claim["claim_epoch"].as_i64().unwrap().to_string();

    // Release with correct revision guard (revision 2 after claim) and the
    // claim's epoch credential
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_bead"));
    cmd.current_dir(workspace.path())
        .args([
            "release",
            &issue_id,
            "--if-revision",
            "2",
            "--fencing-token",
            &epoch,
        ])
        .assert()
        .success();
}

#[test]
fn test_capabilities_report_revision_support() {
    let workspace = create_workspace();

    // Check capabilities include logical revision support
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_bead"))
        .current_dir(workspace.path())
        .args(["capabilities"])
        .output()
        .unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("\"logical_revision\": true"));
}

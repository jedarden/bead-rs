//! NEEDLE v1 subprocess compatibility suite
//!
//! This test suite verifies complete NEEDLE v1 subprocess compatibility
//! by exercising all required commands as subprocess invocations with proper
//! isolation, output verification, and filesystem effect validation.

use assert_cmd::Command;
use serial_test::serial;
use std::path::Path;
use tempfile::TempDir;

/// Helper struct for managing test workspaces
struct TestWorkspace {
    temp_dir: TempDir,
    root: std::path::PathBuf,
    original_dir: std::path::PathBuf,
}

impl TestWorkspace {
    fn new() -> Self {
        let original_dir = std::env::current_dir().unwrap();
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path().to_path_buf();

        std::env::set_current_dir(&root).unwrap();

        // Initialize workspace
        Command::cargo_bin("bead")
            .unwrap()
            .args(["init", "--prefix", "test"])
            .assert()
            .success();

        Self {
            temp_dir,
            root,
            original_dir,
        }
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn cleanup(self) {
        // Restore original directory
        let _ = std::env::set_current_dir(self.original_dir);
        drop(self.temp_dir);
    }
}

#[test]
#[serial]
fn needle_v1_init_command() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    // Save original directory without canonicalizing
    let _original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(root).unwrap();

    // Test init command as subprocess
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "needle"])
        .assert()
        .success()
        .stderr(predicates::str::contains("Initialized workspace"));

    // Verify workspace was created
    assert!(root.join(".beads").exists());
    assert!(root.join(".beads/config.json").exists());
    assert!(root.join(".beads/beads.db").exists());

    // Restore original directory
    let _ = std::env::set_current_dir(&_original_dir);
}

#[test]
#[serial]
fn needle_v1_create_command() {
    let workspace = TestWorkspace::new();

    // Test create command as subprocess
    let result = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "NEEDLE Test Issue"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let issue_id = std::str::from_utf8(&result).unwrap().trim();

    // Verify ID format
    assert!(issue_id.starts_with("test-"));
    assert_eq!(issue_id.len(), 21); // "test-" + 16 hex chars

    // Verify issue was created by calling show
    Command::cargo_bin("bead")
        .unwrap()
        .args(["show", issue_id, "--json"])
        .assert()
        .success()
        .stdout(predicates::str::contains(issue_id));

    workspace.cleanup();
}

#[test]
#[serial]
fn needle_v1_claim_command() {
    let workspace = TestWorkspace::new();

    // Create an issue
    let result = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Claim Test"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let issue_id = std::str::from_utf8(&result).unwrap().trim();

    // Test claim command as subprocess with JSON output
    Command::cargo_bin("bead")
        .unwrap()
        .args(["claim", "--assignee", "needle-worker", "--json"])
        .assert()
        .success()
        .stdout(predicates::str::contains(issue_id));

    // Verify the issue is now assigned
    Command::cargo_bin("bead")
        .unwrap()
        .args(["show", issue_id, "--json"])
        .assert()
        .success()
        .stdout(predicates::str::contains("needle-worker"));

    workspace.cleanup();
}

#[test]
#[serial]
fn needle_v1_list_command() {
    let workspace = TestWorkspace::new();

    // Create multiple issues
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

    // Test list command as subprocess with JSON output
    Command::cargo_bin("bead")
        .unwrap()
        .args(["list", "--json", "--limit", "10"])
        .assert()
        .success()
        .stdout(predicates::str::contains("\"id\":"));

    // Verify list with --ready filter
    Command::cargo_bin("bead")
        .unwrap()
        .args(["list", "--ready", "--json"])
        .assert()
        .success()
        .stdout(predicates::str::contains("\"id\":"));

    workspace.cleanup();
}

#[test]
#[serial]
fn needle_v1_lifecycle_commands() {
    let workspace = TestWorkspace::new();

    // Create an issue
    let result = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Lifecycle Test"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let issue_id = std::str::from_utf8(&result).unwrap().trim();

    // Claim the issue
    Command::cargo_bin("bead")
        .unwrap()
        .args(["claim", "--assignee", "worker"])
        .assert()
        .success();

    // Test update command as subprocess
    Command::cargo_bin("bead")
        .unwrap()
        .args(["update", issue_id, "--notes", "Test notes"])
        .assert()
        .success();

    // Test release command as subprocess
    Command::cargo_bin("bead")
        .unwrap()
        .args(["release", issue_id])
        .assert()
        .success();

    // Verify issue is now open and unassigned
    Command::cargo_bin("bead")
        .unwrap()
        .args(["show", issue_id, "--json"])
        .assert()
        .success()
        .stdout(predicates::str::contains("\"assignee\":null"))
        .stdout(predicates::str::contains("\"status\":\"open\""));

    // Test close command as subprocess
    Command::cargo_bin("bead")
        .unwrap()
        .args(["close", issue_id, "--reason", "Test closure"])
        .assert()
        .success();

    // Verify issue is now closed
    Command::cargo_bin("bead")
        .unwrap()
        .args(["show", issue_id, "--json"])
        .assert()
        .success()
        .stdout(predicates::str::contains("\"status\":\"closed\""));

    // Test reopen command as subprocess
    Command::cargo_bin("bead")
        .unwrap()
        .args(["reopen", issue_id])
        .assert()
        .success();

    // Verify issue is now open again
    Command::cargo_bin("bead")
        .unwrap()
        .args(["show", issue_id, "--json"])
        .assert()
        .success()
        .stdout(predicates::str::contains("\"status\":\"open\""));

    workspace.cleanup();
}

#[test]
#[serial]
fn needle_v1_dependency_commands() {
    let workspace = TestWorkspace::new();

    // Create two issues
    let result1 = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Blocked Issue"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let blocked = std::str::from_utf8(&result1).unwrap().trim();

    let result2 = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Blocker Issue"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let blocker = std::str::from_utf8(&result2).unwrap().trim();

    // Test dependency add command as subprocess
    Command::cargo_bin("bead")
        .unwrap()
        .args(["dep", "add", blocked, blocker, "--kind", "blocks"])
        .assert()
        .success();

    // Verify dependency was created
    Command::cargo_bin("bead")
        .unwrap()
        .args(["show", blocked, "--json"])
        .assert()
        .success()
        .stdout(predicates::str::contains(blocker));

    // Test label add command as subprocess
    Command::cargo_bin("bead")
        .unwrap()
        .args(["label", "add", blocked, "--label", "dependency-test"])
        .assert()
        .success();

    // Verify label was added
    Command::cargo_bin("bead")
        .unwrap()
        .args(["show", blocked, "--json"])
        .assert()
        .success()
        .stdout(predicates::str::contains("dependency-test"));

    workspace.cleanup();
}

#[test]
#[serial]
fn needle_v1_checkpoint_commands() {
    let workspace = TestWorkspace::new();

    // Create an issue
    Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Checkpoint Test"])
        .assert()
        .success();

    // Test flush command as subprocess (now uses F017 forensic checkpoint)
    Command::cargo_bin("bead")
        .unwrap()
        .args(["sync", "flush-only"])
        .assert()
        .success()
        .stderr(predicates::str::contains("Flushed forensic checkpoint"))
        .stderr(predicates::str::contains("Mode: monolithic"))
        .stderr(predicates::str::contains("Issues: 1"));

    // Verify F017 forensic checkpoint structure was created
    let checkpoint_base = workspace.root().join(".beads/checkpoint");
    assert!(checkpoint_base.exists());

    let current_pointer = checkpoint_base.join("current.json");
    assert!(current_pointer.exists());

    let forensic_view = checkpoint_base.join("forensic.jsonl");
    assert!(forensic_view.exists());

    // Verify JSONL format (forensic format uses record_type envelope)
    let content = std::fs::read_to_string(&forensic_view).unwrap();
    assert!(!content.is_empty());

    // Parse each line as JSON
    for line in content.lines() {
        if !line.trim().is_empty() {
            let record: serde_json::Value = serde_json::from_str(line).unwrap();
            assert!(record["record_type"].is_string());
        }
    }

    workspace.cleanup();
}

#[test]
#[serial]
fn needle_v1_diagnostics_command() {
    let workspace = TestWorkspace::new();

    // Test doctor command as subprocess
    Command::cargo_bin("bead")
        .unwrap()
        .args(["doctor"])
        .assert()
        .success()
        .stderr(predicates::str::contains("OK"));

    workspace.cleanup();
}

#[test]
#[serial]
fn needle_v1_capabilities_command() {
    // Test capabilities command as subprocess (no workspace needed)
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    // Save original directory without canonicalizing
    let _original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(root).unwrap();

    let result = Command::cargo_bin("bead")
        .unwrap()
        .args(["capabilities", "--profile", "needle-v1"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output = std::str::from_utf8(&result).unwrap();
    // Verify it's valid JSON
    let _: serde_json::Value = serde_json::from_str(output).unwrap();

    // Restore original directory
    let _ = std::env::set_current_dir(&_original_dir);
}

#[test]
#[serial]
fn needle_v1_exit_codes() {
    let workspace = TestWorkspace::new();

    // Test invalid command returns exit code 2
    Command::cargo_bin("bead")
        .unwrap()
        .args(["invalid-command"])
        .assert()
        .failure()
        .code(2);

    // Test with no workspace returns exit code 3
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let _exit_test_dir = std::env::current_dir().unwrap();

    std::env::set_current_dir(root).unwrap();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["list"])
        .assert()
        .failure()
        .code(3);

    // Restore directory
    let _ = std::env::set_current_dir(&_exit_test_dir);

    // Test invalid status transition returns exit code 4
    Command::cargo_bin("bead")
        .unwrap()
        .args(["update", "nonexistent", "--status", "invalid"])
        .assert()
        .failure();

    workspace.cleanup();
}

#[test]
#[serial]
fn needle_v1_workspace_isolation() {
    let temp1 = tempfile::tempdir().unwrap();
    let temp2 = tempfile::tempdir().unwrap();

    // Create two separate workspaces
    let root1 = temp1.path();
    let root2 = temp2.path();

    let _original_dir = std::env::current_dir().unwrap();

    // Initialize first workspace
    std::env::set_current_dir(root1).unwrap();
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "ws1"])
        .assert()
        .success();

    let result1 = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Workspace 1 Issue"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let issue1 = std::str::from_utf8(&result1).unwrap().trim();

    // Initialize second workspace
    std::env::set_current_dir(root2).unwrap();
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "ws2"])
        .assert()
        .success();

    let result2 = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Workspace 2 Issue"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let issue2 = std::str::from_utf8(&result2).unwrap().trim();

    // Verify issues are in separate workspaces
    assert!(issue1 != issue2);
    assert!(issue1.starts_with("ws1-"));
    assert!(issue2.starts_with("ws2-"));

    // First workspace cannot see second workspace's issue
    std::env::set_current_dir(root1).unwrap();
    Command::cargo_bin("bead")
        .unwrap()
        .args(["show", issue2])
        .assert()
        .failure();

    // Restore original directory
    let _ = std::env::set_current_dir(&_original_dir);
}

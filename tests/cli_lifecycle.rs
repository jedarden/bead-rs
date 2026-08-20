//! Integration tests for lifecycle commands (update, release, close, reopen)

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn setup_workspace() -> TempDir {
    let temp_dir = TempDir::new().unwrap();
    let workspace_path = temp_dir.path();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .current_dir(workspace_path)
        .assert()
        .success();

    temp_dir
}

fn create_issue(workspace: &std::path::Path, title: &str) -> String {
    let output = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", title])
        .current_dir(workspace)
        .assert()
        .success();

    let issue_id = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    issue_id.trim().to_string()
}

#[test]
fn test_update_status() {
    let workspace = setup_workspace();
    let issue_id = create_issue(workspace.path(), "Test Issue");

    // Update status to in_progress
    Command::cargo_bin("bead")
        .unwrap()
        .args(["update", &issue_id, "--status", "in_progress"])
        .current_dir(workspace.path())
        .assert()
        .success()
        .stdout(predicate::str::contains(issue_id.clone()));

    // Verify the status was updated
    Command::cargo_bin("bead")
        .unwrap()
        .args(["show", &issue_id, "--json"])
        .current_dir(workspace.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("\"status\":\"in_progress\""));
}

#[test]
fn test_update_assignee() {
    let workspace = setup_workspace();
    let issue_id = create_issue(workspace.path(), "Test Issue");

    // Update assignee
    Command::cargo_bin("bead")
        .unwrap()
        .args(["update", &issue_id, "--assignee", "worker-1"])
        .current_dir(workspace.path())
        .assert()
        .success()
        .stdout(predicate::str::contains(issue_id.clone()));

    // Verify the assignee was updated
    Command::cargo_bin("bead")
        .unwrap()
        .args(["show", &issue_id, "--json"])
        .current_dir(workspace.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("\"assignee\":\"worker-1\""));
}

#[test]
fn test_update_notes() {
    let workspace = setup_workspace();
    let issue_id = create_issue(workspace.path(), "Test Issue");

    // Update notes
    Command::cargo_bin("bead")
        .unwrap()
        .args(["update", &issue_id, "--notes", "Some notes"])
        .current_dir(workspace.path())
        .assert()
        .success()
        .stdout(predicate::str::contains(issue_id.clone()));

    // Verify notes were saved by querying the database directly
    // (notes are not included in the basic JSON output)
    let db_path = workspace.path().join(".beads").join("beads.db");
    let conn = rusqlite::Connection::open(&db_path).unwrap();

    let notes: Option<String> = conn
        .query_row(
            "SELECT notes FROM issues WHERE id = ?",
            [&issue_id],
            |row| row.get(0),
        )
        .unwrap();

    assert_eq!(notes, Some("Some notes".to_string()));
}

#[test]
fn test_update_clear_assignee_on_open() {
    let workspace = setup_workspace();
    let issue_id = create_issue(workspace.path(), "Test Issue");

    // Assign the issue
    Command::cargo_bin("bead")
        .unwrap()
        .args(["update", &issue_id, "--assignee", "worker-1"])
        .current_dir(workspace.path())
        .assert()
        .success();

    // Clear assignee (should succeed for open issue)
    Command::cargo_bin("bead")
        .unwrap()
        .args(["update", &issue_id, "--clear-assignee"])
        .current_dir(workspace.path())
        .assert()
        .success();

    // Verify assignee was cleared
    Command::cargo_bin("bead")
        .unwrap()
        .args(["show", &issue_id, "--json"])
        .current_dir(workspace.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("\"assignee\":null"));
}

#[test]
fn test_update_clear_assignee_idempotent() {
    let workspace = setup_workspace();
    let issue_id = create_issue(workspace.path(), "Test Issue");

    // Clear assignee on already unassigned issue (should succeed idempotently)
    Command::cargo_bin("bead")
        .unwrap()
        .args(["update", &issue_id, "--clear-assignee"])
        .current_dir(workspace.path())
        .assert()
        .success();
}

#[test]
fn test_update_clear_assignee_conflicts_on_in_progress() {
    let workspace = setup_workspace();
    let issue_id = create_issue(workspace.path(), "Test Issue");

    // Set to in_progress
    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "update",
            &issue_id,
            "--status",
            "in_progress",
            "--assignee",
            "worker-1",
        ])
        .current_dir(workspace.path())
        .assert()
        .success();

    // Try to clear assignee on in_progress issue (should conflict)
    Command::cargo_bin("bead")
        .unwrap()
        .args(["update", &issue_id, "--clear-assignee"])
        .current_dir(workspace.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("Use 'release' command"));
}

#[test]
fn test_update_invalid_status_transition() {
    let workspace = setup_workspace();
    let issue_id = create_issue(workspace.path(), "Test Issue");

    // Set to closed
    Command::cargo_bin("bead")
        .unwrap()
        .args(["close", &issue_id, "--reason", "Done"])
        .current_dir(workspace.path())
        .assert()
        .success();

    // Try to update closed issue (should conflict)
    Command::cargo_bin("bead")
        .unwrap()
        .args(["update", &issue_id, "--status", "in_progress"])
        .current_dir(workspace.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("Invalid status transition"));
}

#[test]
fn test_update_both_assignee_and_clear_conflict() {
    let workspace = setup_workspace();
    let issue_id = create_issue(workspace.path(), "Test Issue");

    // Try to specify both --assignee and --clear-assignee (should conflict)
    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "update",
            &issue_id,
            "--assignee",
            "worker-1",
            "--clear-assignee",
        ])
        .current_dir(workspace.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("Cannot specify both"));
}

#[test]
fn test_release_in_progress() {
    let workspace = setup_workspace();
    let issue_id = create_issue(workspace.path(), "Test Issue");

    // Claim and set to in_progress
    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "update",
            &issue_id,
            "--status",
            "in_progress",
            "--assignee",
            "worker-1",
        ])
        .current_dir(workspace.path())
        .assert()
        .success();

    // Release the issue
    Command::cargo_bin("bead")
        .unwrap()
        .args(["release", &issue_id])
        .current_dir(workspace.path())
        .assert()
        .success()
        .stdout(predicate::str::contains(issue_id.clone()));

    // Verify status is open and assignee is cleared
    Command::cargo_bin("bead")
        .unwrap()
        .args(["show", &issue_id, "--json"])
        .current_dir(workspace.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("\"status\":\"open\""))
        .stdout(predicate::str::contains("\"assignee\":null"));
}

#[test]
fn test_release_open_unassigned_idempotent() {
    let workspace = setup_workspace();
    let issue_id = create_issue(workspace.path(), "Test Issue");

    // Release an open unassigned issue (should succeed idempotently)
    Command::cargo_bin("bead")
        .unwrap()
        .args(["release", &issue_id])
        .current_dir(workspace.path())
        .assert()
        .success();
}

#[test]
fn test_release_open_assigned_conflicts() {
    let workspace = setup_workspace();
    let issue_id = create_issue(workspace.path(), "Test Issue");

    // Assign the issue
    Command::cargo_bin("bead")
        .unwrap()
        .args(["update", &issue_id, "--assignee", "worker-1"])
        .current_dir(workspace.path())
        .assert()
        .success();

    // Try to release open assigned issue (should conflict)
    Command::cargo_bin("bead")
        .unwrap()
        .args(["release", &issue_id])
        .current_dir(workspace.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("Cannot release assigned"));
}

#[test]
fn test_release_deferred_conflicts() {
    let workspace = setup_workspace();
    let issue_id = create_issue(workspace.path(), "Test Issue");

    // Set to deferred
    Command::cargo_bin("bead")
        .unwrap()
        .args(["update", &issue_id, "--status", "deferred"])
        .current_dir(workspace.path())
        .assert()
        .success();

    // Try to release deferred issue (should conflict)
    Command::cargo_bin("bead")
        .unwrap()
        .args(["release", &issue_id])
        .current_dir(workspace.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("Cannot release issue"));
}

#[test]
fn test_close_open_issue() {
    let workspace = setup_workspace();
    let issue_id = create_issue(workspace.path(), "Test Issue");

    // Close the issue
    Command::cargo_bin("bead")
        .unwrap()
        .args(["close", &issue_id, "--reason", "Completed"])
        .current_dir(workspace.path())
        .assert()
        .success()
        .stdout(predicate::str::contains(issue_id.clone()));

    // Verify status is closed and reason is set
    Command::cargo_bin("bead")
        .unwrap()
        .args(["show", &issue_id, "--json"])
        .current_dir(workspace.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("\"status\":\"closed\""));
}

#[test]
fn test_close_in_progress_issue() {
    let workspace = setup_workspace();
    let issue_id = create_issue(workspace.path(), "Test Issue");

    // Set to in_progress
    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "update",
            &issue_id,
            "--status",
            "in_progress",
            "--assignee",
            "worker-1",
        ])
        .current_dir(workspace.path())
        .assert()
        .success();

    // Close the issue
    Command::cargo_bin("bead")
        .unwrap()
        .args(["close", &issue_id, "--reason", "Cancelled"])
        .current_dir(workspace.path())
        .assert()
        .success();

    // Verify status is closed and assignee is retained
    Command::cargo_bin("bead")
        .unwrap()
        .args(["show", &issue_id, "--json"])
        .current_dir(workspace.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("\"status\":\"closed\""))
        .stdout(predicate::str::contains("\"assignee\":\"worker-1\""));
}

#[test]
fn test_close_deferred_issue() {
    let workspace = setup_workspace();
    let issue_id = create_issue(workspace.path(), "Test Issue");

    // Set to deferred
    Command::cargo_bin("bead")
        .unwrap()
        .args(["update", &issue_id, "--status", "deferred"])
        .current_dir(workspace.path())
        .assert()
        .success();

    // Close the issue
    Command::cargo_bin("bead")
        .unwrap()
        .args(["close", &issue_id, "--reason", "No longer needed"])
        .current_dir(workspace.path())
        .assert()
        .success();
}

#[test]
fn test_close_idempotent_same_reason() {
    let workspace = setup_workspace();
    let issue_id = create_issue(workspace.path(), "Test Issue");

    // Close the issue
    Command::cargo_bin("bead")
        .unwrap()
        .args(["close", &issue_id, "--reason", "Completed"])
        .current_dir(workspace.path())
        .assert()
        .success();

    // Close again with same reason (should succeed idempotently)
    Command::cargo_bin("bead")
        .unwrap()
        .args(["close", &issue_id, "--reason", "Completed"])
        .current_dir(workspace.path())
        .assert()
        .success();
}

#[test]
fn test_close_conflicts_different_reason() {
    let workspace = setup_workspace();
    let issue_id = create_issue(workspace.path(), "Test Issue");

    // Close the issue
    Command::cargo_bin("bead")
        .unwrap()
        .args(["close", &issue_id, "--reason", "Completed"])
        .current_dir(workspace.path())
        .assert()
        .success();

    // Try to close again with different reason (should conflict)
    Command::cargo_bin("bead")
        .unwrap()
        .args(["close", &issue_id, "--reason", "Cancelled"])
        .current_dir(workspace.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "already closed with different reason",
        ));
}

#[test]
fn test_close_empty_reason() {
    let workspace = setup_workspace();
    let issue_id = create_issue(workspace.path(), "Test Issue");

    // Try to close with empty reason (should fail validation)
    Command::cargo_bin("bead")
        .unwrap()
        .args(["close", &issue_id, "--reason", "   "])
        .current_dir(workspace.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("Close reason cannot be empty"));
}

#[test]
fn test_reopen_closed_issue() {
    let workspace = setup_workspace();
    let issue_id = create_issue(workspace.path(), "Test Issue");

    // Close the issue
    Command::cargo_bin("bead")
        .unwrap()
        .args(["close", &issue_id, "--reason", "Completed"])
        .current_dir(workspace.path())
        .assert()
        .success();

    // Reopen the issue
    Command::cargo_bin("bead")
        .unwrap()
        .args(["reopen", &issue_id])
        .current_dir(workspace.path())
        .assert()
        .success()
        .stdout(predicate::str::contains(issue_id.clone()));

    // Verify status is open
    Command::cargo_bin("bead")
        .unwrap()
        .args(["show", &issue_id, "--json"])
        .current_dir(workspace.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("\"status\":\"open\""));
}

#[test]
fn test_reopen_retains_assignee() {
    let workspace = setup_workspace();
    let issue_id = create_issue(workspace.path(), "Test Issue");

    // Assign and close the issue
    Command::cargo_bin("bead")
        .unwrap()
        .args(["update", &issue_id, "--assignee", "worker-1"])
        .current_dir(workspace.path())
        .assert()
        .success();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["close", &issue_id, "--reason", "Completed"])
        .current_dir(workspace.path())
        .assert()
        .success();

    // Reopen the issue
    Command::cargo_bin("bead")
        .unwrap()
        .args(["reopen", &issue_id])
        .current_dir(workspace.path())
        .assert()
        .success();

    // Verify assignee is retained
    Command::cargo_bin("bead")
        .unwrap()
        .args(["show", &issue_id, "--json"])
        .current_dir(workspace.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("\"assignee\":\"worker-1\""));
}

#[test]
fn test_reopen_idempotent_on_open() {
    let workspace = setup_workspace();
    let issue_id = create_issue(workspace.path(), "Test Issue");

    // Reopen an already open issue (should succeed idempotently)
    Command::cargo_bin("bead")
        .unwrap()
        .args(["reopen", &issue_id])
        .current_dir(workspace.path())
        .assert()
        .success();
}

#[test]
fn test_reopen_in_progress_conflicts() {
    let workspace = setup_workspace();
    let issue_id = create_issue(workspace.path(), "Test Issue");

    // Set to in_progress
    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "update",
            &issue_id,
            "--status",
            "in_progress",
            "--assignee",
            "worker-1",
        ])
        .current_dir(workspace.path())
        .assert()
        .success();

    // Try to reopen in_progress issue (should conflict)
    Command::cargo_bin("bead")
        .unwrap()
        .args(["reopen", &issue_id])
        .current_dir(workspace.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "only closed issues can be reopened",
        ));
}

#[test]
fn test_reopen_warns_when_preserving_assignee() {
    let workspace = setup_workspace();
    let issue_id = create_issue(workspace.path(), "Test Issue");

    // Assign and close the issue
    Command::cargo_bin("bead")
        .unwrap()
        .args(["update", &issue_id, "--assignee", "worker-1"])
        .current_dir(workspace.path())
        .assert()
        .success();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["close", &issue_id, "--reason", "Completed"])
        .current_dir(workspace.path())
        .assert()
        .success();

    // Reopen should warn about the preserved assignee
    Command::cargo_bin("bead")
        .unwrap()
        .args(["reopen", &issue_id])
        .current_dir(workspace.path())
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "WARNING: This issue has an assignee and will not appear on the ready frontier",
        ))
        .stderr(predicate::str::contains(format!(
            "bead update {} --clear-assignee",
            issue_id
        )));

    // Verify assignee is still retained
    Command::cargo_bin("bead")
        .unwrap()
        .args(["show", &issue_id, "--json"])
        .current_dir(workspace.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("\"assignee\":\"worker-1\""));
}

#[test]
fn test_update_without_workspace() {
    let temp_dir = TempDir::new().unwrap();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["update", "test-id", "--status", "in_progress"])
        .current_dir(temp_dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("No workspace found"));
}

#[test]
fn test_release_without_workspace() {
    let temp_dir = TempDir::new().unwrap();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["release", "test-id"])
        .current_dir(temp_dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("No workspace found"));
}

#[test]
fn test_close_without_workspace() {
    let temp_dir = TempDir::new().unwrap();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["close", "test-id", "--reason", "Test"])
        .current_dir(temp_dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("No workspace found"));
}

#[test]
fn test_reopen_without_workspace() {
    let temp_dir = TempDir::new().unwrap();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["reopen", "test-id"])
        .current_dir(temp_dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("No workspace found"));
}

#[test]
fn test_update_nonexistent_issue() {
    let workspace = setup_workspace();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["update", "nonexistent-id", "--status", "in_progress"])
        .current_dir(workspace.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("Workspace error"));
}

#[test]
fn test_release_nonexistent_issue() {
    let workspace = setup_workspace();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["release", "nonexistent-id"])
        .current_dir(workspace.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("Workspace error"));
}

#[test]
fn test_close_nonexistent_issue() {
    let workspace = setup_workspace();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["close", "nonexistent-id", "--reason", "Test"])
        .current_dir(workspace.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("Workspace error"));
}

#[test]
fn test_reopen_nonexistent_issue() {
    let workspace = setup_workspace();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["reopen", "nonexistent-id"])
        .current_dir(workspace.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("Workspace error"));
}

#[test]
fn test_complete_lifecycle_workflow() {
    let workspace = setup_workspace();
    let issue_id = create_issue(workspace.path(), "Test Workflow");

    // Claim (update to in_progress with assignee)
    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "update",
            &issue_id,
            "--status",
            "in_progress",
            "--assignee",
            "worker-1",
        ])
        .current_dir(workspace.path())
        .assert()
        .success();

    // Release
    Command::cargo_bin("bead")
        .unwrap()
        .args(["release", &issue_id])
        .current_dir(workspace.path())
        .assert()
        .success();

    // Claim again
    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "update",
            &issue_id,
            "--status",
            "in_progress",
            "--assignee",
            "worker-2",
        ])
        .current_dir(workspace.path())
        .assert()
        .success();

    // Close
    Command::cargo_bin("bead")
        .unwrap()
        .args(["close", &issue_id, "--reason", "Completed"])
        .current_dir(workspace.path())
        .assert()
        .success();

    // Reopen
    Command::cargo_bin("bead")
        .unwrap()
        .args(["reopen", &issue_id])
        .current_dir(workspace.path())
        .assert()
        .success();

    // Verify final state is open with assignee retained
    Command::cargo_bin("bead")
        .unwrap()
        .args(["show", &issue_id, "--json"])
        .current_dir(workspace.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("\"status\":\"open\""))
        .stdout(predicate::str::contains("\"assignee\":\"worker-2\""));
}

#[test]
fn test_update_status_blocked_excludes_from_ready_and_is_reversible() {
    // Regression test: `BaseStatus::parse(new_status)?` used to run before
    // the "blocked" special-case check, and BaseStatus has no Blocked
    // variant, so `update --status blocked` always failed with
    // "Unknown status: blocked" -- even though `bead capabilities`
    // advertised "blocked" as a supported status.
    let workspace = setup_workspace();
    let issue_id = create_issue(workspace.path(), "Test Issue");

    // Ready before blocking.
    Command::cargo_bin("bead")
        .unwrap()
        .args(["list", "--ready", "--json"])
        .current_dir(workspace.path())
        .assert()
        .success()
        .stdout(predicate::str::contains(issue_id.clone()));

    Command::cargo_bin("bead")
        .unwrap()
        .args(["update", &issue_id, "--status", "blocked"])
        .current_dir(workspace.path())
        .assert()
        .success();

    // A manually-blocked issue must not appear in the ready frontier.
    Command::cargo_bin("bead")
        .unwrap()
        .args(["list", "--ready", "--json"])
        .current_dir(workspace.path())
        .assert()
        .success()
        .stdout(predicate::str::contains(issue_id.clone()).not());

    // An explicit non-blocked status transition must clear the manual
    // block and restore the issue to the ready frontier -- otherwise
    // "blocked" would be settable but never clearable via `update`.
    Command::cargo_bin("bead")
        .unwrap()
        .args(["update", &issue_id, "--status", "open"])
        .current_dir(workspace.path())
        .assert()
        .success();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["list", "--ready", "--json"])
        .current_dir(workspace.path())
        .assert()
        .success()
        .stdout(predicate::str::contains(issue_id.clone()));
}

#[test]
fn test_update_status_blocked_rejected_on_closed_issue() {
    let workspace = setup_workspace();
    let issue_id = create_issue(workspace.path(), "Test Issue");

    Command::cargo_bin("bead")
        .unwrap()
        .args(["close", &issue_id, "--reason", "done"])
        .current_dir(workspace.path())
        .assert()
        .success();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["update", &issue_id, "--status", "blocked"])
        .current_dir(workspace.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Cannot set manual blocked flag on closed issue",
        ));
}

#[test]
fn test_update_status_closed_requires_close_command_and_preserves_issue() {
    let workspace = setup_workspace();
    let id = create_issue(workspace.path(), "Cannot bypass close metadata");

    Command::cargo_bin("bead")
        .unwrap()
        .args(["update", &id, "--status", "closed"])
        .current_dir(workspace.path())
        .assert()
        .code(4)
        .stderr(predicate::str::contains("Use 'close' command"));

    Command::cargo_bin("bead")
        .unwrap()
        .args(["show", &id, "--json"])
        .current_dir(workspace.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("\"status\":\"open\""));
}

// Integration tests for R023 unified why explanation facade
//
// These tests verify the comprehensive "why" command that provides:
// - Issue state analysis (effective status, readiness)
// - Blocker analysis (active blockers, conditional dependencies)
// - Claim ranking factors (priority, age, attempt tiers, graph impact)
// - Legal operations (what can be done next)
// - Reason codes (detailed explanations)

use std::path::Path;

fn setup_test_workspace() -> tempfile::TempDir {
    let temp_dir = tempfile::tempdir().unwrap();

    // Initialize workspace using cargo run from the project directory
    let project_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    let mut cmd = std::process::Command::new("cargo");
    cmd.arg("run")
        .arg("--quiet")
        .arg("--bin")
        .arg("bead")
        .arg("--")
        .arg("init")
        .current_dir(&project_dir)
        .env("HOME", temp_dir.path().to_str().unwrap())
        .env("RUST_BACKTRACE", "1")
        .output()
        .expect("Failed to initialize workspace");

    temp_dir
}

fn run_bead_command(args: &[&str], workspace_dir: &Path) -> std::process::Output {
    let project_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    let mut cmd = std::process::Command::new("cargo");
    cmd.arg("run")
        .arg("--quiet")
        .arg("--bin")
        .arg("bead")
        .arg("--")
        .args(args)
        .current_dir(&project_dir)
        .env("HOME", workspace_dir.to_str().unwrap())
        .env("RUST_BACKTRACE", "1")
        .env("BEAD_WORKSPACE_ROOT", workspace_dir.to_str().unwrap())
        .output()
        .expect("Failed to run bead command")
}

/// Get combined stdout and stderr from command output
fn get_combined_output(output: &std::process::Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    format!("{}{}", stdout, stderr)
}

fn create_test_issue(workspace_dir: &Path, title: &str) -> String {
    let output = run_bead_command(&["create", "--title", title], workspace_dir);
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

#[test]
fn test_why_explanation_basic() {
    let temp_dir = setup_test_workspace();
    let workspace_path = temp_dir.path();

    // Create a simple issue
    let issue_id = create_test_issue(workspace_path, "Test Issue");

    // Get why explanation
    let output = run_bead_command(&["why", "--id", &issue_id], workspace_path);
    let result = get_combined_output(&output);

    // Verify basic fields are present
    assert!(result.contains(&issue_id));
    assert!(result.contains("Status"));
    assert!(result.contains("Base Status:"));
    assert!(result.contains("Effective Status:"));
    assert!(result.contains("Ready: Yes"));

    // Verify no blockers
    assert!(result.contains("No dependencies"));

    // Verify legal operations section
    assert!(result.contains("Legal Operations"));
}

#[test]
fn test_why_explanation_json_output() {
    let temp_dir = setup_test_workspace();
    let workspace_path = temp_dir.path();

    // Create a simple issue
    let issue_id = create_test_issue(workspace_path, "Test Issue");

    // Get why explanation in JSON format
    let output = run_bead_command(&["why", "--id", &issue_id, "--json"], workspace_path);
    let result = get_combined_output(&output);

    // Verify JSON structure
    assert!(result.contains(&format!("\"issue_id\": \"{}\"", issue_id)));
    assert!(result.contains("\"effective_status\":"));
    assert!(result.contains("\"base_status\":"));
    assert!(result.contains("\"is_ready\":"));
    assert!(result.contains("\"blockers\":"));
    assert!(result.contains("\"ranking_factors\":"));
    assert!(result.contains("\"legal_operations\":"));
    assert!(result.contains("\"reasons\":"));
}

#[test]
fn test_why_explanation_with_blockers() {
    let temp_dir = setup_test_workspace();
    let workspace_path = temp_dir.path();

    // Create blocker issue
    let blocker_id = create_test_issue(workspace_path, "Blocker Issue");

    // Create blocked issue
    let blocked_id = create_test_issue(workspace_path, "Blocked Issue");

    // Add dependency
    run_bead_command(&["dep", "add", &blocked_id, &blocker_id], workspace_path);

    // Get why explanation for blocked issue
    let output = run_bead_command(&["why", "--id", &blocked_id], workspace_path);
    let result = get_combined_output(&output);

    // Verify blocked status
    assert!(result.contains("Effective Status: blocked"));
    assert!(result.contains("Ready: No"));

    // Verify blocker analysis
    assert!(result.contains("Active Blockers: 1"));
    assert!(result.contains(&blocker_id));

    // Verify claim is not valid
    assert!(result.contains("✗ claim"));
}

#[test]
fn test_why_explanation_assigned_issue() {
    let temp_dir = setup_test_workspace();
    let workspace_path = temp_dir.path();

    // Create and assign issue
    let issue_id = create_test_issue(workspace_path, "Assigned Issue");

    // Assign the issue
    run_bead_command(
        &["update", &issue_id, "--assignee", "worker1"],
        workspace_path,
    );

    // Get why explanation
    let output = run_bead_command(&["why", "--id", &issue_id], workspace_path);
    let result = get_combined_output(&output);

    // Verify assignment
    assert!(result.contains("Assigned: worker1"));
    assert!(result.contains("Ready: No"));

    // Verify claim is not valid
    assert!(result.contains("✗ claim"));
    assert!(result.contains("already assigned"));
}

#[test]
fn test_why_explanation_closed_issue() {
    let temp_dir = setup_test_workspace();
    let workspace_path = temp_dir.path();

    // Create and close issue
    let issue_id = create_test_issue(workspace_path, "Closed Issue");

    // Close the issue
    run_bead_command(
        &["close", &issue_id, "--reason", "Completed"],
        workspace_path,
    );

    // Get why explanation
    let output = run_bead_command(&["why", "--id", &issue_id], workspace_path);
    let result = get_combined_output(&output);

    // Verify closed status
    assert!(result.contains("Base Status: closed"));
    assert!(result.contains("Effective Status: closed"));
    assert!(result.contains("Closed:"));

    // Verify reopen is valid
    assert!(result.contains("✓ reopen"));

    // Verify update is not valid
    assert!(result.contains("✗ update"));
    assert!(result.contains("issue is closed"));
}

#[test]
fn test_why_explanation_multiple_blockers() {
    let temp_dir = setup_test_workspace();
    let workspace_path = temp_dir.path();

    // Create multiple blockers
    let blocker_a = create_test_issue(workspace_path, "Blocker A");
    let blocker_b = create_test_issue(workspace_path, "Blocker B");

    // Create blocked issue
    let blocked_id = create_test_issue(workspace_path, "Multi Blocked Issue");

    // Add multiple dependencies
    run_bead_command(&["dep", "add", &blocked_id, &blocker_a], workspace_path);
    run_bead_command(&["dep", "add", &blocked_id, &blocker_b], workspace_path);

    // Get why explanation
    let output = run_bead_command(&["why", "--id", &blocked_id], workspace_path);
    let result = get_combined_output(&output);

    // Verify multiple blockers are tracked
    assert!(result.contains("Active Blockers: 2"));
    assert!(result.contains(&blocker_a));
    assert!(result.contains(&blocker_b));
}

#[test]
fn test_why_explanation_in_progress_issue() {
    let temp_dir = setup_test_workspace();
    let workspace_path = temp_dir.path();

    // Create and update to in_progress
    let issue_id = create_test_issue(workspace_path, "In Progress Issue");

    // Update to in_progress with assignment
    run_bead_command(
        &[
            "update",
            &issue_id,
            "--status",
            "in_progress",
            "--assignee",
            "worker1",
        ],
        workspace_path,
    );

    // Get why explanation
    let output = run_bead_command(&["why", "--id", &issue_id], workspace_path);
    let result = get_combined_output(&output);

    // Verify in-progress status
    assert!(result.contains("Base Status: in_progress"));

    // Verify release is valid
    assert!(result.contains("✓ release"));
}

#[test]
fn test_why_explanation_operations_include_commands() {
    let temp_dir = setup_test_workspace();
    let workspace_path = temp_dir.path();

    // Create issue
    let issue_id = create_test_issue(workspace_path, "Commands Test Issue");

    // Get why explanation
    let output = run_bead_command(&["why", "--id", &issue_id], workspace_path);
    let result = get_combined_output(&output);

    // Verify command examples are shown
    assert!(result.contains(&format!("bead show {}", issue_id)));
    assert!(result.contains("bead list --ready"));
}

#[test]
fn test_why_explanation_ranking_factors() {
    let temp_dir = setup_test_workspace();
    let workspace_path = temp_dir.path();

    // Create issue (will have default priority P2)
    let issue_id = create_test_issue(workspace_path, "Ranking Test Issue");

    // Get why explanation
    let output = run_bead_command(&["why", "--id", &issue_id, "--json"], workspace_path);
    let result = get_combined_output(&output);

    // Verify ranking factors (default priority is P2)
    assert!(result.contains("\"declared_priority\": 2"));
    assert!(result.contains("\"effective_priority\": 2"));
    assert!(result.contains("\"attempt_tier\": 0"));
    assert!(result.contains("\"consecutive_failures\": 0"));
}

#[test]
fn test_why_explanation_nonexistent_issue() {
    let temp_dir = setup_test_workspace();
    let workspace_path = temp_dir.path();

    // Try to get explanation for non-existent issue
    let output = run_bead_command(&["why", "--id", "NONEXISTENT-001"], workspace_path);

    // Should fail
    assert!(!output.status.success());
}

#[test]
fn test_why_explanation_deferred_status() {
    let temp_dir = setup_test_workspace();
    let workspace_path = temp_dir.path();

    // Create and defer issue
    let issue_id = create_test_issue(workspace_path, "Deferred Issue");

    // Defer the issue
    run_bead_command(
        &["update", &issue_id, "--status", "deferred"],
        workspace_path,
    );

    // Get why explanation
    let output = run_bead_command(&["why", "--id", &issue_id], workspace_path);
    let result = get_combined_output(&output);

    // Verify deferred status
    assert!(result.contains("Base Status: deferred"));

    // Verify close is valid
    assert!(result.contains("✓ close"));

    // Should be able to reopen to open
    assert!(result.contains("✓ reopen"));
}

//! Integration tests for R022 general mutation dry-run functionality

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

fn run_bead_command(args: &[&str], workspace_dir: &std::path::Path) -> std::process::Output {
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

fn create_test_issue(workspace_dir: &Path, title: &str) -> String {
    let output = run_bead_command(&["create", "--title", title], workspace_dir);
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

#[test]
fn test_dryrun_update_basic() {
    let temp_dir = setup_test_workspace();
    let workspace_path = temp_dir.path();

    // Create a test issue
    let issue_id = create_test_issue(workspace_path, "Test Issue");

    // Test dry-run update
    let output = run_bead_command(
        &["update", &issue_id, "--status", "in_progress", "--dry-run"],
        workspace_path,
    );

    let result = String::from_utf8_lossy(&output.stdout);

    // Verify JSON output contains expected fields
    assert!(result.contains("\"id\":"));
    assert!(result.contains("\"current_revision\":"));
    assert!(result.contains("\"workspace_sequence\":"));
    assert!(result.contains("\"before\":"));
    assert!(result.contains("\"after\":"));
    assert!(result.contains("\"semantic_change\":"));
    assert!(result.contains("\"message\":"));

    // Verify the status change is reflected
    assert!(result.contains("\"base_status\": \"open\""));
    assert!(result.contains("\"base_status\": \"in_progress\""));
}

#[test]
fn test_dryrun_update_multiple_fields() {
    let temp_dir = setup_test_workspace();
    let workspace_path = temp_dir.path();

    // Create a test issue
    let issue_id = create_test_issue(workspace_path, "Test Issue");

    // Test dry-run update with multiple fields
    let output = run_bead_command(
        &[
            "update",
            &issue_id,
            "--status",
            "in_progress",
            "--assignee",
            "alice",
            "--notes",
            "Test notes",
            "--dry-run",
        ],
        workspace_path,
    );

    let result = String::from_utf8_lossy(&output.stdout);

    // Verify all changes are reflected
    assert!(result.contains("Would update:"));
    assert!(result.contains("status: open -> in_progress"));
    assert!(result.contains("assignee: none -> alice"));
    assert!(result.contains("notes updated"));
}

#[test]
fn test_dryrun_update_idempotent() {
    let temp_dir = setup_test_workspace();
    let workspace_path = temp_dir.path();

    // Create a test issue
    let issue_id = create_test_issue(workspace_path, "Test Issue");

    // Perform actual update
    let _ = run_bead_command(
        &["update", &issue_id, "--status", "in_progress"],
        workspace_path,
    );

    // Test dry-run with same update (should be idempotent)
    let output = run_bead_command(
        &["update", &issue_id, "--status", "in_progress", "--dry-run"],
        workspace_path,
    );

    let result = String::from_utf8_lossy(&output.stdout);

    // Verify idempotent message
    assert!(result.contains("No changes would be made (idempotent)"));
    assert!(result.contains("\"semantic_change\": false"));
}

#[test]
fn test_dryrun_close_basic() {
    let temp_dir = setup_test_workspace();
    let workspace_path = temp_dir.path();

    // Create a test issue
    let issue_id = create_test_issue(workspace_path, "Test Issue");

    // Test dry-run close
    let output = run_bead_command(
        &[
            "close",
            &issue_id,
            "--reason",
            "Completed successfully",
            "--dry-run",
        ],
        workspace_path,
    );

    let result = String::from_utf8_lossy(&output.stdout);

    // Verify JSON structure and close operation
    assert!(result.contains("\"id\":"));
    assert!(result.contains("\"before\":"));
    assert!(result.contains("\"after\":"));
    assert!(result.contains("Would close issue with reason"));
    assert!(result.contains("Completed successfully"));
    assert!(result.contains("\"closed_at\":"));
}

#[test]
fn test_dryrun_close_idempotent() {
    let temp_dir = setup_test_workspace();
    let workspace_path = temp_dir.path();

    // Create and close a test issue
    let issue_id = create_test_issue(workspace_path, "Test Issue");

    let _ = run_bead_command(
        &["close", &issue_id, "--reason", "Completed successfully"],
        workspace_path,
    );

    // Test dry-run close with same reason (should be idempotent)
    let output = run_bead_command(
        &[
            "close",
            &issue_id,
            "--reason",
            "Completed successfully",
            "--dry-run",
        ],
        workspace_path,
    );

    let result = String::from_utf8_lossy(&output.stdout);

    // Verify idempotent message
    assert!(result.contains("Issue already closed with same reason (idempotent)"));
    assert!(result.contains("\"semantic_change\": false"));
}

#[test]
fn test_dryrun_reopen_basic() {
    let temp_dir = setup_test_workspace();
    let workspace_path = temp_dir.path();

    // Create and close a test issue
    let issue_id = create_test_issue(workspace_path, "Test Issue");

    let _ = run_bead_command(
        &["close", &issue_id, "--reason", "Completed successfully"],
        workspace_path,
    );

    // Test dry-run reopen
    let output = run_bead_command(&["reopen", &issue_id, "--dry-run"], workspace_path);

    let result = String::from_utf8_lossy(&output.stdout);

    // Verify JSON structure and reopen operation
    assert!(result.contains("Would reopen issue to open status"));
    assert!(result.contains("\"base_status\": \"closed\""));
    assert!(result.contains("\"base_status\": \"open\""));
    assert!(result.contains("\"semantic_change\": true"));
}

#[test]
fn test_dryrun_release_basic() {
    let temp_dir = setup_test_workspace();
    let workspace_path = temp_dir.path();

    // Create and claim a test issue
    let issue_id = create_test_issue(workspace_path, "Test Issue");

    let _ = run_bead_command(
        &[
            "update",
            &issue_id,
            "--status",
            "in_progress",
            "--assignee",
            "alice",
        ],
        workspace_path,
    );

    // Test dry-run release
    let output = run_bead_command(&["release", &issue_id, "--dry-run"], workspace_path);

    let result = String::from_utf8_lossy(&output.stdout);

    // Verify JSON structure and release operation
    assert!(result.contains("Would release issue from in_progress to open/unassigned"));
    assert!(result.contains("\"assignee\": \"alice\""));
    assert!(result.contains("\"assignee\": null"));
    assert!(result.contains("\"semantic_change\": true"));
}

#[test]
fn test_dryrun_add_dependency_basic() {
    let temp_dir = setup_test_workspace();
    let workspace_path = temp_dir.path();

    // Create two test issues
    let issue1 = create_test_issue(workspace_path, "First Issue");
    let issue2 = create_test_issue(workspace_path, "Second Issue");

    // Test dry-run add dependency
    let output = run_bead_command(
        &[
            "dep",
            "add",
            &issue2,
            &issue1,
            "--kind",
            "blocks",
            "--dry-run",
        ],
        workspace_path,
    );

    let result = String::from_utf8_lossy(&output.stdout);

    // Debug: print the actual result
    eprintln!("Actual result: {}", result);

    // Verify JSON structure and dependency operation
    assert!(result.contains("\"blocked\":"));
    assert!(result.contains("\"blocker\":"));
    assert!(result.contains("\"kind\": \"blocks\""));
    assert!(result.contains("Would add dependency"));
    assert!(result.contains("\"workspace_sequence\":"));
    assert!(result.contains("\"semantic_change\": true"));
}

#[test]
fn test_dryrun_add_dependency_cycle_detection() {
    let temp_dir = setup_test_workspace();
    let workspace_path = temp_dir.path();

    // Create two test issues
    let issue1 = create_test_issue(workspace_path, "First Issue");
    let issue2 = create_test_issue(workspace_path, "Second Issue");

    // Add first dependency
    let _ = run_bead_command(
        &["dep", "add", &issue1, &issue2, "--kind", "blocks"],
        workspace_path,
    );

    // Test dry-run that would create a cycle
    let output = run_bead_command(
        &[
            "dep",
            "add",
            &issue2,
            &issue1,
            "--kind",
            "blocks",
            "--dry-run",
        ],
        workspace_path,
    );

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Verify cycle detection
    assert!(stderr.contains("cycle") || stderr.contains("conflict"));
}

#[test]
fn test_dryrun_remove_dependency_basic() {
    let temp_dir = setup_test_workspace();
    let workspace_path = temp_dir.path();

    // Create two test issues and add dependency
    let issue1 = create_test_issue(workspace_path, "First Issue");
    let issue2 = create_test_issue(workspace_path, "Second Issue");

    let _ = run_bead_command(
        &["dep", "add", &issue2, &issue1, "--kind", "blocks"],
        workspace_path,
    );

    // Test dry-run remove dependency
    let output = run_bead_command(
        &[
            "dep",
            "remove",
            &issue2,
            &issue1,
            "--kind",
            "blocks",
            "--dry-run",
        ],
        workspace_path,
    );

    let result = String::from_utf8_lossy(&output.stdout);

    // Verify JSON structure and dependency removal
    assert!(result.contains("Would remove dependencies"));
    assert!(result.contains("\"blocked\":"));
    assert!(result.contains("\"blocker\":"));
    assert!(result.contains("\"semantic_change\": true"));
}

#[test]
fn test_dryrun_remove_dependency_idempotent() {
    let temp_dir = setup_test_workspace();
    let workspace_path = temp_dir.path();

    // Create two test issues
    let issue1 = create_test_issue(workspace_path, "First Issue");
    let issue2 = create_test_issue(workspace_path, "Second Issue");

    // Test dry-run remove non-existent dependency
    let output = run_bead_command(
        &[
            "dep",
            "remove",
            &issue2,
            &issue1,
            "--kind",
            "blocks",
            "--dry-run",
        ],
        workspace_path,
    );

    let result = String::from_utf8_lossy(&output.stdout);

    // Verify idempotent message
    assert!(result.contains("does not exist (idempotent)"));
    assert!(result.contains("\"semantic_change\": false"));
}

#[test]
fn test_dryrun_json_structure() {
    let temp_dir = setup_test_workspace();
    let workspace_path = temp_dir.path();

    // Create a test issue
    let issue_id = create_test_issue(workspace_path, "Test Issue");

    // Test dry-run update
    let output = run_bead_command(
        &["update", &issue_id, "--status", "in_progress", "--dry-run"],
        workspace_path,
    );

    let result = String::from_utf8_lossy(&output.stdout);

    // Verify it's valid JSON and has required fields
    let json: serde_json::Value = serde_json::from_str(&result).expect("Invalid JSON output");

    assert!(json.is_object());
    let obj = json.as_object().unwrap();

    // Check required fields exist
    assert!(obj.contains_key("id"));
    assert!(obj.contains_key("current_revision"));
    assert!(obj.contains_key("workspace_sequence"));
    assert!(obj.contains_key("before"));
    assert!(obj.contains_key("after"));
    assert!(obj.contains_key("semantic_change"));
    assert!(obj.contains_key("message"));

    // Check before/after structure
    let before = obj.get("before").unwrap().as_object().unwrap();
    assert!(before.contains_key("id"));
    assert!(before.contains_key("title"));
    assert!(before.contains_key("base_status"));
    assert!(before.contains_key("priority"));

    let after = obj.get("after").unwrap().as_object().unwrap();
    assert!(after.contains_key("id"));
    assert!(after.contains_key("title"));
    assert!(after.contains_key("base_status"));
    assert!(after.contains_key("priority"));
}

#[test]
fn test_dryrun_no_workspace() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace_path = temp_dir.path();

    // Test dry-run without workspace
    let output = run_bead_command(
        &[
            "update",
            "bead-123abc456789def",
            "--status",
            "in_progress",
            "--dry-run",
        ],
        workspace_path,
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Debug: print actual output
    eprintln!("STDERR: {}", stderr);
    eprintln!("STDOUT: {}", stdout);
    eprintln!("Exit code: {}", output.status);

    // Should fail with workspace error - actual format is "bead: Workspace error: {id}"
    assert!(output.status.code() == Some(3)); // Exit code 3 for workspace errors
}

#[test]
fn test_dryrun_nonexistent_issue() {
    let temp_dir = setup_test_workspace();
    let workspace_path = temp_dir.path();

    // Test dry-run with non-existent issue
    let output = run_bead_command(
        &[
            "update",
            "bead-nonexistent123",
            "--status",
            "in_progress",
            "--dry-run",
        ],
        workspace_path,
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Debug: print actual output
    eprintln!("STDERR: {}", stderr);
    eprintln!("STDOUT: {}", stdout);
    eprintln!("Exit code: {}", output.status);

    // Should fail with not found error
    assert!(output.status.code() == Some(3)); // Exit code 3 for not found errors
}

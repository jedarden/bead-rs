//! Integration tests for R011 - Namespaced external references
//!
//! These tests verify the complete R011 implementation including:
//! - Generic (namespace, key, value) references
//! - No replacement of native bead IDs
//! - No network resolution
//! - Namespace-scoped uniqueness for deduplication
//! - Cross-tool recognition

use std::process::Command;

fn run_bead_in_workspace(workspace: &std::path::Path, args: &[&str]) -> (String, String, bool) {
    let output = Command::new(env!("CARGO_BIN_EXE_bead"))
        .current_dir(workspace)
        .args(args)
        .output()
        .expect("Failed to execute bead command");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let success = output.status.success();

    (stdout, stderr, success)
}

#[test]
fn test_add_external_reference() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path();

    // Initialize workspace
    run_bead_in_workspace(workspace, &["init", "--prefix", "test"]);

    // Create a test issue
    let (stdout, _, _) = run_bead_in_workspace(
        workspace,
        &["create", "--title", "Test Issue", "--priority", "2"],
    );
    let issue_id = stdout.trim().to_string();

    // Add external reference
    let (stdout, _, success) = run_bead_in_workspace(
        workspace,
        &[
            "ref",
            "add",
            "--id",
            &issue_id,
            "--namespace",
            "github",
            "--key",
            "issue-number",
            "--value",
            "12345",
        ],
    );

    assert!(success);
    assert!(stdout.contains(&format!(
        "Added reference: {} -> github/issue-number/12345",
        issue_id
    )));

    // List references to verify
    let (stdout, _, _) = run_bead_in_workspace(workspace, &["ref", "list", "--id", &issue_id]);

    assert!(stdout.contains("github/issue-number: 12345"));
}

#[test]
fn test_add_multiple_references_same_issue() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path();

    // Initialize workspace
    run_bead_in_workspace(workspace, &["init", "--prefix", "test"]);

    // Create a test issue
    let (stdout, _, _) = run_bead_in_workspace(
        workspace,
        &["create", "--title", "Test Issue", "--priority", "2"],
    );
    let issue_id = stdout.trim().to_string();

    // Add multiple references from different namespaces
    run_bead_in_workspace(
        workspace,
        &[
            "ref",
            "add",
            "--id",
            &issue_id,
            "--namespace",
            "github",
            "--key",
            "pr",
            "--value",
            "42",
        ],
    );

    run_bead_in_workspace(
        workspace,
        &[
            "ref",
            "add",
            "--id",
            &issue_id,
            "--namespace",
            "gitlab",
            "--key",
            "mr",
            "--value",
            "15",
        ],
    );

    run_bead_in_workspace(
        workspace,
        &[
            "ref",
            "add",
            "--id",
            &issue_id,
            "--namespace",
            "jira",
            "--key",
            "ticket",
            "--value",
            "PROJ-123",
        ],
    );

    // List all references
    let (stdout, _, _) = run_bead_in_workspace(workspace, &["ref", "list", "--id", &issue_id]);

    assert!(stdout.contains("github/pr: 42"));
    assert!(stdout.contains("gitlab/mr: 15"));
    assert!(stdout.contains("jira/ticket: PROJ-123"));
}

#[test]
fn test_add_duplicate_reference_idempotent() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path();

    // Initialize workspace
    run_bead_in_workspace(workspace, &["init", "--prefix", "test"]);

    // Create a test issue
    let (stdout, _, _) = run_bead_in_workspace(
        workspace,
        &["create", "--title", "Test Issue", "--priority", "2"],
    );
    let issue_id = stdout.trim().to_string();

    // Add the same reference twice
    run_bead_in_workspace(
        workspace,
        &[
            "ref",
            "add",
            "--id",
            &issue_id,
            "--namespace",
            "github",
            "--key",
            "issue",
            "--value",
            "123",
        ],
    );

    let (stdout, _, _) = run_bead_in_workspace(
        workspace,
        &[
            "ref",
            "add",
            "--id",
            &issue_id,
            "--namespace",
            "github",
            "--key",
            "issue",
            "--value",
            "123",
        ],
    );

    // Should succeed (idempotent)
    assert!(stdout.contains("Added reference"));

    // Should only have one reference
    let (stdout, _, _) = run_bead_in_workspace(workspace, &["ref", "list", "--id", &issue_id]);

    let count = stdout.matches("github/issue: 123").count();
    assert_eq!(count, 1);
}

#[test]
fn test_remove_external_reference() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path();

    // Initialize workspace
    run_bead_in_workspace(workspace, &["init", "--prefix", "test"]);

    // Create a test issue
    let (stdout, _, _) = run_bead_in_workspace(
        workspace,
        &["create", "--title", "Test Issue", "--priority", "2"],
    );
    let issue_id = stdout.trim().to_string();

    // Add external reference
    run_bead_in_workspace(
        workspace,
        &[
            "ref",
            "add",
            "--id",
            &issue_id,
            "--namespace",
            "github",
            "--key",
            "issue",
            "--value",
            "12345",
        ],
    );

    // Remove the reference
    let (stdout, _, _) = run_bead_in_workspace(
        workspace,
        &[
            "ref",
            "remove",
            "--id",
            &issue_id,
            "--namespace",
            "github",
            "--key",
            "issue",
        ],
    );

    assert!(stdout.contains(&format!("Removed reference: {} -> github/issue", issue_id)));

    // Verify it's gone
    let (stdout, _, _) = run_bead_in_workspace(workspace, &["ref", "list", "--id", &issue_id]);

    assert!(stdout.contains("No external references found"));
}

#[test]
fn test_remove_nonexistent_reference_idempotent() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path();

    // Initialize workspace
    run_bead_in_workspace(workspace, &["init", "--prefix", "test"]);

    // Create a test issue
    let (stdout, _, _) = run_bead_in_workspace(
        workspace,
        &["create", "--title", "Test Issue", "--priority", "2"],
    );
    let issue_id = stdout.trim().to_string();

    // Remove non-existent reference - should succeed
    let (stdout, _, _) = run_bead_in_workspace(
        workspace,
        &[
            "ref",
            "remove",
            "--id",
            &issue_id,
            "--namespace",
            "github",
            "--key",
            "nonexistent",
        ],
    );

    assert!(stdout.contains("Removed reference"));
}

#[test]
fn test_find_issues_by_reference() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path();

    // Initialize workspace
    run_bead_in_workspace(workspace, &["init", "--prefix", "test"]);

    // Create multiple test issues
    let (stdout1, _, _) = run_bead_in_workspace(
        workspace,
        &["create", "--title", "Issue 1", "--priority", "2"],
    );
    let issue_id1 = stdout1.trim().to_string();

    let (stdout2, _, _) = run_bead_in_workspace(
        workspace,
        &["create", "--title", "Issue 2", "--priority", "2"],
    );
    let issue_id2 = stdout2.trim().to_string();

    let (stdout3, _, _) = run_bead_in_workspace(
        workspace,
        &["create", "--title", "Issue 3", "--priority", "2"],
    );
    let issue_id3 = stdout3.trim().to_string();

    // Add the same reference to multiple issues (cross-tool recognition)
    run_bead_in_workspace(
        workspace,
        &[
            "ref",
            "add",
            "--id",
            &issue_id1,
            "--namespace",
            "github",
            "--key",
            "issue",
            "--value",
            "123",
        ],
    );

    run_bead_in_workspace(
        workspace,
        &[
            "ref",
            "add",
            "--id",
            &issue_id2,
            "--namespace",
            "github",
            "--key",
            "issue",
            "--value",
            "123",
        ],
    );

    // Third issue has different reference
    run_bead_in_workspace(
        workspace,
        &[
            "ref",
            "add",
            "--id",
            &issue_id3,
            "--namespace",
            "github",
            "--key",
            "issue",
            "--value",
            "456",
        ],
    );

    // Find issues with the same reference
    let (stdout, _, _) = run_bead_in_workspace(
        workspace,
        &["ref", "find", "--namespace", "github", "--value", "123"],
    );

    assert!(stdout.contains(&issue_id1));
    assert!(stdout.contains(&issue_id2));
    assert!(!stdout.contains(&issue_id3));
}

#[test]
fn test_reference_validation_invalid_namespace() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path();

    // Initialize workspace
    run_bead_in_workspace(workspace, &["init", "--prefix", "test"]);

    // Create a test issue
    let (stdout, _, _) = run_bead_in_workspace(
        workspace,
        &["create", "--title", "Test Issue", "--priority", "2"],
    );
    let issue_id = stdout.trim().to_string();

    // Try to add reference with invalid namespace (starts with number)
    let (_stdout, stderr, success) = run_bead_in_workspace(
        workspace,
        &[
            "ref",
            "add",
            "--id",
            &issue_id,
            "--namespace",
            "123invalid",
            "--key",
            "key",
            "--value",
            "value",
        ],
    );

    assert!(!success);
    assert!(stderr.contains("Validation") || stderr.contains("namespace"));
}

#[test]
fn test_reference_validation_empty_fields() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path();

    // Initialize workspace
    run_bead_in_workspace(workspace, &["init", "--prefix", "test"]);

    // Create a test issue
    let (stdout, _, _) = run_bead_in_workspace(
        workspace,
        &["create", "--title", "Test Issue", "--priority", "2"],
    );
    let issue_id = stdout.trim().to_string();

    // Try to add reference with empty value
    let (_stdout, stderr, success) = run_bead_in_workspace(
        workspace,
        &[
            "ref",
            "add",
            "--id",
            &issue_id,
            "--namespace",
            "github",
            "--key",
            "key",
            "--value",
            "",
        ],
    );

    assert!(!success);
    assert!(stderr.contains("Validation") || stderr.contains("value"));
}

#[test]
fn test_reference_json_output() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path();

    // Initialize workspace
    run_bead_in_workspace(workspace, &["init", "--prefix", "test"]);

    // Create a test issue
    let (stdout, _, _) = run_bead_in_workspace(
        workspace,
        &["create", "--title", "Test Issue", "--priority", "2"],
    );
    let issue_id = stdout.trim().to_string();

    // Add external reference
    run_bead_in_workspace(
        workspace,
        &[
            "ref",
            "add",
            "--id",
            &issue_id,
            "--namespace",
            "github",
            "--key",
            "issue",
            "--value",
            "12345",
        ],
    );

    // List references in JSON format
    let (stdout, _, _) =
        run_bead_in_workspace(workspace, &["ref", "list", "--id", &issue_id, "--json"]);

    // Output should be line-by-line JSON objects
    let lines: Vec<&str> = stdout.lines().collect();
    assert!(
        !lines.is_empty(),
        "Should have at least one line of JSON output"
    );

    // Parse each line as a JSON object
    let mut found_github_ref = false;
    for line in lines {
        if !line.trim().is_empty() {
            let json: serde_json::Value = serde_json::from_str(line).expect("Invalid JSON output");
            if let Some(obj) = json.as_object() {
                if obj.get("namespace").and_then(|v| v.as_str()) == Some("github")
                    && obj.get("key").and_then(|v| v.as_str()) == Some("issue")
                    && obj.get("value").and_then(|v| v.as_str()) == Some("12345")
                {
                    found_github_ref = true;
                }
            }
        }
    }
    assert!(
        found_github_ref,
        "Should find the github reference in JSON output"
    );
}

#[test]
fn test_reference_nonexistent_issue() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path();

    // Initialize workspace
    run_bead_in_workspace(workspace, &["init", "--prefix", "test"]);

    // Try to add reference to non-existent issue
    let (_stdout, stderr, success) = run_bead_in_workspace(
        workspace,
        &[
            "ref",
            "add",
            "--id",
            "bead-nonexistent",
            "--namespace",
            "github",
            "--key",
            "issue",
            "--value",
            "12345",
        ],
    );

    assert!(!success);
    assert!(stderr.contains("Issue") || stderr.contains("not found"));
}

#[test]
fn test_reference_list_json_find_json() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path();

    // Initialize workspace
    run_bead_in_workspace(workspace, &["init", "--prefix", "test"]);

    // Create a test issue
    let (stdout, _, _) = run_bead_in_workspace(
        workspace,
        &["create", "--title", "Test Issue", "--priority", "2"],
    );
    let issue_id = stdout.trim().to_string();

    // Add external reference
    run_bead_in_workspace(
        workspace,
        &[
            "ref",
            "add",
            "--id",
            &issue_id,
            "--namespace",
            "github",
            "--key",
            "issue",
            "--value",
            "12345",
        ],
    );

    // Find issues with JSON output
    let (stdout, _, _) = run_bead_in_workspace(
        workspace,
        &[
            "ref",
            "find",
            "--namespace",
            "github",
            "--value",
            "12345",
            "--json",
        ],
    );

    // Should be valid JSON array
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("Invalid JSON output");
    assert!(json.is_array());
    let issues = json.as_array().unwrap();
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0], issue_id);
}

#[test]
fn test_reference_help() {
    // Test that help works for all ref commands
    let commands: Vec<Vec<&str>> = vec![
        vec!["ref", "add", "--help"],
        vec!["ref", "remove", "--help"],
        vec!["ref", "list", "--help"],
        vec!["ref", "find", "--help"],
    ];

    for args in commands {
        let output = Command::new(env!("CARGO_BIN_EXE_bead"))
            .args(&args)
            .output()
            .expect("Failed to execute bead command");

        assert!(output.status.success(), "Help command should succeed");
        let stdout = String::from_utf8_lossy(&output.stdout);
        // Help should contain some content (Usage, Description, etc.)
        assert!(!stdout.trim().is_empty(), "Help output should not be empty");
    }
}

#[test]
fn test_reference_namespace_scoped_uniqueness() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path();

    // Initialize workspace
    run_bead_in_workspace(workspace, &["init", "--prefix", "test"]);

    // Create a test issue
    let (stdout, _, _) = run_bead_in_workspace(
        workspace,
        &["create", "--title", "Test Issue", "--priority", "2"],
    );
    let issue_id = stdout.trim().to_string();

    // Add references with same namespace and key but different values (idempotent)
    run_bead_in_workspace(
        workspace,
        &[
            "ref",
            "add",
            "--id",
            &issue_id,
            "--namespace",
            "github",
            "--key",
            "issue",
            "--value",
            "123",
        ],
    );

    run_bead_in_workspace(
        workspace,
        &[
            "ref",
            "add",
            "--id",
            &issue_id,
            "--namespace",
            "github",
            "--key",
            "issue",
            "--value",
            "456",
        ],
    );

    // Should only have the last value (replacement behavior)
    let (stdout, _, _) = run_bead_in_workspace(workspace, &["ref", "list", "--id", &issue_id]);

    assert!(stdout.contains("github/issue: 456"));
    assert!(!stdout.contains("github/issue: 123"));
}

#[test]
fn test_ref_mutations_advance_change_feed() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path();

    // Initialize workspace
    run_bead_in_workspace(workspace, &["init", "--prefix", "test"]);

    // Create an issue (the create itself appends one event)
    let (stdout, _, _) = run_bead_in_workspace(
        workspace,
        &["create", "--title", "Test Issue", "--priority", "2"],
    );
    let issue_id = stdout.trim().to_string();

    let latest_cursor = || -> i64 {
        let (stdout, _, success) = run_bead_in_workspace(workspace, &["changes", "--latest"]);
        assert!(success);
        stdout
            .lines()
            .find_map(|l| l.strip_prefix("Latest cursor: "))
            .expect("changes --latest prints the cursor")
            .trim()
            .parse()
            .unwrap()
    };

    // Baseline after the create
    let baseline = latest_cursor();

    let ref_add = |value: &str| {
        run_bead_in_workspace(
            workspace,
            &[
                "ref",
                "add",
                "--id",
                &issue_id,
                "--namespace",
                "github",
                "--key",
                "issue-number",
                "--value",
                value,
            ],
        )
    };
    let ref_remove = || {
        run_bead_in_workspace(
            workspace,
            &[
                "ref",
                "remove",
                "--id",
                &issue_id,
                "--namespace",
                "github",
                "--key",
                "issue-number",
            ],
        )
    };

    // ref add appends one event: the cursor advances by exactly one
    let (_, _, success) = ref_add("12345");
    assert!(success);
    assert_eq!(latest_cursor(), baseline + 1);

    // Idempotent re-add of the identical reference appends no event
    let (_, _, success) = ref_add("12345");
    assert!(success);
    assert_eq!(latest_cursor(), baseline + 1);

    // Re-add with a changed value replaces the row: one more event
    let (_, _, success) = ref_add("67890");
    assert!(success);
    assert_eq!(latest_cursor(), baseline + 2);

    // ref remove appends one event
    let (_, _, success) = ref_remove();
    assert!(success);
    assert_eq!(latest_cursor(), baseline + 3);

    // Idempotent remove of the missing reference appends no event
    let (_, _, success) = ref_remove();
    assert!(success);
    assert_eq!(latest_cursor(), baseline + 3);

    // The recorded events carry namespace and key, never the value verbatim
    let conn = rusqlite::Connection::open(workspace.join(".beads/beads.db")).unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT kind, detail FROM events WHERE kind LIKE 'external_ref_%' ORDER BY sequence",
        )
        .unwrap();
    let rows: Vec<(String, String)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(rows.len(), 3);

    let (kind, detail) = &rows[0];
    assert_eq!(kind, "external_ref_added");
    assert!(detail.contains("\"namespace\":\"github\""));
    assert!(detail.contains("\"key\":\"issue-number\""));
    assert!(
        !detail.contains("12345"),
        "value must not be recorded verbatim"
    );

    let (kind, detail) = &rows[1];
    assert_eq!(kind, "external_ref_added");
    assert!(
        !detail.contains("67890"),
        "value must not be recorded verbatim"
    );

    let (kind, detail) = &rows[2];
    assert_eq!(kind, "external_ref_removed");
    assert!(detail.contains("\"namespace\":\"github\""));
    assert!(detail.contains("\"key\":\"issue-number\""));
}

//! R037: Command errors that name the remedy
//!
//! Conformance tests for command error messages that state the domain rule
//! and the remedy instead of surfacing a bare parser error.
//!
//! Acceptance criteria:
//! - fields immutable after create (title, description, priority, issue-type)
//!   produce an error naming the rule and any supported alternative
//! - close --body names --reason
//! - near-miss handling is bounded to flags that exist on sibling commands
//! - conformance scenarios assert the remedy text, not merely a nonzero exit

use assert_cmd::Command;

fn bead_cmd() -> Command {
    Command::cargo_bin("bead").unwrap()
}

#[test]
fn test_update_with_immutable_title_shows_remedy() {
    // When a user tries to update title (immutable after create),
    // the error should name the rule and provide the remedy
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path();

    bead_cmd()
        .arg("--skip-foreign-workspace")
        .arg("init")
        .current_dir(workspace)
        .assert()
        .success();

    bead_cmd()
        .arg("create")
        .arg("--title")
        .arg("Test issue")
        .current_dir(workspace)
        .assert()
        .success();

    let result = bead_cmd()
        .arg("update")
        .arg("bead-")
        .arg("--title")
        .arg("New title")
        .current_dir(workspace)
        .assert();

    // Should exit with code 2 (CLI usage error)
    let output = result.get_output();
    assert_eq!(output.status.code(), Some(2));

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Must name the domain rule
    assert!(stderr.contains("Domain rule:"));
    assert!(stderr.contains("title, description, priority, issue_type, and labels are"));
    assert!(stderr.contains("set at creation time and cannot be changed via update"));

    // Must provide the remedy
    assert!(stderr.contains("Remedies:"));
    assert!(stderr.contains("create a new issue with 'bead create'"));

    // Must mention the specific immutable field
    assert!(stderr.contains("title is immutable after create"));
}

#[test]
fn test_update_with_immutable_description_shows_remedy() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path();

    bead_cmd()
        .arg("--skip-foreign-workspace")
        .arg("init")
        .current_dir(workspace)
        .assert()
        .success();

    bead_cmd()
        .arg("create")
        .arg("--title")
        .arg("Test issue")
        .current_dir(workspace)
        .assert()
        .success();

    let result = bead_cmd()
        .arg("update")
        .arg("bead-")
        .arg("--description")
        .arg("New description")
        .current_dir(workspace)
        .assert();

    let output = result.get_output();
    assert_eq!(output.status.code(), Some(2));

    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(stderr.contains("Domain rule:"));
    assert!(stderr.contains("description is immutable after create"));
    assert!(stderr.contains("Remedies:"));
}

#[test]
fn test_update_with_immutable_priority_shows_remedy() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path();

    bead_cmd()
        .arg("--skip-foreign-workspace")
        .arg("init")
        .current_dir(workspace)
        .assert()
        .success();

    bead_cmd()
        .arg("create")
        .arg("--title")
        .arg("Test issue")
        .current_dir(workspace)
        .assert()
        .success();

    let result = bead_cmd()
        .arg("update")
        .arg("bead-")
        .arg("--priority")
        .arg("0")
        .current_dir(workspace)
        .assert()
        .failure();

    let output = result.get_output();
    assert_eq!(output.status.code(), Some(2));

    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(stderr.contains("Domain rule:"));
    assert!(stderr.contains("priority is immutable after create"));
    assert!(stderr.contains("Remedies:"));
}

#[test]
fn test_update_with_immutable_issue_type_shows_remedy() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path();

    bead_cmd()
        .arg("--skip-foreign-workspace")
        .arg("init")
        .current_dir(workspace)
        .assert()
        .success();

    bead_cmd()
        .arg("create")
        .arg("--title")
        .arg("Test issue")
        .current_dir(workspace)
        .assert()
        .success();

    let result = bead_cmd()
        .arg("update")
        .arg("bead-")
        .arg("--issue-type")
        .arg("bug")
        .current_dir(workspace)
        .assert()
        .failure();

    let output = result.get_output();
    assert_eq!(output.status.code(), Some(2));

    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(stderr.contains("Domain rule:"));
    assert!(stderr.contains("issue_type is immutable after create"));
    assert!(stderr.contains("Remedies:"));
}

#[test]
fn test_update_with_labels_shows_label_command_remedy() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path();

    bead_cmd()
        .arg("--skip-foreign-workspace")
        .arg("init")
        .current_dir(workspace)
        .assert()
        .success();

    bead_cmd()
        .arg("create")
        .arg("--title")
        .arg("Test issue")
        .current_dir(workspace)
        .assert()
        .success();

    let result = bead_cmd()
        .arg("update")
        .arg("bead-")
        .arg("--label")
        .arg("urgent")
        .current_dir(workspace)
        .assert()
        .failure();

    let output = result.get_output();
    assert_eq!(output.status.code(), Some(2));

    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(stderr.contains("Domain rule:"));
    assert!(stderr.contains("labels are managed via 'bead label add|remove'"));
    assert!(stderr.contains("Remedies:"));
    assert!(stderr.contains("bead label add"));
    assert!(stderr.contains("bead label remove"));
}

#[test]
fn test_close_with_body_flag_shows_reason_remedy() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path();

    bead_cmd()
        .arg("--skip-foreign-workspace")
        .arg("init")
        .current_dir(workspace)
        .assert()
        .success();

    bead_cmd()
        .arg("create")
        .arg("--title")
        .arg("Test issue")
        .current_dir(workspace)
        .assert()
        .success();

    let result = bead_cmd()
        .arg("close")
        .arg("bead-")
        .arg("--body")
        .arg("Done")
        .current_dir(workspace)
        .assert()
        .failure();

    let output = result.get_output();
    assert_eq!(output.status.code(), Some(2));

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Must name the correct flag
    assert!(stderr.contains("close command requires '--reason', not '--body'"));

    // Must state the domain rule
    assert!(stderr.contains("Domain rule:"));
    assert!(stderr.contains("close requires a non-empty reason argument"));

    // Must provide the remedy
    assert!(stderr.contains("Remedy:"));
    assert!(stderr.contains("bead close"));
    assert!(stderr.contains("--reason"));

    // Should provide an example
    assert!(stderr.contains("Example:"));
}

#[test]
fn test_create_with_status_shows_remedy() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path();

    bead_cmd()
        .arg("--skip-foreign-workspace")
        .arg("init")
        .current_dir(workspace)
        .assert()
        .success();

    let result = bead_cmd()
        .arg("create")
        .arg("--title")
        .arg("Test issue")
        .arg("--status")
        .arg("open")
        .current_dir(workspace)
        .assert()
        .failure();

    let output = result.get_output();
    assert_eq!(output.status.code(), Some(2));

    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(stderr.contains("Domain rule:"));
    assert!(stderr.contains("create initializes an issue with basic fields"));
    assert!(stderr.contains("Remedies:"));
    assert!(stderr.contains("new issues start as 'open'"));
    assert!(stderr.contains("bead update"));
}

#[test]
fn test_label_without_subcommand_shows_remedy() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path();

    bead_cmd()
        .arg("--skip-foreign-workspace")
        .arg("init")
        .current_dir(workspace)
        .assert()
        .success();

    let result = bead_cmd()
        .arg("label")
        .arg("bead-")
        .arg("--label")
        .arg("urgent")
        .current_dir(workspace)
        .assert()
        .failure();

    let output = result.get_output();
    assert_eq!(output.status.code(), Some(2));

    let stderr = String::from_utf8_lossy(&output.stderr);

    // The label command requires add/remove subcommand, so clap handles this
    // This is not a near-miss case that R037 needs to handle specially
    assert!(stderr.contains("unrecognized subcommand") || stderr.contains("required"));
}

#[test]
fn test_update_with_multiple_immutable_fields_shows_all() {
    // Test that multiple errors are all reported together
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path();

    bead_cmd()
        .arg("--skip-foreign-workspace")
        .arg("init")
        .current_dir(workspace)
        .assert()
        .success();

    bead_cmd()
        .arg("create")
        .arg("--title")
        .arg("Test issue")
        .current_dir(workspace)
        .assert()
        .success();

    let result = bead_cmd()
        .arg("update")
        .arg("bead-")
        .arg("--title")
        .arg("New title")
        .arg("--priority")
        .arg("0")
        .current_dir(workspace)
        .assert()
        .failure();

    let output = result.get_output();
    assert_eq!(output.status.code(), Some(2));

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should list all the errors
    assert!(stderr.contains("title is immutable after create"));
    assert!(stderr.contains("priority is immutable after create"));

    // Should still provide domain rule and remedies once
    assert!(stderr.contains("Domain rule:"));
    assert!(stderr.contains("Remedies:"));
}

#[test]
fn test_help_reference_in_error_message() {
    // Error messages should reference --help for complete usage
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path();

    bead_cmd()
        .arg("--skip-foreign-workspace")
        .arg("init")
        .current_dir(workspace)
        .assert()
        .success();

    let result = bead_cmd()
        .arg("update")
        .arg("bead-")
        .arg("--title")
        .arg("New")
        .current_dir(workspace)
        .assert()
        .failure();

    let stderr = String::from_utf8_lossy(&result.get_output().stderr);

    // Should reference help for complete usage information
    assert!(stderr.contains("--help"));
}

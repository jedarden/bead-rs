//! Integration tests for R024 explicit recurring-bead materialization

use std::path::Path;
use std::process::Command;

fn create_test_workspace() -> tempfile::TempDir {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace_dir = temp_dir.path();

    run_bead_command(&["init"], workspace_dir);

    temp_dir
}

fn run_bead_command(args: &[&str], workspace_dir: &Path) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_bead"))
        .args(args)
        .current_dir(workspace_dir)
        .output()
        .expect("Failed to execute bead command");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        panic!(
            "Command failed: {:?}\nstdout: {}\nstderr: {}",
            args, stdout, stderr
        );
    }

    format!("{}{}", stdout, stderr)
}

fn run_bead_command_full(args: &[&str], workspace_dir: &Path) -> (String, String, i32) {
    let output = Command::new(env!("CARGO_BIN_EXE_bead"))
        .args(args)
        .current_dir(workspace_dir)
        .output()
        .expect("Failed to execute bead command");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let exit_code = output.status.code().unwrap_or(1);

    (stdout, stderr, exit_code)
}

#[test]
fn test_recurrence_create_basic() {
    let temp_dir = create_test_workspace();

    let output = run_bead_command(
        &[
            "recurrence",
            "create",
            "--id",
            "template-001",
            "--title",
            "Daily Review",
            "--base-title-template",
            "Daily Review {n}",
            "--priority",
            "2",
        ],
        temp_dir.path(),
    );

    assert!(output.contains("Created recurrence template"));
    assert!(output.contains("template-001"));
    assert!(output.contains("Daily Review"));
}

#[test]
fn test_recurrence_create_with_labels() {
    let temp_dir = create_test_workspace();

    let output = run_bead_command(
        &[
            "recurrence",
            "create",
            "--id",
            "template-001",
            "--title",
            "Weekly Planning",
            "--base-title-template",
            "Week {n} Planning",
            "--labels",
            "weekly,planning",
        ],
        temp_dir.path(),
    );

    assert!(output.contains("Created recurrence template"));
    assert!(output.contains("weekly, planning"));
}

#[test]
fn test_recurrence_create_duplicate() {
    let temp_dir = create_test_workspace();

    // Create first template
    run_bead_command(
        &[
            "recurrence",
            "create",
            "--id",
            "template-001",
            "--title",
            "Daily Review",
            "--base-title-template",
            "Daily Review {n}",
        ],
        temp_dir.path(),
    );

    // Try to create duplicate
    let (_stdout, stderr, exit_code) = run_bead_command_full(
        &[
            "recurrence",
            "create",
            "--id",
            "template-001",
            "--title",
            "Another Template",
            "--base-title-template",
            "Another {n}",
        ],
        temp_dir.path(),
    );

    assert_ne!(exit_code, 0);
    assert!(stderr.contains("already exists") || stderr.contains("conflict"));
}

#[test]
fn test_recurrence_show() {
    let temp_dir = create_test_workspace();

    // Create template
    run_bead_command(
        &[
            "recurrence",
            "create",
            "--id",
            "template-001",
            "--title",
            "Daily Review",
            "--base-title-template",
            "Daily Review {n}",
            "--description",
            "Daily standup review",
        ],
        temp_dir.path(),
    );

    // Show template
    let output = run_bead_command(
        &["recurrence", "show", "--id", "template-001"],
        temp_dir.path(),
    );

    assert!(output.contains("Recurrence Template"));
    assert!(output.contains("template-001"));
    assert!(output.contains("Daily Review"));
    assert!(output.contains("Daily standup review"));
    assert!(output.contains("No occurrences materialized yet"));
}

#[test]
fn test_recurrence_show_json() {
    let temp_dir = create_test_workspace();

    // Create template
    run_bead_command(
        &[
            "recurrence",
            "create",
            "--id",
            "template-001",
            "--title",
            "Daily Review",
            "--base-title-template",
            "Daily Review {n}",
        ],
        temp_dir.path(),
    );

    // Show template with JSON
    let output = run_bead_command(
        &["recurrence", "show", "--id", "template-001", "--json"],
        temp_dir.path(),
    );

    assert!(output.contains("\"id\":"));
    assert!(output.contains("\"title\":"));
    assert!(output.contains("\"base_title_template\":"));
}

#[test]
fn test_recurrence_list_empty() {
    let temp_dir = create_test_workspace();

    let output = run_bead_command(&["recurrence", "list"], temp_dir.path());

    assert!(output.contains("No recurrence templates found"));
}

#[test]
fn test_recurrence_list() {
    let temp_dir = create_test_workspace();

    // Create two templates
    run_bead_command(
        &[
            "recurrence",
            "create",
            "--id",
            "template-001",
            "--title",
            "Daily Review",
            "--base-title-template",
            "Daily Review {n}",
        ],
        temp_dir.path(),
    );

    run_bead_command(
        &[
            "recurrence",
            "create",
            "--id",
            "template-002",
            "--title",
            "Weekly Planning",
            "--base-title-template",
            "Week {n} Planning",
        ],
        temp_dir.path(),
    );

    // List templates
    let output = run_bead_command(&["recurrence", "list"], temp_dir.path());

    assert!(output.contains("template-001"));
    assert!(output.contains("Daily Review"));
    assert!(output.contains("template-002"));
    assert!(output.contains("Weekly Planning"));
}

#[test]
fn test_recurrence_list_json() {
    let temp_dir = create_test_workspace();

    // Create template
    run_bead_command(
        &[
            "recurrence",
            "create",
            "--id",
            "template-001",
            "--title",
            "Daily Review",
            "--base-title-template",
            "Daily Review {n}",
        ],
        temp_dir.path(),
    );

    // List templates with JSON
    let output = run_bead_command(&["recurrence", "list", "--json"], temp_dir.path());

    assert!(output.contains("["));
    assert!(output.contains("\"id\":"));
}

#[test]
fn test_recurrence_delete() {
    let temp_dir = create_test_workspace();

    // Create template
    run_bead_command(
        &[
            "recurrence",
            "create",
            "--id",
            "template-001",
            "--title",
            "Daily Review",
            "--base-title-template",
            "Daily Review {n}",
        ],
        temp_dir.path(),
    );

    // Delete template
    let output = run_bead_command(
        &["recurrence", "delete", "--id", "template-001"],
        temp_dir.path(),
    );

    assert!(output.contains("Deleted recurrence template"));
    assert!(output.contains("template-001"));
}

#[test]
fn test_recurrence_materialize_basic() {
    let temp_dir = create_test_workspace();

    // Create template
    run_bead_command(
        &[
            "recurrence",
            "create",
            "--id",
            "template-001",
            "--title",
            "Daily Review",
            "--base-title-template",
            "Daily Review {n}",
            "--priority",
            "2",
        ],
        temp_dir.path(),
    );

    // Materialize first occurrence
    let output = run_bead_command(
        &["recurrence", "materialize", "--id", "template-001"],
        temp_dir.path(),
    );

    assert!(output.contains("Materialized next occurrence"));
    assert!(output.contains("Sequence: 1"));
    assert!(output.contains("template-001"));

    // Verify the issue was created
    let show_output = run_bead_command(
        &["recurrence", "show", "--id", "template-001"],
        temp_dir.path(),
    );
    assert!(show_output.contains("Sequence 1"));
}

#[test]
fn test_recurrence_materialize_sequence_incrementing() {
    let temp_dir = create_test_workspace();

    // Create template
    run_bead_command(
        &[
            "recurrence",
            "create",
            "--id",
            "template-001",
            "--title",
            "Daily Review",
            "--base-title-template",
            "Daily Review {n}",
        ],
        temp_dir.path(),
    );

    // Materialize first occurrence
    run_bead_command(
        &["recurrence", "materialize", "--id", "template-001"],
        temp_dir.path(),
    );

    // Materialize second occurrence
    let output = run_bead_command(
        &["recurrence", "materialize", "--id", "template-001"],
        temp_dir.path(),
    );

    assert!(output.contains("Sequence: 2"));
}

#[test]
fn test_recurrence_materialize_with_actor() {
    let temp_dir = create_test_workspace();

    // Create template
    run_bead_command(
        &[
            "recurrence",
            "create",
            "--id",
            "template-001",
            "--title",
            "Daily Review",
            "--base-title-template",
            "Daily Review {n}",
        ],
        temp_dir.path(),
    );

    // Materialize with actor
    let output = run_bead_command(
        &[
            "recurrence",
            "materialize",
            "--id",
            "template-001",
            "--actor",
            "scheduler-1",
        ],
        temp_dir.path(),
    );

    assert!(output.contains("Actor: scheduler-1"));
}

#[test]
fn test_recurrence_materialize_creates_valid_issue() {
    let temp_dir = create_test_workspace();

    // Create template
    run_bead_command(
        &[
            "recurrence",
            "create",
            "--id",
            "template-001",
            "--title",
            "Daily Review",
            "--base-title-template",
            "Daily Review {n}",
            "--labels",
            "daily,recurring",
        ],
        temp_dir.path(),
    );

    // Materialize occurrence
    let output = run_bead_command(
        &["recurrence", "materialize", "--id", "template-001"],
        temp_dir.path(),
    );

    // Extract issue ID from output
    let issue_id = output
        .lines()
        .find(|line| line.contains("Issue ID:"))
        .unwrap()
        .split("Issue ID:")
        .nth(1)
        .unwrap()
        .trim()
        .to_string();

    // Verify the issue exists and has correct properties
    let show_output = run_bead_command(&["show", &issue_id, "--json"], temp_dir.path());
    assert!(show_output.contains("Daily Review 1"));
    assert!(show_output.contains("daily"));
    assert!(show_output.contains("recurring"));
}

#[test]
fn test_recurrence_history() {
    let temp_dir = create_test_workspace();

    // Create template
    run_bead_command(
        &[
            "recurrence",
            "create",
            "--id",
            "template-001",
            "--title",
            "Daily Review",
            "--base-title-template",
            "Daily Review {n}",
        ],
        temp_dir.path(),
    );

    // Materialize two occurrences
    run_bead_command(
        &["recurrence", "materialize", "--id", "template-001"],
        temp_dir.path(),
    );
    run_bead_command(
        &["recurrence", "materialize", "--id", "template-001"],
        temp_dir.path(),
    );

    // Show history
    let output = run_bead_command(
        &["recurrence", "history", "--id", "template-001"],
        temp_dir.path(),
    );

    assert!(output.contains("Materialization History"));
    assert!(output.contains("Sequence 1"));
    assert!(output.contains("Sequence 2"));
}

#[test]
fn test_recurrence_history_json() {
    let temp_dir = create_test_workspace();

    // Create template and materialize occurrence
    run_bead_command(
        &[
            "recurrence",
            "create",
            "--id",
            "template-001",
            "--title",
            "Daily Review",
            "--base-title-template",
            "Daily Review {n}",
        ],
        temp_dir.path(),
    );

    run_bead_command(
        &["recurrence", "materialize", "--id", "template-001"],
        temp_dir.path(),
    );

    // Show history with JSON
    let output = run_bead_command(
        &["recurrence", "history", "--id", "template-001", "--json"],
        temp_dir.path(),
    );

    assert!(output.contains("["));
    assert!(output.contains("\"series_sequence\":"));
}

#[test]
fn test_recurrence_invalid_template_id() {
    let temp_dir = create_test_workspace();

    let (_stdout, stderr, exit_code) = run_bead_command_full(
        &["recurrence", "show", "--id", "nonexistent"],
        temp_dir.path(),
    );

    assert_ne!(exit_code, 0);
    assert!(stderr.contains("not found") || stderr.contains("No recurrence template"));
}

#[test]
fn test_recurrence_materialize_nonexistent_template() {
    let temp_dir = create_test_workspace();

    let (_stdout, stderr, exit_code) = run_bead_command_full(
        &["recurrence", "materialize", "--id", "nonexistent"],
        temp_dir.path(),
    );

    assert_ne!(exit_code, 0);
    assert!(stderr.contains("not found"));
}

#[test]
fn test_recurrence_delete_nonexistent_template() {
    let temp_dir = create_test_workspace();

    let (_stdout, stderr, exit_code) = run_bead_command_full(
        &["recurrence", "delete", "--id", "nonexistent"],
        temp_dir.path(),
    );

    assert_ne!(exit_code, 0);
    assert!(stderr.contains("not found"));
}

#[test]
fn test_recurrence_help() {
    let temp_dir = create_test_workspace();

    // `-h` shows the summary.
    let short = run_bead_command(&["recurrence", "-h"], temp_dir.path());
    assert!(short.contains("Manage recurrence templates"));

    // `--help` shows the long description, which must be distinct from the
    // summary -- if the two are identical the long help has been shadowed.
    let output = run_bead_command(&["recurrence", "--help"], temp_dir.path());
    assert!(output.contains("Define templates that mint repeat issues on demand"));
    assert!(!output.contains("Manage recurrence templates"));

    for subcommand in ["create", "show", "list", "delete", "materialize", "history"] {
        assert!(
            output.contains(subcommand),
            "`bead recurrence --help` does not list `{subcommand}`"
        );
    }
}

#[test]
fn test_recurrence_no_workspace() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace_dir = temp_dir.path();

    let (_stdout, stderr, exit_code) =
        run_bead_command_full(&["recurrence", "list"], workspace_dir);

    assert_ne!(exit_code, 0);
    assert!(stderr.contains("No workspace found") || stderr.contains("init"));
}

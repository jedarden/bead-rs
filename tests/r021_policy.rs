// Integration tests for R021 workspace policy lint

use assert_cmd::Command;
use tempfile::TempDir;

fn run_bead_command(args: &[&str], workspace_path: &std::path::Path) -> std::process::Output {
    Command::new("/home/needle/target/debug/bead")
        .args(args)
        .current_dir(workspace_path)
        .output()
        .expect("Failed to execute bead command")
}

fn create_test_workspace() -> TempDir {
    let temp_dir = TempDir::new().unwrap();
    let workspace_path = temp_dir.path();

    // Initialize workspace
    let _output = run_bead_command(&["init"], workspace_path);

    temp_dir
}

#[test]
fn test_policy_check_basic() {
    let workspace = create_test_workspace();

    // Test basic policy check
    let output = run_bead_command(&["policy", "check"], workspace.path());

    assert!(output.status.success(), "Policy check failed: {:?}", output);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Workspace Policy Validation"));
    assert!(stdout.contains("Schema Version:"));
    assert!(stdout.contains("Policy Version:"));
}

#[test]
fn test_policy_check_json_output() {
    let workspace = create_test_workspace();

    // Test JSON output
    let output = run_bead_command(&["policy", "check", "--format", "json"], workspace.path());

    assert!(
        output.status.success(),
        "Policy check JSON failed: {:?}",
        output
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("config_schema_version"));
    assert!(stdout.contains("policy_version"));
    assert!(stdout.contains("status"));
    assert!(stdout.contains("findings"));
    assert!(stdout.contains("summary"));
}

#[test]
fn test_policy_check_fifo_v1() {
    let workspace = create_test_workspace();

    // Test fifo-v1 policy
    let output = run_bead_command(
        &["policy", "check", "--policy", "fifo-v1"],
        workspace.path(),
    );

    assert!(
        output.status.success(),
        "FIFO-v1 policy check failed: {:?}",
        output
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("fifo-v1") || stdout.contains("Workspace Policy Validation"));
}

#[test]
fn test_policy_check_balanced_v1() {
    let workspace = create_test_workspace();

    // Test balanced-v1 policy
    let output = run_bead_command(
        &["policy", "check", "--policy", "balanced-v1"],
        workspace.path(),
    );

    assert!(
        output.status.success(),
        "Balanced-v1 policy check failed: {:?}",
        output
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("balanced-v1") || stdout.contains("Workspace Policy Validation"));
}

#[test]
fn test_policy_check_unknown_version() {
    let workspace = create_test_workspace();

    // Test with unknown policy version
    let output = run_bead_command(
        &["policy", "check", "--policy-version", "v99"],
        workspace.path(),
    );

    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Unknown Version") || stdout.contains("Unknown configuration version"));
}

#[test]
fn test_policy_check_no_workspace() {
    let temp_dir = TempDir::new().unwrap();

    // Test without workspace
    let output = run_bead_command(&["policy", "check"], temp_dir.path());

    assert!(!output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    let error_output = format!("{}{}", stdout, stderr);
    assert!(
        error_output.contains("No workspace found")
            || error_output.contains("Run `bead init` first")
    );
}

#[test]
fn test_policy_check_help() {
    // Test that help is available
    let output = run_bead_command(&["policy", "check", "--help"], std::path::Path::new("."));

    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Diagnose"));
    assert!(stdout.contains("scheduling") || stdout.contains("Scheduling"));
    assert!(stdout.contains("retention") || stdout.contains("Retention"));
}

#[test]
fn test_policy_check_aging_v1() {
    let workspace = create_test_workspace();

    // Test aging-v1 policy
    let output = run_bead_command(
        &["policy", "check", "--policy", "aging-v1"],
        workspace.path(),
    );

    assert!(
        output.status.success(),
        "Aging-v1 policy check failed: {:?}",
        output
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("aging-v1") || stdout.contains("Workspace Policy Validation"));
}

#[test]
fn test_policy_check_rotation_v1() {
    let workspace = create_test_workspace();

    // Test rotation-v1 policy
    let output = run_bead_command(
        &["policy", "check", "--policy", "rotation-v1"],
        workspace.path(),
    );

    assert!(
        output.status.success(),
        "Rotation-v1 policy check failed: {:?}",
        output
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("rotation-v1") || stdout.contains("Workspace Policy Validation"));
}

#[test]
fn test_policy_check_impact_v1() {
    let workspace = create_test_workspace();

    // Test impact-v1 policy
    let output = run_bead_command(
        &["policy", "check", "--policy", "impact-v1"],
        workspace.path(),
    );

    assert!(
        output.status.success(),
        "Impact-v1 policy check failed: {:?}",
        output
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("impact-v1") || stdout.contains("Workspace Policy Validation"));
}

#[test]
fn test_policy_check_json_structure() {
    let workspace = create_test_workspace();

    // Test JSON structure is valid
    let output = run_bead_command(&["policy", "check", "--format", "json"], workspace.path());

    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify JSON structure
    assert!(stdout.contains(r#""config_schema_version":"#));
    assert!(stdout.contains(r#""policy_version":"#));
    assert!(stdout.contains(r#""status":"#));
    assert!(stdout.contains(r#""findings":"#));
    assert!(stdout.contains(r#""summary":"#));
    assert!(stdout.contains(r#""total_findings":"#));
    assert!(stdout.contains(r#""critical_count":"#));
    assert!(stdout.contains(r#""error_count":"#));
    assert!(stdout.contains(r#""warning_count":"#));
    assert!(stdout.contains(r#""info_count":"#));
    assert!(stdout.contains(r#""validation_success":"#));
}

#[test]
fn test_policy_check_with_various_policies() {
    let workspace = create_test_workspace();

    let policies = vec![
        "fifo-v1",
        "balanced-v1",
        "aging-v1",
        "impact-v1",
        "rotation-v1",
    ];

    for policy in policies {
        let output = run_bead_command(&["policy", "check", "--policy", policy], workspace.path());

        assert!(
            output.status.success(),
            "Policy {} check failed: {:?}",
            policy,
            output
        );

        let stdout = String::from_utf8_lossy(&output.stdout);
        // Check that the command succeeded and shows validation results
        assert!(stdout.contains("Workspace Policy Validation"));
        assert!(stdout.contains("Schema Version:") || stdout.contains("config_schema_version"));
    }
}

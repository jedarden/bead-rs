// Integration tests for R020 cross-profile semantic comparison
//
// These tests verify that the comparison command works correctly for
// rendering selected native records through two explicit installed profiles
// and reporting preserved, transformed, omitted, and unsupported semantic fields.

use assert_cmd::Command;
use serial_test::serial;

#[test]
#[serial]
fn test_comparison_help_available() {
    Command::cargo_bin("bead")
        .unwrap()
        .arg("compare")
        .arg("--help")
        .assert()
        .success();
}

#[test]
#[serial]
fn test_comparison_basic_native_to_needle() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace_dir = temp_dir.path();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .arg("init")
        .current_dir(workspace_dir)
        .env("HOME", workspace_dir.to_str().unwrap())
        .assert()
        .success();

    // Create a test issue
    let issue_id = Command::cargo_bin("bead")
        .unwrap()
        .arg("create")
        .arg("--title")
        .arg("Test comparison issue")
        .arg("--description")
        .arg("This is a test issue for profile comparison")
        .arg("--priority")
        .arg("1")
        .current_dir(workspace_dir)
        .env("HOME", workspace_dir.to_str().unwrap())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let issue_id = String::from_utf8_lossy(&issue_id).trim().to_string();
    assert!(!issue_id.is_empty());

    // Compare between native-v1 and needle-v1
    Command::cargo_bin("bead")
        .unwrap()
        .arg("compare")
        .arg("--id")
        .arg(&issue_id)
        .arg("--source")
        .arg("native-v1")
        .arg("--target")
        .arg("needle-v1")
        .current_dir(workspace_dir)
        .env("HOME", workspace_dir.to_str().unwrap())
        .assert()
        .success()
        .stdout(predicates::str::contains("Preserved"))
        .stdout(predicates::str::contains("native-v1"))
        .stdout(predicates::str::contains("needle-v1"));
}

#[test]
#[serial]
fn test_comparison_json_output() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace_dir = temp_dir.path();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .arg("init")
        .current_dir(workspace_dir)
        .env("HOME", workspace_dir.to_str().unwrap())
        .assert()
        .success();

    // Create a test issue
    let issue_id = Command::cargo_bin("bead")
        .unwrap()
        .arg("create")
        .arg("--title")
        .arg("Test JSON comparison")
        .arg("--priority")
        .arg("2")
        .current_dir(workspace_dir)
        .env("HOME", workspace_dir.to_str().unwrap())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let issue_id = String::from_utf8_lossy(&issue_id).trim().to_string();

    // Test JSON output
    Command::cargo_bin("bead")
        .unwrap()
        .arg("compare")
        .arg("--id")
        .arg(&issue_id)
        .arg("--source")
        .arg("native-v1")
        .arg("--target")
        .arg("needle-v1")
        .arg("--json")
        .current_dir(workspace_dir)
        .env("HOME", workspace_dir.to_str().unwrap())
        .assert()
        .success()
        .stdout(predicates::str::contains(r#""issue_id"#))
        .stdout(predicates::str::contains(r#""source_profile""#))
        .stdout(predicates::str::contains(r#""target_profile""#))
        .stdout(predicates::str::contains(r#""field_comparisons""#))
        .stdout(predicates::str::contains(r#""summary""#));
}

#[test]
#[serial]
fn test_comparison_nonexistent_issue() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace_dir = temp_dir.path();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .arg("init")
        .current_dir(workspace_dir)
        .env("HOME", workspace_dir.to_str().unwrap())
        .assert()
        .success();

    // Try to compare non-existent issue
    Command::cargo_bin("bead")
        .unwrap()
        .arg("compare")
        .arg("--id")
        .arg("nonexistent-id")
        .arg("--source")
        .arg("native-v1")
        .arg("--target")
        .arg("needle-v1")
        .current_dir(workspace_dir)
        .env("HOME", workspace_dir.to_str().unwrap())
        .assert()
        .failure()
        .stderr(predicates::str::contains("not found"));
}

#[test]
#[serial]
fn test_comparison_invalid_profile() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace_dir = temp_dir.path();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .arg("init")
        .current_dir(workspace_dir)
        .env("HOME", workspace_dir.to_str().unwrap())
        .assert()
        .success();

    // Create a test issue
    let issue_id = Command::cargo_bin("bead")
        .unwrap()
        .arg("create")
        .arg("--title")
        .arg("Test invalid profile")
        .current_dir(workspace_dir)
        .env("HOME", workspace_dir.to_str().unwrap())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let issue_id = String::from_utf8_lossy(&issue_id).trim().to_string();

    // Try to use invalid profile
    Command::cargo_bin("bead")
        .unwrap()
        .arg("compare")
        .arg("--id")
        .arg(&issue_id)
        .arg("--source")
        .arg("invalid-profile")
        .arg("--target")
        .arg("needle-v1")
        .current_dir(workspace_dir)
        .env("HOME", workspace_dir.to_str().unwrap())
        .assert()
        .failure()
        .stderr(predicates::str::contains("Unsupported profile"));
}

#[test]
#[serial]
fn test_comparison_no_workspace() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace_dir = temp_dir.path();

    // Try to compare without workspace. With the directory properly isolated
    // this reaches real workspace discovery, which reports the same message
    // every other command uses for a missing workspace.
    Command::cargo_bin("bead")
        .unwrap()
        .arg("compare")
        .arg("--id")
        .arg("some-id")
        .arg("--source")
        .arg("native-v1")
        .arg("--target")
        .arg("needle-v1")
        .current_dir(workspace_dir)
        .env("HOME", workspace_dir.to_str().unwrap())
        .assert()
        .failure()
        .stderr(predicates::str::contains("No workspace found"));
}

#[test]
#[serial]
fn test_comparison_bound_record_count() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace_dir = temp_dir.path();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .arg("init")
        .current_dir(workspace_dir)
        .env("HOME", workspace_dir.to_str().unwrap())
        .assert()
        .success();

    // Create a test issue
    let issue_id = Command::cargo_bin("bead")
        .unwrap()
        .arg("create")
        .arg("--title")
        .arg("Test bound record count")
        .arg("--description")
        .arg("Testing that comparison handles single issue")
        .current_dir(workspace_dir)
        .env("HOME", workspace_dir.to_str().unwrap())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let issue_id_str = String::from_utf8_lossy(&issue_id).trim().to_string();

    // Verify comparison is bound to single issue (not affecting other records)
    Command::cargo_bin("bead")
        .unwrap()
        .arg("compare")
        .arg("--id")
        .arg(&issue_id_str)
        .arg("--source")
        .arg("native-v1")
        .arg("--target")
        .arg("needle-v1")
        .current_dir(workspace_dir)
        .env("HOME", workspace_dir.to_str().unwrap())
        .assert()
        .success()
        .stdout(predicates::str::contains(&issue_id_str));

    // Create another issue to verify it wasn't affected
    Command::cargo_bin("bead")
        .unwrap()
        .arg("create")
        .arg("--title")
        .arg("Second issue")
        .current_dir(workspace_dir)
        .env("HOME", workspace_dir.to_str().unwrap())
        .assert()
        .success();

    // Show the first issue to verify it's unchanged
    let show_output = Command::cargo_bin("bead")
        .unwrap()
        .arg("show")
        .arg(&issue_id_str)
        .arg("--json")
        .current_dir(workspace_dir)
        .env("HOME", workspace_dir.to_str().unwrap())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    // Verify the original issue still exists and has the same ID
    let show_json = String::from_utf8_lossy(&show_output);
    assert!(
        show_json.contains(&format!("\"id\":\"{}\"", issue_id_str)),
        "Original issue should still exist"
    );
}

#[test]
#[serial]
fn test_comparison_read_only_operation() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace_dir = temp_dir.path();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .arg("init")
        .current_dir(workspace_dir)
        .env("HOME", workspace_dir.to_str().unwrap())
        .assert()
        .success();

    // Create a test issue
    let issue_id = Command::cargo_bin("bead")
        .unwrap()
        .arg("create")
        .arg("--title")
        .arg("Test read-only")
        .arg("--priority")
        .arg("2")
        .current_dir(workspace_dir)
        .env("HOME", workspace_dir.to_str().unwrap())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let test_issue_id = String::from_utf8_lossy(&issue_id).trim().to_string();

    // Get initial issue state
    let initial_show_bytes = Command::cargo_bin("bead")
        .unwrap()
        .arg("show")
        .arg(&test_issue_id)
        .arg("--json")
        .current_dir(workspace_dir)
        .env("HOME", workspace_dir.to_str().unwrap())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let initial_show = initial_show_bytes.clone();

    // Run comparison (should not modify anything)
    Command::cargo_bin("bead")
        .unwrap()
        .arg("compare")
        .arg("--id")
        .arg(&test_issue_id)
        .arg("--source")
        .arg("native-v1")
        .arg("--target")
        .arg("needle-v1")
        .current_dir(workspace_dir)
        .env("HOME", workspace_dir.to_str().unwrap())
        .assert()
        .success();

    // Get issue state after comparison
    let after_show_bytes = Command::cargo_bin("bead")
        .unwrap()
        .arg("show")
        .arg(&test_issue_id)
        .arg("--json")
        .current_dir(workspace_dir)
        .env("HOME", workspace_dir.to_str().unwrap())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let after_show = after_show_bytes;

    // Verify state is unchanged
    assert_eq!(
        initial_show, after_show,
        "Comparison should not modify issue state"
    );
}

#[test]
#[serial]
fn test_comparison_reports_real_dependencies_and_labels() {
    // Regression test: `compare_issue_profiles` used to call the base,
    // record-less `native_to_profile(&issue)` trait method on both sides,
    // which every adapter either hardcodes empty dependencies/labels for
    // (needle-v1) or can only fill from a `native_record_to_profile` call
    // it never received (native-v1) -- so `compare` always reported
    // every real dependency/label as "Added in target" with an empty
    // value, regardless of which profiles were compared or what data the
    // issue actually had.
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace_dir = temp_dir.path();

    Command::cargo_bin("bead")
        .unwrap()
        .arg("init")
        .current_dir(workspace_dir)
        .env("HOME", workspace_dir.to_str().unwrap())
        .assert()
        .success();

    let blocker_id = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Blocker"])
        .current_dir(workspace_dir)
        .env("HOME", workspace_dir.to_str().unwrap())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let blocker_id = String::from_utf8_lossy(&blocker_id).trim().to_string();

    let blocked_id = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Blocked"])
        .current_dir(workspace_dir)
        .env("HOME", workspace_dir.to_str().unwrap())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let blocked_id = String::from_utf8_lossy(&blocked_id).trim().to_string();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["dep", "add", &blocked_id, &blocker_id, "--kind", "blocks"])
        .current_dir(workspace_dir)
        .env("HOME", workspace_dir.to_str().unwrap())
        .assert()
        .success();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["label", "add", &blocked_id, "--label", "urgent"])
        .current_dir(workspace_dir)
        .env("HOME", workspace_dir.to_str().unwrap())
        .assert()
        .success();

    let output = Command::cargo_bin("bead")
        .unwrap()
        .args([
            "compare",
            "--id",
            &blocked_id,
            "--source",
            "native-v1",
            "--target",
            "needle-v1",
        ])
        .current_dir(workspace_dir)
        .env("HOME", workspace_dir.to_str().unwrap())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let output = String::from_utf8_lossy(&output);

    assert!(
        output.contains(&blocker_id),
        "compare output should contain the real blocker id, not an empty dependencies array:\n{output}"
    );
    assert!(
        output.contains("urgent"),
        "compare output should contain the real label, not an empty labels array:\n{output}"
    );
    assert!(
        output.contains("[dependencies] Preserved") || output.contains("[dependencies] Transformed"),
        "dependencies should be reported as Preserved/Transformed, not Added/Omitted, once both sides receive real record data:\n{output}"
    );
}

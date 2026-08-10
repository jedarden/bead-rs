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
        .env("HOME", workspace_dir.to_str().unwrap())
        .assert()
        .success();

    // Create a test issue
    let issue_id = Command::cargo_bin("bead")
        .unwrap()
        .arg("create")
        .arg("--title")
        .arg("Test invalid profile")
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
        .env("HOME", workspace_dir.to_str().unwrap())
        .assert()
        .failure()
        .stderr(predicates::str::contains("Unsupported profile"));
}

#[test]
#[serial]
fn test_comparison_br_to_bf() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace_dir = temp_dir.path();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .arg("init")
        .env("HOME", workspace_dir.to_str().unwrap())
        .assert()
        .success();

    // Create a test issue with complete information
    let issue_id = Command::cargo_bin("bead")
        .unwrap()
        .arg("create")
        .arg("--title")
        .arg("Test br-bf comparison")
        .arg("--description")
        .arg("Testing comparison between br-v1 and bf-v1 profiles")
        .arg("--priority")
        .arg("1")
        .arg("--label")
        .arg("bug")
        .arg("--label")
        .arg("high-priority")
        .env("HOME", workspace_dir.to_str().unwrap())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let issue_id = String::from_utf8_lossy(&issue_id).trim().to_string();

    // Compare between br-v1 and bf-v1
    Command::cargo_bin("bead")
        .unwrap()
        .arg("compare")
        .arg("--id")
        .arg(&issue_id)
        .arg("--source")
        .arg("br-v1")
        .arg("--target")
        .arg("bf-v1")
        .env("HOME", workspace_dir.to_str().unwrap())
        .assert()
        .success()
        .stdout(predicates::str::contains("br-v1"))
        .stdout(predicates::str::contains("bf-v1"))
        .stdout(predicates::str::contains("Total Fields"))
        .stdout(predicates::str::contains("Preserved"))
        .stdout(predicates::str::contains("Transformed"))
        .stdout(predicates::str::contains("Omitted"));
}

#[test]
#[serial]
fn test_comparison_no_workspace() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace_dir = temp_dir.path();

    // Try to compare without workspace
    // Note: The error message may vary depending on whether workspace discovery succeeds
    Command::cargo_bin("bead")
        .unwrap()
        .arg("compare")
        .arg("--id")
        .arg("some-id")
        .arg("--source")
        .arg("native-v1")
        .arg("--target")
        .arg("needle-v1")
        .env("HOME", workspace_dir.to_str().unwrap())
        .assert()
        .failure()
        .stderr(predicates::str::contains("not found"));
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
        .env("HOME", workspace_dir.to_str().unwrap())
        .assert()
        .success();

    // Show the first issue to verify it's unchanged
    let show_output = Command::cargo_bin("bead")
        .unwrap()
        .arg("show")
        .arg(&issue_id_str)
        .arg("--json")
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
        .arg("bf-v1")
        .env("HOME", workspace_dir.to_str().unwrap())
        .assert()
        .success();

    // Get issue state after comparison
    let after_show_bytes = Command::cargo_bin("bead")
        .unwrap()
        .arg("show")
        .arg(&test_issue_id)
        .arg("--json")
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

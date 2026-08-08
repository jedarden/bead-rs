//! Integration tests for `bead capabilities` command

use assert_cmd::Command;
use serde_json::Value;
use serial_test::serial;

#[test]
#[serial]
fn test_capabilities_no_workspace() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Save original directory to restore later
    let original_dir = std::env::current_dir().unwrap().canonicalize().unwrap();

    // Capabilities should work even without a workspace
    let result = Command::cargo_bin("bead")
        .unwrap()
        .args(["capabilities"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output = std::str::from_utf8(&result).unwrap();
    // Verify it's valid JSON
    let _: Value = serde_json::from_str(output).unwrap();

    // Restore original directory before dropping temp
    std::env::set_current_dir(original_dir).unwrap();
}

#[test]
#[serial]
fn test_capabilities_native_profile() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Save original directory to restore later
    let original_dir = std::env::current_dir().unwrap().canonicalize().unwrap();

    // Test with native-v1 profile
    let result = Command::cargo_bin("bead")
        .unwrap()
        .args(["capabilities", "--profile", "native-v1"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output = std::str::from_utf8(&result).unwrap();
    let caps: Value = serde_json::from_str(output).unwrap();

    // Verify structure
    assert_eq!(caps["contract"], "native-v1");
    assert_eq!(caps["implementation"], "bead-rs");
    assert_eq!(caps["version"], "0.1.0");
    assert_eq!(caps["store_layout"], 1);
    assert_eq!(caps["atomic_claim"], true);
    assert_eq!(caps["priorities"]["min"], 0);
    assert_eq!(caps["priorities"]["max"], 4);
    assert_eq!(caps["priorities"]["default"], 2);
    assert_eq!(caps["priorities"]["p4_claimable_by_fifo"], true);

    // Verify statuses array
    let statuses = caps["statuses"].as_array().unwrap();
    assert!(statuses.contains(&Value::String("open".to_string())));
    assert!(statuses.contains(&Value::String("closed".to_string())));
    assert!(statuses.contains(&Value::String("in_progress".to_string())));
    assert!(statuses.contains(&Value::String("deferred".to_string())));
    assert!(statuses.contains(&Value::String("blocked".to_string())));

    // Verify checkpoint modes
    let modes = caps["checkpoint_modes"].as_array().unwrap();
    assert!(modes.contains(&Value::String("flush-only".to_string())));
    assert!(modes.contains(&Value::String("import-only".to_string())));

    // Verify checkpoint formats
    let formats = caps["checkpoint_formats"].as_array().unwrap();
    assert!(formats.contains(&Value::String("issues-jsonl-v1".to_string())));

    // Verify schema_ref
    assert_eq!(
        caps["schema_ref"],
        "urn:bead-rs:schema:capabilities:native-v1"
    );

    // Verify schemas array
    let schemas = caps["schemas"].as_array().unwrap();
    assert!(!schemas.is_empty());

    // Verify commands array
    let commands = caps["commands"].as_array().unwrap();
    assert!(commands.contains(&Value::String("capabilities".to_string())));
    assert!(commands.contains(&Value::String("claim".to_string())));
    assert!(commands.contains(&Value::String("create".to_string())));
    assert!(commands.contains(&Value::String("list".to_string())));

    // Restore original directory before dropping temp
    std::env::set_current_dir(original_dir).unwrap();
}

#[test]
#[serial]
fn test_capabilities_needle_profile() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Save original directory to restore later
    let original_dir = std::env::current_dir().unwrap().canonicalize().unwrap();

    // Test with needle-v1 profile
    let result = Command::cargo_bin("bead")
        .unwrap()
        .args(["capabilities", "--profile", "needle-v1"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output = std::str::from_utf8(&result).unwrap();
    let caps: Value = serde_json::from_str(output).unwrap();

    // Verify contract is needle-v1
    assert_eq!(caps["contract"], "needle-v1");

    // Restore original directory before dropping temp
    std::env::set_current_dir(original_dir).unwrap();
}

#[test]
#[serial]
fn test_capabilities_invalid_profile() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Save original directory to restore later
    let original_dir = std::env::current_dir().unwrap().canonicalize().unwrap();

    // Test with invalid profile
    Command::cargo_bin("bead")
        .unwrap()
        .args(["capabilities", "--profile", "invalid-profile"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("Unsupported profile"));

    // Restore original directory before dropping temp
    std::env::set_current_dir(original_dir).unwrap();
}

#[test]
#[serial]
fn test_capabilities_default_profile() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Save original directory to restore later
    let original_dir = std::env::current_dir().unwrap().canonicalize().unwrap();

    // Test with default profile (no --profile flag)
    let result = Command::cargo_bin("bead")
        .unwrap()
        .args(["capabilities"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output = std::str::from_utf8(&result).unwrap();
    let caps: Value = serde_json::from_str(output).unwrap();

    // Verify default profile is native-v1
    assert_eq!(caps["contract"], "native-v1");

    // Restore original directory before dropping temp
    std::env::set_current_dir(original_dir).unwrap();
}

#[test]
#[serial]
fn test_capabilities_schema_entries() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Save original directory to restore later
    let original_dir = std::env::current_dir().unwrap().canonicalize().unwrap();

    // Test schema entries
    let result = Command::cargo_bin("bead")
        .unwrap()
        .args(["capabilities"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output = std::str::from_utf8(&result).unwrap();
    let caps: Value = serde_json::from_str(output).unwrap();

    // Verify issue schema entry
    let schemas = caps["schemas"].as_array().unwrap();
    let issue_schema = schemas
        .iter()
        .find(|s| s["schema_ref"] == "urn:bead-rs:schema:issue:native-v1")
        .expect("Issue schema not found");

    assert_eq!(issue_schema["document_kind"], "issue");
    assert_eq!(issue_schema["validate"], true);
    assert!(issue_schema["consume"]
        .as_array()
        .unwrap()
        .contains(&Value::String("sync.import-only".to_string())));
    assert!(issue_schema["emit"]
        .as_array()
        .unwrap()
        .contains(&Value::String("sync.flush-only".to_string())));

    // Restore original directory before dropping temp
    std::env::set_current_dir(original_dir).unwrap();
}

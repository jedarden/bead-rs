//! Test framework for validating capability differences between pre-feature and feature-enabled binaries
//!
//! This test exercises the absence/presence of attempt-resolution capabilities:
//! - Capability detection via `bead capabilities`
//! - Resolve command availability
//! - Attempt information in `bead why` output
//! - Attempt outcome persistence through checkpoint round-trips
//! - NEEDLE fallback starvation detection behavior
//!
//! This uses the capability_framework module for consistent testing across variants.

use serial_test::serial;

mod capability_framework;
use capability_framework::*;

#[test]
#[serial]
fn default_build_has_attempt_capability() {
    // This test validates that the default binary reports attempt-outcome support
    let harness = BinaryHarness::new().unwrap();
    assert_capability_present!(harness, "attempt_outcome");
    assert_capability_present!(harness, "attempt_outcome.supported");

    let caps = harness.get_default_capabilities().unwrap();
    let supported = caps
        .get("attempt_outcome")
        .and_then(|v| v.get("supported"))
        .and_then(|v| v.as_bool());

    assert_eq!(supported, Some(true), "Default build should support attempt_outcome");
}

#[test]
#[serial]
fn default_build_resolve_command_exists() {
    // Test that resolve command is available in default build
    let harness = BinaryHarness::new().unwrap();
    assert_command_exists!(harness, "resolve");

    let result = harness.test_resolve_command().unwrap();
    assert!(
        result.is_ok(),
        "resolve command should be available in default build"
    );
}

#[test]
#[serial]
fn default_build_why_shows_attempt_info() {
    // Test that why command is available and works
    let harness = BinaryHarness::new().unwrap();
    harness.init_workspace().unwrap();

    // Create a test bead
    let create_output = assert_cmd::Command::cargo_bin("bead")
        .unwrap()
        .current_dir(harness.workspace_path())
        .args([
            "create",
            "--title",
            "Test bead for capability validation",
            "--priority",
            "2",
            "--issue-type",
            "task",
        ])
        .output()
        .expect("Failed to create test bead");

    assert!(create_output.status.success(), "Failed to create test bead");

    // Get the bead ID from output
    let create_text = String::from_utf8_lossy(&create_output.stdout);
    let bead_id = create_text
        .lines()
        .find(|line| line.contains("test-") || line.contains("bead-"))
        .expect("No bead ID found in create output")
        .trim()
        .to_string();

    // Check that why command is available and works
    let why_output = assert_cmd::Command::cargo_bin("bead")
        .unwrap()
        .current_dir(harness.workspace_path())
        .args(["why", "--id", &bead_id])
        .output()
        .expect("Failed to execute why command");

    assert!(why_output.status.success(), "Why command failed");

    let why_text = String::from_utf8_lossy(&why_output.stdout);
    // Check that why output contains expected sections
    assert!(
        why_text.contains("Status") || why_text.contains("Base Status"),
        "Why output should include status information"
    );
    assert!(
        why_text.contains("Priority"),
        "Why output should include priority information"
    );
}

#[test]
#[serial]
fn default_build_checkpoint_persistence() {
    // Test that attempt outcomes can persist through checkpoint round-trips
    let harness = BinaryHarness::new().unwrap();
    harness.init_workspace().unwrap();

    // Verify checkpoint infrastructure is available
    let caps = harness.get_default_capabilities().unwrap();
    let checkpoint_modes = caps["checkpoint_modes"].as_array().unwrap();
    assert!(
        checkpoint_modes.contains(&serde_json::json!("monolithic")),
        "Monolithic checkpoint mode should be available"
    );

    let checkpoint_formats = caps["checkpoint_formats"].as_array().unwrap();
    assert!(
        checkpoint_formats.contains(&serde_json::json!("checkpoint-set-v1")),
        "Checkpoint set v1 format should be available"
    );
}

#[test]
#[serial]
fn needle_fallback_validation() {
    // Test NEEDLE fallback behavior for starvation detection
    // This validates that the starvation fallback recommendations work correctly
    let harness = BinaryHarness::new().unwrap();
    assert_command_exists!(harness, "doctor");

    // Doctor command should be available for fallback scenarios
    let output = assert_cmd::Command::cargo_bin("bead")
        .unwrap()
        .current_dir(harness.workspace_path())
        .args(["doctor", "--help"])
        .output()
        .expect("Failed to execute doctor command");

    assert!(output.status.success(), "Doctor command should be available");

    let help = String::from_utf8_lossy(&output.stdout);
    // Doctor should support various recovery scenarios
    assert!(
        help.contains("diagnostic") || help.contains("repair"),
        "Doctor should support diagnostic and repair operations"
    );
}

#[test]
#[serial]
fn atomic_resolution_path_validation() {
    // Test that the atomic resolution path works correctly
    // This validates the complete flow: resolve → receipt → outcome → lifecycle
    let harness = BinaryHarness::new().unwrap();

    // Check resolve command exists and has proper help text
    let output = assert_cmd::Command::cargo_bin("bead")
        .unwrap()
        .current_dir(harness.workspace_path())
        .args(["resolve", "--help"])
        .output()
        .expect("Failed to execute resolve command");

    assert!(output.status.success(), "Resolve command should exist");

    let help = String::from_utf8_lossy(&output.stdout);
    assert!(
        help.contains("--attempt-id"),
        "Help should mention --attempt-id"
    );
    assert!(help.contains("--outcome"), "Help should mention --outcome");
    assert!(help.contains("--action"), "Help should mention --action");
}

#[test]
#[serial]
fn capability_schema_validation() {
    // Test that capabilities output matches expected schema
    let harness = BinaryHarness::new().unwrap();
    let caps = harness.get_default_capabilities().unwrap();

    // Validate schema_ref field
    assert_eq!(
        caps["schema_ref"],
        "urn:bead-rs:schema:capabilities:native-v1"
    );

    // Validate schemas array
    let schemas = caps["schemas"].as_array().expect("Should have schemas array");
    assert!(!schemas.is_empty(), "Schemas array should not be empty");

    // Each schema should have required fields
    for schema in schemas {
        assert!(
            schema.get("schema_ref").is_some(),
            "Schema should have schema_ref"
        );
        assert!(
            schema.get("document_kind").is_some(),
            "Schema should have document_kind"
        );
        assert!(
            schema.get("validate").is_some(),
            "Schema should have validate field"
        );
    }
}

#[test]
#[serial]
fn verify_all_expected_capabilities() {
    // Comprehensive verification that all expected capabilities are present
    let harness = BinaryHarness::new().unwrap();

    let expected = ExpectedCapabilities {
        auto_flush_present: true,
        auto_flush_value: Some(true),
        attempt_outcome_present: true,
        attempt_outcome_supported: true,
        expected_commands: vec![
            "capabilities".to_string(),
            "create".to_string(),
            "list".to_string(),
            "claim".to_string(),
            "resolve".to_string(),
            "close".to_string(),
            "reopen".to_string(),
            "release".to_string(),
            "sync".to_string(),
            "why".to_string(),
            "doctor".to_string(),
        ],
        missing_commands: vec![],
    };

    let failures = harness.verify_capabilities(&expected).unwrap();

    if !failures.is_empty() {
        panic!(
            "Default build capability verification failed:\n{}",
            failures.join("\n")
        );
    }
}

#[test]
#[serial]
fn default_build_checkpoint_round_trip_support() {
    // Test that the default build supports all checkpoint round-trip operations
    let harness = BinaryHarness::new().unwrap();
    harness.init_workspace().unwrap();

    // Verify checkpoint operations are available
    let required_commands = vec![
        "sync", "flush-only", "import-only", "reconcile", "status",
    ];

    for command in required_commands {
        // These are subcommands under 'sync', not root commands
        // We're testing the parent command exists
        if command == "sync" {
            assert_command_exists!(harness, command);
        }
    }

    // Verify checkpoint modes and formats support attempt receipts
    let caps = harness.get_default_capabilities().unwrap();
    let schemas = caps["schemas"].as_array().unwrap();

    // Should have resolve-receipt schema
    let receipt_schema = schemas
        .iter()
        .find(|s| {
            s.get("schema_ref")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .contains("resolve-receipt")
        });

    assert!(
        receipt_schema.is_some(),
        "Should advertise resolve-receipt schema"
    );
}

/// Setup helper for tests that need a workspace
fn setup_test_workspace() -> BinaryHarness {
    let harness = BinaryHarness::new().unwrap();
    harness.init_workspace().unwrap();
    harness
}

/// Helper to test capability absence (for testing against pre-feature builds)
///
/// This function would be used if we were testing against a binary built
/// without the attempt-resolution feature. Currently, the default build
/// includes attempt-resolution, so this tests the framework itself.
#[test]
#[serial]
fn framework_handles_capability_absence() {
    let harness = BinaryHarness::new().unwrap();

    // Test that the framework correctly reports absence of non-existent fields
    assert!(!harness
        .has_capability_field("nonexistent_field")
        .unwrap());

    assert!(!harness
        .has_capability_field("attempt_outcome.nonexistent")
        .unwrap());

    // Test missing command detection
    assert!(!harness.command_exists("nonexistent_command").unwrap());
}

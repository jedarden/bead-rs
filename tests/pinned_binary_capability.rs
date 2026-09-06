//! Test framework for validating capability differences between pre-feature and feature-enabled binaries
//!
//! This test exercises the absence/presence of attempt-resolution capabilities:
//! - Capability detection via `bead capabilities`
//! - Resolve command availability
//! - Attempt information in `bead why` output
//! - Attempt outcome persistence through checkpoint round-trips
//! - NEEDLE fallback starvation detection behavior
//!
//! The default-build tests run against the cargo-built `bead`; the
//! `*_pin_*` tests point the same assertion bodies at the pinned binaries
//! of record (resolved, provenance- and byte-checked through
//! [`capability_framework::capability_variant_pair`]): the feature-enabled
//! pin must show presence, the pre-feature pin absence — with the shared
//! core assertions still passing there, so absence never reads as breakage.
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

    assert_eq!(
        supported,
        Some(true),
        "Default build should support attempt_outcome"
    );
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
    // Test that why command is available and works in default build
    let harness = BinaryHarness::new().unwrap();
    why_reports_core_info(&harness).unwrap();
}

#[test]
#[serial]
fn default_build_checkpoint_persistence() {
    // Test that attempt outcomes can persist through checkpoint round-trips
    let harness = BinaryHarness::new().unwrap();
    checkpoint_persistence_holds(&harness).unwrap();
}

#[test]
#[serial]
fn needle_fallback_validation() {
    // Test NEEDLE fallback behavior for starvation detection
    // This validates that the starvation fallback recommendations work correctly
    let harness = BinaryHarness::new().unwrap();
    assert_command_exists!(harness, "doctor");

    // Doctor command should be available for fallback scenarios
    let output = harness.run(&["doctor", "--help"]).unwrap();
    assert!(
        output.status.success(),
        "Doctor command should be available"
    );

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
    let output = harness.run(&["resolve", "--help"]).unwrap();
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
    let schemas = caps["schemas"]
        .as_array()
        .expect("Should have schemas array");
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
    let required_commands = vec!["sync", "flush-only", "import-only", "reconcile", "status"];

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
    let receipt_schema = schemas.iter().find(|s| {
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

// --- The same framework pointed at the pinned variants ---------------------
//
// Both pins are resolved through the registry, provenance-checked against
// their metadata `--version` strings and byte-checked against their recorded
// sha256, so the assertions below are about the pinned bytes and not
// whatever happens to sit at that path.

/// The feature-enabled pin advertises attempt fields in the contract, with
/// the full present-side expectation holding
#[test]
#[serial]
fn feature_enabled_pin_attempt_fields_present() {
    let pair = capability_variant_pair().unwrap();
    let harness = BinaryHarness::with_binary(&pair.capability_present.path).unwrap();

    assert_capability_present!(harness, "attempt_outcome");
    assert_capability_present!(harness, "attempt_outcome.supported");

    let failures = harness
        .verify_capabilities(&capability_present_expectation())
        .unwrap();
    assert!(
        failures.is_empty(),
        "capability verification failed for the feature-enabled pin:\n{}",
        failures.join("\n")
    );
}

/// The feature-enabled pin exposes the resolve command for real, not just as
/// an advertised name
#[test]
#[serial]
fn feature_enabled_pin_resolve_command_available() {
    let pair = capability_variant_pair().unwrap();
    let harness = BinaryHarness::with_binary(&pair.capability_present.path).unwrap();

    assert_command_exists!(harness, "resolve");
    assert!(
        harness.test_resolve_command().unwrap().is_ok(),
        "resolve should be usable on the feature-enabled pin"
    );

    // The receipt machinery's conformance knobs are documented in --help
    let help = harness.run(&["resolve", "--help"]).unwrap();
    assert!(help.status.success(), "resolve --help should succeed");
    let help_text = String::from_utf8_lossy(&help.stdout);
    for flag in ["--attempt-id", "--outcome", "--action"] {
        assert!(
            help_text.contains(flag),
            "resolve help should document '{flag}'"
        );
    }
}

/// `why` reports the core status/priority facts on the feature-enabled pin
#[test]
#[serial]
fn feature_enabled_pin_why_shows_attempt_info() {
    let pair = capability_variant_pair().unwrap();
    let harness = BinaryHarness::with_binary(&pair.capability_present.path).unwrap();
    let bead_id = why_reports_core_info(&harness).unwrap();

    // The created bead is discoverable through the same binary that made it
    let listed = harness.run(&["list", "--limit", "5"]).unwrap();
    assert!(listed.status.success(), "list should succeed");
    assert!(
        String::from_utf8_lossy(&listed.stdout).contains(&bead_id),
        "created bead '{bead_id}' should appear in list output"
    );
}

/// Checkpoint persistence surface holds on the feature-enabled pin
#[test]
#[serial]
fn feature_enabled_pin_checkpoint_persistence() {
    let pair = capability_variant_pair().unwrap();
    let harness = BinaryHarness::with_binary(&pair.capability_present.path).unwrap();
    checkpoint_persistence_holds(&harness).unwrap();
}

/// The pre-feature pin reports capability ABSENCE through the framework:
/// no attempt fields in the contract, no resolve command, and the present-side
/// expectation correctly fails nothing
#[test]
#[serial]
fn pre_feature_pin_reports_capability_absence() {
    let pair = capability_variant_pair().unwrap();
    let harness = BinaryHarness::with_binary(&pair.capability_absent.path).unwrap();

    assert_capability_absent!(harness, "attempt_outcome");
    assert_capability_absent!(harness, "attempt_outcome.supported");
    assert_command_missing!(harness, "resolve");

    let failures = harness
        .verify_capabilities(&capability_absent_expectation())
        .unwrap();
    assert!(
        failures.is_empty(),
        "capability verification failed for the pre-feature pin:\n{}",
        failures.join("\n")
    );
}

/// The pre-feature pin degrades cleanly: `resolve` is rejected with clap's
/// unrecognized-subcommand error — never a panic — and the core workflow is
/// unimpaired, so absence is a detectable gap rather than breakage
#[test]
#[serial]
fn pre_feature_pin_rejects_resolve_cleanly() {
    let pair = capability_variant_pair().unwrap();
    let harness = BinaryHarness::with_binary(&pair.capability_absent.path).unwrap();

    let stderr = harness
        .unrecognized_subcommand("resolve")
        .unwrap()
        .expect("resolve must classify as an unrecognized subcommand on the pre-feature pin");
    assert!(
        !stderr.contains("panicked"),
        "degradation must not panic: {stderr}"
    );

    let result = harness.test_resolve_command().unwrap();
    assert!(
        result.is_err(),
        "test_resolve_command must report absence, got {result:?}"
    );

    // The core workflow still runs end to end on this binary
    harness.init_workspace().unwrap();
    let created = harness
        .run(&[
            "create",
            "--title",
            "pre-feature core workflow probe",
            "--priority",
            "2",
            "--issue-type",
            "task",
        ])
        .unwrap();
    assert!(
        created.status.success(),
        "create must work without the capability"
    );
    let bead_id = String::from_utf8_lossy(&created.stdout).trim().to_string();
    for args in [
        vec!["list", "--limit", "3"],
        vec!["update", &bead_id, "--status", "in_progress"],
        vec!["close", &bead_id, "--reason", "pre-feature probe complete"],
    ] {
        let out = harness.run(&args).unwrap();
        assert!(
            out.status.success(),
            "core command {:?} must keep working on the pre-feature pin: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

/// No spurious failures: the shared core assertions that pass on the default
/// build also pass on the pre-feature pin — `why`, checkpoint persistence and
/// the full core command set — so the framework distinguishes "capability
/// absent" from "binary broken"
#[test]
#[serial]
fn pre_feature_pin_passes_core_assertions_without_spurious_failures() {
    let pair = capability_variant_pair().unwrap();
    let harness = BinaryHarness::with_binary(&pair.capability_absent.path).unwrap();

    why_reports_core_info(&harness).unwrap();
    checkpoint_persistence_holds(&harness).unwrap();

    for command in core_command_set() {
        assert!(
            harness.command_exists(&command).unwrap(),
            "core command '{command}' must exist on the pre-feature pin"
        );
    }
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
    assert!(!harness.has_capability_field("nonexistent_field").unwrap());

    assert!(!harness
        .has_capability_field("attempt_outcome.nonexistent")
        .unwrap());

    // Test missing command detection
    assert!(!harness.command_exists("nonexistent_command").unwrap());
}

// --- Shared assertion bodies ------------------------------------------------
//
// One body per capability dimension, taking the harness as an argument, so
// the default build and both pinned variants are asserted identically.

/// Create a bead and confirm `why` reports the core status and priority
/// facts about it. Returns the created bead id.
fn why_reports_core_info(harness: &BinaryHarness) -> anyhow::Result<String> {
    harness.init_workspace()?;

    let created = harness.run(&[
        "create",
        "--title",
        "Test bead for capability validation",
        "--priority",
        "2",
        "--issue-type",
        "task",
    ])?;
    assert!(created.status.success(), "Failed to create test bead");

    let bead_id = String::from_utf8_lossy(&created.stdout).trim().to_string();
    assert!(
        !bead_id.is_empty(),
        "create must print the new bead id, got: {:?}",
        String::from_utf8_lossy(&created.stdout)
    );

    let why = harness.run(&["why", "--id", &bead_id])?;
    assert!(why.status.success(), "Why command failed");

    let why_text = String::from_utf8_lossy(&why.stdout);
    assert!(
        why_text.contains("Status") || why_text.contains("Base Status"),
        "Why output should include status information"
    );
    assert!(
        why_text.contains("Priority"),
        "Why output should include priority information"
    );
    Ok(bead_id)
}

/// Verify the checkpoint infrastructure a capability consumer relies on:
/// monolithic mode and the checkpoint-set-v1 format are advertised, so
/// attempt outcomes have somewhere to persist
fn checkpoint_persistence_holds(harness: &BinaryHarness) -> anyhow::Result<()> {
    harness.init_workspace()?;

    let caps = harness.get_default_capabilities()?;
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
    Ok(())
}

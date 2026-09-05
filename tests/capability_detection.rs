//! Comprehensive capability detection tests
//!
//! This test suite exercises capability presence and absence across different
//! profiles and binary configurations. Tests use the capability framework to
//! validate that the binary correctly advertises its features.

use serial_test::serial;

mod capability_framework;
use capability_framework::*;

#[test]
#[serial]
fn default_binary_has_auto_flush_capability() {
    let harness = BinaryHarness::new().unwrap();

    // Current default should have auto_flush (R026 is enabled)
    assert_capability_present!(harness, "auto_flush");
    assert!(harness.has_capability_field("auto_flush").unwrap());

    let caps = harness.get_default_capabilities().unwrap();
    let auto_flush = caps.get("auto_flush").and_then(|v| v.as_bool());
    assert_eq!(
        auto_flush,
        Some(true),
        "auto_flush should be true by default"
    );
}

#[test]
#[serial]
fn default_binary_has_attempt_outcome_capability() {
    let harness = BinaryHarness::new().unwrap();

    // Check attempt_outcome capability exists
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
        "attempt_outcome.supported should be true in default build"
    );
}

#[test]
#[serial]
fn attempt_outcome_has_all_required_fields() {
    let harness = BinaryHarness::new().unwrap();
    let caps = harness.get_default_capabilities().unwrap();

    let ao = caps
        .get("attempt_outcome")
        .expect("attempt_outcome should be present");

    // Check all required fields
    let required_fields = vec![
        ("supported", true),
        ("outcomes", true),
        ("actions", true),
        ("replay_detection", true),
        ("revision_guard", true),
        ("fencing_token", true),
        ("evidence_refs", true),
        ("resolve_receipt_schema", true),
        ("resolve_request_schema", true),
    ];

    for (field, _required) in &required_fields {
        assert!(
            ao.get(*field).is_some(),
            "attempt_outcome.{} field is missing",
            field
        );
    }

    // Validate outcomes array has expected values
    let outcomes = ao
        .get("outcomes")
        .and_then(|v| v.as_array())
        .expect("outcomes should be an array");

    let expected_outcomes = vec![
        "verified_success",
        "work_failure",
        "infrastructure_failure",
        "cancelled",
        "indeterminate",
    ];

    for expected in &expected_outcomes {
        assert!(
            outcomes.iter().any(|v| v.as_str() == Some(*expected)),
            "Missing expected outcome: {}",
            expected
        );
    }

    // Validate actions array has expected values
    let actions = ao
        .get("actions")
        .and_then(|v| v.as_array())
        .expect("actions should be an array");

    let expected_actions = vec!["close", "release", "quarantine", "block", "none"];

    for expected in &expected_actions {
        assert!(
            actions.iter().any(|v| v.as_str() == Some(*expected)),
            "Missing expected action: {}",
            expected
        );
    }
}

#[test]
#[serial]
fn resolve_command_is_available() {
    let harness = BinaryHarness::new().unwrap();

    assert_command_exists!(harness, "resolve");

    // Actually test that the command works
    let result = harness.test_resolve_command().unwrap();
    assert!(
        result.is_ok(),
        "resolve command should be available in default build"
    );
}

#[test]
#[serial]
fn all_core_commands_are_present() {
    let harness = BinaryHarness::new().unwrap();

    let core_commands = vec![
        "capabilities",
        "create",
        "list",
        "show",
        "update",
        "close",
        "reopen",
        "claim",
        "release",
        "resolve",
        "label",
        "dep",
        "ref",
        "sync",
        "init",
        "restore",
        "doctor",
        "schema",
        "query",
        "data",
        "why",
        "manifest",
        "changes",
        "compare",
        "analyze-exclusion",
        "recurrence",
        "resource",
        "watchdog",
    ];

    for command in core_commands {
        assert_command_exists!(harness, command);
    }
}

#[test]
#[serial]
fn native_v1_profile_capabilities() {
    let harness = BinaryHarness::new().unwrap();
    let caps = harness
        .get_capabilities(&["capabilities", "--profile", "native-v1"])
        .unwrap();

    assert_eq!(caps["contract"], "native-v1");
    assert_eq!(caps["implementation"], "bead-rs");
    assert_eq!(caps["store_layout"], 1);
    assert_eq!(caps["atomic_claim"], true);

    // Check priority range
    let priorities = &caps["priorities"];
    assert_eq!(priorities["min"], 0);
    assert_eq!(priorities["max"], 4);
    assert_eq!(priorities["default"], 2);
    assert_eq!(priorities["p4_claimable_by_fifo"], true);

    // Check statuses
    let statuses = caps["statuses"].as_array().unwrap();
    assert!(statuses.contains(&serde_json::json!("open")));
    assert!(statuses.contains(&serde_json::json!("in_progress")));
    assert!(statuses.contains(&serde_json::json!("closed")));
    assert!(statuses.contains(&serde_json::json!("deferred")));
    // Blocked is a derived status, not stored
    assert!(!statuses.contains(&serde_json::json!("blocked")));
}

#[test]
#[serial]
fn needle_v1_profile_capabilities() {
    let harness = BinaryHarness::new().unwrap();
    let caps = harness
        .get_capabilities(&["capabilities", "--profile", "needle-v1"])
        .unwrap();

    assert_eq!(caps["contract"], "needle-v1");
    assert_eq!(caps["implementation"], "bead-rs");

    // Needle profile should have the same core capabilities
    assert_eq!(caps["store_layout"], 1);
    assert_eq!(caps["atomic_claim"], true);
}

#[test]
#[serial]
fn checkpoint_capabilities_advertised() {
    let harness = BinaryHarness::new().unwrap();
    let caps = harness.get_default_capabilities().unwrap();

    // Check checkpoint modes
    let modes = caps["checkpoint_modes"].as_array().unwrap();
    assert!(modes.contains(&serde_json::json!("monolithic")));
    assert!(modes.contains(&serde_json::json!("sharded")));

    // Check checkpoint formats
    let formats = caps["checkpoint_formats"].as_array().unwrap();
    assert!(formats.contains(&serde_json::json!("issues-jsonl-v1")));
    assert!(formats.contains(&serde_json::json!("checkpoint-set-v1")));
}

#[test]
#[serial]
fn schema_capabilities_advertised() {
    let harness = BinaryHarness::new().unwrap();
    let caps = harness.get_default_capabilities().unwrap();

    // Check schema_ref
    assert_eq!(
        caps["schema_ref"],
        "urn:bead-rs:schema:capabilities:native-v1"
    );

    // Check schemas array exists and has entries
    let schemas = caps["schemas"].as_array().unwrap();
    assert!(!schemas.is_empty());

    // Find the issue schema
    let issue_schema = schemas
        .iter()
        .find(|s| s["schema_ref"] == "urn:bead-rs:schema:issue:native-v1");
    assert!(issue_schema.is_some(), "Issue schema should be advertised");

    let issue_schema = issue_schema.unwrap();
    assert_eq!(issue_schema["document_kind"], "issue");
    assert_eq!(issue_schema["validate"], true);
}

#[test]
#[serial]
fn capabilities_work_without_workspace() {
    // Capabilities should work even without any workspace
    let temp_dir = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let output = assert_cmd::Command::cargo_bin("bead")
        .unwrap()
        .args(["capabilities"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json_str = String::from_utf8(output).unwrap();
    let _caps: serde_json::Value = serde_json::from_str(&json_str)
        .expect("Capabilities should output valid JSON even without workspace");
}

#[test]
#[serial]
fn auto_flush_tracks_compiled_default() {
    let harness = BinaryHarness::new().unwrap();

    // R026: auto_flush reports the compiled default, not workspace state
    let caps = harness.get_default_capabilities().unwrap();

    // In a compiled-default-enabled build, auto_flush should be present and true
    let auto_flush = caps.get("auto_flush");
    assert!(
        auto_flush.is_some(),
        "auto_flush should be present when compiled default is enabled"
    );

    if let Some(af_val) = auto_flush {
        assert_eq!(
            af_val,
            &serde_json::Value::Bool(true),
            "auto_flush should be true when compiled default is enabled"
        );
    }
}

#[test]
#[serial]
fn capabilities_json_is_valid() {
    let harness = BinaryHarness::new().unwrap();
    let caps = harness.get_default_capabilities().unwrap();

    // Ensure all required top-level fields are present
    let required_fields = vec![
        "contract",
        "implementation",
        "version",
        "store_layout",
        "atomic_claim",
        "priorities",
        "statuses",
        "checkpoint_modes",
        "checkpoint_formats",
        "logical_revision",
        "schema_ref",
        "schemas",
        "commands",
    ];

    for field in &required_fields {
        assert!(
            caps.get(*field).is_some(),
            "Required capability field '{}' is missing",
            field
        );
    }
}

#[test]
#[serial]
fn verify_default_expected_capabilities() {
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
            "sync".to_string(),
        ],
        missing_commands: vec![],
    };

    let failures = harness.verify_capabilities(&expected).unwrap();

    if !failures.is_empty() {
        panic!("Capability verification failed:\n{}", failures.join("\n"));
    }
}

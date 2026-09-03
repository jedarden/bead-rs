//! Capability detection matrix across the two binary variants
//!
//! The same test bodies run against both pinned variants located through
//! `pinned-binaries/commits.json`:
//!
//! - `pre_feature` (`bead-pre-feature`, release 0.2.4) — built before the
//!   attempt-resolution work existed: no `resolve` subcommand, no
//!   `attempt_outcome` capability, no attempt-related schemas.
//! - `attempt_resolution_f25ab5c` — the feature-enabled HEAD pin:
//!   `resolve` present and the capability document advertises
//!   `attempt_outcome` plus the attempt-outcome / resolve-receipt /
//!   resolve-request schemas.
//!
//! One subtlety the tests encode deliberately: the `attempt-resolution`
//! cargo feature is an empty marker that gates no code (documented in
//! `pinned-binaries/README.md` and the pin metadata). The functional
//! "absence" variant is the older tree, not a flag-off build of the
//! current tree — a flag-off build of today's source advertises the same
//! capabilities as a flag-on build, which the feature-flag test asserts.

use serial_test::serial;

mod capability_framework;
use capability_framework::*;

const PRE_FEATURE_ROLE: &str = "pre_feature";
const FEATURE_ENABLED_ROLE: &str = "attempt_resolution_f25ab5c";

const EXPECTED_OUTCOMES: [&str; 5] = [
    "verified_success",
    "work_failure",
    "infrastructure_failure",
    "cancelled",
    "indeterminate",
];

const EXPECTED_ACTIONS: [&str; 5] = ["close", "release", "quarantine", "block", "none"];

const ATTEMPT_SCHEMA_REFS: [&str; 3] = [
    "urn:bead-rs:schema:attempt-outcome:native-v1",
    "urn:bead-rs:schema:resolve-receipt:native-v1",
    "urn:bead-rs:schema:resolve-request:native-v1",
];

/// Core command surface that must survive on every variant — a variant that
/// lost these would not be degrading gracefully, it would be broken
const CORE_COMMANDS: [&str; 10] = [
    "capabilities",
    "init",
    "create",
    "list",
    "claim",
    "close",
    "reopen",
    "release",
    "sync",
    "doctor",
];

fn capability_document(binary: &std::path::Path) -> serde_json::Value {
    match capabilities_of(binary) {
        Ok(caps) => caps,
        Err(e) => panic!("capabilities failed for {}: {}", binary.display(), e),
    }
}

fn document_kind_refs(caps: &serde_json::Value) -> Vec<String> {
    caps["schemas"]
        .as_array()
        .expect("capabilities should carry a schemas array")
        .iter()
        .filter_map(|s| s.get("schema_ref").and_then(|v| v.as_str()))
        .map(|s| s.to_string())
        .collect()
}

fn command_list(caps: &serde_json::Value) -> Vec<String> {
    caps["commands"]
        .as_array()
        .expect("capabilities should carry a commands array")
        .iter()
        .filter_map(|c| c.as_str())
        .map(|c| c.to_string())
        .collect()
}

// ---------------------------------------------------------------------------
// Harness: one harness type, both variants
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn pinned_variants_match_recorded_metadata() {
    // The variant tests are only meaningful against the pinned bytes; check
    // each pin's embedded version against its metadata file before trusting
    // anything else in this file.
    for role in [PRE_FEATURE_ROLE, FEATURE_ENABLED_ROLE] {
        let (binary, meta) = verified_pinned_variant(role)
            .unwrap_or_else(|e| panic!("pin provenance check failed for role '{}': {}", role, e));
        let recorded = meta["binary_name"].as_str().unwrap_or_default();
        let on_disk = binary.file_name().unwrap().to_string_lossy();
        assert_eq!(recorded, on_disk, "registry and disk disagree for {}", role);
    }
}

#[test]
#[serial]
fn harness_runs_both_variants() {
    let pre = pinned_variant(PRE_FEATURE_ROLE).expect("pre-feature pin");
    let feature = pinned_variant(FEATURE_ENABLED_ROLE).expect("feature-enabled pin");

    let pre_harness = BinaryHarness::with_binary(&pre).expect("pre-feature harness");
    let feat_harness = BinaryHarness::with_binary(&feature).expect("feature-enabled harness");

    // The same harness type initializes workspaces under both binaries
    pre_harness.init_workspace().expect("pre-feature init");
    feat_harness.init_workspace().expect("feature-enabled init");

    // ...and they really are two different binaries
    let pre_version = version_of(&pre).expect("pre-feature version");
    let feat_version = version_of(&feature).expect("feature-enabled version");
    assert_ne!(
        pre_version, feat_version,
        "the two variants should be distinct builds"
    );
    assert!(
        pre_version.contains("0.2.4"),
        "pre-feature pin should be the 0.2.4 baseline, got {:?}",
        pre_version
    );
}

// ---------------------------------------------------------------------------
// Attempt-resolution capability detection (capability present)
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn feature_enabled_advertises_attempt_resolution() {
    let (binary, _) = verified_pinned_variant(FEATURE_ENABLED_ROLE).unwrap();
    let caps = capability_document(&binary);

    let outcome = caps
        .get("attempt_outcome")
        .expect("feature-enabled binary should advertise attempt_outcome");
    assert_eq!(
        outcome.get("supported").and_then(|v| v.as_bool()),
        Some(true),
        "attempt_outcome.supported must be true"
    );

    let outcomes: Vec<&str> = outcome["outcomes"]
        .as_array()
        .expect("outcomes array")
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    for expected in EXPECTED_OUTCOMES {
        assert!(
            outcomes.contains(&expected),
            "outcome '{}' missing from capability advertisement",
            expected
        );
    }

    let actions: Vec<&str> = outcome["actions"]
        .as_array()
        .expect("actions array")
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    for expected in EXPECTED_ACTIONS {
        assert!(
            actions.contains(&expected),
            "action '{}' missing from capability advertisement",
            expected
        );
    }

    for flag in [
        "replay_detection",
        "revision_guard",
        "fencing_token",
        "evidence_refs",
    ] {
        assert_eq!(
            outcome.get(flag).and_then(|v| v.as_bool()),
            Some(true),
            "attempt_outcome.{} must be true",
            flag
        );
    }

    assert_eq!(
        outcome
            .get("resolve_receipt_schema")
            .and_then(|v| v.as_str()),
        Some("urn:bead-rs:schema:resolve-receipt:native-v1")
    );
    assert_eq!(
        outcome
            .get("resolve_request_schema")
            .and_then(|v| v.as_str()),
        Some("urn:bead-rs:schema:resolve-request:native-v1")
    );
}

#[test]
#[serial]
fn feature_enabled_advertises_resolve_command() {
    let (binary, _) = verified_pinned_variant(FEATURE_ENABLED_ROLE).unwrap();
    let caps = capability_document(&binary);

    assert!(
        command_list(&caps).iter().any(|c| c == "resolve"),
        "feature-enabled binary should list 'resolve' among commands"
    );

    let harness = BinaryHarness::with_binary(&binary).unwrap();
    assert!(
        harness
            .unrecognized_subcommand("resolve")
            .unwrap()
            .is_none(),
        "'resolve --help' should succeed on the feature-enabled binary"
    );
}

#[test]
#[serial]
fn feature_enabled_advertises_attempt_schemas() {
    let (binary, _) = verified_pinned_variant(FEATURE_ENABLED_ROLE).unwrap();
    let caps = capability_document(&binary);
    let refs = document_kind_refs(&caps);

    for schema_ref in ATTEMPT_SCHEMA_REFS {
        assert!(
            refs.iter().any(|r| r == schema_ref),
            "schema '{}' should be advertised by the feature-enabled binary",
            schema_ref
        );
    }
}

// ---------------------------------------------------------------------------
// Attempt-resolution capability detection (capability absent)
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn pre_feature_omits_attempt_resolution_capability() {
    let (binary, _) = verified_pinned_variant(PRE_FEATURE_ROLE).unwrap();
    let caps = capability_document(&binary);

    assert!(
        caps.get("attempt_outcome").is_none(),
        "pre-feature binary must not advertise attempt_outcome"
    );

    assert!(
        !command_list(&caps).iter().any(|c| c == "resolve"),
        "pre-feature binary must not list 'resolve' among commands"
    );

    let refs = document_kind_refs(&caps);
    for schema_ref in ATTEMPT_SCHEMA_REFS {
        assert!(
            !refs.iter().any(|r| r == schema_ref),
            "pre-feature binary must not advertise schema '{}'",
            schema_ref
        );
    }
}

#[test]
#[serial]
fn pre_feature_core_capability_surface_unchanged() {
    let (binary, _) = verified_pinned_variant(PRE_FEATURE_ROLE).unwrap();
    let caps = capability_document(&binary);

    assert_eq!(
        caps.get("contract").and_then(|v| v.as_str()),
        Some("native-v1"),
        "pre-feature binary should still speak native-v1"
    );

    let commands = command_list(&caps);
    for command in CORE_COMMANDS {
        assert!(
            commands.iter().any(|c| c == command),
            "core command '{}' missing from the pre-feature binary",
            command
        );
    }
}

// ---------------------------------------------------------------------------
// Graceful degradation when the capability is missing
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn pre_feature_missing_capability_degrades_gracefully() {
    let (binary, _) = verified_pinned_variant(PRE_FEATURE_ROLE).unwrap();
    let harness = BinaryHarness::with_binary(&binary).unwrap();

    // Invoking the missing capability fails as a clean usage error: non-zero
    // exit, clap's "unrecognized subcommand", no panic
    let output = harness.run(&["resolve"]).expect("resolve invocation");
    assert!(
        !output.status.success(),
        "resolve must fail on the pre-feature binary"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unrecognized subcommand"),
        "expected a clean usage error, got: {}",
        stderr
    );
    assert!(
        !stderr.contains("panicked"),
        "degradation must not crash: {}",
        stderr
    );

    // ...while the rest of the lifecycle keeps working on the same binary
    harness
        .init_workspace()
        .expect("init on pre-feature binary");

    let created = harness
        .run(&["create", "--title", "degradation probe", "--priority", "2"])
        .expect("create invocation");
    assert!(
        created.status.success(),
        "create must work without the capability"
    );
    let bead_id = String::from_utf8_lossy(&created.stdout)
        .lines()
        .next()
        .expect("create prints the new bead id")
        .trim()
        .to_string();
    assert!(
        bead_id.starts_with("test-"),
        "unexpected create output: {:?}",
        bead_id
    );

    let listed = harness
        .run(&["list", "--limit", "5"])
        .expect("list invocation");
    assert!(
        listed.status.success(),
        "list must work without the capability"
    );
    assert!(
        String::from_utf8_lossy(&listed.stdout).contains(&bead_id),
        "created bead {} should be listable on the pre-feature binary",
        bead_id
    );
}

/// The consumer contract: the capability document predicts the live command
/// surface, on both variants — detect via `capabilities`, then invoke
#[test]
#[serial]
fn capability_document_predicts_command_surface_on_both_variants() {
    for role in [PRE_FEATURE_ROLE, FEATURE_ENABLED_ROLE] {
        let (binary, _) = verified_pinned_variant(role)
            .unwrap_or_else(|e| panic!("pin '{}' unavailable: {}", role, e));
        let caps = capability_document(&binary);
        let advertised = caps.get("attempt_outcome").is_some();

        let harness = BinaryHarness::with_binary(&binary).unwrap();
        let live = harness
            .unrecognized_subcommand("resolve")
            .unwrap_or_else(|e| panic!("resolve probe failed on {}: {}", role, e))
            .is_none();

        assert_eq!(
            advertised, live,
            "on {}: capability document says advertised={}, live resolve availability={}",
            role, advertised, live
        );
    }
}

// ---------------------------------------------------------------------------
// Feature flag handling
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn attempt_resolution_feature_is_declared_and_not_default() {
    let manifest = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"))
        .expect("read Cargo.toml");

    let features_start = manifest
        .find("[features]")
        .expect("Cargo.toml should declare a [features] section");
    let features_end = manifest[features_start..]
        .find("\n[")
        .map(|i| features_start + i)
        .unwrap_or(manifest.len());
    let features_section = &manifest[features_start..features_end];

    assert!(
        features_section.contains("attempt-resolution"),
        "[features] must declare the attempt-resolution flag"
    );
    let default_line = features_section
        .lines()
        .find(|l| l.trim_start().starts_with("default"))
        .expect("[features] must declare a default set");
    assert!(
        !default_line.contains("attempt-resolution"),
        "attempt-resolution must not be a default feature, found in: {}",
        default_line
    );
}

/// The flag is an empty marker that gates no code, so a default-features
/// build (flag off) must advertise exactly the attempt-resolution capability
/// shape the flag-enabled pin advertises. If someone ever makes the flag
/// actually gate the capability, this test forces that contract change to be
/// made consciously in the pin documentation too.
#[test]
#[serial]
fn marker_feature_does_not_gate_capability_advertisement() {
    let (flag_on, _) = verified_pinned_variant(FEATURE_ENABLED_ROLE).unwrap();
    let flag_on_keys = attempt_outcome_keys(&capability_document(&flag_on));

    // The cargo-built test binary is a default-features build (flag off)
    let flag_off = BinaryHarness::new().expect("harness for cargo-built bead");
    let flag_off_caps = flag_off
        .get_default_capabilities()
        .expect("capabilities of the cargo-built bead");
    let flag_off_keys = attempt_outcome_keys(&flag_off_caps);

    assert!(
        !flag_off_keys.is_empty(),
        "the default-features build should still advertise attempt_outcome"
    );
    assert_eq!(
        flag_off_keys, flag_on_keys,
        "flag-off and flag-on builds must advertise the same attempt_outcome shape \
         (the flag is an empty marker that gates no code)"
    );

    let supported = flag_off_caps["attempt_outcome"]
        .get("supported")
        .and_then(|v| v.as_bool());
    assert_eq!(
        supported,
        Some(true),
        "attempt_outcome.supported must be true in a flag-off build"
    );
}

fn attempt_outcome_keys(caps: &serde_json::Value) -> Vec<String> {
    let mut keys: Vec<String> = caps
        .get("attempt_outcome")
        .and_then(|v| v.as_object())
        .expect("capabilities should carry an attempt_outcome object")
        .keys()
        .cloned()
        .collect();
    keys.sort();
    keys
}

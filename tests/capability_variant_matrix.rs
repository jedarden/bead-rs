//! Capability detection matrix across the pinned binary variants
//!
//! The two pins of record bound the capability story: `pre_feature` predates
//! attempt-resolution entirely (no `resolve` subcommand, no `attempt_outcome`
//! capability), while `attempt_resolution_f25ab5c` carries it. Every test here
//! drives the pinned executables themselves — provenance- and byte-checked via
//! [`capability_framework::capability_variant_pair`] — rather than the
//! cargo-built test binary, so absence and presence are exercised for real.
//!
//! A trap this suite pins down: the `attempt-resolution` cargo feature is an
//! empty marker that gates no code (see `pinned-binaries/commits.json`), so a
//! build's recorded flags do not predict its capability surface.
//! `bead-pre-attempt-resolution` is built `--no-default-features` and still
//! reports `attempt_outcome`. Detection therefore consults
//! `bead capabilities`, never build metadata.

mod capability_framework;
use capability_framework::*;
use serde_json::Value;
use serial_test::serial;
use std::path::Path;

/// The harness resolves both pins, provenance- and byte-checked, as distinct
/// binaries — the precondition for every other test in the matrix
#[test]
#[serial]
fn variant_pair_resolves_as_distinct_verified_binaries() {
    let pair = capability_variant_pair().unwrap();
    let absent = &pair.capability_absent;
    let present = &pair.capability_present;

    assert!(
        absent.path.is_file(),
        "absent pin missing: {}",
        absent.path.display()
    );
    assert!(
        present.path.is_file(),
        "present pin missing: {}",
        present.path.display()
    );

    // Provenance: each binary reports the version its metadata records
    // (checked inside capability_variant_pair); surface the strings on failure.
    let absent_version = absent.embedded_version().unwrap();
    let present_version = present.embedded_version().unwrap();
    assert_ne!(
        absent_version, present_version,
        "the two variants must be genuinely different builds"
    );

    // Byte identity with the recorded pins (re-verified here so the
    // assertion failure names this test, not the resolver).
    let absent_sha = absent.verify_sha256().unwrap();
    let present_sha = present.verify_sha256().unwrap();
    assert_ne!(
        absent_sha, present_sha,
        "variant pins must not be byte-identical"
    );
}

/// Capability detection reports the right thing about each variant: absent
/// where the feature predates the build, present and supported where it does
/// not, with the shared contract intact on both sides
#[test]
#[serial]
fn capability_detection_matches_each_variant() {
    let pair = capability_variant_pair().unwrap();

    for (variant, expectation) in [
        (&pair.capability_absent, capability_absent_expectation()),
        (&pair.capability_present, capability_present_expectation()),
    ] {
        let harness = BinaryHarness::with_binary(&variant.path).unwrap();
        let failures = harness.verify_capabilities(&expectation).unwrap();
        assert!(
            failures.is_empty(),
            "capability verification failed for pin '{}':\n{}",
            variant.role,
            failures.join("\n")
        );
    }

    // The exact detection boundary, stated directly
    assert!(
        !pair
            .capability_absent
            .advertises_attempt_resolution()
            .unwrap(),
        "pre-feature pin must not advertise attempt_outcome"
    );
    assert!(
        pair.capability_present
            .advertises_attempt_resolution()
            .unwrap(),
        "feature-enabled pin must advertise attempt_outcome"
    );
}

/// The pre-feature binary degrades gracefully: the capability gap is visible
/// in the contract before invocation, the CLI rejects `resolve` with a clean
/// clap error instead of a panic, and the core workflow is unimpaired
#[test]
#[serial]
fn capability_absence_degrades_gracefully() {
    let pair = capability_variant_pair().unwrap();
    let absent = &pair.capability_absent;
    let harness = BinaryHarness::with_binary(&absent.path).unwrap();

    // Detection surface: a consumer reading `capabilities` learns the
    // capability is missing without ever invoking the command.
    let caps = harness.get_default_capabilities().unwrap();
    assert!(
        caps.get("attempt_outcome").is_none(),
        "attempt_outcome must be absent from the contract, not present-but-unsupported"
    );
    assert!(!harness.command_exists("resolve").unwrap());
    assert!(
        !harness
            .unrecognized_subcommand("resolve")
            .unwrap()
            .is_none(),
        "framework should classify resolve as unrecognized on this variant"
    );

    // CLI surface: --help and a real invocation both reject cleanly.
    let help = harness.run(&["resolve", "--help"]).unwrap();
    assert!(
        !help.status.success(),
        "resolve --help must fail on a binary without the subcommand"
    );
    let help_err = String::from_utf8_lossy(&help.stderr);
    assert!(
        help_err.contains("unrecognized subcommand 'resolve'"),
        "expected clap's unrecognized-subcommand error, got: {help_err}"
    );
    assert!(
        !help_err.contains("panicked"),
        "degradation must not panic: {help_err}"
    );

    let invoked = harness.run(&[
        "resolve",
        "test-degradation",
        "--attempt-id",
        "probe-attempt",
        "--outcome",
        "verified_success",
    ]);
    let invoked = invoked.unwrap();
    assert!(!invoked.status.success());
    let invoked_err = String::from_utf8_lossy(&invoked.stderr);
    assert!(
        invoked_err.contains("unrecognized subcommand"),
        "a real resolve invocation must hit the same clean rejection, got: {invoked_err}"
    );

    // Graceful: the core workflow still runs end to end on this binary.
    harness.init_workspace().unwrap();
    let created = harness
        .run(&[
            "create",
            "--title",
            "degradation probe",
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
    assert!(
        bead_id.starts_with("test-"),
        "expected a test-prefixed bead id, got: {bead_id}"
    );
    for args in [
        vec!["list", "--limit", "3"],
        vec!["update", &bead_id, "--status", "in_progress"],
        vec!["close", &bead_id, "--reason", "degradation probe complete"],
    ] {
        let out = harness.run(&args).unwrap();
        assert!(
            out.status.success(),
            "core command {:?} must keep working on the capability-absent variant: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

/// The feature-enabled binary exposes the full attempt-resolution surface:
/// advertised in the contract, resolvable through clap, with the conformance
/// knobs the receipt machinery relies on
#[test]
#[serial]
fn capability_presence_exposes_full_surface() {
    let pair = capability_variant_pair().unwrap();
    let present = &pair.capability_present;
    let harness = BinaryHarness::with_binary(&present.path).unwrap();

    let caps = harness.get_default_capabilities().unwrap();
    let attempt_outcome = caps
        .get("attempt_outcome")
        .expect("attempt_outcome must be advertised");
    assert_eq!(
        attempt_outcome.get("supported").and_then(|v| v.as_bool()),
        Some(true),
        "attempt_outcome.supported must be true"
    );

    for knob in [
        "replay_detection",
        "revision_guard",
        "fencing_token",
        "evidence_refs",
    ] {
        assert_eq!(
            attempt_outcome.get(knob).and_then(|v| v.as_bool()),
            Some(true),
            "conformance knob '{knob}' must be advertised"
        );
    }

    let outcomes: Vec<String> = attempt_outcome["outcomes"]
        .as_array()
        .expect("outcomes array")
        .iter()
        .map(|v| v.as_str().expect("outcome string").to_string())
        .collect();
    for expected in [
        "verified_success",
        "work_failure",
        "infrastructure_failure",
        "cancelled",
        "indeterminate",
    ] {
        assert!(
            outcomes.iter().any(|o| o == expected),
            "outcome '{expected}' must be advertised; got {outcomes:?}"
        );
    }

    // The command is really there, not just advertised.
    let help = harness.run(&["resolve", "--help"]).unwrap();
    assert!(help.status.success(), "resolve --help must succeed");
    let help_text = String::from_utf8_lossy(&help.stdout);
    for flag in ["--attempt-id", "--outcome", "--action"] {
        assert!(
            help_text.contains(flag),
            "resolve help must document '{flag}'"
        );
    }

    // And clap routes it: a resolve invocation is recognized (it fails on
    // domain grounds, never as an unknown subcommand).
    let dispatched = harness
        .run(&[
            "resolve",
            "test-recognition",
            "--attempt-id",
            "probe-attempt",
            "--outcome",
            "verified_success",
            "--action",
            "close",
        ])
        .unwrap();
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&dispatched.stdout),
        String::from_utf8_lossy(&dispatched.stderr)
    );
    assert!(
        !combined.contains("unrecognized subcommand"),
        "resolve must be a recognized subcommand on this variant: {combined}"
    );
}

/// Feature-flag handling: the cargo flag of record is declared, but flags do
/// not predict capability — the empty-marker pin proves detection has to come
/// from the binary's own contract
#[test]
#[serial]
fn feature_flag_handling_is_decoupled_from_capability() {
    let manifest =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml")).unwrap();
    assert!(
        manifest.contains("attempt-resolution = []"),
        "the attempt-resolution feature flag must stay declared in Cargo.toml"
    );

    let pair = capability_variant_pair().unwrap();

    // The empty-marker pin: built WITHOUT the flag, yet capability-present.
    let marker_path = pinned_variant(EMPTY_MARKER_ROLE).unwrap();
    let marker_caps = capabilities_of(&marker_path).unwrap();
    assert_eq!(
        marker_caps
            .get("attempt_outcome")
            .and_then(|v| v.get("supported"))
            .and_then(|v| v.as_bool()),
        Some(true),
        "the --no-default-features pin still reports attempt_outcome: the flag gates no code"
    );
    let marker_commands = marker_caps["commands"].as_array().expect("commands array");
    assert!(
        marker_commands
            .iter()
            .any(|c| c.as_str() == Some("resolve")),
        "the --no-default-features pin still exposes the resolve subcommand"
    );
    let marker_meta = metadata_for(&marker_path).unwrap();
    let marker_flags = marker_meta
        .get("build_features")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    assert!(
        !marker_flags.contains("attempt-resolution"),
        "registry records the marker pin as built without the flag, got: {marker_flags}"
    );

    // The capability boundary runs between the pair, exactly as the
    // registry documents it — independent of what flags were passed.
    assert!(!pair
        .capability_absent
        .advertises_attempt_resolution()
        .unwrap());
    assert!(pair
        .capability_present
        .advertises_attempt_resolution()
        .unwrap());
}

/// Both variants agree on the core contract, so a consumer can feature-detect
/// and then fall back to shared operations on either binary
#[test]
#[serial]
fn variants_share_the_core_contract() {
    let pair = capability_variant_pair().unwrap();

    for variant in [&pair.capability_absent, &pair.capability_present] {
        let caps = capabilities_of(&variant.path).unwrap();
        assert_eq!(
            caps.get("contract").and_then(|v| v.as_str()),
            Some("native-v1"),
            "pin '{}' must speak the same contract",
            variant.role
        );
        assert_eq!(
            caps.get("implementation").and_then(|v| v.as_str()),
            Some("bead-rs"),
            "pin '{}' must be the same implementation",
            variant.role
        );
        let commands = caps["commands"].as_array().expect("commands array");
        for core in core_command_set() {
            assert!(
                commands.iter().any(|c| c.as_str() == Some(core.as_str())),
                "core command '{core}' missing from pin '{}'",
                variant.role
            );
        }
    }

    // The capability delta is exactly the advertised command delta: the
    // absent side does not half-advertise what its CLI cannot do.
    let absent_caps = capabilities_of(&pair.capability_absent.path).unwrap();
    let present_caps = capabilities_of(&pair.capability_present.path).unwrap();
    let has = |caps: &Value, cmd: &str| {
        caps["commands"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c.as_str() == Some(cmd))
    };
    assert!(!has(&absent_caps, "resolve"));
    assert!(has(&present_caps, "resolve"));
}

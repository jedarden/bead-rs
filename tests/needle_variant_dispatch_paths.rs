//! NEEDLE dispatch-path validation across the pinned binary variants
//!
//! The capability matrix (`capability_variant_matrix.rs`) establishes what each
//! pin *advertises*. This suite validates the paths NEEDLE's dispatch loop
//! actually drives — atomic claim, revision fencing, the claim→close lifecycle,
//! starvation fallback — against the pinned executables themselves, and pins
//! the behavioral differences a consumer must handle.
//!
//! Deliberately self-contained: the pin resolution below re-implements the
//! registry lookup instead of importing `capability_framework`, so this suite
//! stays decoupled from that file's ongoing rework and states its own
//! preconditions. Evidence and the deploy-safety verdict this suite encodes
//! live in `docs/verification/needle-variant-dispatch-validation.md`.

use serial_test::serial;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;

/// Registry role whose tree predates attempt-resolution entirely (release 0.2.4)
const PRE_FEATURE_ROLE: &str = "pre_feature";
/// Registry role carrying the attempt-resolution surface (0.2.6 pin)
const FEATURE_ENABLED_ROLE: &str = "attempt_resolution_f25ab5c";

/// Commands NEEDLE's dispatch loop drives on every bead, whatever the variant
const NEEDLE_CORE_COMMANDS: [&str; 12] = [
    "capabilities",
    "init",
    "create",
    "list",
    "claim",
    "update",
    "close",
    "reopen",
    "release",
    "sync",
    "why",
    "doctor",
];

/// A pinned binary resolved from `pinned-binaries/commits.json`, verified
/// against its recorded provenance before any test drives it
struct Variant {
    role: &'static str,
    binary: PathBuf,
    version: String,
}

impl Variant {
    fn run(&self, args: &[&str], workspace: &Path) -> Output {
        Command::new(&self.binary)
            .args(args)
            .current_dir(workspace)
            .output()
            .expect("pinned binary must be executable")
    }

    fn stdout(&self, output: &Output) -> String {
        String::from_utf8_lossy(&output.stdout).to_string()
    }

    fn stderr(&self, output: &Output) -> String {
        String::from_utf8_lossy(&output.stderr).to_string()
    }
}

fn pinned_binaries_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("pinned-binaries")
}

/// Resolve a registry role to its on-disk pin, checking the embedded version
/// and the sha256 of the bytes against the pin's metadata
fn resolve_variant(role: &'static str) -> Variant {
    let registry: serde_json::Value =
        serde_json::from_slice(&std::fs::read(pinned_binaries_dir().join("commits.json")).unwrap())
            .unwrap();
    let name = registry[role]["binary_name"].as_str().unwrap_or_else(|| {
        panic!("pin role '{role}' has no binary_name in pinned-binaries/commits.json")
    });
    let binary = pinned_binaries_dir().join(name);
    assert!(
        binary.is_file(),
        "pin role '{role}' missing on disk: {binary:?}"
    );

    let meta: serde_json::Value = serde_json::from_slice(
        &std::fs::read(pinned_binaries_dir().join(format!("{name}.metadata.json"))).unwrap(),
    )
    .unwrap();

    // Byte identity: a silently swapped pin must fail here, not downstream
    let bytes = std::fs::read(&binary).unwrap();
    let digest = hex::encode(Sha256::digest(&bytes));
    assert_eq!(
        digest,
        meta["binary_sha256"].as_str().unwrap(),
        "pin '{role}' bytes do not match the recorded sha256"
    );

    let scratch = scratch_dir();
    let version = String::from_utf8_lossy(
        &Command::new(&binary)
            .args(["--version"])
            .current_dir(scratch.path())
            .output()
            .unwrap()
            .stdout,
    )
    .trim()
    .to_string();
    if let Some(recorded) = meta["embedded_version_string"].as_str() {
        assert_eq!(
            version, recorded,
            "pin '{role}' reports a version its metadata does not record"
        );
    }

    Variant {
        role,
        binary,
        version,
    }
}

/// The two variants of record, both provenance-checked
fn variant_pair() -> (Variant, Variant) {
    (
        resolve_variant(PRE_FEATURE_ROLE),
        resolve_variant(FEATURE_ENABLED_ROLE),
    )
}

/// Disposable workspace; /var/tmp so no ancestor carries a foreign `.beads`
fn scratch_dir() -> TempDir {
    tempfile::Builder::new()
        .prefix("bead-needle-dispatch-")
        .tempdir_in("/var/tmp")
        .unwrap()
}

fn init_workspace(variant: &Variant) -> TempDir {
    let dir = scratch_dir();
    let out = variant.run(&["init", "--prefix", "probe"], dir.path());
    assert!(
        out.status.success(),
        "init failed for {}: {}",
        variant.role,
        variant.stderr(&out)
    );
    dir
}

/// Create one P1 task and return its id
fn create_bead(variant: &Variant, workspace: &Path, title: &str) -> String {
    let out = variant.run(
        &[
            "create",
            "--title",
            title,
            "--priority",
            "1",
            "--issue-type",
            "task",
        ],
        workspace,
    );
    assert!(
        out.status.success(),
        "create failed for {}: {}",
        variant.role,
        variant.stderr(&out)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Status and assignee as `bead show` reports them
fn state_of(variant: &Variant, workspace: &Path, id: &str) -> String {
    let out = variant.run(&["show", id], workspace);
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| l.starts_with("Status:") || l.starts_with("Assignee:"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn capabilities(variant: &Variant) -> serde_json::Value {
    let scratch = scratch_dir();
    let out = variant.run(&["capabilities"], scratch.path());
    assert!(
        out.status.success(),
        "capabilities failed for {}",
        variant.role
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

#[test]
#[serial]
fn both_variant_pins_resolve_as_distinct_verified_binaries() {
    let (pre, enabled) = variant_pair();
    assert_ne!(
        pre.version, enabled.version,
        "variants must be distinct builds"
    );
    assert!(
        pre.version.contains("0.2.4"),
        "pre-feature pin should be the 0.2.4 baseline, got: {}",
        pre.version
    );
    assert!(
        enabled.version.contains("0.2.6"),
        "feature-enabled pin should be a 0.2.6 build, got: {}",
        enabled.version
    );
}

/// The NEEDLE-relevant contract fields are identical on both sides, so a
/// consumer can drive the dispatch loop without variant-specific code paths
#[test]
#[serial]
fn shared_dispatch_contract_is_identical_across_variants() {
    let (pre, enabled) = variant_pair();
    let pre_caps = capabilities(&pre);
    let enabled_caps = capabilities(&enabled);

    for field in [
        "contract",
        "implementation",
        "store_layout",
        "atomic_claim",
        "logical_revision",
        "auto_flush",
    ] {
        assert_eq!(
            pre_caps.get(field),
            enabled_caps.get(field),
            "contract field '{field}' differs across variants"
        );
    }
    assert_eq!(
        pre_caps["atomic_claim"], true,
        "atomic_claim must hold on both pins"
    );
    assert_eq!(
        pre_caps["logical_revision"], true,
        "--if-revision fencing must hold on both pins"
    );

    let commands = |caps: &serde_json::Value| {
        caps["commands"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c.as_str().unwrap().to_string())
            .collect::<Vec<_>>()
    };
    for core in NEEDLE_CORE_COMMANDS {
        assert!(
            commands(&pre_caps).iter().any(|c| c == core),
            "pre-feature pin lacks NEEDLE core command '{core}'"
        );
        assert!(
            commands(&enabled_caps).iter().any(|c| c == core),
            "feature-enabled pin lacks NEEDLE core command '{core}'"
        );
    }

    // The capability delta is exactly the attempt-resolution surface
    assert!(pre_caps.get("attempt_outcome").is_none());
    assert_eq!(enabled_caps["attempt_outcome"]["supported"], true);
    let enabled_commands = commands(&enabled_caps);
    for extra in ["resolve", "watchdog", "resource", "analyze-exclusion"] {
        assert!(
            !commands(&pre_caps).iter().any(|c| c == extra),
            "pre-feature pin unexpectedly advertises '{extra}'"
        );
        assert!(
            enabled_commands.iter().any(|c| c == extra),
            "feature-enabled pin must advertise '{extra}'"
        );
    }
}

/// Atomic claim exclusivity, exercised for real: N concurrent claimant
/// processes on a one-bead queue must yield exactly one winner, with the
/// losers getting a clean exit-0 null — never a duplicate assignment
#[test]
#[serial]
fn atomic_claim_is_exclusive_under_parallel_invocation_on_both_variants() {
    for variant in [variant_pair().0, variant_pair().1] {
        let ws = init_workspace(&variant);
        let id = create_bead(&variant, ws.path(), "parallel claim probe");

        let mut children = Vec::new();
        for i in 0..8 {
            let binary = variant.binary.clone();
            let dir = ws.path().to_path_buf();
            children.push(std::thread::spawn(move || {
                Command::new(binary)
                    .args(["claim", "--assignee", &format!("worker-{i}"), "--json"])
                    .current_dir(dir)
                    .output()
                    .unwrap()
            }));
        }
        let mut winners = 0;
        let mut clean_losers = 0;
        for child in children {
            let out = child.join().unwrap();
            assert!(
                out.status.success(),
                "claim process failed on {}",
                variant.role
            );
            let stdout = String::from_utf8_lossy(&out.stdout);
            if stdout.contains(&format!("\"bead_id\":\"{id}\"")) {
                winners += 1;
            } else if stdout.contains("\"bead_id\":null") {
                clean_losers += 1;
            }
        }
        assert_eq!(
            winners, 1,
            "exactly one claimant may win, not {winners}, on {}",
            variant.role
        );
        assert_eq!(
            clean_losers, 7,
            "every loser must get a clean null on {}",
            variant.role
        );
        assert!(
            state_of(&variant, ws.path(), &id).contains("InProgress"),
            "the winning claim must have moved the bead to in_progress"
        );
    }
}

/// An empty ready frontier is `bead_id: null` with exit 0 on both variants —
/// NEEDLE treats that as "no work", never as a failure
#[test]
#[serial]
fn empty_frontier_claim_is_exit_zero_null_on_both_variants() {
    for variant in [variant_pair().0, variant_pair().1] {
        let ws = init_workspace(&variant);
        let out = variant.run(
            &["claim", "--assignee", "lonely-worker", "--json"],
            ws.path(),
        );
        assert!(out.status.success());
        assert!(
            variant.stdout(&out).contains("\"bead_id\":null"),
            "empty frontier must claim null, got: {}",
            variant.stdout(&out)
        );
    }
}

/// Stale-revision fencing rejects with exit 4, a Conflict message naming both
/// revisions, and no state change — on both variants
#[test]
#[serial]
fn stale_revision_fencing_rejects_cleanly_on_both_variants() {
    for variant in [variant_pair().0, variant_pair().1] {
        let ws = init_workspace(&variant);
        let id = create_bead(&variant, ws.path(), "fencing probe");

        // Bump the revision once so a fence at 1 is genuinely stale
        let out = variant.run(&["update", &id, "--status", "in_progress"], ws.path());
        assert!(out.status.success(), "unfenced update must succeed");
        let before = state_of(&variant, ws.path(), &id);

        let out = variant.run(
            &["update", &id, "--status", "deferred", "--if-revision", "1"],
            ws.path(),
        );
        assert_eq!(out.status.code(), Some(4), "stale fence must exit 4");
        let stderr = variant.stderr(&out);
        assert!(
            stderr.contains("Conflict: Revision mismatch") && stderr.contains("expected 1"),
            "fencing error must name the conflict, got: {stderr}"
        );
        assert!(
            !stderr.contains("panicked"),
            "fencing rejection must not panic"
        );
        assert_eq!(
            state_of(&variant, ws.path(), &id),
            before,
            "a fenced-off update must not change state"
        );

        // And the same update at the current revision goes through.
        let out = variant.run(
            &["update", &id, "--status", "deferred", "--if-revision", "2"],
            ws.path(),
        );
        assert!(out.status.success(), "current-revision update must succeed");
    }
}

/// The full NEEDLE lifecycle round trip on both variants: claim → release →
/// assign → clear-assignee → close → reopen → close. This is the fallback
/// path a consumer drives when the attempt-resolution surface is unavailable.
#[test]
#[serial]
fn needle_dispatch_lifecycle_round_trip_on_both_variants() {
    for variant in [variant_pair().0, variant_pair().1] {
        let ws = init_workspace(&variant);
        let id = create_bead(&variant, ws.path(), "lifecycle probe");

        let run = |args: &[&str]| {
            let out = variant.run(args, ws.path());
            assert!(
                out.status.success(),
                "{:?} failed on {}: {}",
                args,
                variant.role,
                variant.stderr(&out)
            );
        };

        run(&["claim", "--assignee", "worker-a"]);
        assert!(
            state_of(&variant, ws.path(), &id).contains("InProgress Assignee: worker-a"),
            "claim must assign and start the bead"
        );

        run(&["release", &id]);
        assert_eq!(state_of(&variant, ws.path(), &id), "Status: Open");

        // Assigned-but-open: invisible to claim, recoverable via --clear-assignee
        run(&["update", &id, "--assignee", "stuck-worker"]);
        let out = variant.run(
            &["claim", "--assignee", "other-worker", "--json"],
            ws.path(),
        );
        assert!(
            variant.stdout(&out).contains("\"bead_id\":null"),
            "claim must never hand out an assigned-open bead"
        );
        run(&["update", &id, "--clear-assignee"]);
        assert_eq!(state_of(&variant, ws.path(), &id), "Status: Open");

        run(&["close", &id, "--reason", "lifecycle probe done"]);
        assert_eq!(state_of(&variant, ws.path(), &id), "Status: Closed");

        run(&["reopen", &id]);
        let reopened = state_of(&variant, ws.path(), &id);
        assert!(
            reopened.contains("Open"),
            "reopen must return the bead to open, got: {reopened}"
        );
        assert!(
            !reopened.contains("Assignee:"),
            "reopen must clear the assignee so the frontier can reclaim it"
        );

        run(&["close", &id, "--reason", "lifecycle probe finished"]);
    }
}

/// The pre-feature pin's degradation surface: capability-gated commands are
/// absent from the contract before invocation and rejected by clap with exit 2
/// — a clean fallback signal, never a panic
#[test]
#[serial]
fn pre_feature_pin_rejects_capability_commands_cleanly() {
    let (pre, _) = variant_pair();
    let ws = init_workspace(&pre);
    let id = create_bead(&pre, ws.path(), "degradation probe");

    for args in [
        vec![
            "resolve",
            &id,
            "--attempt-id",
            "att-1",
            "--outcome",
            "verified_success",
        ],
        vec!["watchdog"],
    ] {
        let out = pre.run(&args, ws.path());
        assert_eq!(
            out.status.code(),
            Some(2),
            "capability-gated command {:?} must be clap-rejected on the pre-feature pin",
            args
        );
        let stderr = pre.stderr(&out);
        assert!(
            stderr.contains("unrecognized subcommand"),
            "expected clap's unrecognized-subcommand error, got: {stderr}"
        );
        assert!(!stderr.contains("panicked"), "degradation must not panic");
    }

    // The core loop is unimpaired on the same binary
    let out = pre.run(
        &["close", &id, "--reason", "fallback path works"],
        ws.path(),
    );
    assert!(out.status.success());
}

/// The feature-enabled pin recognizes the attempt-resolution surface; a
/// resolve invocation fails on domain grounds, never as an unknown subcommand
#[test]
#[serial]
fn feature_enabled_pin_recognizes_the_resolution_surface() {
    let (_, enabled) = variant_pair();
    let ws = init_workspace(&enabled);
    let id = create_bead(&enabled, ws.path(), "recognition probe");

    let help = enabled.run(&["resolve", "--help"], ws.path());
    assert!(help.status.success(), "resolve --help must succeed");
    let help_text = enabled.stdout(&help);
    for flag in ["--attempt-id", "--outcome", "--action"] {
        assert!(
            help_text.contains(flag),
            "resolve help must document {flag}"
        );
    }

    let out = enabled.run(
        &[
            "resolve",
            &id,
            "--attempt-id",
            "att-recognition",
            "--outcome",
            "verified_success",
            "--action",
            "close",
        ],
        ws.path(),
    );
    let combined = format!("{}{}", enabled.stdout(&out), enabled.stderr(&out));
    assert!(
        !combined.contains("unrecognized subcommand"),
        "resolve must be a recognized subcommand on the feature-enabled pin"
    );
    assert!(
        !combined.contains("panicked"),
        "failure must be a clean error"
    );
}

/// RECORDED DEFECT (documented in
/// docs/verification/needle-variant-dispatch-validation.md): the
/// feature-enabled pin's resolve execution path selects `updated_at_revision`,
/// a column no store migration creates (src/service/attempt.rs get_issue_state
/// vs src/store/migrations.rs), so every resolve fails with an exit-5
/// integrity error even on a workspace its own init created.
///
/// This test pins the failure mode: clean exit 5, no panic, and — the part
/// that keeps the variant deployable — no state change. When the column bug
/// is fixed this test must be updated to assert the successful resolve +
/// receipt instead.
#[test]
#[serial]
fn resolve_execution_fails_with_recorded_integrity_defect_on_feature_enabled_pin() {
    let (_, enabled) = variant_pair();
    let ws = init_workspace(&enabled);
    let id = create_bead(&enabled, ws.path(), "resolve defect probe");

    let before = state_of(&enabled, ws.path(), &id);
    let out = enabled.run(
        &[
            "resolve",
            &id,
            "--attempt-id",
            "att-defect",
            "--outcome",
            "verified_success",
            "--action",
            "close",
        ],
        ws.path(),
    );
    assert_eq!(
        out.status.code(),
        Some(5),
        "resolve currently fails with the recorded integrity error"
    );
    let stderr = enabled.stderr(&out);
    assert!(
        stderr.contains("no such column: updated_at_revision"),
        "expected the recorded missing-column integrity error, got: {stderr}"
    );
    assert_eq!(
        state_of(&enabled, ws.path(), &id),
        before,
        "the failed resolve must be atomic: no lifecycle change may leak"
    );
}

/// DOCUMENTED DIFFERENCE: starvation visibility. With an assigned-open bead in
/// the workspace, the pre-feature pin still lists it as ready (overstating
/// what claim will deliver) and writes no diagnostic; the feature-enabled pin
/// excludes it from the frontier and writes the starvation diagnostic naming
/// the remedy. Claim itself refuses the bead on both — this is a visibility
/// difference, not a double-assignment hazard.
#[test]
#[serial]
fn starvation_visibility_differs_across_variants() {
    for (variant, listed, diagnostic) in [
        (variant_pair().0, true, false),
        (variant_pair().1, false, true),
    ] {
        let ws = init_workspace(&variant);
        let id = create_bead(&variant, ws.path(), "starvation visibility probe");
        let out = variant.run(&["update", &id, "--assignee", "stuck-worker"], ws.path());
        assert!(out.status.success());

        let ready = variant.run(&["list", "--ready"], ws.path());
        assert!(ready.status.success());
        let stdout = variant.stdout(&ready);
        assert_eq!(
            stdout.contains(&id),
            listed,
            "ready-frontier visibility of assigned-open beads differs by variant (pin {})",
            variant.role
        );

        let log = ws
            .path()
            .join(".beads/diagnostics/pluck-starvation-diagnostic.log");
        assert_eq!(
            log.exists(),
            diagnostic,
            "starvation diagnostic presence differs by variant (pin {})",
            variant.role
        );
        if diagnostic {
            let content = std::fs::read_to_string(&log).unwrap();
            assert!(
                content.contains(&id) && content.contains("--clear-assignee"),
                "diagnostic must name the bead and the remedy, got: {content}"
            );
        }
    }
}

/// NEEDLE may drive one workspace with either binary across an upgrade, so
/// each variant must be able to finish work the other started
#[test]
#[serial]
fn variants_are_interoperable_on_shared_workspaces() {
    let (pre, enabled) = variant_pair();

    // Workspace initialized by the pre-feature pin, finished by the new one
    let ws = init_workspace(&pre);
    let id = create_bead(&pre, ws.path(), "cross-variant old-init");
    let out = enabled.run(&["claim", "--assignee", "upgraded-worker"], ws.path());
    assert!(
        out.status.success(),
        "new binary must claim on an old-init workspace"
    );
    let out = enabled.run(
        &["close", &id, "--reason", "finished by new binary"],
        ws.path(),
    );
    assert!(out.status.success());
    assert!(
        pre.run(&["show", &id], ws.path()).status.success(),
        "old binary must still read the workspace the new one mutated"
    );

    // And the reverse: new-init workspace finished by the old binary
    let ws = init_workspace(&enabled);
    let id = create_bead(&enabled, ws.path(), "cross-variant new-init");
    let out = pre.run(&["claim", "--assignee", "downgraded-worker"], ws.path());
    assert!(
        out.status.success(),
        "old binary must claim on a new-init workspace"
    );
    let out = pre.run(
        &[
            "update",
            &id,
            "--status",
            "in_progress",
            "--if-revision",
            "2",
        ],
        ws.path(),
    );
    assert!(
        out.status.success(),
        "old binary must fence-update a new-init workspace"
    );
    let out = pre.run(
        &["close", &id, "--reason", "finished by old binary"],
        ws.path(),
    );
    assert!(out.status.success());
    assert!(
        enabled.run(&["show", &id], ws.path()).status.success(),
        "new binary must still read the workspace the old one mutated"
    );
}

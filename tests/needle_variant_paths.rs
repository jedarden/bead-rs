//! NEEDLE consumer-path validation across the pinned binary variants
//!
//! Drives the two behaviors NEEDLE actually relies on, against the pinned
//! executables themselves (provenance- and byte-checked via
//! [`capability_framework::capability_variant_pair`]):
//!
//! 1. the fallback path — the `needle-v1` required-command surface a consumer
//!    reconciles through when attempt-resolution is absent. This is the whole
//!    integration story on the pre-feature pin, and it must be intact on the
//!    feature-enabled pin too, because ADR-012 lets NEEDLE fall back to the
//!    legacy reconciliation sequence at any time.
//! 2. the atomic paths — server-selected claim and `--if-revision`, which
//!    carry the fleet's correctness guarantees on both variants.
//!
//! Findings recorded 2026-09-03 (validation bead beadrs-146944d1; full report
//! in `docs/verification/needle-variant-paths-validation-2026-09-03.md`):
//!
//! - The needle-v1 surface, atomic claim, and the revision guard are
//!   behaviorally identical on both pins. The only output delta is additive
//!   (0.2.6 adds `effective_status` and `manual_blocked` to list/show
//!   records; nothing is removed or re-typed), which the
//!   needle-cli-contract-v1 additional-fields rule permits.
//! - DEFECT (documented, not asserted-away): both feature-enabled pins
//!   advertise `attempt_outcome.supported: true` while `resolve` fails on
//!   every invocation with `no such column: updated_at_revision`
//!   (`src/service/attempt.rs` `get_issue_state` selects a column no
//!   migration creates). `resolve_failure_is_loud_and_non_corrupting`
//!   pins the safety invariant that matters for deploy safety and exercises
//!   the atomic receipt path on whichever side of the fix it runs.

mod capability_framework;
use capability_framework::*;
use serial_test::serial;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;
use std::sync::{Arc, Barrier};
use tempfile::TempDir;

/// Parsed first record from a `show --json` / `list --json` response,
/// accepting either a JSON array or one JSON object per line (the contract
/// allows both record-stream shapes)
fn first_record(stdout: &str) -> serde_json::Value {
    let trimmed = stdout.trim();
    let parsed: serde_json::Value = serde_json::from_str(trimmed)
        .unwrap_or_else(|_| panic!("machine-readable output must be valid JSON, got: {trimmed}"));
    match parsed {
        serde_json::Value::Array(items) => {
            assert!(
                !items.is_empty(),
                "expected at least one record, got: {trimmed}"
            );
            items.into_iter().next().expect("non-empty array")
        }
        // One JSON object per line: take the first line that mentions nothing
        // else — a single object response is the whole payload
        obj @ serde_json::Value::Object(_) => {
            let first_line = trimmed.lines().next().expect("non-empty output");
            serde_json::from_str(first_line).unwrap_or(obj)
        }
        other => panic!("expected array or object record stream, got: {other}"),
    }
}

/// The `bead_id` field of a claim response, requiring a successful exit
fn claimed_bead_id(output: &std::process::Output) -> Option<String> {
    assert!(
        output.status.success(),
        "claim must exit 0 even when the queue is empty: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let response: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout))
            .expect("claim --json must emit a JSON object");
    response
        .get("bead_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Run one full pass of the needle-cli-contract-v1 required-command surface
/// in a fresh workspace backed by `binary`, failing on the first gap
fn needle_v1_surface_pass(binary: &Path) {
    let harness = BinaryHarness::with_binary(binary).unwrap();
    harness.init_workspace().unwrap();

    // Version: nonempty name and version
    let version = harness.run(&["--version"]).unwrap();
    assert!(version.status.success(), "--version must succeed");
    assert!(
        !String::from_utf8_lossy(&version.stdout).trim().is_empty(),
        "--version must print something"
    );

    // Create: stdout contains the new ID only
    let created = harness
        .run(&[
            "create",
            "--title",
            "needle surface probe",
            "--priority",
            "2",
            "--issue-type",
            "task",
        ])
        .unwrap();
    assert!(created.status.success(), "create must succeed");
    let id = String::from_utf8_lossy(&created.stdout).trim().to_string();
    assert!(
        !id.is_empty() && !id.contains('\n'),
        "create must print the ID only, got: {id:?}"
    );

    // List: record stream mentioning the new issue
    let listed = harness
        .run(&["list", "--json", "--limit", "999999"])
        .unwrap();
    assert!(listed.status.success(), "list --json must succeed");
    assert!(
        String::from_utf8_lossy(&listed.stdout).contains(&id),
        "list must surface the created issue"
    );

    // Show: the issue JSON minimum per the contract
    let shown = harness.run(&["show", &id, "--json"]).unwrap();
    assert!(shown.status.success(), "show --json must succeed");
    let record = first_record(&String::from_utf8_lossy(&shown.stdout));
    for field in [
        "id",
        "title",
        "description",
        "priority",
        "status",
        "assignee",
        "dependencies",
        "created_at",
        "updated_at",
    ] {
        assert!(
            record.get(field).is_some(),
            "show record must carry contract field '{field}': {record}"
        );
    }

    // Claim: selection and assignment are one atomic transaction; the empty
    // queue afterwards is a successful domain result with no bead_id
    let claim = harness
        .run(&["claim", "--assignee", "needle-worker", "--json"])
        .unwrap();
    assert_eq!(
        claimed_bead_id(&claim).as_deref(),
        Some(id.as_str()),
        "the only eligible bead must be claimed"
    );
    let drained = harness
        .run(&["claim", "--assignee", "needle-worker", "--json"])
        .unwrap();
    assert_eq!(
        claimed_bead_id(&drained),
        None,
        "empty queue must return exit 0 with no bead_id"
    );

    // Update, label, dependency
    let updated = harness
        .run(&["update", &id, "--notes", "surface probe note"])
        .unwrap();
    assert!(updated.status.success(), "update must succeed");
    let labeled = harness
        .run(&["label", "add", &id, "--label", "needle-probe"])
        .unwrap();
    assert!(labeled.status.success(), "label add must succeed");
    let second = harness
        .run(&[
            "create",
            "--title",
            "blocker probe",
            "--priority",
            "1",
            "--issue-type",
            "task",
        ])
        .unwrap();
    let blocker = String::from_utf8_lossy(&second.stdout).trim().to_string();
    let dep = harness
        .run(&["dep", "add", &id, &blocker, "--kind", "blocks"])
        .unwrap();
    assert!(dep.status.success(), "dep add must succeed");

    // Close and reopen
    let closed = harness
        .run(&["close", &id, "--reason", "surface probe done"])
        .unwrap();
    assert!(closed.status.success(), "close must succeed");
    let verified = harness.run(&["show", &id, "--json"]).unwrap();
    let record = first_record(&String::from_utf8_lossy(&verified.stdout));
    assert_eq!(record["status"], "closed", "close must close the issue");
    let reopened = harness.run(&["reopen", &id]).unwrap();
    assert!(reopened.status.success(), "reopen must succeed");

    // Diagnostics and checkpoint
    let doctor = harness.run(&["doctor"]).unwrap();
    assert!(doctor.status.success(), "doctor must succeed");
    let flush = harness.run(&["sync", "flush-only"]).unwrap();
    assert!(flush.status.success(), "sync flush-only must succeed");
    let checkpoint = harness.workspace_path().join(".beads/checkpoint");
    assert!(
        checkpoint.join("current.json").exists(),
        "checkpoint pointer must exist"
    );
    assert!(
        checkpoint.join("forensic.jsonl").exists(),
        "forensic checkpoint must exist"
    );
}

/// The needle-v1 required-command surface works end to end on both variants —
/// this IS the fallback path NEEDLE reconciles through when the
/// attempt-resolution capability is absent
#[test]
#[serial]
fn needle_v1_required_surface_works_on_both_variants() {
    let pair = capability_variant_pair().unwrap();
    for variant in [&pair.capability_absent, &pair.capability_present] {
        needle_v1_surface_pass(&variant.path);
    }
}

/// Concurrent claimers against one workspace receive distinct work on both
/// variants — the atomic server-selected claim the fleet's no-duplicate-work
/// guarantee rests on
#[test]
#[serial]
fn concurrent_claims_receive_distinct_beads_on_both_variants() {
    let pair = capability_variant_pair().unwrap();
    const CLAIMERS: usize = 8;
    const SEED: usize = CLAIMERS + 2;

    for variant in [&pair.capability_absent, &pair.capability_present] {
        let harness = BinaryHarness::with_binary(&variant.path).unwrap();
        harness.init_workspace().unwrap();
        for i in 0..SEED {
            let out = harness
                .run(&[
                    "create",
                    "--title",
                    &format!("atomic seed {i}"),
                    "--priority",
                    "3",
                    "--issue-type",
                    "task",
                ])
                .unwrap();
            assert!(out.status.success(), "seed create must succeed");
        }

        let barrier = Arc::new(Barrier::new(CLAIMERS));
        let mut handles = Vec::new();
        for i in 0..CLAIMERS {
            let binary: PathBuf = variant.path.clone();
            let workspace = harness.workspace_path().to_path_buf();
            let barrier = Arc::clone(&barrier);
            handles.push(std::thread::spawn(move || {
                barrier.wait();
                StdCommand::new(binary)
                    .current_dir(workspace)
                    .args([
                        "claim",
                        "--assignee",
                        &format!("racing-worker-{i}"),
                        "--json",
                    ])
                    .output()
                    .expect("claim process must run")
            }));
        }

        let mut claimed = Vec::new();
        for handle in handles {
            let output = handle.join().expect("claim thread must not panic");
            claimed.push(claimed_bead_id(&output));
        }

        assert!(
            claimed.iter().all(Option::is_some),
            "every concurrent claim must win a bead on pin '{}': {claimed:?}",
            variant.role
        );
        let ids: Vec<String> = claimed.into_iter().flatten().collect();
        let distinct: std::collections::HashSet<&String> = ids.iter().collect();
        assert_eq!(
            distinct.len(),
            ids.len(),
            "duplicate claim detected on pin '{}': {ids:?}",
            variant.role
        );
    }
}

/// The optimistic-concurrency guard behaves identically on both variants: a
/// current `--if-revision` succeeds, a stale one is rejected with exit 4 and
/// leaves the committed state untouched
#[test]
#[serial]
fn stale_revision_guard_rejects_identically_on_both_variants() {
    let pair = capability_variant_pair().unwrap();
    for variant in [&pair.capability_absent, &pair.capability_present] {
        let harness = BinaryHarness::with_binary(&variant.path).unwrap();
        harness.init_workspace().unwrap();
        let created = harness
            .run(&[
                "create",
                "--title",
                "revision guard probe",
                "--priority",
                "2",
                "--issue-type",
                "task",
            ])
            .unwrap();
        let id = String::from_utf8_lossy(&created.stdout).trim().to_string();
        let shown = harness.run(&["show", &id, "--json"]).unwrap();
        let record = first_record(&String::from_utf8_lossy(&shown.stdout));
        let revision = record["revision"]
            .as_i64()
            .expect("revision must be an integer");

        let current = harness
            .run(&[
                "update",
                &id,
                "--if-revision",
                &revision.to_string(),
                "--notes",
                "committed write",
            ])
            .unwrap();
        assert!(
            current.status.success(),
            "update at the current revision must succeed on pin '{}': {}",
            variant.role,
            String::from_utf8_lossy(&current.stderr)
        );

        let stale = harness
            .run(&[
                "update",
                &id,
                "--if-revision",
                &revision.to_string(),
                "--notes",
                "stale write",
            ])
            .unwrap();
        assert_eq!(
            stale.status.code(),
            Some(4),
            "stale --if-revision must exit 4 on pin '{}'",
            variant.role
        );

        // The rejected write must not have landed
        let verified = harness.run(&["show", &id, "--json"]).unwrap();
        let record = first_record(&String::from_utf8_lossy(&verified.stdout));
        assert_eq!(
            record["notes"], "committed write",
            "the stale write must be rejected without touching committed state"
        );
    }
}

/// Process-boundary error handling is stable across variants: an unknown
/// subcommand is a clean clap rejection (exit 2) and running outside any
/// workspace fails with exit 3 — never a panic, never a corrupt exit code
#[test]
#[serial]
fn error_exit_codes_are_stable_across_variants() {
    let pair = capability_variant_pair().unwrap();
    // A directory with no .beads ancestor, for the no-workspace case
    let bare = TempDir::with_prefix_in("needle-bare-", "/var/tmp").unwrap();

    for variant in [&pair.capability_absent, &pair.capability_present] {
        let role = variant.role;
        let unknown = StdCommand::new(&variant.path)
            .current_dir(bare.path())
            .arg("definitely-not-a-subcommand")
            .output()
            .unwrap();
        assert_eq!(
            unknown.status.code(),
            Some(2),
            "unknown subcommand must exit 2 on pin '{}'",
            variant.role
        );
        let stderr = String::from_utf8_lossy(&unknown.stderr);
        assert!(
            stderr.contains("unrecognized subcommand"),
            "unknown subcommand must degrade through clap on pin '{role}': {stderr}"
        );
        assert!(
            !stderr.contains("panicked"),
            "degradation must not panic: {stderr}"
        );

        let no_workspace = StdCommand::new(&variant.path)
            .current_dir(bare.path())
            .args(["list"])
            .output()
            .unwrap();
        assert_eq!(
            no_workspace.status.code(),
            Some(3),
            "list outside a workspace must exit 3 on pin '{}'",
            variant.role
        );
    }
}

/// The advertised attempt-resolution capability must be safe for a consumer
/// that negotiated it: a resolve either lands atomically (receipt, idempotent
/// replay) or fails loudly without corrupting the issue
///
/// DEFECT GUARD, recorded 2026-09-03: on both feature-enabled pins resolve
/// currently takes the failure arm on EVERY invocation —
/// `Error: Integrity error: ... no such column: updated_at_revision`
/// (`get_issue_state` in `src/service/attempt.rs` selects a column no
/// migration creates) — against a workspace the same binary initialized. The
/// failure is loud (exit 5, structured error) and non-corrupting (issue left
/// open, revision unchanged), which is why the capability-present pin stays
/// deploy-safe behind the negotiation gate, but the advertised exactly-once
/// resolution is unavailable until the schema defect is fixed. When the fix
/// lands this test keeps passing on the success arm and starts enforcing the
/// receipt and replay-idempotency contract.
#[test]
#[serial]
fn resolve_failure_is_loud_and_non_corrupting_on_feature_enabled_pin() {
    let pair = capability_variant_pair().unwrap();
    let present = &pair.capability_present;
    let harness = BinaryHarness::with_binary(&present.path).unwrap();
    harness.init_workspace().unwrap();

    // The capability is advertised — a consumer would negotiate exactly this
    let caps = harness.get_default_capabilities().unwrap();
    assert_eq!(
        caps["attempt_outcome"]["supported"], true,
        "precondition: the feature-enabled pin advertises attempt-resolution"
    );

    let created = harness
        .run(&[
            "create",
            "--title",
            "resolve target",
            "--priority",
            "2",
            "--issue-type",
            "task",
        ])
        .unwrap();
    let id = String::from_utf8_lossy(&created.stdout).trim().to_string();

    let resolved = harness
        .run(&[
            "resolve",
            &id,
            "--attempt-id",
            "att-needle-validation",
            "--outcome",
            "verified_success",
            "--action",
            "close",
        ])
        .unwrap();

    if resolved.status.success() {
        // Post-fix arm: enforce the atomic receipt contract
        let receipt: serde_json::Value =
            serde_json::from_str(&String::from_utf8_lossy(&resolved.stdout))
                .expect("a successful resolve must emit a JSON receipt");
        assert_eq!(receipt["attempt_id"], "att-needle-validation");
        assert_eq!(receipt["issue_id"], id);
        assert_eq!(
            receipt["is_replay"], false,
            "first resolution is not a replay"
        );

        let shown = harness.run(&["show", &id, "--json"]).unwrap();
        let record = first_record(&String::from_utf8_lossy(&shown.stdout));
        assert_eq!(
            record["status"], "closed",
            "the close action must have applied"
        );

        // Replay with the same attempt id is idempotent
        let replay = harness
            .run(&[
                "resolve",
                &id,
                "--attempt-id",
                "att-needle-validation",
                "--outcome",
                "verified_success",
                "--action",
                "close",
            ])
            .unwrap();
        assert!(
            replay.status.success(),
            "replay of a resolved attempt must succeed: {}",
            String::from_utf8_lossy(&replay.stderr)
        );
        let receipt: serde_json::Value =
            serde_json::from_str(&String::from_utf8_lossy(&replay.stdout))
                .expect("replay must emit a JSON receipt");
        assert_eq!(
            receipt["is_replay"], true,
            "second resolution must be flagged as replay"
        );
    } else {
        // Current arm: the documented schema defect. The failure must be loud
        // and structured, and must not have mutated the issue.
        let stderr = String::from_utf8_lossy(&resolved.stderr);
        assert!(
            !stderr.is_empty(),
            "a failed resolve must explain itself on stderr"
        );
        assert!(
            !stderr.contains("panicked"),
            "resolve failure must be a structured error, not a panic: {stderr}"
        );
        assert!(
            stderr.contains("no such column: updated_at_revision"),
            "expected the documented updated_at_revision schema defect; got: {stderr} — \
             if resolve now succeeds for a real reason, re-review this test's success arm"
        );
        let shown = harness.run(&["show", &id, "--json"]).unwrap();
        let record = first_record(&String::from_utf8_lossy(&shown.stdout));
        assert_eq!(
            record["status"], "open",
            "a failed resolve must leave the issue untouched"
        );
    }
}

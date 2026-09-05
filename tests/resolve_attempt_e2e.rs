//! End-to-end CLI tests for `bead resolve` (attempt-outcome-v1)
//!
//! `attempt_outcome_round_trip.rs` exercises the attempt tables by writing
//! rows into SQLite directly; before this suite nothing drove the `resolve`
//! subcommand through `get_issue_state`, which is how the `updated_at_revision`
//! schema defect (beadrs-6b891bb7: `get_issue_state` selected a column no
//! migration creates, so every resolve exited 5 with "no such column") stayed
//! invisible while `capabilities` advertised `attempt_outcome.supported: true`.
//!
//! These tests drive the real binary against a real workspace, so the whole
//! execution path a consumer depends on — issue-state read, revision guard,
//! replay detection, tier progression, receipt emission, lifecycle action —
//! is exercised the way NEEDLE drives it.

use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

fn bead() -> Command {
    Command::cargo_bin("bead").expect("the bead binary must be built for the test")
}

fn setup_workspace() -> TempDir {
    let temp_dir = TempDir::new().unwrap();
    bead()
        .args(["init", "--prefix", "test", "--skip-foreign-workspace"])
        .current_dir(temp_dir.path())
        .assert()
        .success();
    temp_dir
}

fn create_issue(workspace: &std::path::Path, title: &str) -> String {
    let output = bead()
        .args(["create", "--title", title])
        .current_dir(workspace)
        .assert()
        .success();
    String::from_utf8(output.get_output().stdout.clone())
        .unwrap()
        .trim()
        .to_string()
}

/// Run `bead resolve` with `--format json` and return the parsed receipt
fn resolve(workspace: &std::path::Path, args: &[&str]) -> Value {
    let output = bead()
        .args(["resolve"])
        .args(args)
        .args(["--format", "json"])
        .current_dir(workspace)
        .assert()
        .success();
    serde_json::from_str(&String::from_utf8_lossy(&output.get_output().stdout))
        .expect("a successful resolve must emit a JSON receipt")
}

/// `show --json` record for one issue, accepting either an array or a single
/// object as the response shape
fn show(workspace: &std::path::Path, id: &str) -> Value {
    let output = bead()
        .args(["show", id, "--json"])
        .current_dir(workspace)
        .assert()
        .success();
    let text = String::from_utf8_lossy(&output.get_output().stdout);
    let parsed: Value = serde_json::from_str(text.trim())
        .unwrap_or_else(|e| panic!("show --json must be JSON: {e} — {text}"));
    match parsed {
        Value::Array(items) => items.into_iter().next().expect("show must return a record"),
        obj @ Value::Object(_) => obj,
        other => panic!("expected a record from show, got: {other}"),
    }
}

#[test]
fn resolve_verified_success_close_emits_receipt_and_closes() {
    let ws = setup_workspace();
    let id = create_issue(ws.path(), "resolve e2e: close");

    let receipt = resolve(
        ws.path(),
        &[
            &id,
            "--attempt-id",
            "att-e2e-close",
            "--outcome",
            "verified_success",
            "--action",
            "close",
            "--reason",
            "resolution verified",
        ],
    );

    assert_eq!(receipt["issue_id"], id.as_str());
    assert_eq!(receipt["attempt_id"], "att-e2e-close");
    assert_eq!(
        receipt["is_replay"], false,
        "the first resolution is not a replay"
    );
    assert_eq!(receipt["resulting_state"], "closed");
    assert_eq!(
        receipt["resulting_issue_revision"], 2,
        "the close action bumps the logical revision"
    );
    assert!(
        receipt["receipt_id"]
            .as_str()
            .is_some_and(|s| !s.is_empty()),
        "the receipt must carry an id"
    );

    let record = show(ws.path(), &id);
    assert_eq!(record["status"], "closed", "the close action must apply");
    assert_eq!(
        record["revision"], 2,
        "the stored revision must match the receipt"
    );
}

#[test]
fn resolve_replay_is_idempotent() {
    let ws = setup_workspace();
    let id = create_issue(ws.path(), "resolve e2e: replay");

    let first = resolve(
        ws.path(),
        &[
            &id,
            "--attempt-id",
            "att-e2e-replay",
            "--outcome",
            "verified_success",
            "--action",
            "close",
            "--reason",
            "resolution verified",
        ],
    );

    // Same attempt id + same request: the original receipt comes back
    // unchanged, flagged as a replay, with no second lifecycle mutation
    let replay = resolve(
        ws.path(),
        &[
            &id,
            "--attempt-id",
            "att-e2e-replay",
            "--outcome",
            "verified_success",
            "--action",
            "close",
            "--reason",
            "resolution verified",
        ],
    );

    assert_eq!(replay["is_replay"], true, "the second call is a replay");
    assert_eq!(
        replay["receipt_id"], first["receipt_id"],
        "a replay returns the original receipt"
    );
    assert_eq!(
        replay["resulting_issue_revision"], first["resulting_issue_revision"],
        "a replay must not bump the revision again"
    );
}

#[test]
fn resolve_work_failure_progresses_tier() {
    let ws = setup_workspace();
    let id = create_issue(ws.path(), "resolve e2e: tier progression");

    // Default action (none) keeps the lifecycle state while the bead-scoped
    // failure count climbs: unproven → retryable → struggling → quarantined
    let tiers = [1, 2, 3];
    for (n, expected_tier) in tiers.iter().enumerate() {
        let receipt = resolve(
            ws.path(),
            &[
                &id,
                "--attempt-id",
                &format!("att-e2e-fail-{n}"),
                "--outcome",
                "work_failure",
            ],
        );
        assert_eq!(
            receipt["resulting_attempt_tier"], *expected_tier,
            "failure #{n} must land on tier {expected_tier}"
        );
    }

    let record = show(ws.path(), &id);
    assert_eq!(record["status"], "open", "action none leaves status alone");
    // Each resolve above recomputed its tier from the row get_issue_state
    // read back, so the 1→2→3 climb across separate CLI invocations is the
    // proof that tier and consecutive_failures persist between resolves.
}

#[test]
fn resolve_revision_guard_conflicts_with_wrong_expected_revision() {
    let ws = setup_workspace();
    let id = create_issue(ws.path(), "resolve e2e: revision guard");

    // The issue sits at revision 1; expecting 5 must conflict, not resolve
    bead()
        .args([
            "resolve",
            &id,
            "--attempt-id",
            "att-e2e-stale",
            "--outcome",
            "verified_success",
            "--action",
            "close",
            "--reason",
            "resolution verified",
            "--if-revision",
            "5",
        ])
        .current_dir(ws.path())
        .assert()
        .failure()
        .code(4)
        .stderr(predicates::str::contains("revision 5"));

    // And the failed guard must have changed nothing
    let record = show(ws.path(), &id);
    assert_eq!(record["status"], "open");
    assert_eq!(record["revision"], 1);
}

#[test]
fn resolve_unknown_issue_is_a_not_found_not_an_integrity_error() {
    let ws = setup_workspace();

    bead()
        .args([
            "resolve",
            "test-nonexistent",
            "--attempt-id",
            "att-e2e-missing",
            "--outcome",
            "verified_success",
        ])
        .current_dir(ws.path())
        .assert()
        .failure()
        .code(3)
        .stderr(predicates::str::contains("not found"));
}

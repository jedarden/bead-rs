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

/// Claim the workspace's one ready issue for `assignee` and hand back the
/// claim result, whose `claim_epoch` is the credential that claimant owns for
/// as long as it holds the claim.
fn claim_ready(workspace: &std::path::Path, assignee: &str, lease_ttl: Option<&str>) -> Value {
    let mut cmd = bead();
    cmd.args(["claim", "--assignee", assignee]);
    if let Some(ttl) = lease_ttl {
        cmd.args(["--lease-ttl", ttl]);
    }
    let output = cmd
        .args(["--json"])
        .current_dir(workspace)
        .assert()
        .success();
    serde_json::from_slice(&output.get_output().stdout).expect("claim must emit a JSON result")
}

/// The `attempt_outcome` record the published checkpoint carries for
/// `attempt_id`, if that resolution was ever recorded. A refused resolve must
/// leave this `None` — the receipt is written in the same transaction the
/// credential is checked in, so a refusal cannot publish one.
fn published_outcome(workspace: &std::path::Path, attempt_id: &str) -> Option<Value> {
    let forensic = std::fs::read_to_string(
        workspace
            .join(".beads")
            .join("checkpoint")
            .join("forensic.jsonl"),
    )
    .expect("the checkpoint must be published");
    forensic
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .find_map(|record| {
            (record["record_type"] == "attempt_outcome"
                && record["attempt_outcome"]["attempt_id"] == attempt_id)
                .then(|| record["attempt_outcome"].clone())
        })
}

/// Open the workspace's own SQLite store read-only.
fn open_store(workspace: &std::path::Path) -> rusqlite::Connection {
    rusqlite::Connection::open_with_flags(
        workspace.join(".beads").join("beads.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .expect("the workspace store must exist")
}

/// Count the `attempt_outcomes` rows the store holds for `attempt_id`. The
/// published checkpoint is a derived view of what a mutation committed, so a
/// clean one is consistent with the store by construction; counting the rows
/// at the source is what distinguishes "nothing was written" from "something
/// was written and simply not published".
fn stored_receipts(workspace: &std::path::Path, attempt_id: &str) -> usize {
    open_store(workspace)
        .query_row(
            "SELECT COUNT(*) FROM attempt_outcomes WHERE attempt_id = ?1",
            [attempt_id],
            |row| row.get::<_, i64>(0),
        )
        .map(|n| n as usize)
        .expect("attempt_outcomes must be queryable")
}

/// Count the audit events the store holds for `issue_id`.
fn stored_event_count(workspace: &std::path::Path, issue_id: &str) -> usize {
    open_store(workspace)
        .query_row(
            "SELECT COUNT(*) FROM events WHERE issue_id = ?1",
            [issue_id],
            |row| row.get::<_, i64>(0),
        )
        .map(|n| n as usize)
        .expect("events must be queryable")
}

/// Resolve on a claimed issue is claimant-owned: with no credential the fence
/// refuses it with exit 4 and the whole resolution — receipt, tier, audit
/// event, revision — stays unwritten.
#[test]
fn resolve_on_a_claimed_issue_without_a_credential_records_nothing() {
    let ws = setup_workspace();
    let id = create_issue(ws.path(), "resolve e2e: credentialless fence");

    claim_ready(ws.path(), "worker-one", None);
    let held = show(ws.path(), &id);
    assert_eq!(held["status"].as_str(), Some("in_progress"));
    let events_before = stored_event_count(ws.path(), &id);

    bead()
        .args([
            "resolve",
            &id,
            "--attempt-id",
            "att-e2e-fence",
            "--outcome",
            "verified_success",
        ])
        .current_dir(ws.path())
        .assert()
        .failure()
        .code(4)
        .stderr(predicates::str::contains("Claim-epoch credential"));

    // The refusal was a pure no-op: claim, revision, and attempt untouched.
    let after = show(ws.path(), &id);
    assert_eq!(after["status"], held["status"]);
    assert_eq!(after["assignee"], held["assignee"]);
    assert_eq!(
        after["revision"], held["revision"],
        "a refused resolve must not bump the revision"
    );
    assert_eq!(after["claim_epoch"], held["claim_epoch"]);
    assert_eq!(
        stored_receipts(ws.path(), "att-e2e-fence"),
        0,
        "a refused resolve must not write an attempt receipt"
    );
    assert_eq!(
        stored_event_count(ws.path(), &id),
        events_before,
        "a refused resolve must append no audit event"
    );
    assert!(
        published_outcome(ws.path(), "att-e2e-fence").is_none(),
        "a refused resolve must not write an attempt receipt, not even to the checkpoint"
    );

    // Nothing was recorded, so the claimant's own resolve of that same
    // attempt id is the first resolution, not a replay of one the refusal
    // invented — and the claim survives it.
    let epoch = held["claim_epoch"].as_i64().unwrap().to_string();
    let receipt = resolve(
        ws.path(),
        &[
            &id,
            "--attempt-id",
            "att-e2e-fence",
            "--outcome",
            "verified_success",
            "--fencing-token",
            &epoch,
        ],
    );
    assert_eq!(
        receipt["is_replay"], false,
        "the credentialless refusal must not have left a receipt behind"
    );
    assert_eq!(
        show(ws.path(), &id)["assignee"].as_str(),
        Some("worker-one"),
        "the claimant's own resolve must keep the claim"
    );
}

/// A credential from a superseded epoch cannot resolve a claim that moved to
/// a new holder: exit 4, the new claim stands, and the attempt is still
/// unrecorded for the current holder to resolve.
#[test]
fn resolve_with_a_superseded_credential_leaves_the_new_claim_intact() {
    let ws = setup_workspace();
    let id = create_issue(ws.path(), "resolve e2e: superseded credential");

    let first_epoch = claim_ready(ws.path(), "worker-one", None)["claim_epoch"]
        .as_i64()
        .unwrap();
    bead()
        .args(["release", &id, "--fencing-token", &first_epoch.to_string()])
        .current_dir(ws.path())
        .assert()
        .success();
    let second_epoch = claim_ready(ws.path(), "worker-two", None)["claim_epoch"]
        .as_i64()
        .unwrap();
    assert!(
        second_epoch > first_epoch,
        "a new claim must mint a later epoch"
    );

    let held = show(ws.path(), &id);
    let events_before = stored_event_count(ws.path(), &id);

    // The previous holder still remembers the credential it was issued...
    bead()
        .args([
            "resolve",
            &id,
            "--attempt-id",
            "att-e2e-superseded",
            "--outcome",
            "verified_success",
            "--fencing-token",
            &first_epoch.to_string(),
        ])
        .current_dir(ws.path())
        .assert()
        .failure()
        .code(4)
        .stderr(predicates::str::contains("Claim-epoch credential mismatch"));

    // ...but the tenure it names is gone: the new claim stands and the
    // attempt is untouched.
    let after = show(ws.path(), &id);
    assert_eq!(after["assignee"].as_str(), Some("worker-two"));
    assert_eq!(after["claim_epoch"], held["claim_epoch"]);
    assert_eq!(
        after["revision"], held["revision"],
        "a stale credential must not bump the revision"
    );
    assert_eq!(
        stored_receipts(ws.path(), "att-e2e-superseded"),
        0,
        "a stale credential must not write an attempt receipt"
    );
    assert_eq!(
        stored_event_count(ws.path(), &id),
        events_before,
        "a stale credential must append no audit event"
    );
    assert!(published_outcome(ws.path(), "att-e2e-superseded").is_none());

    // The current holder's credential is the one that admits the attempt.
    let receipt = resolve(
        ws.path(),
        &[
            &id,
            "--attempt-id",
            "att-e2e-superseded",
            "--outcome",
            "verified_success",
            "--fencing-token",
            &second_epoch.to_string(),
        ],
    );
    assert_eq!(receipt["is_replay"], false);
}

/// A credential that does not parse never reaches the fence at all: it is a
/// usage error about the token itself, refused before any authorization
/// decision, and it records nothing -- same no-op, different words and code.
#[test]
fn resolve_with_an_unparseable_credential_is_usage_and_records_nothing() {
    let ws = setup_workspace();
    let id = create_issue(ws.path(), "resolve e2e: unparseable credential");

    claim_ready(ws.path(), "worker-one", None);
    let held = show(ws.path(), &id);
    let events_before = stored_event_count(ws.path(), &id);

    bead()
        .args([
            "resolve",
            &id,
            "--attempt-id",
            "att-e2e-unparseable",
            "--outcome",
            "verified_success",
            "--fencing-token",
            "not-an-epoch",
        ])
        .current_dir(ws.path())
        .assert()
        .failure()
        .code(2)
        .stderr(predicates::str::contains("Invalid fencing token"));

    let after = show(ws.path(), &id);
    assert_eq!(after["status"], held["status"]);
    assert_eq!(after["assignee"], held["assignee"]);
    assert_eq!(
        after["revision"], held["revision"],
        "a usage refusal must not bump the revision"
    );
    assert_eq!(
        stored_receipts(ws.path(), "att-e2e-unparseable"),
        0,
        "a usage refusal must not write an attempt receipt"
    );
    assert_eq!(
        stored_event_count(ws.path(), &id),
        events_before,
        "a usage refusal must append no audit event"
    );
}

/// A leased claim adds an expiry dimension, not an exemption: its holder
/// still names the epoch to resolve against it, and only that holder's
/// credential resolves — repeatedly, with the same receipt, exactly as an
/// unleased claim resolves.
#[test]
fn a_leased_claim_is_resolved_only_with_its_own_credential() {
    let ws = setup_workspace();
    let id = create_issue(ws.path(), "resolve e2e: leased fence");

    let claim = claim_ready(ws.path(), "worker-one", Some("300"));
    let epoch = claim["claim_epoch"].as_i64().unwrap();
    assert!(claim["lease"]["fencing_token"].as_i64().unwrap() > 0);
    let held = show(ws.path(), &id);
    let events_before = stored_event_count(ws.path(), &id);

    // No credential: with nothing to compare the lease against, the refusal
    // comes from the epoch check behind it.
    bead()
        .args([
            "resolve",
            &id,
            "--attempt-id",
            "att-e2e-leased",
            "--outcome",
            "verified_success",
        ])
        .current_dir(ws.path())
        .assert()
        .failure()
        .code(4)
        .stderr(predicates::str::contains("Claim-epoch credential"));

    // A superseded credential trips the live lease's own dimension first,
    // which reports the mismatch in its own words -- same exit code, same
    // no-op either way.
    bead()
        .args([
            "resolve",
            &id,
            "--attempt-id",
            "att-e2e-leased",
            "--outcome",
            "verified_success",
            "--fencing-token",
            "999",
        ])
        .current_dir(ws.path())
        .assert()
        .failure()
        .code(4)
        .stderr(predicates::str::contains("Fencing token mismatch"));

    let after = show(ws.path(), &id);
    assert_eq!(after["status"], held["status"]);
    assert_eq!(after["assignee"], held["assignee"]);
    assert_eq!(after["revision"], held["revision"]);
    assert_eq!(
        stored_receipts(ws.path(), "att-e2e-leased"),
        0,
        "neither refusal may write an attempt receipt"
    );
    assert_eq!(
        stored_event_count(ws.path(), &id),
        events_before,
        "neither refusal may append an audit event"
    );
    assert!(published_outcome(ws.path(), "att-e2e-leased").is_none());

    // The lease's own holder resolves as before, and the idempotent replay
    // returns the original receipt rather than recording a second one.
    let credential = epoch.to_string();
    let receipt = resolve(
        ws.path(),
        &[
            &id,
            "--attempt-id",
            "att-e2e-leased",
            "--outcome",
            "verified_success",
            "--fencing-token",
            &credential,
        ],
    );
    assert_eq!(receipt["is_replay"], false);
    let replay = resolve(
        ws.path(),
        &[
            &id,
            "--attempt-id",
            "att-e2e-leased",
            "--outcome",
            "verified_success",
            "--fencing-token",
            &credential,
        ],
    );
    assert_eq!(
        replay["is_replay"], true,
        "the current claimant's replay must stay idempotent"
    );
    assert_eq!(
        replay["receipt_id"], receipt["receipt_id"],
        "a replay returns the original receipt"
    );
    assert_eq!(
        show(ws.path(), &id)["assignee"].as_str(),
        Some("worker-one"),
        "resolving must not disturb the lease it was admitted by"
    );
}

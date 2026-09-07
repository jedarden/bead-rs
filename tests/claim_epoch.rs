//! End-to-end contract tests for claim-epoch issuance, visibility, and
//! enforcement.
//!
//! The credential is issued by every successful claim and projected by `show
//! --json`; while an issue is claimed it is also the fence every claimant-owned
//! mutation must present. The suites below exercise each mutation family --
//! update, release, close, reopen, resource-lock add and remove, and atomic
//! attempt resolve -- with no credential, a superseded one, and the current
//! one.

use assert_cmd::Command;
use serde_json::Value;
use std::path::Path;
use std::process::Output;

fn run<I, S>(workspace: &Path, args: I) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let argv: Vec<String> = args
        .into_iter()
        .map(|a| a.as_ref().to_string_lossy().into_owned())
        .collect();
    let output = Command::cargo_bin("bead")
        .unwrap()
        .current_dir(workspace)
        .arg("--skip-foreign-workspace")
        .args(&argv)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "bead {argv:?} failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    output
}

fn claim(workspace: &Path, assignee: &str, leased: bool) -> Value {
    let mut args = vec!["claim", "--assignee", assignee, "--json"];
    if leased {
        args.extend(["--lease-ttl", "300"]);
    }
    serde_json::from_slice(&run(workspace, &args).stdout).unwrap()
}

fn shown_issue(workspace: &Path, id: &str) -> Value {
    let shown: Value =
        serde_json::from_slice(&run(workspace, ["show", id, "--json"]).stdout).unwrap();
    shown.as_array().unwrap()[0].clone()
}

fn checkpoint_issue(workspace: &Path, id: &str) -> Value {
    let forensic = std::fs::read_to_string(
        workspace
            .join(".beads")
            .join("checkpoint")
            .join("forensic.jsonl"),
    )
    .unwrap();
    forensic
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .find_map(|record| {
            (record["record_type"] == "issue" && record["issue"]["id"] == id)
                .then(|| record["issue"].clone())
        })
        .expect("claimed issue in published checkpoint")
}

/// The state a fenced mutation must leave untouched: who holds the claim, at
/// what revision, and in what status.
fn held_state(workspace: &Path, id: &str) -> (String, String, i64) {
    let issue = shown_issue(workspace, id);
    (
        issue["status"].as_str().unwrap().to_string(),
        issue["assignee"].as_str().unwrap().to_string(),
        issue["revision"].as_i64().unwrap(),
    )
}

/// Events the checkpoint has published for `id` -- the audit surface a
/// rejected mutation must not advance.
fn published_event_count(workspace: &Path, id: &str) -> usize {
    published_events(workspace, id).len()
}

/// Those events in publication order, for the assertions that need what an
/// event says rather than merely that it exists (or doesn't).
fn published_events(workspace: &Path, id: &str) -> Vec<Value> {
    let forensic = std::fs::read_to_string(
        workspace
            .join(".beads")
            .join("checkpoint")
            .join("forensic.jsonl"),
    )
    .unwrap();
    forensic
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .filter(|record| record["record_type"] == "event" && record["event"]["issue_id"] == *id)
        .map(|record| record["event"].clone())
        .collect()
}

/// Run `bead` and hand back the raw result, for the cases where a non-zero
/// exit *is* the assertion.
fn run_raw<I, S>(workspace: &Path, args: I) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    Command::cargo_bin("bead")
        .unwrap()
        .current_dir(workspace)
        .arg("--skip-foreign-workspace")
        .args(args)
        .output()
        .unwrap()
}

/// Assert the mutation was refused by the credential gate specifically --
/// exit 4 plus the credential message, so a command failing for some other
/// reason (bad status transition, unknown id) cannot pass for a fence.
fn assert_credential_conflict(output: &Output, label: &str) {
    assert_eq!(
        output.status.code(),
        Some(4),
        "{label} must conflict with exit 4, got {:?}; stderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr),
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Claim-epoch credential"),
        "{label} must be refused by the credential gate, got: {stderr}"
    );
}

/// Every claimant-owned mutation of a claimed issue, as full `bead` argument
/// vectors carrying *no* credential. Kept as data so the missing-credential
/// and stale-credential sweeps below cover exactly the same surface.
fn claimant_mutations(id: &str) -> Vec<(&'static str, Vec<String>)> {
    let owned = |subcommand: &'static str, extra: &[&str]| -> Vec<String> {
        let mut args: Vec<String> = vec![subcommand.to_string(), id.to_string()];
        args.extend(extra.iter().map(|a| a.to_string()));
        args
    };
    vec![
        ("update", owned("update", &["--notes", "probed"])),
        ("release", owned("release", &[])),
        ("close", owned("close", &["--reason", "probed"])),
        ("reopen", owned("reopen", &[])),
        ("resource add", resource_mutation(id, "add")),
        ("resource remove", resource_mutation(id, "remove")),
        (
            "resolve",
            owned(
                "resolve",
                &[
                    "--attempt-id",
                    "urn:needle:attempt:fence-probe",
                    "--outcome",
                    "verified_success",
                ],
            ),
        ),
    ]
}

/// `bead resource <verb> <ID> --key K` -- the id follows the nested
/// subcommand, unlike the top-level mutations above.
fn resource_mutation(id: &str, verb: &str) -> Vec<String> {
    vec![
        "resource".to_string(),
        verb.to_string(),
        id.to_string(),
        "--key".to_string(),
        "gpu:0".to_string(),
    ]
}

/// Append the given claim-epoch credential to an argument vector.
fn with_credential(mut args: Vec<String>, credential: &str) -> Vec<String> {
    args.push("--fencing-token".to_string());
    args.push(credential.to_string());
    args
}

#[test]
fn every_claim_mints_a_visible_monotonic_epoch_that_survives_rebuild() {
    let workspace = tempfile::tempdir().unwrap();
    run(workspace.path(), ["init", "--prefix", "epoch"]);
    let id = String::from_utf8(
        run(
            workspace.path(),
            ["create", "--title", "claim epoch target"],
        )
        .stdout,
    )
    .unwrap()
    .trim()
    .to_string();

    let first = claim(workspace.path(), "worker-one", false);
    let first_epoch = first["claim_epoch"].as_i64().expect("plain claim epoch");
    assert!(first_epoch > 0);
    assert!(first["lease"].is_null());
    assert_eq!(
        shown_issue(workspace.path(), &id)["claim_epoch"],
        first_epoch
    );
    assert_eq!(
        checkpoint_issue(workspace.path(), &id)["claim_epoch"],
        first_epoch
    );

    // The claimant presents the credential it was issued to release its own
    // claim; releasing without it is the conflict
    // `a_claimed_issue_rejects_every_credentialless_claimant_mutation` pins.
    run(
        workspace.path(),
        ["release", &id, "--fencing-token", &first_epoch.to_string()],
    );
    let second = claim(workspace.path(), "worker-two", true);
    let second_epoch = second["claim_epoch"].as_i64().expect("leased claim epoch");
    assert!(second_epoch > first_epoch);
    assert_eq!(second["lease"]["fencing_token"], second_epoch);

    // Simulate clone/restart recovery from the auto-published checkpoint.
    let saved_checkpoint = workspace.path().join("saved-forensic.jsonl");
    std::fs::copy(
        workspace
            .path()
            .join(".beads")
            .join("checkpoint")
            .join("forensic.jsonl"),
        &saved_checkpoint,
    )
    .unwrap();
    std::fs::remove_file(workspace.path().join(".beads").join("beads.db")).unwrap();
    run(workspace.path(), ["init"]);
    run(
        workspace.path(),
        [
            "sync",
            "import-only",
            "--input",
            saved_checkpoint.to_str().unwrap(),
            "--restore-into-empty",
            "--actor",
            "claim-epoch-test",
        ],
    );
    assert_eq!(
        shown_issue(workspace.path(), &id)["claim_epoch"],
        second_epoch
    );

    run(
        workspace.path(),
        ["release", &id, "--fencing-token", &second_epoch.to_string()],
    );
    let third = claim(workspace.path(), "worker-three", false);
    assert!(third["claim_epoch"].as_i64().unwrap() > second_epoch);
}

/// A claimed issue refuses every claimant-owned mutation that presents no
/// credential, and each refusal is a pure no-op: no status move, no assignee
/// change, no revision bump, and no published event.
#[test]
fn a_claimed_issue_rejects_every_credentialless_claimant_mutation() {
    let workspace = tempfile::tempdir().unwrap();
    run(workspace.path(), ["init", "--prefix", "epoch"]);
    let id = String::from_utf8(run(workspace.path(), ["create", "--title", "fence target"]).stdout)
        .unwrap()
        .trim()
        .to_string();

    // The sweep presents no credential at all, so the epoch the claim minted
    // is irrelevant here -- only that a claim exists to be fenced.
    claim(workspace.path(), "worker-one", false);
    let before = held_state(workspace.path(), &id);
    let events_before = published_event_count(workspace.path(), &id);

    for (label, args) in claimant_mutations(&id) {
        let output = run_raw(workspace.path(), &args);
        assert_credential_conflict(&output, &format!("{label} with no credential"));

        // Nothing moved: the gate runs inside the mutation's own IMMEDIATE
        // transaction, so a refusal cannot leave a half-applied write behind.
        assert_eq!(
            held_state(workspace.path(), &id),
            before,
            "{label} must not change status, assignee, or revision"
        );
        assert_eq!(
            published_event_count(workspace.path(), &id),
            events_before,
            "{label} must not publish an event"
        );
    }
}

/// The current claimant, presenting the credential its claim was issued, can
/// run the whole claimant surface. `close` is the interesting one: it closes
/// the issue but leaves the claim -- and therefore the fence -- standing, so
/// the reopening is itself a claimant-owned mutation.
#[test]
fn the_current_claimant_can_perform_every_claimant_mutation() {
    let workspace = tempfile::tempdir().unwrap();
    run(workspace.path(), ["init", "--prefix", "epoch"]);
    let id =
        String::from_utf8(run(workspace.path(), ["create", "--title", "current holder"]).stdout)
            .unwrap()
            .trim()
            .to_string();

    let epoch = claim(workspace.path(), "worker-one", false)["claim_epoch"]
        .as_i64()
        .unwrap();
    let credential = epoch.to_string();

    // The update leg, pinned hardest: the current credential lands it, and the
    // claim survives as the same tenure -- same assignee, same epoch, exactly
    // one revision bump. The claim-keeping mutations below do not move the
    // revision at all, so the bump is the update's own.
    let update_args = claimant_mutations(&id)
        .into_iter()
        .find(|(label, _)| *label == "update")
        .map(|(_, args)| args)
        .expect("update is part of the claimant surface");
    let revision_before = held_state(workspace.path(), &id).2;
    run(workspace.path(), with_credential(update_args, &credential));
    let updated = held_state(workspace.path(), &id);
    assert_eq!(
        updated.1, "worker-one",
        "update with the current credential must keep the claim"
    );
    assert_eq!(
        updated.2,
        revision_before + 1,
        "update with the current credential must bump the revision exactly once"
    );
    assert_eq!(
        shown_issue(workspace.path(), &id)["claim_epoch"].as_i64(),
        Some(epoch),
        "update with the current credential must leave the epoch standing"
    );

    // The remaining mutations that keep the claim: each lands and the claim
    // survives. `close` and `release` change the claim's standing and `reopen`
    // is only reachable once it is closed, so those three run below, in order.
    for (label, args) in claimant_mutations(&id) {
        if matches!(label, "close" | "release" | "reopen" | "update") {
            continue;
        }
        run(workspace.path(), with_credential(args, &credential));
        assert_eq!(
            held_state(workspace.path(), &id).1,
            "worker-one",
            "{label} with the current credential must keep the claim"
        );
    }

    let close = claimant_mutations(&id)
        .into_iter()
        .find(|(label, _)| *label == "close")
        .map(|(_, args)| args)
        .expect("close is part of the claimant surface");

    // `close` ends the issue's openness but not the claim's tenure.
    run(workspace.path(), with_credential(close, &credential));
    let closed = shown_issue(workspace.path(), &id);
    assert_eq!(closed["status"].as_str(), Some("closed"));
    assert_eq!(closed["assignee"].as_str(), Some("worker-one"));
    assert_eq!(closed["claim_epoch"].as_i64(), Some(epoch));

    // So reopening it is still a claimant-owned mutation, and the holder's
    // own credential admits it.
    run(
        workspace.path(),
        ["reopen", &id, "--fencing-token", &credential],
    );
    assert_eq!(
        shown_issue(workspace.path(), &id)["assignee"],
        Value::Null,
        "reopen hands the claim back"
    );

    // With no holder the fence is gone, and `release` proves the last leg of
    // the surface on an issue that is claimed again.
    let next_epoch = claim(workspace.path(), "worker-two", false)["claim_epoch"]
        .as_i64()
        .unwrap();
    run(
        workspace.path(),
        ["release", &id, "--fencing-token", &next_epoch.to_string()],
    );
    assert_eq!(shown_issue(workspace.path(), &id)["assignee"], Value::Null);
}

/// The credential that authorizes a close is recorded on the `closed` event
/// itself, in the same transaction that closes: a reader of the change feed
/// can tell which epoch admitted the close rather than inferring it from the
/// claim the issue happened to still carry afterwards. An unclaimed close has
/// no credential to record, and says so with nulls rather than by dropping
/// the fields.
#[test]
fn the_closed_event_carries_the_credential_that_authorized_it() {
    let workspace = tempfile::tempdir().unwrap();
    run(workspace.path(), ["init", "--prefix", "epoch"]);
    let id =
        String::from_utf8(run(workspace.path(), ["create", "--title", "audited close"]).stdout)
            .unwrap()
            .trim()
            .to_string();

    let epoch = claim(workspace.path(), "worker-one", false)["claim_epoch"]
        .as_i64()
        .unwrap();

    run(
        workspace.path(),
        with_credential(lifecycle_mutation(&id, "close"), &epoch.to_string()),
    );

    let closed = published_events(workspace.path(), &id)
        .into_iter()
        .find(|event| event["kind"] == "closed")
        .expect("a close appends a closed event");

    assert_eq!(
        closed["detail"]["claim_epoch"].as_i64(),
        Some(epoch),
        "the closed event names the epoch that fenced it"
    );
    assert_eq!(
        closed["detail"]["presented_fencing_token"].as_i64(),
        Some(epoch),
        "the closed event names the credential the claimant presented"
    );
    assert_eq!(
        closed["detail"]["reason"].as_str(),
        Some("probed"),
        "the reason is recorded alongside the credential"
    );

    // Closing does not retire the claim, so the epoch the event named is
    // still the one the issue carries -- the event is not describing a
    // tenure the row has already forgotten.
    assert_eq!(shown_issue(workspace.path(), &id)["claim_epoch"], epoch);

    // The unclaimed close has no credential behind it, and the event says so
    // rather than leaving the reader to distinguish a null from an omitted
    // key.
    let unclaimed =
        String::from_utf8(run(workspace.path(), ["create", "--title", "unaudited close"]).stdout)
            .unwrap()
            .trim()
            .to_string();
    run(
        workspace.path(),
        ["close", &unclaimed, "--reason", "no claim"],
    );

    let unclaimed_closed = published_events(workspace.path(), &unclaimed)
        .into_iter()
        .find(|event| event["kind"] == "closed")
        .expect("an unclaimed close still appends a closed event");
    assert_eq!(unclaimed_closed["detail"]["claim_epoch"], Value::Null);
    assert_eq!(
        unclaimed_closed["detail"]["presented_fencing_token"],
        Value::Null
    );
}

/// A credential from a superseded epoch is as good as none: after the claim
/// moves to a new holder, the previous epoch cannot mutate the issue, and the
/// new holder's credential can.
#[test]
fn a_superseded_credential_cannot_mutate_the_claim_that_replaced_it() {
    let workspace = tempfile::tempdir().unwrap();
    run(workspace.path(), ["init", "--prefix", "epoch"]);
    let id = String::from_utf8(
        run(
            workspace.path(),
            ["create", "--title", "superseded credential"],
        )
        .stdout,
    )
    .unwrap()
    .trim()
    .to_string();

    let first_epoch = claim(workspace.path(), "worker-one", false)["claim_epoch"]
        .as_i64()
        .unwrap();
    run(
        workspace.path(),
        ["release", &id, "--fencing-token", &first_epoch.to_string()],
    );

    let second_epoch = claim(workspace.path(), "worker-two", false)["claim_epoch"]
        .as_i64()
        .unwrap();
    assert!(
        second_epoch > first_epoch,
        "a new claim must mint a later epoch"
    );

    let held = held_state(workspace.path(), &id);
    let events_before = published_event_count(workspace.path(), &id);

    for (label, args) in claimant_mutations(&id) {
        let stale = with_credential(args, &first_epoch.to_string());
        let output = run_raw(workspace.path(), &stale);
        assert_credential_conflict(&output, &format!("{label} with the superseded credential"));
        assert_eq!(
            held_state(workspace.path(), &id),
            held,
            "{label} with a superseded credential must leave the current claim intact"
        );
        assert_eq!(
            published_event_count(workspace.path(), &id),
            events_before,
            "{label} with a superseded credential must not publish an event"
        );
    }

    // The current holder is not fenced by its own claim.
    let update = with_credential(
        claimant_mutations(&id).remove(0).1,
        &second_epoch.to_string(),
    );
    run(workspace.path(), &update);
    assert_eq!(
        held_state(workspace.path(), &id).1,
        "worker-two",
        "the current claimant keeps mutating its own claim"
    );
}

/// Reassigning is a change of ownership tenure, not just a field edit: the
/// new holder gets the next epoch and the previous holder's still-remembered
/// credential stops working. This is the release-and-reassign hole.
#[test]
fn reassigning_mints_a_new_epoch_that_fences_the_previous_holder() {
    let workspace = tempfile::tempdir().unwrap();
    run(workspace.path(), ["init", "--prefix", "epoch"]);
    let id =
        String::from_utf8(run(workspace.path(), ["create", "--title", "reassign fence"]).stdout)
            .unwrap()
            .trim()
            .to_string();

    let first_epoch = claim(workspace.path(), "worker-one", false)["claim_epoch"]
        .as_i64()
        .unwrap();

    run(
        workspace.path(),
        [
            "update",
            &id,
            "--assignee",
            "worker-two",
            "--fencing-token",
            &first_epoch.to_string(),
        ],
    );

    let reassigned = shown_issue(workspace.path(), &id);
    assert_eq!(reassigned["assignee"].as_str(), Some("worker-two"));
    let second_epoch = reassigned["claim_epoch"].as_i64().unwrap();
    assert!(
        second_epoch > first_epoch,
        "reassignment must mint the next epoch, not inherit {first_epoch}"
    );

    // The holder that just handed the claim over cannot come back with the
    // credential it was issued for the tenure it no longer holds.
    for (label, args) in claimant_mutations(&id) {
        let stale = with_credential(args, &first_epoch.to_string());
        let output = run_raw(workspace.path(), &stale);
        assert_credential_conflict(&output, &format!("{label} from the previous holder"));
    }
    assert_eq!(held_state(workspace.path(), &id).1, "worker-two");
}

/// A *leased* claim is fenced by the same credential as an ordinary one: the
/// lease adds an expiry dimension, it does not exempt its holder from naming
/// the epoch. The sweep runs the four claimant-owned lifecycle mutations
/// against a live lease with no credential, then with one from a superseded
/// epoch, then the holder's own.
#[test]
fn a_leased_claim_is_fenced_by_the_same_credential() {
    let workspace = tempfile::tempdir().unwrap();
    run(workspace.path(), ["init", "--prefix", "epoch"]);
    let id = String::from_utf8(run(workspace.path(), ["create", "--title", "leased fence"]).stdout)
        .unwrap()
        .trim()
        .to_string();

    // The superseded epoch comes from a previous *leased* tenure: its
    // credential is the one a crashed worker would still be holding.
    let superseded = claim(workspace.path(), "worker-one", true)["claim_epoch"]
        .as_i64()
        .unwrap();
    run(
        workspace.path(),
        ["release", &id, "--fencing-token", &superseded.to_string()],
    );
    let leased = claim(workspace.path(), "worker-two", true);
    let current = leased["claim_epoch"].as_i64().unwrap().to_string();
    assert!(leased["lease"]["fencing_token"].as_i64().unwrap() > 0);
    assert!(current.parse::<i64>().unwrap() > superseded);

    // A missing credential is refused by the credential gate itself: with no
    // expected token the lease dimension has nothing to compare, so the
    // refusal comes from the epoch check behind it.
    let held = held_state(workspace.path(), &id);
    for label in ["update", "release", "close", "reopen"] {
        let args = lifecycle_mutation(&id, label);
        assert_credential_conflict(
            &run_raw(workspace.path(), &args),
            &format!("{label} on a leased claim with no credential"),
        );
        assert_eq!(
            held_state(workspace.path(), &id),
            held,
            "{label} must not move a leased claim"
        );
    }

    // A superseded credential is also a conflict. Here the lease dimension
    // fires first -- it compares the presented token against the epoch the
    // live lease fences -- so the message is the fencing-token one rather
    // than the claim-epoch one; the exit code and the no-op are what the
    // fence promises either way.
    for label in ["update", "release", "close", "reopen"] {
        let stale = with_credential(lifecycle_mutation(&id, label), &superseded.to_string());
        let output = run_raw(workspace.path(), &stale);
        assert_eq!(
            output.status.code(),
            Some(4),
            "{label} with a superseded credential on a leased claim must conflict, got {:?}",
            output.status.code(),
        );
        assert_eq!(
            held_state(workspace.path(), &id),
            held,
            "{label} with a superseded credential must leave the lease standing"
        );
    }

    // The lease's own holder still lands them, and landing `update` is exactly
    // one revision bump on an unchanged epoch: the lease narrows who may
    // mutate, not what a landed mutation does. The sweeps above are asserted to
    // have moved nothing, so `held` is still the pre-bump state. `update` and
    // `close` keep the claim; `reopen` hands it back, so the fourth mutation is
    // proved on a fresh lease.
    run(
        workspace.path(),
        with_credential(lifecycle_mutation(&id, "update"), &current),
    );
    let updated = held_state(workspace.path(), &id);
    assert_eq!(
        updated.1, "worker-two",
        "the lease's own holder keeps the claim across an update"
    );
    assert_eq!(
        updated.2,
        held.2 + 1,
        "the holder's own update must bump the revision exactly once"
    );
    assert_eq!(
        shown_issue(workspace.path(), &id)["claim_epoch"].as_i64(),
        Some(current.parse::<i64>().unwrap()),
        "the holder's own update must leave the leased epoch standing"
    );

    run(
        workspace.path(),
        with_credential(lifecycle_mutation(&id, "close"), &current),
    );
    let closed = shown_issue(workspace.path(), &id);
    assert_eq!(closed["status"].as_str(), Some("closed"));
    // Closing is not a change of holder: the assignee and the epoch that
    // fences it survive, so the same credential still opens the claim a later
    // reopen re-instates. The lease row is not projected here at all --
    // `lease` belongs to the claim result, and the rows are append-only
    // history rather than live state -- so there is nothing about it to read
    // back off the issue.
    assert_eq!(closed["assignee"].as_str(), Some("worker-two"));
    assert_eq!(closed["claim_epoch"].as_i64(), Some(2));

    run(
        workspace.path(),
        with_credential(lifecycle_mutation(&id, "reopen"), &current),
    );
    assert_eq!(
        shown_issue(workspace.path(), &id)["assignee"],
        Value::Null,
        "reopen hands the claim back"
    );

    let last = claim(workspace.path(), "worker-three", true)["claim_epoch"]
        .as_i64()
        .unwrap()
        .to_string();
    run(
        workspace.path(),
        with_credential(lifecycle_mutation(&id, "release"), &last),
    );
    assert_eq!(shown_issue(workspace.path(), &id)["assignee"], Value::Null);
}

/// One of the four claimant-owned lifecycle mutations, as a `bead` argument
/// vector carrying no credential -- the subset of [`claimant_mutations`] this
/// module's lease sweep needs, without the attempt and resource-lock entries
/// whose siblings fence elsewhere.
fn lifecycle_mutation(id: &str, label: &str) -> Vec<String> {
    match label {
        "update" => vec![
            "update".into(),
            id.into(),
            "--notes".into(),
            "probed".into(),
        ],
        "release" => vec!["release".into(), id.into()],
        "close" => vec![
            "close".into(),
            id.into(),
            "--reason".into(),
            "probed".into(),
        ],
        "reopen" => vec!["reopen".into(), id.into()],
        other => panic!("not a claimant-owned lifecycle mutation: {other}"),
    }
}

/// The state of an issue nobody holds: status, assignee, revision.
/// [`held_state`] unwraps the assignee, so it cannot read an unclaimed issue.
fn unheld_state(workspace: &Path, id: &str) -> (String, Option<String>, i64) {
    let issue = shown_issue(workspace, id);
    (
        issue["status"].as_str().unwrap().to_string(),
        issue["assignee"].as_str().map(str::to_string),
        issue["revision"].as_i64().unwrap(),
    )
}

/// Lease rows the store holds for `id`. A row is the second credential
/// dimension the fence can consult, so the credential-free arm is only honest
/// when it is read straight off the store rather than inferred from the issue.
fn lease_row_count(workspace: &Path, id: &str) -> i64 {
    let conn = rusqlite::Connection::open_with_flags(
        workspace.join(".beads").join("beads.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    conn.query_row(
        "SELECT COUNT(*) FROM leases WHERE issue_id = ?1",
        [id],
        |row| row.get(0),
    )
    .unwrap()
}

/// The credential-free arm of the fence. An issue nobody holds accepts every
/// claimant-owned mutation it is open to, with no credential of any kind: no
/// `--fencing-token` on the command line and no lease row behind the issue.
/// The claimed arm is pinned by
/// `a_claimed_issue_rejects_every_credentialless_claimant_mutation` and its
/// siblings; this is the complementary arm, and it is what a later tightening
/// would break -- a fence that started reading the holder's absence as a
/// missing credential would fail here without ever touching a claimed issue.
///
/// `release`, `reopen` and `resolve` are not in the sweep: an open, unassigned
/// issue is not in a state they accept, so their status guards refuse them
/// before the credential gate is reached and there is no credential-free
/// acceptance of them to pin.
///
/// Every acceptance is proved by store state -- the status move, the revision
/// bump, the key set, or the published event -- never by the exit code alone.
#[test]
fn an_unclaimed_issue_accepts_every_claimant_mutation_with_no_credential() {
    let workspace = tempfile::tempdir().unwrap();
    run(workspace.path(), ["init", "--prefix", "epoch"]);
    let id =
        String::from_utf8(run(workspace.path(), ["create", "--title", "never claimed"]).stdout)
            .unwrap()
            .trim()
            .to_string();

    let start = unheld_state(workspace.path(), &id);
    assert_eq!(start.0, "open", "the sweep starts from an open issue");
    assert_eq!(start.1, None, "the sweep starts from an unassigned issue");
    assert_eq!(
        lease_row_count(workspace.path(), &id),
        0,
        "the sweep starts with no lease row to present against"
    );

    // The feed opens with the create's own event; every count below is
    // absolute, so that event is the extra one each length assertion carries.
    let baseline = published_events(workspace.path(), &id);
    assert_eq!(
        baseline.len(),
        1,
        "the sweep starts from a feed holding only the create, got {baseline:?}"
    );
    assert_eq!(baseline[0]["kind"], "created");

    // `update` is the mutation the fence pins hardest on a claimed issue, so
    // it is the one proved hardest here: it lands, moves nothing but the
    // revision, and publishes exactly one event.
    run(
        workspace.path(),
        ["update", &id, "--notes", "no holder yet"],
    );
    let updated = unheld_state(workspace.path(), &id);
    assert_eq!(
        updated.0, "open",
        "a credential-free update must not move the status"
    );
    assert_eq!(
        updated.1, None,
        "a credential-free update must not assign the issue"
    );
    assert_eq!(
        updated.2,
        start.2 + 1,
        "a credential-free update must bump the revision exactly once"
    );
    assert_eq!(lease_row_count(workspace.path(), &id), 0);
    let events = published_events(workspace.path(), &id);
    assert_eq!(
        events.len(),
        2,
        "a credential-free update must publish exactly one event, got {events:?}"
    );
    assert_eq!(events[1]["kind"], "updated");

    // The resource-lock pair. Neither can be proved by a revision bump -- a
    // declaration is scheduling metadata and does not touch the issue row --
    // so each is proved by the key set it leaves behind plus the event it
    // published, and the untouched revision is what keeps the close's later
    // bump attributable to the close alone.
    let added: Vec<String> = serde_json::from_slice(
        &run(workspace.path(), ["resource", "add", &id, "--key", "gpu:0"]).stdout,
    )
    .unwrap();
    assert_eq!(added, vec!["gpu:0".to_string()]);
    let listed: Vec<String> =
        serde_json::from_slice(&run(workspace.path(), ["resource", "list", &id, "--json"]).stdout)
            .unwrap();
    assert_eq!(
        listed,
        vec!["gpu:0".to_string()],
        "the add must have committed"
    );
    assert_eq!(
        unheld_state(workspace.path(), &id).2,
        updated.2,
        "a credential-free resource add must commit without bumping the revision"
    );
    let events = published_events(workspace.path(), &id);
    assert_eq!(
        events.len(),
        3,
        "a credential-free resource add must publish exactly one event, got {events:?}"
    );
    assert_eq!(events[2]["kind"], "resource_keys_added");
    assert_eq!(
        events[2]["detail"]["resource_keys"],
        serde_json::json!(["gpu:0"])
    );

    let removed: Vec<String> = serde_json::from_slice(
        &run(
            workspace.path(),
            ["resource", "remove", &id, "--key", "gpu:0"],
        )
        .stdout,
    )
    .unwrap();
    assert_eq!(removed, Vec::<String>::new());
    let listed: Vec<String> =
        serde_json::from_slice(&run(workspace.path(), ["resource", "list", &id, "--json"]).stdout)
            .unwrap();
    assert!(listed.is_empty(), "the remove must have committed");
    assert_eq!(
        unheld_state(workspace.path(), &id).2,
        updated.2,
        "a credential-free resource remove must commit without bumping the revision"
    );
    let events = published_events(workspace.path(), &id);
    assert_eq!(
        events.len(),
        4,
        "a credential-free resource remove must publish exactly one event, got {events:?}"
    );
    assert_eq!(events[3]["kind"], "resource_keys_removed");
    assert_eq!(events[3]["detail"]["resource_keys"], serde_json::json!([]));

    // `close` ends the sweep the way it begins: no credential, and the issue
    // stays unassigned through the transition. The closed event carries nulls
    // for both credential fields rather than omitting them, so a reader of the
    // change feed can tell an unclaimed close from a fenced one.
    run(
        workspace.path(),
        ["close", &id, "--reason", "no holder needed"],
    );
    let closed = unheld_state(workspace.path(), &id);
    assert_eq!(
        closed.0, "closed",
        "a credential-free close must close the issue"
    );
    assert_eq!(
        closed.1, None,
        "a credential-free close must leave it unassigned"
    );
    assert_eq!(
        closed.2,
        updated.2 + 1,
        "a credential-free close must bump the revision exactly once"
    );
    assert_eq!(lease_row_count(workspace.path(), &id), 0);
    let events = published_events(workspace.path(), &id);
    assert_eq!(
        events.len(),
        5,
        "a credential-free close must publish exactly one event, got {events:?}"
    );
    assert_eq!(events[4]["kind"], "closed");
    assert_eq!(events[4]["detail"]["reason"], "no holder needed");
    assert_eq!(events[4]["detail"]["claim_epoch"], Value::Null);
    assert_eq!(events[4]["detail"]["presented_fencing_token"], Value::Null);

    // And the arm holds the moment a claim is handed back, too -- on a fresh
    // issue, since the one above is closed now. This leg takes its claim
    // *leased*, so the release leaves a lease row behind: the fence follows
    // the holder, not the history, and the leftover row must not be enough to
    // start demanding a credential again.
    let returned =
        String::from_utf8(run(workspace.path(), ["create", "--title", "handed back"]).stdout)
            .unwrap()
            .trim()
            .to_string();
    let claimed = claim(workspace.path(), "worker-one", true);
    assert_eq!(
        claimed["bead_id"].as_str(),
        Some(returned.as_str()),
        "the fresh issue is the only claimable work"
    );
    let epoch = claimed["claim_epoch"]
        .as_i64()
        .expect("a leased claim mints an epoch");
    assert!(
        lease_row_count(workspace.path(), &returned) > 0,
        "the leased claim must leave the row this leg is about"
    );
    run(
        workspace.path(),
        ["release", &returned, "--fencing-token", &epoch.to_string()],
    );
    let released = unheld_state(workspace.path(), &returned);
    assert_eq!(released.0, "open");
    assert_eq!(released.1, None);
    assert!(
        lease_row_count(workspace.path(), &returned) > 0,
        "the lease row is history, not a fence"
    );

    run(
        workspace.path(),
        ["update", &returned, "--notes", "released again"],
    );
    let after = unheld_state(workspace.path(), &returned);
    assert_eq!(
        after.1, None,
        "a credential-free update after release must not assign"
    );
    assert_eq!(
        after.2,
        released.2 + 1,
        "a credential-free update must still land once the claim is handed back"
    );
}

/// The fence is the last line of defence behind atomic claim selection, so it
/// must not cost the selection its guarantee: twenty contenders racing one
/// ready issue still produce exactly one claimant.
#[test]
fn twenty_simultaneous_claims_still_yield_one_claimant() {
    let workspace = tempfile::tempdir().unwrap();
    run(workspace.path(), ["init", "--prefix", "epoch"]);
    let path = workspace.path().to_path_buf();
    let id =
        String::from_utf8(run(&path, ["create", "--title", "one issue, many contenders"]).stdout)
            .unwrap()
            .trim()
            .to_string();

    const CONTENDERS: usize = 20;
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(CONTENDERS));
    let mut handles = Vec::new();
    for worker in 0..CONTENDERS {
        let barrier = std::sync::Arc::clone(&barrier);
        let path = path.clone();
        handles.push(std::thread::spawn(move || {
            let assignee = format!("worker-{worker}");
            barrier.wait();
            // Losing the writer lock to a rival is a scheduling outcome, not
            // a verdict on the claim; only an answered claim attempt counts.
            for _ in 0..200 {
                let output = Command::cargo_bin("bead")
                    .unwrap()
                    .current_dir(&path)
                    .arg("--skip-foreign-workspace")
                    .args(["claim", "--assignee", &assignee, "--json"])
                    .output()
                    .unwrap();
                if output.status.code() == Some(6) {
                    continue;
                }
                assert!(
                    output.status.success(),
                    "claim by {assignee} failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
                let parsed: Value = serde_json::from_slice(&output.stdout).unwrap();
                return parsed["bead_id"].as_str().map(str::to_string);
            }
            panic!("claim by {assignee} never got an answer under contention");
        }));
    }

    let mut winners = Vec::new();
    for handle in handles {
        if let Some(bead_id) = handle.join().unwrap() {
            winners.push(bead_id);
        }
    }

    assert_eq!(
        winners.len(),
        1,
        "exactly one contender may claim the issue, got {winners:?}"
    );
    assert_eq!(winners[0], id);

    // The single claimant holds a credential the fence will honour.
    let issue = shown_issue(&path, &id);
    assert_eq!(issue["status"].as_str(), Some("in_progress"));
    assert!(issue["claim_epoch"].as_i64().unwrap() > 0);
    run(
        &path,
        [
            "release",
            &id,
            "--fencing-token",
            &issue["claim_epoch"].as_i64().unwrap().to_string(),
        ],
    );
}

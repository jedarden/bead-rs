//! Post-commit checkpoint publication chokepoint tests (plan 6.2.1 items
//! 1-3, 5, 7, and 8, ADR-003, R026 step A1).
//!
//! Every mutating command publishes a checkpoint generation covering its
//! own committed sequence from one shared chokepoint in `execute_command`:
//!
//! - publication runs only after the command's transaction committed, so a
//!   publication failure cannot roll back committed work; a mutation that
//!   commits and then fails to publish reports the split explicitly (item
//!   5): success output preserved on stdout, failure and remedy on stderr,
//!   exit 1;
//! - which commands publish is decided by observing the live event sequence
//!   advance across the invocation -- the signal plan 6.2.1 P3 made sound
//!   and `tests/mutating_command_event_contract.rs` enforces -- so read-only
//!   commands never publish and a newly added mutating command inherits
//!   publication with no wiring at its call site;
//! - publication is skipped when the checkpoint already covers the live
//!   event sequence (item 3), so a mutation that changes nothing mints no
//!   generation and no object, and a pointer another publisher already
//!   carried to this sequence -- the residue of a lost publication race --
//!   is treated as success rather than published over;
//! - publication is silent on success: no command's output changes;
//! - `--no-auto-flush` and `checkpoint.auto_flush` are the two escape
//!   hatches (item 7): each suppresses publication, the flag wins over the
//!   configuration key, a suppressed workspace is left dirty exactly as
//!   explicit flush leaves it, and `sync --status` reports that state;
//! - `sync flush-only` stays an explicit, idempotent operation (item 8):
//!   against a clean checkpoint it publishes nothing and exits 0.
//!
//! The automatic default is active (the R026 activation flipped the
//! compiled default, plan section 13): a workspace with no
//! `checkpoint.auto_flush` key publishes on every mutation, and
//! `checkpoint.auto_flush = false` is the durable opt-out. The
//! compiled-default test pins that resolution.

use assert_cmd::Command;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

/// Record `checkpoint.auto_flush = VALUE` in the workspace's
/// `.beads/config.json`, preserving every other key.
fn set_auto_flush(workspace: &Path, value: bool) {
    let path = workspace.join(".beads/config.json");
    let mut config: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    config
        .as_object_mut()
        .unwrap()
        .entry("checkpoint")
        .or_insert(Value::Object(Default::default()))
        .as_object_mut()
        .unwrap()
        .insert("auto_flush".into(), Value::Bool(value));
    fs::write(&path, serde_json::to_string_pretty(&config).unwrap()).unwrap();
}

fn bead(workspace: &Path) -> Command {
    let mut cmd = Command::cargo_bin("bead").unwrap();
    cmd.current_dir(workspace);
    cmd
}

fn run(workspace: &Path, args: &[&str]) -> std::process::Output {
    bead(workspace)
        .args(args)
        .assert()
        .success()
        .get_output()
        .clone()
}

/// `sync status --format json` parsed from stdout.
fn status(workspace: &Path) -> Value {
    let output = bead(workspace)
        .args(["sync", "status", "--format", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice(&output).unwrap()
}

/// Assert the durable checkpoint covers the live event sequence: the
/// property automatic flush exists to guarantee.
fn assert_covers_live(workspace: &Path, context: &str) {
    let status = status(workspace);
    assert_eq!(
        status["checkpoint_present"],
        Value::Bool(true),
        "{context}: no checkpoint was published"
    );
    assert_eq!(
        status["covered_sequence"], status["live_sequence"],
        "{context}: checkpoint covers {} but the live sequence is {} -- the \
         durable checkpoint is silently behind the database",
        status["covered_sequence"], status["live_sequence"]
    );
}

/// The pointer's generation identity: changes exactly when a publication
/// runs, so it detects a publication even when content-addressed object
/// reuse keeps the object set identical.
fn generation_id(workspace: &Path) -> String {
    status(workspace)["generation_id"]
        .as_str()
        .expect("generation_id present once a checkpoint exists")
        .to_string()
}

fn create_issue(workspace: &Path, title: &str) -> String {
    let output = run(workspace, &["create", "--title", title]);
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

/// Every file under `.beads/checkpoint/` with its bytes, keyed by
/// checkpoint-relative path. Publication rewrites `current.json`,
/// `previous.json`, and the `forensic.jsonl` view and mints new objects,
/// so any publication at all changes this map -- and only a publication
/// does: skipping one leaves it byte-identical.
fn snapshot_checkpoint(workspace: &Path) -> std::collections::BTreeMap<String, Vec<u8>> {
    fn walk(dir: &Path, prefix: String, out: &mut std::collections::BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_str().unwrap().to_string();
            if path.is_dir() {
                walk(&path, format!("{prefix}{name}/"), out);
            } else {
                out.insert(format!("{prefix}{name}"), fs::read(&path).unwrap());
            }
        }
    }
    let mut out = std::collections::BTreeMap::new();
    walk(
        &workspace.join(".beads/checkpoint"),
        String::new(),
        &mut out,
    );
    out
}

/// Rewrite the pointer's `snapshot_sequence`, preserving every other
/// field: the same authoritative value `read_covered_event_sequence` and
/// `sync --status` read, so a forged value is indistinguishable from one a
/// real publisher recorded.
fn forge_covered_sequence(workspace: &Path, covered: i64) {
    let pointer_path = workspace.join(".beads/checkpoint/current.json");
    let mut pointer: Value =
        serde_json::from_str(&fs::read_to_string(&pointer_path).unwrap()).unwrap();
    pointer
        .as_object_mut()
        .unwrap()
        .insert("snapshot_sequence".into(), Value::from(covered));
    fs::write(
        &pointer_path,
        serde_json::to_string_pretty(&pointer).unwrap(),
    )
    .unwrap();
}

/// Fixture workspace state the sweep's invocations refer to, modeled on
/// `tests/mutating_command_event_contract.rs`.
struct Fixture {
    /// Keeps the workspace directories alive for the whole test.
    _dirs: Vec<tempfile::TempDir>,
    workspace: PathBuf,
    /// Open, unassigned: target for `update --notes`.
    update_target: String,
    /// Carries `chokepoint-label` from setup: target for `label remove`.
    labeled: String,
    /// Any open issue: target for `label add`.
    label_target: String,
    /// Pair used for `dep add` / `dep remove`.
    blocked: String,
    blocker: String,
    /// Carries the `github/probe` reference from setup: target for
    /// `ref remove`.
    with_ref: String,
    /// Any issue without a reference: target for `ref add`.
    ref_target: String,
    /// Carries the `cfg` structured data from setup: target for
    /// `data remove`.
    with_data: String,
    /// Any issue without structured data: target for `data set`.
    data_target: String,
    /// Held `in_progress` from setup: target for `release`.
    in_progress: String,
    /// Open: target for `close`.
    to_close: String,
    /// Closed in setup: target for `reopen`.
    closed: String,
    /// Checkpoint of a second workspace, merged in by `sync import-only`.
    foreign_checkpoint: PathBuf,
}

fn build_fixture() -> Fixture {
    let dir = tempfile::tempdir().expect("fixture tempdir");
    let workspace = dir.path().to_path_buf();

    run(&workspace, &["init", "--prefix", "choke"]);

    // Automatic publication is armed from the first mutation on, so every
    // setup mutation below also exercises the chokepoint.
    set_auto_flush(&workspace, true);

    let update_target = create_issue(&workspace, "update target");
    let labeled = create_issue(&workspace, "labeled target");
    let label_target = create_issue(&workspace, "label target");
    let blocked = create_issue(&workspace, "blocked target");
    let blocker = create_issue(&workspace, "blocker target");
    let with_ref = create_issue(&workspace, "ref target");
    let ref_target = create_issue(&workspace, "ref add target");
    let with_data = create_issue(&workspace, "data target");
    let data_target = create_issue(&workspace, "data set target");
    let in_progress = create_issue(&workspace, "release target");
    let to_close = create_issue(&workspace, "close target");
    let closed = create_issue(&workspace, "reopen target");

    run(
        &workspace,
        &["update", &in_progress, "--status", "in_progress"],
    );
    run(&workspace, &["close", &closed, "--reason", "fixture setup"]);
    run(
        &workspace,
        &["label", "add", &labeled, "--label", "chokepoint-label"],
    );
    run(
        &workspace,
        &[
            "ref",
            "add",
            "--id",
            &with_ref,
            "--namespace",
            "github",
            "--key",
            "probe",
            "--value",
            "chokepoint-1",
        ],
    );
    run(
        &workspace,
        &[
            "data",
            "set",
            "--id",
            &with_data,
            "--namespace",
            "cfg",
            "--schema-ref",
            "probe:v1",
            "--value",
            "{\"setup\": true}",
        ],
    );
    run(
        &workspace,
        &[
            "recurrence",
            "create",
            "--id",
            "choke-template",
            "--title",
            "Chokepoint Probe",
            "--base-title-template",
            "Chokepoint Probe {n}",
        ],
    );

    // A second workspace supplies the checkpoint that `sync import-only`
    // merges in: one foreign issue, explicitly flushed.
    let foreign = tempfile::tempdir().expect("foreign tempdir");
    let foreign_ws = foreign.path().to_path_buf();
    run(&foreign_ws, &["init", "--prefix", "choke"]);
    create_issue(&foreign_ws, "foreign issue");
    run(&foreign_ws, &["sync", "flush-only"]);
    let foreign_checkpoint = foreign_ws.join(".beads/checkpoint/forensic.jsonl");

    Fixture {
        _dirs: vec![dir, foreign],
        workspace,
        update_target,
        labeled,
        label_target,
        blocked,
        blocker,
        with_ref,
        ref_target,
        with_data,
        data_target,
        in_progress,
        to_close,
        closed,
        foreign_checkpoint,
    }
}

/// Every mutating command in the section 5 contract table publishes a
/// generation covering its own committed sequence from the one chokepoint.
/// A newly added mutating command lands here by appending one entry; it
/// needs no publication wiring of its own anywhere else.
#[test]
fn every_mutating_command_publishes_a_covering_generation() {
    let fixture = build_fixture();

    let sweep: Vec<(&str, Vec<String>)> = vec![
        (
            "bead create",
            vec![
                "create".into(),
                "--title".into(),
                "chokepoint create".into(),
            ],
        ),
        (
            "bead update",
            vec![
                "update".into(),
                fixture.update_target.clone(),
                "--notes".into(),
                "chokepoint note".into(),
            ],
        ),
        (
            "bead claim",
            vec![
                "claim".into(),
                "--assignee".into(),
                "chokepoint-worker".into(),
            ],
        ),
        (
            "bead release",
            vec!["release".into(), fixture.in_progress.clone()],
        ),
        (
            "bead close",
            vec![
                "close".into(),
                fixture.to_close.clone(),
                "--reason".into(),
                "chokepoint complete".into(),
            ],
        ),
        ("bead reopen", vec!["reopen".into(), fixture.closed.clone()]),
        (
            "bead label add",
            vec![
                "label".into(),
                "add".into(),
                fixture.label_target.clone(),
                "--label".into(),
                "chokepoint-label".into(),
            ],
        ),
        (
            "bead label remove",
            vec![
                "label".into(),
                "remove".into(),
                fixture.labeled.clone(),
                "--label".into(),
                "chokepoint-label".into(),
            ],
        ),
        (
            "bead dep add",
            vec![
                "dep".into(),
                "add".into(),
                fixture.blocked.clone(),
                fixture.blocker.clone(),
            ],
        ),
        (
            "bead dep remove",
            vec![
                "dep".into(),
                "remove".into(),
                fixture.blocked.clone(),
                fixture.blocker.clone(),
            ],
        ),
        (
            "bead ref add",
            vec![
                "ref".into(),
                "add".into(),
                "--id".into(),
                fixture.ref_target.clone(),
                "--namespace".into(),
                "github".into(),
                "--key".into(),
                "probe".into(),
                "--value".into(),
                "chokepoint-2".into(),
            ],
        ),
        (
            "bead ref remove",
            vec![
                "ref".into(),
                "remove".into(),
                "--id".into(),
                fixture.with_ref.clone(),
                "--namespace".into(),
                "github".into(),
                "--key".into(),
                "probe".into(),
            ],
        ),
        (
            "bead data set",
            vec![
                "data".into(),
                "set".into(),
                "--id".into(),
                fixture.data_target.clone(),
                "--namespace".into(),
                "cfg".into(),
                "--schema-ref".into(),
                "probe:v1".into(),
                "--value".into(),
                "{\"probe\": true}".into(),
            ],
        ),
        (
            "bead data remove",
            vec![
                "data".into(),
                "remove".into(),
                "--id".into(),
                fixture.with_data.clone(),
                "--namespace".into(),
                "cfg".into(),
            ],
        ),
        (
            "bead recurrence materialize",
            vec![
                "recurrence".into(),
                "materialize".into(),
                "--id".into(),
                "choke-template".into(),
            ],
        ),
        (
            "bead sync import-only",
            vec![
                "sync".into(),
                "import-only".into(),
                "--input".into(),
                fixture.foreign_checkpoint.display().to_string(),
                "--merge".into(),
                "--actor".into(),
                "chokepoint".into(),
            ],
        ),
    ];

    for (name, args) in sweep {
        let argv: Vec<&str> = args.iter().map(String::as_str).collect();
        run(&fixture.workspace, &argv);
        assert_covers_live(
            &fixture.workspace,
            &format!("after mutating command {name}"),
        );
    }
}

/// Read-only commands never publish -- not against a clean checkpoint
/// (where a publication would still mint a new generation), and not against
/// a dirty one (where publication would have work to do).
#[test]
fn read_only_commands_do_not_publish() {
    let fixture = build_fixture();
    let workspace = &fixture.workspace;

    // Dirty the checkpoint by removing the pointer, then confirm a batch of
    // read-only commands leaves it unpublished.
    fs::remove_file(workspace.join(".beads/checkpoint/current.json")).unwrap();
    assert_eq!(
        status(workspace)["checkpoint_present"],
        Value::Bool(false),
        "setup: pointer removal must leave no checkpoint"
    );

    let read_only: Vec<Vec<&str>> = vec![
        vec!["list", "--json", "--limit", "10"],
        vec!["show", &fixture.update_target, "--json"],
        vec!["why", "--id", &fixture.update_target],
        vec![
            "compare",
            "--id",
            &fixture.update_target,
            "--source",
            "native-v1",
            "--target",
            "needle-v1",
        ],
        vec!["changes", "--latest"],
        vec!["ref", "list", "--id", &fixture.ref_target],
        vec![
            "ref",
            "find",
            "--namespace",
            "github",
            "--value",
            "chokepoint-1",
        ],
        vec!["data", "list", "--id", &fixture.data_target],
        vec!["recurrence", "list"],
        vec!["recurrence", "show", "--id", "choke-template"],
        vec!["recurrence", "history", "--id", "choke-template"],
        vec!["doctor"],
        vec!["sync", "status"],
        vec!["policy", "check"],
        vec!["capabilities"],
        vec!["schema", "list"],
    ];

    for args in &read_only {
        run(workspace, args);
        assert_eq!(
            status(workspace)["checkpoint_present"],
            Value::Bool(false),
            "read-only command {args:?} published a checkpoint; only a \
             committed semantic mutation may publish"
        );
    }

    // Against a clean checkpoint a publication would be detectable as a new
    // generation identity; the read-only command must leave it untouched.
    run(workspace, &["sync", "flush-only"]);
    let before = generation_id(workspace);
    run(workspace, &["list", "--json"]);
    assert_eq!(
        generation_id(workspace),
        before,
        "read-only command against a clean checkpoint published a new generation"
    );
    assert_covers_live(
        workspace,
        "after read-only commands against clean checkpoint",
    );
}

/// A no-op mutation commits no event, advances nothing, and publishes
/// nothing: adding a label an issue already carries leaves the pointer
/// exactly as it was.
#[test]
fn idempotent_no_op_mutation_publishes_nothing() {
    let fixture = build_fixture();
    let workspace = &fixture.workspace;

    // `labeled` already carries `chokepoint-label` from fixture setup.
    run(
        workspace,
        &[
            "label",
            "add",
            &fixture.labeled,
            "--label",
            "chokepoint-label",
        ],
    );
    let before = generation_id(workspace);
    run(
        workspace,
        &[
            "label",
            "add",
            &fixture.labeled,
            "--label",
            "chokepoint-label",
        ],
    );
    assert_eq!(
        generation_id(workspace),
        before,
        "an idempotent no-op mutation published a generation"
    );
}

/// Since the R026 activation flipped the compiled default, a workspace
/// without `checkpoint.auto_flush` publishes automatically: every mutation
/// covers its own committed sequence with no opt-in key, and an explicit
/// false is the durable opt-out the plan's escape hatch describes (a
/// checkpoint section without the key resolves to the compiled default).
#[test]
fn compiled_default_publishes_automatically() {
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path();
    run(workspace, &["init", "--prefix", "default"]);
    create_issue(workspace, "default-on issue");

    assert_covers_live(
        workspace,
        "a mutation without checkpoint.auto_flush set must publish; the \
         compiled default is on since the R026 activation (plan 6.2.1, \
         section 13)",
    );

    set_auto_flush(workspace, false);
    create_issue(workspace, "explicitly-suppressed issue");
    let covered = status(workspace)["covered_sequence"].clone();
    create_issue(workspace, "still suppressed");
    assert_eq!(
        status(workspace)["covered_sequence"],
        covered,
        "checkpoint.auto_flush = false failed to suppress publication"
    );
}

/// The full split-failure contract of plan 6.2.1 item 5: a mutation that
/// commits and then fails to publish must keep the mutation committed and
/// visible, preserve its own success output on stdout, report the
/// publication failure on stderr in words that distinguish "the mutation
/// happened and the checkpoint did not advance" from "the mutation did not
/// happen", and exit exactly 1 -- which never implies rollback. The remedy
/// the message names must also actually close the gap.
#[test]
fn publication_failure_reports_the_split_without_rolling_back() {
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path();
    run(workspace, &["init", "--prefix", "split"]);
    set_auto_flush(workspace, true);
    create_issue(workspace, "before failure");
    let checkpoint_dir = workspace.join(".beads/checkpoint");

    // Make publication fail by replacing the checkpoint directory it must
    // write into with a regular file, so every attempt to create a file
    // inside it fails with ENOTDIR.
    //
    // Removing write permission (0o555) does NOT work: CI runs as root in a
    // container, root holds CAP_DAC_OVERRIDE, and the write simply succeeds,
    // so `create` exits 0 and this test fails with "Unexpected success".
    // ENOTDIR is a path-type error rather than a permission check, so no
    // privilege level can bypass it and the injection behaves identically
    // for root and non-root.
    //
    // The real directory is moved aside rather than deleted: the assertions
    // below require the checkpoint written before the failure, so that the
    // split is observable as covered_sequence < live_sequence.
    let parked = workspace.join(".beads/checkpoint.parked");
    fs::rename(&checkpoint_dir, &parked).unwrap();
    fs::write(&checkpoint_dir, b"not a directory").unwrap();

    let output = bead(workspace)
        .args(["create", "--title", "survives publication failure"])
        .assert()
        .failure()
        .get_output()
        .clone();

    // Restore the real checkpoint so the assertions below see the state
    // written before the failure, and the tempdir can clean itself up.
    fs::remove_file(&checkpoint_dir).unwrap();
    fs::rename(&parked, &checkpoint_dir).unwrap();

    // The mutation happened: its success output is preserved on stdout --
    // exactly the new issue ID, nothing about the publication failure.
    let stdout = String::from_utf8(output.stdout).unwrap();
    let issue_id = stdout.trim();
    assert!(
        !issue_id.is_empty() && stdout.ends_with('\n') && stdout.lines().count() == 1,
        "the mutation's own success output must be preserved on stdout, got {stdout:?}"
    );

    // Exit exactly 1: the split outcome is pinned to that code (plan 6.2.1
    // item 5), and a machine consumer seeing 1 plus the preserved success
    // output knows the mutation was not rolled back.
    assert_eq!(
        output.status.code(),
        Some(1),
        "a committed mutation whose publication failed must exit 1, got {:?}",
        output.status.code()
    );

    // The failure is reported on stderr and names the split: the mutation
    // is still committed, the checkpoint did not advance, and the remedy is
    // named (ADR-007).
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(
        stderr.contains("bead:"),
        "the publication failure must be reported on stderr, got {stderr:?}"
    );
    assert!(
        stderr.contains("checkpoint publication failed after the mutation committed"),
        "stderr must state that the mutation committed and publication failed, got {stderr:?}"
    );
    assert!(
        stderr.contains("sync flush-only"),
        "stderr must name the remedy, got {stderr:?}"
    );

    // The mutation is still present and visible: exit 1 never rolled it back.
    run(workspace, &["show", issue_id]);
    let listing = run(workspace, &["list", "--json"]);
    assert!(
        String::from_utf8_lossy(&listing.stdout).contains(issue_id),
        "the committed mutation must still be visible to list"
    );

    // The split is real, not just reported: the durable checkpoint is
    // behind the live store, exactly the state a machine consumer must be
    // able to distinguish from "the mutation did not happen".
    let report = status(workspace);
    assert!(
        report["covered_sequence"].as_i64().unwrap() < report["live_sequence"].as_i64().unwrap(),
        "after a split failure the checkpoint must be dirty (covered {} < live {})",
        report["covered_sequence"],
        report["live_sequence"]
    );

    // The remedy the message names closes the gap.
    run(workspace, &["sync", "flush-only"]);
    assert_covers_live(workspace, "after the remedy the split message names");
}

/// An invalid automatic-publication setting must not turn the R026 default
/// into a silent opt-out. Read-only commands remain usable because they have
/// nothing to publish, but a semantic mutation reports the post-commit split,
/// preserves its normal success output, and leaves an explicitly visible
/// dirty checkpoint until the operator fixes the configuration and flushes.
#[test]
fn invalid_auto_flush_config_fails_closed_after_a_committed_mutation() {
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path();
    run(workspace, &["init", "--prefix", "badcfg"]);
    create_issue(workspace, "published before invalid config");
    assert_covers_live(workspace, "setup: compiled default publishes");

    let config_path = workspace.join(".beads/config.json");
    let mut config: Value =
        serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
    config
        .as_object_mut()
        .unwrap()
        .entry("checkpoint")
        .or_insert(Value::Object(Default::default()))
        .as_object_mut()
        .unwrap()
        .insert("auto_flush".into(), Value::String("invalid".into()));
    fs::write(&config_path, serde_json::to_string_pretty(&config).unwrap()).unwrap();

    // Invalid checkpoint configuration must not break read-only inspection.
    bead(workspace).args(["list", "--json"]).assert().success();

    let before = status(workspace);
    let output = bead(workspace)
        .args(["create", "--title", "committed with invalid config"])
        .assert()
        .failure()
        .get_output()
        .clone();

    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8(output.stdout).unwrap();
    let issue_id = stdout.trim();
    assert!(
        issue_id.starts_with("badcfg-") && stdout.lines().count() == 1,
        "the committed mutation's normal output must survive, got {stdout:?}"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("checkpoint publication failed after the mutation committed"),
        "the invalid setting must report the split outcome, got {stderr:?}"
    );
    assert!(
        stderr.contains("checkpoint.auto_flush must be a boolean"),
        "the error must identify the invalid setting, got {stderr:?}"
    );

    // The mutation committed, while the checkpoint did not move silently.
    bead(workspace)
        .args(["show", issue_id, "--json"])
        .assert()
        .success();
    let dirty = status(workspace);
    assert_eq!(dirty["dirty"], Value::Bool(true));
    assert_eq!(
        dirty["live_sequence"].as_i64().unwrap(),
        before["live_sequence"].as_i64().unwrap() + 1
    );
    assert_eq!(dirty["covered_sequence"], before["covered_sequence"]);

    set_auto_flush(workspace, true);
    run(workspace, &["sync", "flush-only"]);
    assert_covers_live(workspace, "after repairing configuration and flushing");
}

/// Automatic publication is silent on success: `bead create` still prints
/// only the new ID plus LF (plan 6.2.1 item 6).
#[test]
fn publication_is_silent_on_success() {
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path();
    run(workspace, &["init", "--prefix", "quiet"]);
    set_auto_flush(workspace, true);

    let output = bead(workspace)
        .args(["create", "--title", "quiet creation"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let stdout = String::from_utf8(output).unwrap();
    assert_eq!(
        stdout.lines().count(),
        1,
        "bead create with automatic publication must print exactly one line, got {stdout:?}"
    );
    assert!(
        stdout.starts_with("quiet-"),
        "the one line must be the new issue ID, got {stdout:?}"
    );
    assert_covers_live(workspace, "after silent publication");
}

/// `--no-auto-flush` suppresses publication for exactly one invocation
/// (plan 6.2.1 item 7): the mutation still commits and stays visible, the
/// checkpoint is left dirty exactly as an unflushed mutation under the
/// explicit-flush default is, `sync --status` reports that dirtiness, the
/// suppression does not outlive the invocation, and the suppressed state is
/// published by the explicit `sync flush-only` that has always covered it.
/// The flag never disturbs the command's own output, so `bead create` still
/// prints only the new ID plus LF (item 6).
#[test]
fn no_auto_flush_suppresses_publication_for_one_invocation() {
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path();
    run(workspace, &["init", "--prefix", "esc"]);
    set_auto_flush(workspace, true);
    create_issue(workspace, "published before suppression");
    assert_covers_live(workspace, "setup: automatic publication is on");

    let before = snapshot_checkpoint(workspace);
    let before_generation = generation_id(workspace);
    let before_live = status(workspace)["live_sequence"].as_i64().unwrap();

    let output = bead(workspace)
        .args(["create", "--no-auto-flush", "--title", "suppressed once"])
        .assert()
        .success()
        .get_output()
        .clone();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(
        stdout.lines().count(),
        1,
        "a suppressed invocation must keep create's one-line ID output, got {stdout:?}"
    );
    let suppressed_id = stdout.trim();

    // The mutation committed and is visible even though nothing published.
    let listing = run(workspace, &["list", "--json"]);
    assert!(
        String::from_utf8_lossy(&listing.stdout).contains(suppressed_id),
        "the suppressed mutation must still be committed and visible"
    );

    // The checkpoint is dirty exactly as an unflushed explicit-flush
    // mutation leaves it: no generation, no object, no byte moved, and the
    // live sequence one ahead of the covered one.
    let report = status(workspace);
    assert_eq!(report["dirty"], Value::Bool(true));
    assert_eq!(
        report["live_sequence"].as_i64().unwrap(),
        before_live + 1,
        "the suppressed mutation must still advance the live sequence"
    );
    assert_eq!(
        report["covered_sequence"].as_i64().unwrap(),
        before_live,
        "a suppressed invocation must not advance the covered sequence"
    );
    assert_eq!(
        generation_id(workspace),
        before_generation,
        "a suppressed invocation minted a new generation"
    );
    assert_eq!(
        snapshot_checkpoint(workspace),
        before,
        "a suppressed invocation changed the checkpoint set; suppression \
         must leave it byte-identical, exactly as explicit flush does today"
    );

    // Suppression lasts one invocation: the next mutation without the flag
    // publishes normally again.
    create_issue(workspace, "published after suppression");
    assert_covers_live(workspace, "the invocation after a suppressed one");

    // Suppress once more, then prove the explicit flush that has always
    // covered unflushed work publishes the suppressed state.
    run(
        workspace,
        &[
            "create",
            "--no-auto-flush",
            "--title",
            "suppressed for flush",
        ],
    );
    let dirty = status(workspace);
    assert_eq!(dirty["dirty"], Value::Bool(true));
    run(workspace, &["sync", "flush-only"]);
    let clean = status(workspace);
    assert_eq!(clean["dirty"], Value::Bool(false));
    assert_eq!(
        clean["ready_to_commit"],
        Value::Bool(true),
        "after flushing the suppressed state the checkpoint must be ready \
         to commit, exactly as an explicitly flushed workspace is"
    );
    assert_covers_live(workspace, "after flushing the suppressed state");
}

/// The flag wins over `checkpoint.auto_flush` in both directions (plan
/// 6.2.1 item 7, plan section 5 command table): it suppresses a workspace
/// that opted in, and against an already-suppressed workspace it changes
/// nothing. Being a global flag, it suppresses from both argument
/// positions.
#[test]
fn no_auto_flush_flag_wins_over_the_configuration_key() {
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path();
    run(workspace, &["init", "--prefix", "over"]);
    set_auto_flush(workspace, true);
    create_issue(workspace, "opted in");
    assert_covers_live(workspace, "setup: the workspace opted into publication");

    // Config on, flag present: the flag suppresses -- from before the
    // subcommand and from after it, because the flag is global.
    for args in [
        vec!["--no-auto-flush", "create", "--title", "suppressed before"],
        vec!["create", "--no-auto-flush", "--title", "suppressed after"],
    ] {
        let covered = status(workspace)["covered_sequence"].clone();
        run(workspace, &args);
        assert_eq!(
            status(workspace)["covered_sequence"],
            covered,
            "the flag failed to suppress the opted-in workspace for {args:?}"
        );
    }
    assert!(
        status(workspace)["covered_sequence"].as_i64().unwrap()
            < status(workspace)["live_sequence"].as_i64().unwrap(),
        "a flag-suppressed opted-in workspace must be reported dirty"
    );

    // Config off, flag present: publication stays suppressed and the
    // mutation still succeeds -- the flag agrees with the key rather than
    // fighting it.
    set_auto_flush(workspace, false);
    let covered = status(workspace)["covered_sequence"].clone();
    run(
        workspace,
        &["create", "--no-auto-flush", "--title", "both suppress"],
    );
    assert_eq!(
        status(workspace)["covered_sequence"],
        covered,
        "the flag must not let a suppressed workspace publish"
    );
}

/// A mutation whose sequence the checkpoint already covers publishes
/// nothing (plan 6.2.1 item 3). The pointer is forged to the exact
/// sequence the mutation is about to reach -- the residue of a lost
/// publication race, where a concurrent publisher already carried the
/// checkpoint to this sequence: the plan requires that be treated as
/// success, not something to publish over. Advancing the sequence alone
/// must not force a generation when there is nothing new to carry.
#[test]
fn mutation_when_the_checkpoint_covers_the_live_sequence_publishes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path();
    run(workspace, &["init", "--prefix", "raced"]);
    set_auto_flush(workspace, true);
    let issue = create_issue(workspace, "raced issue");

    let live = status(workspace)["live_sequence"].as_i64().unwrap();
    assert_covers_live(workspace, "setup");

    // The sequence `update --notes` will reach: one `updated` event.
    forge_covered_sequence(workspace, live + 1);
    let before = snapshot_checkpoint(workspace);
    let before_generation = generation_id(workspace);

    run(
        workspace,
        &["update", &issue, "--notes", "committed silently"],
    );

    let after = status(workspace);
    assert_eq!(
        after["live_sequence"].as_i64().unwrap(),
        live + 1,
        "the mutation must still commit -- skipping publication is never \
         skipping the mutation itself"
    );
    assert_eq!(
        after["covered_sequence"].as_i64().unwrap(),
        live + 1,
        "covered sequence changed without a publication"
    );
    assert_eq!(
        generation_id(workspace),
        before_generation,
        "a mutation the checkpoint already covered minted a new generation"
    );
    assert_eq!(
        snapshot_checkpoint(workspace),
        before,
        "a mutation the checkpoint already covered changed the checkpoint \
         set -- no generation and no object may be created"
    );
}

/// A real mutation against a covering-but-stale checkpoint publishes
/// exactly one generation, and that generation covers the mutation's own
/// committed sequence (plan 6.2.1 items 3 and 1).
#[test]
fn real_mutation_publishes_exactly_one_generation() {
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path();
    run(workspace, &["init", "--prefix", "exact"]);
    set_auto_flush(workspace, true);
    let issue = create_issue(workspace, "exactly one generation");
    // A second setup mutation puts the checkpoint in its steady state
    // (current + previous + one object per retained generation) before the
    // snapshot, so the mutation under test is the only thing that can move
    // the file set.
    create_issue(workspace, "steady state setup");

    let before = snapshot_checkpoint(workspace);
    let before_generation = generation_id(workspace);

    run(
        workspace,
        &["update", &issue, "--notes", "one generation only"],
    );

    let after_status = status(workspace);
    assert_covers_live(workspace, "after one real mutation");
    let after_generation = generation_id(workspace);
    assert_ne!(
        after_generation, before_generation,
        "a real mutation published no generation"
    );

    // Exactly one generation: the new pointer selects a new root, and the
    // displaced pointer it retained is the one this invocation found -- an
    // intermediate generation (a double publication) would show up as
    // `previous.json` naming something else.
    let previous: Value = serde_json::from_str(
        &fs::read_to_string(workspace.join(".beads/checkpoint/previous.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        previous["generation_id"].as_str().unwrap(),
        before_generation,
        "previous.json must retain the generation this invocation displaced; \
         an intermediate generation was published"
    );

    // Exactly one object: the only file publication added is the new
    // generation's content-addressed root, and it is the root the pointer
    // selects.
    let after = snapshot_checkpoint(workspace);
    let added: Vec<&String> = after.keys().filter(|k| !before.contains_key(*k)).collect();
    let root_path = after_status["root_path"].as_str().unwrap().to_string();
    assert_eq!(
        added,
        vec![&root_path],
        "one real mutation must mint exactly one object (the new root), \
         instead the checkpoint set changed by {added:?}"
    );
    assert_eq!(
        after_status["root_verified"],
        Value::Bool(true),
        "the published root must verify against the pointer"
    );
    assert_eq!(
        after_status["ready_to_commit"],
        Value::Bool(true),
        "the published generation must leave the checkpoint ready to commit"
    );
}

/// `sync flush-only` is idempotent (plan 6.2.1 item 8): against a clean,
/// ready-to-commit checkpoint it publishes no new generation, creates no
/// object, and exits 0. A dirty checkpoint still publishes -- the skip may
/// never swallow a flush that has work to do.
#[test]
fn sync_flush_only_is_idempotent_against_a_clean_checkpoint() {
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path();
    run(workspace, &["init", "--prefix", "idem"]);
    set_auto_flush(workspace, true);
    create_issue(workspace, "idempotent flush issue");
    assert_covers_live(workspace, "setup");

    let before = snapshot_checkpoint(workspace);
    let before_generation = generation_id(workspace);

    let output = run(workspace, &["sync", "flush-only"]);
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("already current"),
        "a clean flush must say so, got {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        generation_id(workspace),
        before_generation,
        "a clean flush published a new generation"
    );
    assert_eq!(
        snapshot_checkpoint(workspace),
        before,
        "a clean flush changed the checkpoint set -- it must publish nothing"
    );

    // Contrast: dirty the checkpoint by suppressing publication for one
    // mutation, and the same command must publish.
    set_auto_flush(workspace, false);
    create_issue(workspace, "suppressed until explicit flush");
    let dirty = status(workspace);
    assert!(
        dirty["covered_sequence"].as_i64().unwrap() < dirty["live_sequence"].as_i64().unwrap(),
        "setup: the checkpoint must be dirty before the contrast flush"
    );
    let output = run(workspace, &["sync", "flush-only"]);
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("Flushed forensic checkpoint"),
        "a dirty flush must still publish, got {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_ne!(
        generation_id(workspace),
        before_generation,
        "a dirty flush published nothing"
    );
    assert_covers_live(workspace, "after explicit flush of a dirty checkpoint");
}

/// The flush skip is bounded by readiness, not just cleanliness: a
/// checkpoint with an unresolved tombstone -- the state an interrupted
/// cleanup leaves behind -- is not ready to commit, so `sync flush-only`
/// republishes and reapplies the cleanup rather than skipping.
#[test]
fn flush_only_republishes_a_not_ready_checkpoint() {
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path();
    run(workspace, &["init", "--prefix", "tomb"]);
    set_auto_flush(workspace, true);

    // Three mutations leave the first generation's root unreferenced by
    // both pointers, hence tombstoned by the third publication.
    create_issue(workspace, "tombstone issue one");
    create_issue(workspace, "tombstone issue two");
    create_issue(workspace, "tombstone issue three");

    let report = status(workspace);
    let tombstoned = report["unresolved_tombstones"].as_array().unwrap().len();
    assert_eq!(
        tombstoned, 0,
        "setup: a completed publication leaves no unresolved tombstones"
    );
    let pointer: Value = serde_json::from_str(
        &fs::read_to_string(workspace.join(".beads/checkpoint/current.json")).unwrap(),
    )
    .unwrap();
    let deleted: Vec<String> = pointer["deleted_paths"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect();
    assert!(
        !deleted.is_empty(),
        "setup: three generations must have tombstoned the first root"
    );

    // Simulate the interrupted cleanup: a declared-deleted object still on
    // disk keeps the checkpoint not ready to commit.
    let leftover = &deleted[0];
    fs::write(
        workspace.join(".beads/checkpoint").join(leftover),
        "interrupted cleanup residue\n",
    )
    .unwrap();
    let report = status(workspace);
    assert_eq!(
        report["ready_to_commit"],
        Value::Bool(false),
        "setup: an unresolved tombstone must keep the checkpoint not ready"
    );

    let output = run(workspace, &["sync", "flush-only"]);
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("Flushed forensic checkpoint"),
        "a not-ready checkpoint must be republished, not skipped, got {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !workspace.join(".beads/checkpoint").join(leftover).exists(),
        "the republish must reapply the interrupted tombstone cleanup"
    );
    let report = status(workspace);
    assert_eq!(
        report["ready_to_commit"],
        Value::Bool(true),
        "the republished checkpoint must be ready to commit"
    );
    assert_covers_live(workspace, "after cleanup-reapplying flush");
}

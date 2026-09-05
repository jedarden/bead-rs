//! Atomic creation of dependent work (BR-T28).
//!
//! A planned dependent issue must never appear on the ready frontier before
//! the dependency graph that gates it has committed. Two native paths make
//! that a property of the store rather than a discipline for the planner:
//! `bead create --depends-on`, which validates and attaches `blocks` edges
//! inside the create transaction, and a bulk manifest, which composes
//! creates with `dep_add` in one transaction.
//!
//! The fixtures prove the invariant directly instead of trusting SQLite
//! isolation. One drives real concurrent claimers over their own
//! connections while a dependent graph is in flight; the rest hold every
//! validation failure to an all-or-none standard -- no partial issue, no
//! partial edge, no second checkpoint generation. A negative control shows
//! that the two-command composition the atomic paths replace does leave a
//! claimable intermediate state behind, so the guards demonstrably measure
//! the hazard rather than passing vacuously.

use bead_rs::service::claim::claim_issue;
use bead_rs::service::dependencies::add_dependency_in_tx;
use bead_rs::service::issues::create_issue_with_unique_ref;
use bead_rs::store::{open_configured_connection, WorkspaceConfig};
use bead_rs::Error;
use rusqlite::{Connection, Transaction, TransactionBehavior};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

fn run(workspace: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_bead"))
        .current_dir(workspace)
        .arg("--skip-foreign-workspace")
        .args(args)
        .output()
        .expect("bead command should start")
}

fn setup(prefix: &str) -> tempfile::TempDir {
    let workspace = tempfile::tempdir().unwrap();
    let output = run(workspace.path(), &["init", "--prefix", prefix]);
    assert!(output.status.success(), "init failed: {output:?}");
    workspace
}

fn db_path(workspace: &Path) -> PathBuf {
    workspace.join(".beads/beads.db")
}

/// A workspace config for direct library calls. Only the prefix reaches the
/// create path, and it is the prefix `setup` initialized the store with.
fn config_for(workspace: &Path, prefix: &str) -> WorkspaceConfig {
    WorkspaceConfig {
        root: workspace.to_path_buf(),
        uuid: "fixture-workspace-uuid".to_string(),
        prefix: prefix.to_string(),
    }
}

fn open_db(workspace: &Path) -> Connection {
    open_configured_connection(&db_path(workspace)).unwrap()
}

fn immediate(conn: &Connection) -> Transaction<'_> {
    Transaction::new_unchecked(conn, TransactionBehavior::Immediate).unwrap()
}

fn count(workspace: &Path, sql: &str) -> i64 {
    let conn = open_db(workspace);
    conn.query_row(sql, [], |row| row.get(0)).unwrap()
}

fn issue_count(workspace: &Path) -> i64 {
    count(workspace, "SELECT COUNT(*) FROM issues")
}

fn edge_count(workspace: &Path) -> i64 {
    count(workspace, "SELECT COUNT(*) FROM dependencies")
}

/// Run `bead claim` and return the claimed ID, or None when the frontier
/// was empty.
fn claim_id(workspace: &Path) -> Option<String> {
    let output = run(workspace, &["claim", "--assignee", "fixture-worker"]);
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).unwrap();
    // Success prints "Claimed: <id>" followed by the assignee line.
    text.lines()
        .find_map(|line| line.strip_prefix("Claimed: "))
        .map(str::to_string)
}

fn create_issue(workspace: &Path, args: &[&str]) -> String {
    let output = run(workspace, args);
    assert!(
        output.status.success(),
        "create failed: {output:?}\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

/// `sync status --format json` parsed from stdout.
fn status(workspace: &Path) -> Value {
    let output = run(workspace, &["sync", "status", "--format", "json"]);
    assert!(output.status.success(), "status failed: {output:?}");
    serde_json::from_slice(&output.stdout).unwrap()
}

/// Every file under `.beads/checkpoint/` with its bytes, keyed by
/// checkpoint-relative path. Publication rewrites the pointers and mints
/// content-addressed objects, so only a publication moves this map.
fn snapshot_checkpoint(workspace: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(dir: &Path, prefix: String, out: &mut BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let name = entry.file_name().to_str().unwrap().to_string();
            if path.is_dir() {
                walk(&path, format!("{prefix}{name}/"), out);
            } else {
                out.insert(format!("{prefix}{name}"), fs::read(&path).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(
        &workspace.join(".beads/checkpoint"),
        String::new(),
        &mut out,
    );
    out
}

fn write_manifest(workspace: &Path, name: &str, body: &str) -> PathBuf {
    let path = workspace.join(name);
    fs::write(&path, body).unwrap();
    path
}

fn manifest_commit(workspace: &Path, path: &Path) -> (Output, Value) {
    let output = run(
        workspace,
        &[
            "manifest",
            "commit",
            "--input",
            &path.display().to_string(),
            "--format",
            "json",
        ],
    );
    let report = if output.status.success() {
        serde_json::from_slice(&output.stdout).unwrap_or(Value::Null)
    } else {
        Value::Null
    };
    (output, report)
}

fn assert_exit(output: &Output, code: i32, context: &str) {
    let actual = output.status.code().unwrap_or(-1);
    assert_eq!(
        actual,
        code,
        "{context}: expected exit {code}, got {actual}\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

/// The dependent issue is gated from its first committed instant and
/// becomes claimable only when the blocker closes.
#[test]
fn dependent_create_is_never_claimable_before_its_blocker_closes() {
    let workspace = setup("adpnc");

    // The blocker is deliberately the *least* urgent issue in the store and
    // the dependent the most urgent. If the edge were missing or late, the
    // frontier would hand back the dependent first and this test would
    // fail -- the assertion cannot pass on ordering luck.
    let blocker = create_issue(
        workspace.path(),
        &["create", "--title", "blocker", "--priority", "4"],
    );
    let dependent = create_issue(
        workspace.path(),
        &[
            "create",
            "--title",
            "dependent conformance work",
            "--priority",
            "0",
            "--depends-on",
            &blocker,
        ],
    );

    assert_ne!(blocker, dependent);

    // The only ready issue is the blocker: the dependent is already gated.
    assert_eq!(
        claim_id(workspace.path()).as_deref(),
        Some(blocker.as_str()),
        "the frontier offered something other than the blocker"
    );
    // The dependent is not claimable while the blocker is merely claimed.
    assert_eq!(
        claim_id(workspace.path()),
        None,
        "the dependent became claimable while its blocker was still open"
    );

    // Closing the blocker is what releases the dependent.
    let closed = run(workspace.path(), &["close", &blocker, "--reason", "done"]);
    assert!(
        closed.status.success(),
        "close failed: {}",
        String::from_utf8_lossy(&closed.stderr)
    );
    assert_eq!(
        claim_id(workspace.path()).as_deref(),
        Some(dependent.as_str()),
        "the dependent stayed gated after its blocker closed"
    );
}

/// Negative control: the composition the atomic paths replace really does
/// publish a claimable intermediate state. Kept as an executable statement
/// of the dispatch race, so the guards above are known to detect it.
#[test]
fn two_separate_commands_leave_a_claimable_intermediate() {
    let workspace = setup("adpint");

    let dependent = create_issue(
        workspace.path(),
        &[
            "create",
            "--title",
            "urgent unplanned work",
            "--priority",
            "0",
        ],
    );
    // No edge exists yet, so the frontier serves the bead immediately --
    // the window a second command cannot close after the fact.
    assert_eq!(
        claim_id(workspace.path()).as_deref(),
        Some(dependent.as_str()),
        "expected the bare create to be claimable before its dependency lands"
    );

    let blocker = create_issue(
        workspace.path(),
        &["create", "--title", "late blocker", "--priority", "4"],
    );
    let edge = run(
        workspace.path(),
        &["dep", "add", &dependent, &blocker, "--kind", "blocks"],
    );
    assert!(
        edge.status.success(),
        "dep failed: {}",
        String::from_utf8_lossy(&edge.stderr)
    );
}

/// A claimer running on its own connection, concurrently with an in-flight
/// dependent create, observes zero wins: it sees either the pre-commit
/// store, where the bead does not exist, or the post-commit one, where the
/// edge already gates it. There is no third state.
#[test]
fn concurrent_claimers_observe_zero_wins_before_graph_commit() {
    let workspace = setup("adpcc");
    let config = config_for(workspace.path(), "adpcc");

    // Give the frontier a decoy so the claimers do real work throughout the
    // window instead of spinning on an empty frontier.
    let decoy = {
        let conn = open_db(workspace.path());
        let tx = immediate(&conn);
        let result = create_issue_with_unique_ref(
            &tx,
            &config,
            "decoy work".to_string(),
            None,
            3,
            None,
            None,
            vec![],
            vec![],
            None,
        )
        .unwrap();
        tx.commit().unwrap();
        result.issue.id
    };

    let stop = Arc::new(AtomicBool::new(false));
    let mut handles = Vec::new();
    for worker in 0..2 {
        let stop = Arc::clone(&stop);
        let path = db_path(workspace.path());
        handles.push(std::thread::spawn(move || {
            let conn = open_configured_connection(&path).unwrap();
            let mut claimed = Vec::new();
            let mut attempt = 0u64;
            while !stop.load(Ordering::SeqCst) {
                attempt += 1;
                // A distinct assignee per attempt, so a claimer that already
                // holds work keeps pulling rather than tripping the
                // single-claim guard.
                let assignee = format!("claimer-{worker}-{attempt}");
                let tx = immediate(&conn);
                match claim_issue(&tx, &assignee, None, None, None, false) {
                    Ok(result) => {
                        tx.commit().unwrap();
                        if let Some(id) = result.bead_id {
                            claimed.push(id);
                        }
                    }
                    // Losing the IMMEDIATE write lock to a rival is a
                    // scheduling outcome, not a contract violation; retry.
                    Err(error) if is_lock_contention(&error) => {}
                    Err(error) => panic!("claim failed unexpectedly: {error}"),
                }
                // Pace the loop so the writer lock is periodically released
                // and the in-flight create can actually acquire it.
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            claimed
        }));
    }

    // The dependent graph is built inside one transaction and held open
    // across many claimer attempts, so any half-built visibility would
    // surface here.
    let dependent = {
        let conn = open_db(workspace.path());
        let tx = immediate(&conn);
        let created = create_issue_with_unique_ref(
            &tx,
            &config,
            "gated conformance work".to_string(),
            None,
            0,
            None,
            None,
            vec![],
            vec![],
            None,
        )
        .unwrap_or_else(|error| panic!("the in-flight create failed to run: {error}"));
        // Hold the uncommitted graph open while the claimers run, then add
        // the edge and commit once.
        for _ in 0..25 {
            std::thread::yield_now();
        }
        add_dependency_in_tx(&tx, &created.issue.id, &decoy, "blocks", None).unwrap();
        let id = created.issue.id.clone();
        tx.commit().unwrap();
        id
    };

    stop.store(true, Ordering::SeqCst);
    let mut every_win = Vec::new();
    for handle in handles {
        every_win.extend(handle.join().unwrap());
    }

    assert!(
        !every_win.contains(&dependent),
        "a concurrent claimer won the dependent bead {dependent:?} while its \
         graph was still in flight (wins: {every_win:?})"
    );
    assert!(
        !every_win.is_empty(),
        "the claimers won nothing at all, so the zero-wins assertion is vacuous"
    );

    // After the commit the edge is in place and the dependent is still not
    // served: concurrency never got a chance to bypass the gate either.
    let conn = open_db(workspace.path());
    let tx = immediate(&conn);
    let after = claim_issue(&tx, "after-commit", None, None, None, true).unwrap();
    tx.commit().unwrap();
    assert_ne!(
        after.bead_id.as_deref(),
        Some(dependent.as_str()),
        "the dependent was claimable immediately after its graph committed"
    );
}

/// A lock contention error from SQLite, which a concurrent claimer treats
/// as a lost race to retry rather than a contract failure.
fn is_lock_contention(error: &Error) -> bool {
    fn busy(message: &str) -> bool {
        message.contains("busy") || message.contains("locked")
    }
    match error {
        Error::DatabaseBusy(message) => busy(message),
        Error::Sqlite(source) => busy(&source.to_string()),
        _ => false,
    }
}

/// A cycle inside one manifest refuses the whole document's effects: no
/// issue, no edge, and no checkpoint publication survive the rollback.
#[test]
fn cycle_in_a_manifest_rolls_back_every_issue_and_edge() {
    let workspace = setup("adpcyc");

    // Setup mutations put the store and checkpoint in a steady state before
    // the counters are read, so only the manifest under test can move them.
    create_issue(workspace.path(), &["create", "--title", "setup 1"]);
    create_issue(workspace.path(), &["create", "--title", "setup 2"]);
    let issues_before = issue_count(workspace.path());
    let edges_before = edge_count(workspace.path());
    let checkpoint_before = snapshot_checkpoint(workspace.path());

    let path = write_manifest(
        workspace.path(),
        "cycle.json",
        r#"{"manifest_version": 1, "operations": [
            {"op": "create", "local_id": "a", "title": "side a", "priority": 0},
            {"op": "create", "local_id": "b", "title": "side b", "priority": 0},
            {"op": "dep_add", "blocked": "$a", "blocker": "$b"},
            {"op": "dep_add", "blocked": "$b", "blocker": "$a"}
        ]}"#,
    );

    let (output, report) = manifest_commit(workspace.path(), &path);
    assert_exit(&output, 4, "a cyclical manifest must be refused");
    assert!(
        report.is_null(),
        "a refused manifest must not report a commit result"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("cycle"),
        "the refusal should name the cycle, got: {stderr}"
    );

    assert_eq!(issue_count(workspace.path()), issues_before);
    assert_eq!(edge_count(workspace.path()), edges_before);
    assert_eq!(
        snapshot_checkpoint(workspace.path()),
        checkpoint_before,
        "a rolled-back manifest must not publish a generation"
    );
}

/// A self-edge is a cycle of length one and is refused the same way.
#[test]
fn self_edge_in_a_manifest_rolls_back_the_create() {
    let workspace = setup("adpself");
    create_issue(workspace.path(), &["create", "--title", "setup 1"]);
    let issues_before = issue_count(workspace.path());
    let edges_before = edge_count(workspace.path());

    let path = write_manifest(
        workspace.path(),
        "self.json",
        r#"{"manifest_version": 1, "operations": [
            {"op": "create", "local_id": "a", "title": "self gated"},
            {"op": "dep_add", "blocked": "$a", "blocker": "$a"}
        ]}"#,
    );

    let (output, _report) = manifest_commit(workspace.path(), &path);
    assert_exit(&output, 4, "a self-edge must be refused");
    assert_eq!(issue_count(workspace.path()), issues_before);
    assert_eq!(edge_count(workspace.path()), edges_before);
}

/// A blocker that does not exist aborts the create. Neither the CLI flag
/// nor the manifest form leaves the new issue behind. A missing ID is a
/// workspace error (exit 3), distinct from the cycle conflicts (exit 4).
#[test]
fn missing_blocker_rolls_back_the_create() {
    let workspace = setup("adpmiss");
    create_issue(workspace.path(), &["create", "--title", "setup 1"]);
    let issues_before = issue_count(workspace.path());
    let edges_before = edge_count(workspace.path());

    let refused = run(
        workspace.path(),
        &[
            "create",
            "--title",
            "gated on a ghost",
            "--depends-on",
            "bead-doesnotexist",
        ],
    );
    assert_exit(&refused, 3, "create --depends-on a missing blocker");
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(
        stderr.contains("bead-doesnotexist"),
        "the refusal should name the missing blocker, got: {stderr}"
    );
    assert_eq!(issue_count(workspace.path()), issues_before);
    assert_eq!(edge_count(workspace.path()), edges_before);

    let path = write_manifest(
        workspace.path(),
        "ghost.json",
        r#"{"manifest_version": 1, "operations": [
            {"op": "create", "local_id": "a", "title": "gated on a ghost too"},
            {"op": "dep_add", "blocked": "$a", "blocker": "bead-doesnotexist"}
        ]}"#,
    );
    let (output, _report) = manifest_commit(workspace.path(), &path);
    assert_exit(&output, 3, "manifest dep_add on a missing blocker");
    assert_eq!(issue_count(workspace.path()), issues_before);
    assert_eq!(edge_count(workspace.path()), edges_before);

    // The refused creates were never minted, so a later manifest that fixes
    // the blocker cannot collide with an orphan from the failed attempt.
    assert_eq!(
        count(
            workspace.path(),
            "SELECT COUNT(*) FROM issues WHERE title LIKE 'gated on a ghost%'"
        ),
        0
    );
}

/// Replaying a committed dependent-create manifest changes nothing: every
/// create resolves through its unique reference, no duplicate issue or edge
/// appears, and because every operation is a no-op no second checkpoint
/// generation is published.
///
/// Every create in the plan carries a `unique_ref` deliberately. A create
/// without one mints a fresh bead on every replay, so a plan that names any
/// bare create is not replay-safe at all -- that requirement is part of the
/// planner contract this fixture pins.
#[test]
fn unique_reference_replay_is_idempotent_and_publishes_no_second_generation() {
    let workspace = setup("adpreplay");
    create_issue(workspace.path(), &["create", "--title", "setup 1"]);

    let path = write_manifest(
        workspace.path(),
        "plan.json",
        r#"{"manifest_version": 1, "operations": [
            {"op": "create", "local_id": "a", "title": "planned dependent",
             "priority": 0, "labels": ["conformance"],
             "resource_keys": ["store:atomic-planning"],
             "unique_ref": "plan:atomic-dependent"},
            {"op": "create", "local_id": "b", "title": "planned blocker",
             "unique_ref": "plan:atomic-blocker"},
            {"op": "dep_add", "blocked": "$a", "blocker": "$b"}
        ]}"#,
    );

    let (first, first_report) = manifest_commit(workspace.path(), &path);
    assert_exit(&first, 0, "first manifest commit");
    assert_eq!(first_report["semantic_changes"], Value::from(3));
    let results = first_report["results"].as_array().unwrap();
    let dependent = results[0]["issue_id"].as_str().unwrap().to_string();
    let blocker = results[1]["issue_id"].as_str().unwrap().to_string();

    let issues_after_first = issue_count(workspace.path());
    let checkpoint_after_first = snapshot_checkpoint(workspace.path());
    let generation_after_first = status(workspace.path())["generation_id"]
        .as_str()
        .unwrap()
        .to_string();

    // Replay the byte-identical manifest.
    let (second, second_report) = manifest_commit(workspace.path(), &path);
    assert_exit(&second, 0, "replayed manifest commit");
    assert_eq!(
        second_report["semantic_changes"],
        Value::from(0),
        "a replay must not report semantic changes"
    );
    let replayed = second_report["results"].as_array().unwrap();
    assert_eq!(replayed[0]["outcome"], Value::String("existing".into()));
    assert_eq!(replayed[0]["issue_id"], Value::String(dependent.clone()));
    assert_eq!(
        replayed[2]["outcome"],
        Value::String("no-op".into()),
        "the dependency edge must not be re-added"
    );

    assert_eq!(issue_count(workspace.path()), issues_after_first);

    // A no-op manifest publishes nothing: the checkpoint files and the
    // generation pointer are byte-identical after the replay.
    assert_eq!(
        snapshot_checkpoint(workspace.path()),
        checkpoint_after_first,
        "an idempotent replay must not publish a generation"
    );
    assert_eq!(
        status(workspace.path())["generation_id"],
        Value::String(generation_after_first)
    );

    // Exactly one edge, still binding the same two real beads.
    let conn = open_db(workspace.path());
    let edges: (i64, i64) = conn
        .query_row(
            "SELECT COUNT(*),
                    (SELECT COUNT(*) FROM dependencies
                     WHERE blocked_issue_id = ?1 AND blocker_issue_id = ?2
                       AND kind = 'blocks')
             FROM dependencies",
            rusqlite::params![dependent, blocker],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(edges, (1, 1), "the replay changed the graph");

    // The replayed bead keeps the resource keys declared in the manifest.
    let keys: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM issue_resource_keys
             WHERE issue_id = ?1 AND resource_key = 'store:atomic-planning'",
            [&dependent],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(keys, 1, "resource keys were lost on replay");
}

/// Resource keys declared alongside a dependent create are on the bead at
/// its first committed instant, so scheduling exclusion never has to be
/// retrofitted with a second command.
#[test]
fn resource_keys_are_present_at_first_visibility() {
    let workspace = setup("adpkeys");
    let blocker = create_issue(
        workspace.path(),
        &["create", "--title", "blocker", "--priority", "4"],
    );

    let dependent = create_issue(
        workspace.path(),
        &[
            "create",
            "--title",
            "gated and key-scoped",
            "--priority",
            "0",
            "--resource-key",
            "docker:daemon",
            "--resource-key",
            "gpu:0",
            "--depends-on",
            &blocker,
        ],
    );

    // First read after the commit: the keys are already there and the bead
    // is already gated, so there is no moment where either is missing.
    let conn = open_db(workspace.path());
    let (keys, edges): (i64, i64) = conn
        .query_row(
            "SELECT (SELECT COUNT(*) FROM issue_resource_keys
                     WHERE issue_id = issues.id AND resource_key = 'docker:daemon'),
                    (SELECT COUNT(*) FROM dependencies
                     WHERE blocked_issue_id = issues.id AND kind = 'blocks')
             FROM issues WHERE id = ?1",
            [&dependent],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        keys, 1,
        "the declared resource key was missing at first visibility"
    );
    assert_eq!(edges, 1, "the gating edge was missing at first visibility");

    let second: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM issue_resource_keys
             WHERE issue_id = ?1 AND resource_key = 'gpu:0'",
            [&dependent],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(second, 1);
}

/// One committed dependent-create manifest covers its whole event span and
/// names both beads in the published checkpoint: audit and checkpoint
/// advance together or not at all.
#[test]
fn checkpoint_covers_the_whole_dependent_graph() {
    let workspace = setup("adpckpt");
    // Setup mutations put the checkpoint in its steady state.
    create_issue(workspace.path(), &["create", "--title", "setup 1"]);
    create_issue(workspace.path(), &["create", "--title", "setup 2"]);

    let generation_before = status(workspace.path())["generation_id"]
        .as_str()
        .unwrap()
        .to_string();

    let path = write_manifest(
        workspace.path(),
        "plan.json",
        r#"{"manifest_version": 1, "operations": [
            {"op": "create", "local_id": "a", "title": "gated work", "priority": 0,
             "resource_keys": ["store:atomic-planning"]},
            {"op": "create", "local_id": "b", "title": "gating work"},
            {"op": "dep_add", "blocked": "$a", "blocker": "$b"}
        ]}"#,
    );
    let (output, report) = manifest_commit(workspace.path(), &path);
    assert_exit(&output, 0, "manifest commit");
    let results = report["results"].as_array().unwrap();
    let dependent = results[0]["issue_id"].as_str().unwrap().to_string();
    let blocker = results[1]["issue_id"].as_str().unwrap().to_string();

    let state = status(workspace.path());
    assert_eq!(
        state["covered_sequence"], state["live_sequence"],
        "the published generation must cover the manifest's whole event span"
    );
    assert_eq!(
        state["ready_to_commit"],
        Value::Bool(true),
        "the published generation left the checkpoint dirty"
    );
    assert_ne!(
        state["generation_id"],
        Value::String(generation_before),
        "a semantic manifest must publish a new generation"
    );

    // The published checkpoint carries the graph, not just the issues:
    // `current.json` is only a pointer, so search every published artifact
    // for both ends of the edge.
    let published = snapshot_checkpoint(workspace.path());
    let mut names_dependent = false;
    let mut names_blocker = false;
    for bytes in published.values() {
        let text = String::from_utf8_lossy(bytes);
        names_dependent |= text.contains(dependent.as_str());
        names_blocker |= text.contains(blocker.as_str());
    }
    assert!(
        names_dependent,
        "the published checkpoint does not name the dependent {dependent}"
    );
    assert!(
        names_blocker,
        "the published checkpoint does not name the blocker {blocker}"
    );

    // The audit trail carries both create events plus the dependency event,
    // recorded in the same committed span.
    let conn = open_db(workspace.path());
    let events: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM events WHERE issue_id IN (?1, ?2)",
            rusqlite::params![dependent, blocker],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        events, 3,
        "expected two create events and one dependency_added event, got {events}"
    );
    let dependency_event: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM events WHERE issue_id = ?1 AND kind = 'dependency_added'",
            [&dependent],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(dependency_event, 1);
}

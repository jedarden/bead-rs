//! Concurrency behavior of the declared `verifies` kind (R025, ADR-001,
//! beadrs-aa877d96).
//!
//! The R025 conformance suite (`verifies_edges.rs`) pins the static contract.
//! The two properties that only show under concurrent mutation are pinned
//! here, each by racing real connections against one store:
//!
//! - cycle rejection is unchanged: a `blocks` edge that closes a cycle is
//!   rejected from inside its own IMMEDIATE transaction even when a full
//!   `verifies` ring already connects the same beads -- a naive check over
//!   all kinds would see that ring as a path and reject the first legal
//!   edge -- and the racing pair commits exactly one edge, never a ring;
//! - readiness computation is unchanged: a reader polling the ready frontier
//!   observes the committed edge set only. The frontier keeps the bead while
//!   an uncommitted `blocks` edge is in flight and drops it exactly when that
//!   edge commits; a `verifies` edge in flight never drops it at all.
//!
//! A third race pins the converse of the first: opposite `verifies` edges
//! racing each other both land, because the kind is outside cycle detection
//! on every attempt and the writer-lock retry never drops a committed row.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use bead_rs::service::dependencies::add_dependency_in_tx;
use bead_rs::store::open_configured_connection;
use bead_rs::Error;
use rusqlite::{Transaction, TransactionBehavior};
use serde_json::Value;

fn run(workspace: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_bead"))
        .current_dir(workspace)
        .arg("--skip-foreign-workspace")
        .args(args)
        .output()
        .expect("bead command should start")
}

/// Disposable workspace under /var/tmp so no ancestor carries a foreign
/// `.beads` directory.
fn setup(prefix: &str) -> tempfile::TempDir {
    let workspace = tempfile::Builder::new()
        .prefix("bead-verifies-conc-")
        .tempdir_in("/var/tmp")
        .unwrap();
    let output = run(workspace.path(), &["init", "--prefix", prefix]);
    assert!(output.status.success(), "init failed: {output:?}");
    workspace
}

fn create(workspace: &Path, title: &str) -> String {
    let output = run(
        workspace,
        &["create", "--title", title, "--issue-type", "task"],
    );
    assert!(output.status.success(), "create failed: {output:?}");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn dep_add(workspace: &Path, blocked: &str, blocker: &str, kind: &str) -> Output {
    run(workspace, &["dep", "add", blocked, blocker, "--kind", kind])
}

fn db_path(workspace: &Path) -> PathBuf {
    workspace.join(".beads/beads.db")
}

fn count_rows(workspace: &Path, kind: &str) -> i64 {
    let conn = open_configured_connection(&db_path(workspace)).unwrap();
    conn.query_row(
        "SELECT COUNT(*) FROM dependencies WHERE kind = ?",
        [kind],
        |row| row.get(0),
    )
    .unwrap()
}

/// IDs of beads on the ready frontier. `list --json` emits JSONL -- one
/// record per line -- not a single JSON array.
fn ready_ids(workspace: &Path) -> Vec<String> {
    let output = run(workspace, &["list", "--ready", "--json"]);
    assert!(output.status.success(), "list --ready failed: {output:?}");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str::<Value>(line).unwrap_or_else(|e| {
                panic!("list --ready --json line must be valid JSON ({line}): {e}")
            })
        })
        .filter_map(|record| record["id"].as_str().map(str::to_string))
        .collect()
}

/// Run the dependencies doctor scope and return its parsed JSON report.
/// Advisory findings keep the exit code at 0; the assert pins that.
fn doctor_dependencies(workspace: &Path) -> Value {
    let output = run(
        workspace,
        &["doctor", "--scope", "dependencies", "--format", "json"],
    );
    assert!(
        output.status.success(),
        "doctor must exit 0 for advisory findings: {output:?}"
    );
    serde_json::from_str(String::from_utf8_lossy(&output.stdout).trim())
        .expect("doctor --format json must emit valid JSON")
}

fn doctor_check<'a>(report: &'a Value, name: &str) -> &'a Value {
    report["checks"]
        .as_array()
        .expect("report carries a checks array")
        .iter()
        .find(|check| check["name"] == name)
        .unwrap_or_else(|| panic!("dependencies scope reports {name}"))
}

/// Whether a rusqlite error is a lock-timing outcome rather than a contract
/// failure: BEGIN IMMEDIATE losing the writer-lock race to a rival retries,
/// and nothing else does.
fn is_busy(error: &rusqlite::Error) -> bool {
    error.sqlite_error_code() == Some(rusqlite::ErrorCode::DatabaseBusy)
}

/// The terminal state of one `dep add` attempt on its own connection.
#[derive(Debug, PartialEq, Eq)]
enum DepOutcome {
    /// The transaction committed; the flag says whether this attempt
    /// inserted the edge row (false: it was already there).
    Committed(bool),
    /// The in-transaction cycle check rejected a `blocks` edge.
    CycleRejected,
}

/// Attempt one `dep add` on a private connection, retrying only while the
/// failure is writer-lock contention. Cycle detection runs inside the same
/// IMMEDIATE transaction as the insert, so a rejection observes every edge
/// some rival has committed and no edge any rival has not.
fn attempt_dep_add(path: &Path, blocked: &str, blocker: &str, kind: &str) -> DepOutcome {
    for _ in 0..200 {
        let conn = open_configured_connection(path).unwrap();
        let tx = match Transaction::new_unchecked(&conn, TransactionBehavior::Immediate) {
            Ok(tx) => tx,
            Err(error) if is_busy(&error) => continue,
            Err(error) => panic!("begin immediate failed: {error}"),
        };
        match add_dependency_in_tx(&tx, blocked, blocker, kind, None) {
            Ok(inserted) => match tx.commit() {
                Ok(()) => return DepOutcome::Committed(inserted),
                Err(error) if is_busy(&error) => {}
                Err(error) => panic!("commit failed: {error}"),
            },
            Err(Error::Conflict(message)) if message.contains("cycle") => {
                return DepOutcome::CycleRejected;
            }
            Err(error) => panic!("dep add failed unexpectedly: {error}"),
        }
    }
    panic!("dep add never acquired the writer lock in 200 attempts");
}

/// The racing pair of opposite `blocks` edges admits exactly one survivor,
/// and the pre-existing `verifies` ring does not perturb the outcome.
///
/// The ring is the interesting hazard: a cycle check that walked every kind
/// would read it as a two-node path and reject even the first `blocks` edge,
/// which is legal. The check walks `blocks` edges only, so exactly one
/// racing edge commits, the other is rejected as cycle-closing from inside
/// its own transaction, and no `blocks` ring ever exists -- regardless of
/// which thread wins.
#[test]
fn blocks_cycle_race_admits_exactly_one_edge_despite_verifies_ring() {
    let workspace = setup("vcr");
    let first = create(workspace.path(), "First racing bead");
    let second = create(workspace.path(), "Second racing bead");

    // Seed the full two-node `verifies` ring: each bead checks the other's
    // work. The declared kind never enters cycle detection, so both edges
    // commit.
    dep_add(workspace.path(), &first, &second, "verifies").assert_ok("first verifies edge");
    dep_add(workspace.path(), &second, &first, "verifies").assert_ok("reverse verifies edge");

    // Race the opposite `blocks` gates on their own connections.
    let path = db_path(workspace.path());
    let mut handles = Vec::new();
    for (blocked, blocker) in [
        (first.clone(), second.clone()),
        (second.clone(), first.clone()),
    ] {
        let path = path.clone();
        handles.push(std::thread::spawn(move || {
            attempt_dep_add(&path, &blocked, &blocker, "blocks")
        }));
    }
    let mut outcomes: Vec<DepOutcome> = Vec::new();
    for handle in handles {
        outcomes.push(handle.join().unwrap());
    }

    assert_eq!(
        outcomes
            .iter()
            .filter(|o| **o == DepOutcome::Committed(true))
            .count(),
        1,
        "exactly one racing edge inserts, got: {outcomes:?}"
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|o| **o == DepOutcome::CycleRejected)
            .count(),
        1,
        "the loser is rejected as cycle-closing, never by lock timing, got: {outcomes:?}"
    );

    // The store holds the seeded ring plus one `blocks` gate -- no ring.
    assert_eq!(count_rows(workspace.path(), "verifies"), 2);
    assert_eq!(count_rows(workspace.path(), "blocks"), 1);

    // The surviving graph is acyclic on `blocks` and, because the winner's
    // blocker also verifies the blocked bead, the same pair is diagnosed as
    // an inverted verification gate -- advisory, exit 0, never an error.
    let report = doctor_dependencies(workspace.path());
    assert_eq!(doctor_check(&report, "dependency_graph")["status"], "ok");
    assert_eq!(
        doctor_check(&report, "inverted_verification_gates")["status"],
        "warning"
    );
    assert!(!report["has_errors"].as_bool().unwrap_or(true));
}

/// Racing opposite `verifies` edges both land. The kind is outside cycle
/// detection on every attempt, so neither racing connection may lose its
/// insert to its rival or to the writer-lock retry loop: the committed pair
/// is the full ring. That ring alone is also not an inverted gate -- the
/// doctor's graph stays healthy with no findings -- and the frontier is
/// unmoved, because `verifies` never gates readiness even under contention.
#[test]
fn racing_opposite_verifies_edges_both_commit() {
    let workspace = setup("vvo");
    let first = create(workspace.path(), "First verified bead");
    let second = create(workspace.path(), "Second verified bead");

    let path = db_path(workspace.path());
    let mut handles = Vec::new();
    for (blocked, blocker) in [
        (first.clone(), second.clone()),
        (second.clone(), first.clone()),
    ] {
        let path = path.clone();
        handles.push(std::thread::spawn(move || {
            attempt_dep_add(&path, &blocked, &blocker, "verifies")
        }));
    }
    let mut outcomes: Vec<DepOutcome> = Vec::new();
    for handle in handles {
        outcomes.push(handle.join().unwrap());
    }

    assert_eq!(
        outcomes
            .iter()
            .filter(|o| **o == DepOutcome::Committed(true))
            .count(),
        2,
        "both racing verifies edges insert, got: {outcomes:?}"
    );
    assert!(
        !outcomes.contains(&DepOutcome::CycleRejected),
        "cycle detection never rejects a verifies edge, got: {outcomes:?}"
    );

    // The ring is complete and nothing was lost to the race.
    assert_eq!(count_rows(workspace.path(), "verifies"), 2);

    // A `verifies` ring alone is not an inverted gate and not an error.
    let report = doctor_dependencies(workspace.path());
    assert_eq!(doctor_check(&report, "dependency_graph")["status"], "ok");
    assert_eq!(
        doctor_check(&report, "inverted_verification_gates")["status"],
        "ok"
    );
    assert!(!report["has_errors"].as_bool().unwrap_or(true));

    // Neither bead left the frontier: the ring never gates readiness.
    let ready = ready_ids(workspace.path());
    assert!(
        ready.contains(&first) && ready.contains(&second),
        "a verifies ring must not move the frontier, got: {ready:?}"
    );
}

/// Poll the ready frontier while one edge commits and compare what the
/// reader saw inside the in-flight window against the post-commit state.
///
/// Returns the ready-flags observed in interior snapshots (started and
/// finished while the writer held its open transaction) plus the final
/// post-commit frontier. The interior list is asserted non-empty so the
/// window was genuinely raced rather than skipped.
struct WindowObservation {
    interior_ready: Vec<bool>,
    final_ready: Vec<String>,
}

fn observe_readiness_during(
    workspace: &tempfile::TempDir,
    blocked: &str,
    blocker: &str,
    kind: &'static str,
) -> WindowObservation {
    let path = db_path(workspace.path());
    let window_open = Arc::new(AtomicBool::new(false));
    let writer_done = Arc::new(AtomicBool::new(false));

    let writer = {
        let window_open = Arc::clone(&window_open);
        let writer_done = Arc::clone(&writer_done);
        let path = path.clone();
        let blocked = blocked.to_string();
        let blocker = blocker.to_string();
        std::thread::spawn(move || {
            let conn = open_configured_connection(&path).unwrap();
            let tx = Transaction::new_unchecked(&conn, TransactionBehavior::Immediate).unwrap();
            add_dependency_in_tx(&tx, &blocked, &blocker, kind, None).unwrap();
            // The edge row exists only inside this uncommitted transaction
            // for the whole window; every reader snapshot in it must reflect
            // the last committed state, never the in-flight row. The window
            // is generous because a snapshot only counts when it starts AND
            // finishes inside it, and under a loaded runner (cold extraction,
            // cgroup-limited) each `bead list` process can take tens of
            // milliseconds to spawn.
            window_open.store(true, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(500));
            window_open.store(false, Ordering::SeqCst);
            tx.commit().unwrap();
            writer_done.store(true, Ordering::SeqCst);
        })
    };

    let mut interior_ready = Vec::new();
    while !writer_done.load(Ordering::SeqCst) {
        let started_open = window_open.load(Ordering::SeqCst);
        let ready = ready_ids(workspace.path());
        let finished_open = window_open.load(Ordering::SeqCst);
        if started_open && finished_open {
            interior_ready.push(ready.iter().any(|id| id == blocked));
        }
    }
    writer.join().unwrap();

    let final_ready = ready_ids(workspace.path());
    assert!(
        !interior_ready.is_empty(),
        "no snapshot landed inside the in-flight window; the race never happened"
    );
    WindowObservation {
        interior_ready,
        final_ready,
    }
}

/// Readiness is computed from the committed edge set only, and only the
/// `blocks` kind ever moves the frontier.
///
/// While a `verifies` edge is in flight and after it commits, the verified
/// bead stays ready. While the `blocks` twin is in flight -- same pair, same
/// still-open blocker -- the bead is still ready, because the uncommitted row
/// is invisible; the moment it commits, the bead leaves the frontier. Every
/// interior snapshot of both phases therefore reads ready, and the frontier
/// flips exactly once, at the `blocks` commit.
#[test]
fn readiness_flips_only_at_the_blocks_commit_under_concurrent_readers() {
    let workspace = setup("vrd");
    let work = create(workspace.path(), "Implement the observed work");
    let verifier = create(workspace.path(), "Verify the observed work");

    // Phase 1: the declared check relationship commits under observation.
    let verifies = observe_readiness_during(&workspace, &work, &verifier, "verifies");
    assert!(
        verifies.interior_ready.iter().all(|ready| *ready),
        "an in-flight or committed verifies edge must never drop the bead, got: {:?}",
        verifies.interior_ready
    );
    assert!(
        verifies.final_ready.contains(&work),
        "the bead stays on the frontier after the verifies commit, got: {:?}",
        verifies.final_ready
    );

    // Phase 2: the gate commits under the same observation.
    let blocks = observe_readiness_during(&workspace, &work, &verifier, "blocks");
    assert!(
        blocks.interior_ready.iter().all(|ready| *ready),
        "an uncommitted blocks edge is invisible to readers, got: {:?}",
        blocks.interior_ready
    );
    assert!(
        !blocks.final_ready.contains(&work),
        "the committed blocks edge removes the bead, got: {:?}",
        blocks.final_ready
    );

    // Both edges committed exactly once each.
    assert_eq!(count_rows(workspace.path(), "verifies"), 1);
    assert_eq!(count_rows(workspace.path(), "blocks"), 1);
}

/// Small helper: assert the command succeeded, naming `what` on failure.
trait AssertOkExt {
    fn assert_ok(self, what: &str);
}

impl AssertOkExt for Output {
    fn assert_ok(self, what: &str) {
        assert!(self.status.success(), "{what} failed: {self:?}");
    }
}

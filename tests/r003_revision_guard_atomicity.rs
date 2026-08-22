//! R003 revision guard atomicity tests
//!
//! `--if-revision` is enforced *inside* the lifecycle write transaction,
//! against the snapshot the UPDATE lands on -- not against a read taken
//! before it. The multi-worker shared workspace is the supported model, and
//! a guard validated outside the transaction leaves a TOCTOU window: a
//! concurrent process commits between the read and the write, the guard
//! passes against the stale revision, and the unconditional UPDATE silently
//! overwrites the newer state -- the exact lost update the guard exists to
//! fail with exit 4.
//!
//! These tests hold two separately opened connections to one workspace:
//!
//! - the contract tests read a revision on connection A, commit a competing
//!   mutation on connection B (revision N -> N+1), and require every
//!   lifecycle op carrying `--if-revision N` on connection A to fail as a
//!   conflict (exit 4 semantics), never to succeed;
//! - the atomicity test pins the mechanism itself: connection B holds an
//!   *uncommitted* write transaction while connection A's mutation runs, so
//!   A's read sees revision N unless A waits for the write lock before
//!   reading. The guard must wait, re-read, and conflict -- not validate
//!   against the pre-commit snapshot and clobber B's change on commit.

use assert_cmd::Command;
use bead_rs::service::{close_issue, get_issue_by_id, release_issue, reopen_issue, update_issue};
use bead_rs::store::open_configured_connection;
use bead_rs::Error;
use rusqlite::{Transaction, TransactionBehavior};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn bead_binary() -> String {
    env!("CARGO_BIN_EXE_bead").to_string()
}

fn run(workspace: &Path, args: &[&str]) {
    let output = std::process::Command::new(bead_binary())
        .current_dir(workspace)
        .args(args)
        .output()
        .expect("failed to run bead");
    assert!(
        output.status.success(),
        "`bead {}` failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn create_workspace() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_bead"));
    cmd.current_dir(dir.path()).arg("init").assert().success();
    dir
}

/// Create an issue and return its ID.
fn create_issue(workspace: &Path, title: &str) -> String {
    let output = std::process::Command::new(bead_binary())
        .current_dir(workspace)
        .args(["create", "--title", title])
        .output()
        .expect("failed to run bead create");
    assert!(
        output.status.success(),
        "bead create failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

/// Open a second connection to the workspace database the way a concurrent
/// worker process would.
fn worker_connection(workspace: &Path) -> rusqlite::Connection {
    open_configured_connection(&workspace.join(".beads/beads.db")).unwrap()
}

/// The revision currently visible on `conn`.
fn revision_on(conn: &rusqlite::Connection, id: &str) -> i64 {
    get_issue_by_id(conn, id)
        .unwrap()
        .unwrap_or_else(|| panic!("issue {id} disappeared"))
        .revision
        .unwrap_or(1)
}

/// Commit a competing mutation on `conn`, advancing the revision by one.
fn concurrent_change(conn: &rusqlite::Connection, id: &str) {
    update_issue(
        conn,
        id,
        None,
        None,
        false,
        Some("concurrent worker change"),
        None,
        None,
    )
    .unwrap_or_else(|e| panic!("concurrent worker change failed: {e}"));
}

/// Assert the error is the revision-guard conflict: the `Conflict` variant,
/// the documented exit-4 mapping, and the recognized message.
fn assert_revision_conflict(context: &str, err: Error) {
    assert!(
        matches!(&err, Error::Conflict(_)),
        "{context}: expected a Conflict error, got {err:?}"
    );
    assert_eq!(
        err.exit_code(),
        4,
        "{context}: revision guard must map to exit 4"
    );
    assert!(
        err.to_string().contains("Revision mismatch"),
        "{context}: unrecognized conflict message: {err}"
    );
}

/// Connection A reads revision N; connection B commits a change (N -> N+1);
/// connection A's `update` with `--if-revision N` must conflict, and the
/// retry at the fresh revision must succeed.
#[test]
fn stale_revision_conflicts_on_update() {
    let workspace = create_workspace();
    let id = create_issue(workspace.path(), "update guard");
    let conn_a = worker_connection(workspace.path());
    let conn_b = worker_connection(workspace.path());

    let stale = revision_on(&conn_a, &id);
    concurrent_change(&conn_b, &id);

    let err = update_issue(
        &conn_a,
        &id,
        None,
        Some("worker-a"),
        false,
        None,
        Some(stale),
        None,
    )
    .unwrap_err();
    assert_revision_conflict("update with a stale revision", err);

    // The rejected mutation left no trace: the assignee write never landed.
    let issue = get_issue_by_id(&conn_a, &id).unwrap().unwrap();
    assert_eq!(issue.assignee, None, "the conflicting update was applied");

    // Retrying at the fresh revision succeeds, proving the guard rejects
    // only the stale precondition rather than the operation.
    update_issue(
        &conn_a,
        &id,
        None,
        Some("worker-a"),
        false,
        None,
        Some(stale + 1),
        None,
    )
    .expect("update at the fresh revision must succeed");
}

/// The same contract for `release`: the issue is claimed (in progress,
/// revision 2); B's change advances it to 3; A's release at revision 2
/// conflicts.
#[test]
fn stale_revision_conflicts_on_release() {
    let workspace = create_workspace();
    let id = create_issue(workspace.path(), "release guard");
    run(workspace.path(), &["claim", "--assignee", "worker"]);
    let conn_a = worker_connection(workspace.path());
    let conn_b = worker_connection(workspace.path());

    let stale = revision_on(&conn_a, &id);
    concurrent_change(&conn_b, &id);

    let err = release_issue(&conn_a, &id, Some(stale), None).unwrap_err();
    assert_revision_conflict("release with a stale revision", err);

    // The issue was not released behind the failed guard.
    let issue = get_issue_by_id(&conn_a, &id).unwrap().unwrap();
    assert!(
        issue.assignee.is_some(),
        "the conflicting release was applied"
    );
}

/// The same contract for `close`.
#[test]
fn stale_revision_conflicts_on_close() {
    let workspace = create_workspace();
    let id = create_issue(workspace.path(), "close guard");
    let conn_a = worker_connection(workspace.path());
    let conn_b = worker_connection(workspace.path());

    let stale = revision_on(&conn_a, &id);
    concurrent_change(&conn_b, &id);

    let err = close_issue(&conn_a, &id, "done", Some(stale), None).unwrap_err();
    assert_revision_conflict("close with a stale revision", err);

    let issue = get_issue_by_id(&conn_a, &id).unwrap().unwrap();
    assert!(
        issue.close_reason.is_none(),
        "the conflicting close was applied"
    );
}

/// The same contract for `reopen`: closed at revision 2, B advances it to 3,
/// A's reopen at revision 2 conflicts.
#[test]
fn stale_revision_conflicts_on_reopen() {
    let workspace = create_workspace();
    let id = create_issue(workspace.path(), "reopen guard");
    run(workspace.path(), &["close", &id, "--reason", "done"]);
    let conn_a = worker_connection(workspace.path());
    let conn_b = worker_connection(workspace.path());

    let stale = revision_on(&conn_a, &id);
    concurrent_change(&conn_b, &id);

    let err = reopen_issue(&conn_a, &id, Some(stale), None).unwrap_err();
    assert_revision_conflict("reopen with a stale revision", err);

    let issue = get_issue_by_id(&conn_a, &id).unwrap().unwrap();
    assert!(
        issue.close_reason.is_some(),
        "the conflicting reopen was applied"
    );
}

/// The CLI surfaces the same conflict as exit code 4 (README: lost updates
/// fail with exit 4), not as success or a generic failure.
#[test]
fn stale_revision_conflict_exits_4_through_the_cli() {
    let workspace = create_workspace();
    let id = create_issue(workspace.path(), "cli exit code");
    let conn_b = worker_connection(workspace.path());
    concurrent_change(&conn_b, &id);

    let output = std::process::Command::new(bead_binary())
        .current_dir(workspace.path())
        .args([
            "update",
            &id,
            "--assignee",
            "worker-a",
            "--if-revision",
            "1",
        ])
        .output()
        .expect("failed to run bead update");
    assert_eq!(
        output.status.code(),
        Some(4),
        "stale --if-revision must exit 4, stdout: {}, stderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("Revision mismatch"),
        "stderr must carry the revision mismatch: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The discriminating test for the atomicity fix. Connection B holds an
/// *uncommitted* write transaction that advances the revision; connection
/// A's guarded mutation starts while that change is in flight. A guard
/// validated against a pre-transaction read passes (WAL readers see only
/// committed state) and its UPDATE then lands on top of B's commit -- the
/// silent lost update. The fixed behavior waits for the write lock before
/// reading, so the guard sees B's committed revision and fails.
#[test]
fn guard_is_validated_inside_the_write_transaction() {
    let workspace = create_workspace();
    let id = create_issue(workspace.path(), "atomic guard");
    let conn_a = worker_connection(workspace.path());

    let writer_holds_lock = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&writer_holds_lock);
    let db_path = workspace.path().join(".beads/beads.db");
    let id_for_writer = id.clone();

    let writer = std::thread::spawn(move || {
        let conn_b = open_configured_connection(&db_path).unwrap();
        let tx = Transaction::new_unchecked(&conn_b, TransactionBehavior::Immediate)
            .expect("writer BEGIN IMMEDIATE");
        // From here to commit below, this connection holds the only write
        // lock on the database.
        flag.store(true, Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(300));
        tx.execute(
            "UPDATE issues SET revision = revision + 1 WHERE id = ?1",
            [&id_for_writer],
        )
        .unwrap();
        tx.commit().unwrap();
    });

    // Proceed only once the competing writer provably holds the write lock,
    // so connection A's mutation genuinely overlaps the in-flight change.
    let deadline = Instant::now() + Duration::from_secs(5);
    while !writer_holds_lock.load(Ordering::SeqCst) {
        assert!(
            Instant::now() < deadline,
            "concurrent writer never acquired the write lock"
        );
        std::thread::sleep(Duration::from_millis(5));
    }

    // A's read of revision 1 predates B's commit; only validating inside
    // the (immediate) write transaction turns that into a conflict instead
    // of a lost update.
    let err = update_issue(
        &conn_a,
        &id,
        None,
        Some("worker-a"),
        false,
        None,
        Some(1),
        None,
    )
    .unwrap_err();
    assert_revision_conflict("update racing an uncommitted writer", err);

    writer.join().expect("writer thread panicked");

    // B's change survived untouched: revision 2, no assignee from A.
    let issue = get_issue_by_id(&conn_a, &id).unwrap().unwrap();
    assert_eq!(issue.revision, Some(2), "the concurrent change was lost");
    assert_eq!(issue.assignee, None, "the racing update was applied");
}

//! Conformance tests for the shared pragma-configured connection opener.
//!
//! Every bead-rs code path that touches a workspace database -- the store
//! itself, the command handlers in the binary, and the doctor diagnostics --
//! opens through `bead_rs::store::open_configured_connection` (the handlers
//! reach it via `store::open_configured_connection` in main.rs and doctor.rs;
//! a repo-wide grep for raw `Connection::open` outside the helper guards
//! that routing). Both `foreign_keys` and `busy_timeout` are per-connection
//! SQLite defaults (OFF and 0), so the pragma configuration is observable
//! behavior, not an implementation detail: without it, the migrations'
//! `ON DELETE CASCADE` rules go unenforced and lock acquisition on a
//! concurrently written workspace fails immediately with `SQLITE_BUSY`
//! instead of waiting out the other writer.

use assert_cmd::Command;
use std::time::{Duration, Instant};
use tempfile::TempDir;

fn create_workspace() -> TempDir {
    let temp_dir = TempDir::new().unwrap();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["--skip-foreign-workspace", "init", "--prefix", "bead"])
        .current_dir(temp_dir.path())
        .assert()
        .success();

    temp_dir
}

/// The shared opener every handler uses must configure the connection the
/// way `SqliteStore` configures its own: `foreign_keys = ON` (1) and
/// `busy_timeout = 5000`, plus the WAL journal and `synchronous = NORMAL`
/// (1) the store relies on.
#[test]
fn test_open_configured_connection_applies_store_pragmas() {
    let temp_dir = create_workspace();
    let db_path = temp_dir.path().join(".beads/beads.db");

    let conn = bead_rs::store::open_configured_connection(&db_path).unwrap();

    let foreign_keys: i64 = conn
        .query_row("PRAGMA foreign_keys;", [], |row| row.get(0))
        .unwrap();
    assert_eq!(foreign_keys, 1, "foreign_keys must be ON");

    let busy_timeout: i64 = conn
        .query_row("PRAGMA busy_timeout;", [], |row| row.get(0))
        .unwrap();
    assert_eq!(busy_timeout, 5000, "busy_timeout must be 5000ms");

    let journal_mode: String = conn
        .query_row("PRAGMA journal_mode;", [], |row| row.get(0))
        .unwrap();
    assert_eq!(journal_mode, "wal", "journal_mode must be WAL");

    let synchronous: i64 = conn
        .query_row("PRAGMA synchronous;", [], |row| row.get(0))
        .unwrap();
    assert_eq!(synchronous, 1, "synchronous must be NORMAL (1)");
}

/// The `busy_timeout` the opener arms must be functionally in effect, not
/// just reported by the pragma: a write attempted while another connection
/// holds the WAL write lock waits out the lock and then succeeds, instead
/// of failing immediately with `SQLITE_BUSY` (the raw-connection behavior
/// this shared opener replaced). Both connections come from the same
/// opener the command handlers use.
///
/// Mutating handlers avoid SQLite's busy-handler exception for deferred
/// read-to-write upgrades by acquiring an immediate transaction before their
/// first read. The next test pins that handler-specific behavior separately.
#[test]
fn test_configured_connection_waits_for_concurrent_writer() {
    let temp_dir = create_workspace();
    let db_path = temp_dir.path().join(".beads/beads.db");

    let writer = bead_rs::store::open_configured_connection(&db_path).unwrap();
    writer
        .execute("CREATE TABLE IF NOT EXISTS pragma_probe (x INTEGER)", [])
        .unwrap();

    // Hold the WAL write lock from a second configured connection.
    let holder = bead_rs::store::open_configured_connection(&db_path).unwrap();
    holder.execute("BEGIN IMMEDIATE", []).unwrap();

    let start = Instant::now();
    let writer = std::thread::spawn(move || {
        // Fresh autocommit write: plain lock acquisition, the exact path
        // the busy timeout governs.
        writer
            .execute("INSERT INTO pragma_probe VALUES (42)", [])
            .unwrap();
        writer
            .query_row("SELECT COUNT(*) FROM pragma_probe", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap()
    });

    std::thread::sleep(Duration::from_secs(2));
    holder.execute("COMMIT", []).unwrap();
    drop(holder);

    let rows = writer.join().unwrap();
    assert_eq!(rows, 1, "the waiting write must have committed");

    let elapsed = start.elapsed();
    assert!(
        elapsed >= Duration::from_millis(1200),
        "write must have waited for the 2s lock hold, took {elapsed:?}"
    );
}

/// A mutating service handler must acquire its write lock before its first
/// read. Before the immediate-transaction fix, `add_label` began a deferred
/// transaction, read the issue, and then failed its INSERT immediately with
/// `SQLITE_BUSY`; SQLite does not invoke `busy_timeout` while upgrading an
/// existing read snapshot. With `BEGIN IMMEDIATE`, lock acquisition waits for
/// the configured timeout and the mutation proceeds after the holder commits.
#[test]
fn test_mutating_handler_waits_before_reading_under_write_contention() {
    let temp_dir = create_workspace();

    let issue_id = Command::cargo_bin("bead")
        .unwrap()
        .args(["--no-auto-flush", "create", "--title", "contention target"])
        .current_dir(temp_dir.path())
        .output()
        .unwrap();
    assert!(issue_id.status.success());
    let issue_id = String::from_utf8(issue_id.stdout).unwrap();
    let issue_id = issue_id.trim().to_string();

    let db_path = temp_dir.path().join(".beads/beads.db");
    let waiting = bead_rs::store::open_configured_connection(&db_path).unwrap();
    let holder = bead_rs::store::open_configured_connection(&db_path).unwrap();
    holder.execute("BEGIN IMMEDIATE", []).unwrap();

    let start = Instant::now();
    let waiter = std::thread::spawn(move || {
        let mut store = bead_rs::store::SqliteStore::from_conn(waiting);
        bead_rs::service::add_label(&mut store, &issue_id, "waited").unwrap();
        store
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM labels WHERE issue_id = ?1 AND label = 'waited'",
                [&issue_id],
                |row| row.get::<_, i64>(0),
            )
            .unwrap()
    });

    std::thread::sleep(Duration::from_secs(1));
    holder.execute("COMMIT", []).unwrap();
    drop(holder);

    assert_eq!(
        waiter.join().unwrap(),
        1,
        "the waiting mutation must commit"
    );
    let elapsed = start.elapsed();
    assert!(
        elapsed >= Duration::from_millis(700),
        "handler must have waited for the 1s write lock, took {elapsed:?}"
    );
}

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
        .args(["init", "--prefix", "bead"])
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
/// Known limit, deliberate out-of-scope: SQLite does not invoke the busy
/// handler when a connection that already holds a read snapshot (a
/// DEFERRED transaction that has read) attempts to upgrade to a write
/// transaction -- that path returns `SQLITE_BUSY` immediately regardless
/// of `busy_timeout`. The mutating handlers' `unchecked_transaction()`
/// calls are deferred, so handler-vs-handler write contention on that
/// path is a separate defect from the pragma routing these tests pin
/// down.
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

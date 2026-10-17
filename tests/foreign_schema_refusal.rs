//! A `.beads/beads.db` with data but without the bead-rs schema-identity
//! marker (the `schema_migrations`/`workspace` table pair every native
//! schema has carried since migration 1) must be refused by every
//! subcommand before any migration, probe, or repair touches it -- never
//! surfaced as a generic "no such column" error, and never silently
//! absorbed into a reinitialized schema.
//!
//! The fixture below is deliberately generic: arbitrary table names that
//! are not bead-rs's own and are not modeled on any real foreign
//! implementation (clean-room boundary -- the guard this test pins derives
//! purely from the *absence* of bead-rs's own fingerprint tables, never from
//! inspecting or naming another tool's actual schema).

use rusqlite::Connection;
use std::fs;
use std::path::{Path, PathBuf};

fn bead(dir: &Path) -> assert_cmd::Command {
    let mut cmd = assert_cmd::Command::cargo_bin("bead").unwrap();
    cmd.current_dir(dir);
    cmd
}

/// Build a workspace with a genuine bead-rs `.beads/config.json` (so
/// directory-level discovery (R030) succeeds and the walk reaches the
/// database-shape check this test exercises), then overwrite `beads.db`
/// with a foreign-shaped database: it has tables and data, but neither
/// `schema_migrations` nor `workspace`.
fn foreign_schema_workspace() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().to_path_buf();
    bead(&root)
        .args(["init", "--prefix", "fx"])
        .assert()
        .success();

    let db_path = root.join(".beads/beads.db");
    // WAL/SHM siblings from the real init would otherwise confuse the
    // byte-identity check below with files that aren't the database itself.
    let _ = fs::remove_file(root.join(".beads/beads.db-wal"));
    let _ = fs::remove_file(root.join(".beads/beads.db-shm"));
    fs::remove_file(&db_path).unwrap();

    let conn = Connection::open(&db_path).unwrap();
    conn.execute_batch(
        "CREATE TABLE ledger_entries (
             entry_id TEXT PRIMARY KEY,
             headline TEXT NOT NULL,
             state TEXT NOT NULL
         );
         CREATE TABLE tracker_meta (
             meta_key TEXT PRIMARY KEY,
             meta_value TEXT
         );
         INSERT INTO ledger_entries (entry_id, headline, state)
             VALUES ('e-1', 'pre-existing foreign record', 'open');
         INSERT INTO tracker_meta (meta_key, meta_value) VALUES ('fmt', 'other-tool-v3');",
    )
    .unwrap();
    drop(conn);

    (temp, root, db_path)
}

fn snapshot(db_path: &Path) -> Vec<u8> {
    fs::read(db_path).unwrap()
}

/// Every subcommand that touches the workspace must refuse a foreign-shaped
/// database -- read or mutating -- and must do so without writing a single
/// byte to it.
#[test]
fn all_subcommands_refuse_a_foreign_shaped_database_with_zero_writes() {
    let (_temp, root, db_path) = foreign_schema_workspace();
    let before = snapshot(&db_path);

    let refusing_invocations: &[&[&str]] = &[
        &["list"],
        &["show", "fx-1"],
        &["create", "--title", "should never land"],
        &["update", "fx-1", "--notes", "should never land"],
        &["close", "fx-1", "--reason", "should never land"],
        &["claim", "--assignee", "someone"],
    ];

    for args in refusing_invocations {
        let assert = bead(&root).args(*args).assert().failure();
        let output = assert.get_output();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("schema-identity marker") || stderr.contains("bead-rs"),
            "refusal for `bead {}` did not name the mismatch: {}",
            args.join(" "),
            stderr
        );

        let after = snapshot(&db_path);
        assert_eq!(
            before,
            after,
            "`bead {}` wrote to a foreign-shaped database",
            args.join(" ")
        );
    }
}

/// `init` must never rebuild a schema into the foreign-shaped file.
#[test]
fn init_refuses_rather_than_rebuilding_a_foreign_shaped_database() {
    let (_temp, root, db_path) = foreign_schema_workspace();
    let before = snapshot(&db_path);

    bead(&root).args(["init"]).assert().failure();

    let after = snapshot(&db_path);
    assert_eq!(
        before, after,
        "`bead init` wrote to a foreign-shaped database instead of refusing it"
    );
}

/// `bead doctor` must diagnose the mismatch, not open the database.
#[test]
fn doctor_diagnoses_a_foreign_shaped_database_without_opening_it() {
    let (_temp, root, db_path) = foreign_schema_workspace();
    let before = snapshot(&db_path);

    let assert = bead(&root).args(["doctor"]).assert().failure();
    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("FAIL workspace_config"),
        "doctor did not report a workspace_config failure: {}",
        stdout
    );
    assert!(
        stdout.contains("schema-identity marker"),
        "doctor's diagnostic did not name the schema-identity marker: {}",
        stdout
    );

    let after = snapshot(&db_path);
    assert_eq!(
        before, after,
        "`bead doctor` wrote to a foreign-shaped database while diagnosing it"
    );
}

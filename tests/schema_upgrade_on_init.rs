//! Upgrading the binary must not strand existing workspaces at an old schema.
//!
//! `bead init` used to return early whenever a workspace was already Ready,
//! so nothing in the CLI ever applied a pending migration to an existing
//! store. Installing a newer binary therefore left every workspace short of
//! the tables the new code queries, and the failure surfaced later as a bare
//! `no such table` from whichever command touched one first -- with no
//! command available to repair it.

use assert_cmd::Command;
use rusqlite::Connection;
use tempfile::TempDir;

fn schema_version(db: &Connection) -> i64 {
    db.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        [],
        |row| row.get(0),
    )
    .unwrap()
}

fn table_exists(db: &Connection, name: &str) -> bool {
    db.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [name],
        |row| row.get::<_, i64>(0),
    )
    .unwrap()
        > 0
}

#[test]
fn init_applies_pending_migrations_to_an_existing_workspace() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();

    Command::cargo_bin("bead")
        .unwrap()
        .current_dir(root)
        .args(["init", "--prefix", "upg"])
        .assert()
        .success();

    // A bead created before the simulated downgrade must survive the upgrade.
    Command::cargo_bin("bead")
        .unwrap()
        .current_dir(root)
        .args(["create", "--title", "predates the upgrade"])
        .assert()
        .success();

    let db_path = root.join(".beads/beads.db");
    let current = {
        let db = Connection::open(&db_path).unwrap();
        let v = schema_version(&db);
        assert!(v > 1, "fixture should start at a real schema version");
        assert!(table_exists(&db, "issue_resource_keys"));

        // Simulate a store written by an older binary: drop a table a later
        // migration introduced and roll the recorded version back behind it.
        db.execute_batch(
            "DROP TABLE issue_resource_keys;
             DELETE FROM schema_migrations WHERE version >= 10;",
        )
        .unwrap();
        assert!(!table_exists(&db, "issue_resource_keys"));
        v
    };

    // Re-running init on the Ready workspace must bring the schema forward.
    Command::cargo_bin("bead")
        .unwrap()
        .current_dir(root)
        .args(["init"])
        .assert()
        .success()
        .stderr(predicates::str::contains("Applied pending migrations"));

    let db = Connection::open(&db_path).unwrap();
    assert_eq!(
        schema_version(&db),
        current,
        "init must restore the store to the binary's current schema version"
    );
    assert!(
        table_exists(&db, "issue_resource_keys"),
        "the table a later migration introduced must be recreated"
    );

    let issues: i64 = db
        .query_row("SELECT COUNT(*) FROM issues", [], |row| row.get(0))
        .unwrap();
    assert_eq!(issues, 1, "migrating must not disturb existing rows");
}

#[test]
fn init_reports_an_already_current_schema_without_changing_it() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();

    Command::cargo_bin("bead")
        .unwrap()
        .current_dir(root)
        .args(["init", "--prefix", "upg"])
        .assert()
        .success();

    let before = schema_version(&Connection::open(root.join(".beads/beads.db")).unwrap());

    Command::cargo_bin("bead")
        .unwrap()
        .current_dir(root)
        .args(["init"])
        .assert()
        .success()
        .stderr(predicates::str::contains("Schema up to date"));

    let after = schema_version(&Connection::open(root.join(".beads/beads.db")).unwrap());
    assert_eq!(before, after);
}

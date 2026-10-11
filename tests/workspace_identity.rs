//! Regression coverage for migration-era workspace identity repair.

use assert_cmd::Command;
use predicates::prelude::*;
use rusqlite::Connection;
use serial_test::serial;
use std::fs;
use tempfile::TempDir;

fn run(workspace: &TempDir, args: &[&str]) {
    Command::cargo_bin("bead")
        .unwrap()
        .args(args)
        .current_dir(workspace.path())
        .env("HOME", workspace.path())
        .assert()
        .success();
}

fn event_identities(workspace: &TempDir) -> Vec<(String, i64)> {
    let checkpoint =
        fs::read_to_string(workspace.path().join(".beads/checkpoint/forensic.jsonl")).unwrap();
    checkpoint
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter_map(|record| record.get("event").cloned())
        .map(|event| {
            (
                event["origin_store_uuid"].as_str().unwrap().to_string(),
                event["origin_event_sequence"].as_i64().unwrap(),
            )
        })
        .collect()
}

#[test]
#[serial]
fn doctor_repairs_empty_uuid_without_changing_published_event_identity() {
    let workspace = tempfile::tempdir().unwrap();
    run(&workspace, &["init", "--skip-foreign-workspace"]);

    // Reproduce the affected path before the first mutation: the checkpoint
    // will intentionally carry the empty origin UUID.
    let db = workspace.path().join(".beads/beads.db");
    let conn = Connection::open(db).unwrap();
    conn.execute("UPDATE workspace SET uuid = ''", []).unwrap();
    drop(conn);
    run(&workspace, &["create", "--title", "legacy identity"]);
    run(&workspace, &["sync", "flush-only"]);
    let before = event_identities(&workspace);
    assert!(!before.is_empty());
    assert!(before.iter().all(|(uuid, _)| uuid.is_empty()));

    Command::cargo_bin("bead")
        .unwrap()
        .args(["doctor", "--repair"])
        .current_dir(workspace.path())
        .env("HOME", workspace.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("workspace_uuid_repair"));

    let after_repair_db = Connection::open(workspace.path().join(".beads/beads.db")).unwrap();
    let repaired_uuid: String = after_repair_db
        .query_row("SELECT uuid FROM workspace WHERE id = 1", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert!(!repaired_uuid.is_empty());
    drop(after_repair_db);

    run(&workspace, &["sync", "flush-only"]);
    assert_eq!(before, event_identities(&workspace));
}

#[test]
#[serial]
fn doctor_fails_on_nonempty_database_config_uuid_divergence() {
    let workspace = tempfile::tempdir().unwrap();
    run(&workspace, &["init", "--skip-foreign-workspace"]);
    let db = workspace.path().join(".beads/beads.db");
    let conn = Connection::open(db).unwrap();
    conn.execute(
        "UPDATE workspace SET uuid = '11111111-1111-4111-8111-111111111111'",
        [],
    )
    .unwrap();
    drop(conn);

    Command::cargo_bin("bead")
        .unwrap()
        .args(["doctor", "--format", "json"])
        .current_dir(workspace.path())
        .env("HOME", workspace.path())
        .assert()
        .failure()
        .code(5)
        .stdout(predicate::str::contains("uuid_divergence"))
        .stdout(predicate::str::contains("differs from config.json UUID"));
}

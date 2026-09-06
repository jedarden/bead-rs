//! BR-T17 exceptional publication and recovery-precedence conformance.

use assert_cmd::Command;
use bead_rs::model::redaction::REDACTION_MARKER;
use bead_rs::service::secret_diagnostics::scan_live_findings;
use rusqlite::params;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

fn bead(workspace: &Path) -> Command {
    let mut command = Command::cargo_bin("bead").unwrap();
    command.current_dir(workspace).env("HOME", workspace);
    command
}

fn temp_workspace(name: &str) -> tempfile::TempDir {
    let workspace = tempfile::Builder::new()
        .prefix(&format!("bead-redaction-publication-{name}-"))
        .tempdir_in("/var/tmp")
        .unwrap();
    bead(workspace.path())
        .args(["init", "--prefix", "redact", "--no-auto-flush"])
        .assert()
        .success();
    workspace
}

fn database(root: &Path) -> PathBuf {
    root.join(".beads/beads.db")
}

fn shaped_value() -> String {
    ["AK", "IA", "7Q9W2E4R6T8Y1U3I"].concat()
}

fn insert_issue(root: &Path, id: &str, description: &str) {
    let conn = rusqlite::Connection::open(database(root)).unwrap();
    conn.execute(
        "INSERT INTO issues (
            id, title, description, notes, priority, issue_type, base_status,
            created_at, updated_at, revision
         ) VALUES (?1, 'stable title', ?2, 'stable notes', 2, 'task', 'open',
                   '2026-09-03T00:00:00Z', '2026-09-03T00:00:00Z', 1)",
        params![id, description],
    )
    .unwrap();
}

fn finding(root: &Path) -> String {
    let conn = rusqlite::Connection::open(database(root)).unwrap();
    scan_live_findings(&conn)
        .unwrap()
        .into_iter()
        .find(|finding| finding.rule_id == "aws-access-key-id" && finding.is_blocking_match())
        .expect("fixture must produce one live blocking finding")
        .fingerprint
}

fn force_mode(root: &Path, mode: &str) {
    let path = root.join(".beads/config.json");
    let mut config: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    config["checkpoint"]["mode"] = Value::String(mode.to_string());
    fs::write(path, serde_json::to_vec_pretty(&config).unwrap()).unwrap();
}

fn pointer(root: &Path, name: &str) -> Value {
    serde_json::from_slice(&fs::read(root.join(".beads/checkpoint").join(name)).unwrap()).unwrap()
}

fn read_tree(root: &Path, bytes: &mut Vec<u8>) {
    for entry in fs::read_dir(root).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            read_tree(&entry.path(), bytes);
        } else if entry.file_type().unwrap().is_file() {
            bytes.extend(fs::read(entry.path()).unwrap());
        }
    }
}

fn copy_tree(source: &Path, target: &Path) {
    fs::create_dir_all(target).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let destination = target.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &destination);
        } else if entry.file_type().unwrap().is_file() {
            fs::copy(entry.path(), destination).unwrap();
        }
    }
}

fn assert_publication(mode: &str) {
    let workspace = temp_workspace(mode);
    force_mode(workspace.path(), mode);
    let secret = shaped_value();
    insert_issue(
        workspace.path(),
        "redact-publish",
        &format!("before {secret} after"),
    );
    bead(workspace.path())
        .args(["sync", "flush-only"])
        .assert()
        .success();

    let dirty_pointer = pointer(workspace.path(), "current.json");
    let dirty_generation = dirty_pointer["generation_id"].as_str().unwrap().to_string();
    let dirty_root = dirty_pointer["active_root"]["path"]
        .as_str()
        .unwrap()
        .to_string();
    let fingerprint = finding(workspace.path());
    let checkpoint_before =
        fs::read(workspace.path().join(".beads/checkpoint/current.json")).unwrap();
    let conn = rusqlite::Connection::open(database(workspace.path())).unwrap();
    let before: (String, i64, i64) = conn
        .query_row(
            "SELECT description, revision,
                    (SELECT COUNT(*) FROM redaction_receipts)
             FROM issues WHERE id = 'redact-publish'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    drop(conn);

    let dry_run = bead(workspace.path())
        .args([
            "redact",
            "--finding",
            &fingerprint,
            "--actor",
            "publication-test",
            "--reason",
            "remove historical fixture",
            "--dry-run",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(
        dry_run.status.success(),
        "{}",
        String::from_utf8_lossy(&dry_run.stderr)
    );
    assert!(!dry_run
        .stdout
        .windows(secret.len())
        .any(|window| window == secret.as_bytes()));
    let preview: Value = serde_json::from_slice(&dry_run.stdout).unwrap();
    assert_eq!(preview["finding_fingerprint"], fingerprint);
    assert_eq!(preview["previous_generation_reset"], true);
    let conn = rusqlite::Connection::open(database(workspace.path())).unwrap();
    let after_preview: (String, i64, i64) = conn
        .query_row(
            "SELECT description, revision,
                    (SELECT COUNT(*) FROM redaction_receipts)
             FROM issues WHERE id = 'redact-publish'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    drop(conn);
    assert_eq!(after_preview, before);
    assert_eq!(
        fs::read(workspace.path().join(".beads/checkpoint/current.json")).unwrap(),
        checkpoint_before
    );

    let redaction = bead(workspace.path())
        .args([
            "redact",
            "--finding",
            &fingerprint,
            "--actor",
            "publication-test",
            "--reason",
            "remove historical fixture",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(
        redaction.status.success(),
        "{}",
        String::from_utf8_lossy(&redaction.stderr)
    );
    assert!(!redaction
        .stdout
        .windows(secret.len())
        .any(|window| window == secret.as_bytes()));
    let receipt: Value = serde_json::from_slice(&redaction.stdout).unwrap();
    assert_eq!(receipt["publication_state"], "published");
    let receipt_id = receipt["receipt_id"].as_str().unwrap();

    let current = pointer(workspace.path(), "current.json");
    let previous = pointer(workspace.path(), "previous.json");
    assert_eq!(current["mode"], mode);
    assert_eq!(current["generation_id"], previous["generation_id"]);
    assert_eq!(current["active_root"], previous["active_root"]);
    assert_eq!(current["previous_generation_reset"], true);
    assert_eq!(previous["previous_generation_reset"], true);
    assert!(current["superseded_generations"]
        .as_array()
        .unwrap()
        .iter()
        .any(|generation| generation == &dirty_generation));
    assert_eq!(receipt["resulting_generation_id"], current["generation_id"]);
    assert!(!workspace
        .path()
        .join(".beads/checkpoint")
        .join(dirty_root)
        .exists());

    let mut checkpoint_bytes = Vec::new();
    read_tree(
        &workspace.path().join(".beads/checkpoint"),
        &mut checkpoint_bytes,
    );
    assert!(!checkpoint_bytes
        .windows(secret.len())
        .any(|window| window == secret.as_bytes()));
    assert!(!fs::read_dir(workspace.path().join(".beads"))
        .unwrap()
        .filter_map(Result::ok)
        .any(|entry| entry
            .file_name()
            .to_string_lossy()
            .starts_with(".redaction-publish-")));

    let conn = rusqlite::Connection::open(database(workspace.path())).unwrap();
    let published: (String, i64, i64) = conn
        .query_row(
            "SELECT publication_state,
                    (SELECT revision FROM issues WHERE id = 'redact-publish'),
                    (SELECT COUNT(*) FROM events WHERE kind = 'historical_redaction')
             FROM redaction_receipts WHERE receipt_id = ?1",
            [receipt_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    drop(conn);
    assert_eq!(published, ("published".to_string(), 2, 1));

    let current_before_resume =
        fs::read(workspace.path().join(".beads/checkpoint/current.json")).unwrap();
    let resumed = bead(workspace.path())
        .args(["redact", "--resume", receipt_id, "--json"])
        .output()
        .unwrap();
    assert!(resumed.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&resumed.stdout).unwrap(),
        receipt
    );
    assert_eq!(
        fs::read(workspace.path().join(".beads/checkpoint/current.json")).unwrap(),
        current_before_resume
    );
}

#[test]
fn monolithic_redaction_resets_the_retained_generation_set() {
    assert_publication("monolithic");
}

#[test]
fn sharded_redaction_resets_the_retained_generation_set() {
    assert_publication("sharded");
}

#[test]
fn resume_publishes_an_already_committed_semantic_redaction() {
    let workspace = temp_workspace("resume");
    let secret = shaped_value();
    insert_issue(workspace.path(), "redact-resume", &secret);
    bead(workspace.path())
        .args(["sync", "flush-only"])
        .assert()
        .success();
    let fingerprint = finding(workspace.path());
    let mut store = bead_rs::store::SqliteStore::from_conn(
        bead_rs::store::open_configured_connection(&database(workspace.path())).unwrap(),
    );
    let committed = bead_rs::service::redact_finding(
        &mut store,
        workspace.path(),
        &fingerprint,
        "publication-test",
        "resume committed fixture",
    )
    .unwrap();
    assert_eq!(committed.receipt.publication_state.as_str(), "committed");
    let receipt_id = committed.receipt.receipt_id.clone();
    drop(store);

    let resumed = bead(workspace.path())
        .args(["redact", "--resume", &receipt_id, "--json"])
        .output()
        .unwrap();
    assert!(
        resumed.status.success(),
        "{}",
        String::from_utf8_lossy(&resumed.stderr)
    );
    let receipt: Value = serde_json::from_slice(&resumed.stdout).unwrap();
    assert_eq!(receipt["receipt_id"], receipt_id);
    assert_eq!(receipt["publication_state"], "published");
}

#[test]
fn old_restore_and_newer_merge_cannot_resurrect_redacted_bytes() {
    let workspace = temp_workspace("anti-resurrection");
    let secret = shaped_value();
    insert_issue(workspace.path(), "redact-recovery", &secret);
    bead(workspace.path())
        .args(["sync", "flush-only"])
        .assert()
        .success();
    let old_generation = pointer(workspace.path(), "current.json")["generation_id"]
        .as_str()
        .unwrap()
        .to_string();
    let archive = workspace.path().join("old-checkpoint");
    copy_tree(&workspace.path().join(".beads/checkpoint"), &archive);
    let stale_source = temp_workspace("stale-source");
    fs::remove_dir_all(stale_source.path().join(".beads")).unwrap();
    copy_tree(
        &workspace.path().join(".beads"),
        &stale_source.path().join(".beads"),
    );

    let fingerprint = finding(workspace.path());
    bead(workspace.path())
        .args([
            "redact",
            "--finding",
            &fingerprint,
            "--actor",
            "publication-test",
            "--reason",
            "protect recovery boundary",
        ])
        .assert()
        .success();

    let restore = bead(workspace.path())
        .args([
            "restore",
            "--source",
            archive.to_str().unwrap(),
            "--generation",
            &old_generation,
            "--actor",
            "publication-test",
            "--allow-non-empty",
            "--no-auto-flush",
        ])
        .output()
        .unwrap();
    assert!(!restore.status.success());
    assert!(String::from_utf8_lossy(&restore.stderr)
        .contains("would discard known historical-redaction state"));

    let conn = rusqlite::Connection::open(database(stale_source.path())).unwrap();
    conn.execute(
        "UPDATE issues
         SET revision = 3, updated_at = '2026-09-03T01:00:00Z'
         WHERE id = 'redact-recovery'",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO events (
            issue_id, kind, actor, time, detail, origin_store_uuid,
            origin_event_sequence, event_sha256
         ) VALUES ('redact-recovery', 'stale_fixture', 'publication-test',
                   '2026-09-03T01:00:00Z', '{}', 'stale-fixture', 1, ?1)",
        ["e".repeat(64)],
    )
    .unwrap();
    drop(conn);
    bead(stale_source.path())
        .args(["sync", "flush-only"])
        .assert()
        .success();
    let source_pointer = pointer(stale_source.path(), "current.json");
    let source_root = stale_source
        .path()
        .join(".beads/checkpoint")
        .join(source_pointer["active_root"]["path"].as_str().unwrap());
    let source_issue = fs::read_to_string(source_root)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .find(|record| record["record_type"] == "issue")
        .unwrap();
    assert_eq!(source_issue["issue"]["revision"], 3);
    assert_eq!(source_issue["issue"]["updated_at"], "2026-09-03T01:00:00Z");

    let merge = bead(workspace.path())
        .args([
            "sync",
            "import-only",
            "--input",
            stale_source
                .path()
                .join(".beads/checkpoint")
                .to_str()
                .unwrap(),
            "--merge",
            "--actor",
            "publication-test",
            "--no-auto-flush",
        ])
        .output()
        .unwrap();
    if merge.status.success() {
        let conn = rusqlite::Connection::open(database(workspace.path())).unwrap();
        let (description, revision, tombstones): (String, i64, i64) = conn
            .query_row(
                "SELECT description, revision,
                        (SELECT COUNT(*) FROM redaction_tombstones)
                 FROM issues WHERE id = 'redact-recovery'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        let fingerprint_live = scan_live_findings(&conn)
            .unwrap()
            .iter()
            .any(|finding| finding.fingerprint == fingerprint);
        panic!(
            "merge unexpectedly succeeded: revision={revision}, secret_live={}, tombstones={tombstones}, fingerprint_live={fingerprint_live}",
            description.contains(&secret)
        );
    }
    assert!(
        String::from_utf8_lossy(&merge.stderr).contains("matches historical-redaction tombstone")
    );

    let conn = rusqlite::Connection::open(database(workspace.path())).unwrap();
    let state: (String, i64, i64) = conn
        .query_row(
            "SELECT description, revision,
                    (SELECT COUNT(*) FROM redaction_receipts)
             FROM issues WHERE id = 'redact-recovery'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(state, (REDACTION_MARKER.to_string(), 2, 1));
}

#[test]
fn redact_is_discoverable_and_cannot_disable_publication() {
    let help = bead(std::env::current_dir().unwrap().as_path())
        .arg("--help")
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&help.stdout).contains("redact"));
    let redact_help = bead(std::env::current_dir().unwrap().as_path())
        .args(["redact", "--help"])
        .output()
        .unwrap();
    let redact_help = String::from_utf8_lossy(&redact_help.stdout);
    assert!(redact_help.contains("--finding"));
    assert!(redact_help.contains("--resume"));

    let capabilities: Value = serde_json::from_slice(
        &bead(std::env::current_dir().unwrap().as_path())
            .arg("capabilities")
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    assert!(capabilities["commands"]
        .as_array()
        .unwrap()
        .iter()
        .any(|command| command == "redact"));
    for field in [
        "atomic_redact",
        "anti_resurrection",
        "sanitized_generation_set",
        "resumable_publication",
    ] {
        assert_eq!(capabilities["historical_redaction"][field], true);
    }

    let workspace = temp_workspace("mandatory-publication");
    bead(workspace.path())
        .args([
            "--no-auto-flush",
            "redact",
            "--finding",
            &"a".repeat(64),
            "--actor",
            "publication-test",
            "--reason",
            "must publish",
        ])
        .assert()
        .code(2);
}

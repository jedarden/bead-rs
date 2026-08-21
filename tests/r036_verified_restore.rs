//! R036 conformance: first-class verified restore.
//!
//! The command selects a named pointer generation, verifies the complete
//! content-addressed closure before target mutation, guards non-empty targets,
//! attributes the actor, and refuses explicitly non-importable R029 views.

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn bead(dir: &Path) -> Command {
    let mut command = Command::cargo_bin("bead").unwrap();
    command.current_dir(dir);
    command.arg("--skip-foreign-workspace");
    command
}

fn run(dir: &Path, args: &[&str]) -> std::process::Output {
    bead(dir).args(args).assert().success().get_output().clone()
}

fn create_issue(dir: &Path, title: &str) -> String {
    String::from_utf8(run(dir, &["create", "--title", title]).stdout)
        .unwrap()
        .trim()
        .to_string()
}

struct Source {
    _dir: TempDir,
    workspace: PathBuf,
    checkpoint: PathBuf,
    generation: String,
    issue_id: String,
    pointer: Value,
}

fn source_checkpoint() -> Source {
    source_checkpoint_with_mode(false)
}

fn source_checkpoint_with_mode(sharded: bool) -> Source {
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path().to_path_buf();
    run(&workspace, &["init", "--prefix", "recover"]);
    if sharded {
        let config_path = workspace.join(".beads/config.json");
        let mut config: Value = serde_json::from_slice(&fs::read(&config_path).unwrap()).unwrap();
        config["checkpoint"] = serde_json::json!({ "mode": "sharded" });
        fs::write(config_path, serde_json::to_vec_pretty(&config).unwrap()).unwrap();
    }
    let issue_id = create_issue(&workspace, "survives verified restore");
    run(&workspace, &["sync", "flush-only"]);
    let checkpoint = workspace.join(".beads/checkpoint");
    let pointer: Value =
        serde_json::from_slice(&fs::read(checkpoint.join("current.json")).unwrap()).unwrap();
    let generation = pointer["generation_id"].as_str().unwrap().to_string();
    Source {
        _dir: dir,
        workspace,
        checkpoint,
        generation,
        issue_id,
        pointer,
    }
}

fn restore_args(source: &Source) -> Vec<String> {
    vec![
        "restore".into(),
        "--source".into(),
        source.checkpoint.display().to_string(),
        "--generation".into(),
        source.generation.clone(),
        "--actor".into(),
        "recovery-operator".into(),
        "--format".into(),
        "json".into(),
    ]
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let source = entry.path();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&source, &target);
        } else {
            fs::copy(source, target).unwrap();
        }
    }
}

#[test]
fn empty_target_is_initialized_verified_restored_and_receipted_by_one_command() {
    let source = source_checkpoint();
    let target = tempfile::tempdir().unwrap();
    let args = restore_args(&source);
    let output = run(
        target.path(),
        &args.iter().map(String::as_str).collect::<Vec<_>>(),
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();

    assert_eq!(report["generation_id"], source.generation);
    assert_eq!(
        report["source_root_sha256"],
        source.pointer["active_root"]["sha256"]
    );
    assert_eq!(report["actor"], "recovery-operator");
    assert_eq!(report["issues_restored"], 1);
    assert_eq!(report["events_restored"], source.pointer["event_count"]);
    assert_eq!(report["provenance_receipts_restored"], 0);
    assert_eq!(report["non_empty_override"], false);
    assert!(report["restore_receipt_id"]
        .as_str()
        .unwrap()
        .starts_with("restore-"));

    let listed = run(target.path(), &["list", "--json", "--limit", "999"]);
    assert!(String::from_utf8(listed.stdout)
        .unwrap()
        .contains(&source.issue_id));

    let conn = rusqlite::Connection::open(target.path().join(".beads/beads.db")).unwrap();
    let receipt: (String, String, String, String) = conn
        .query_row(
            "SELECT actor, source_root_sha256, kind, summary_event_identity
             FROM provenance_receipts WHERE receipt_id = ?1",
            [report["restore_receipt_id"].as_str().unwrap()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(receipt.0, "recovery-operator");
    assert_eq!(receipt.1, report["source_root_sha256"]);
    assert_eq!(receipt.2, "restore");
    assert!(receipt.3.starts_with("local-"));
}

#[test]
fn retained_generation_named_native_root_is_hash_verified_and_restored() {
    let source = source_checkpoint();
    let copied = tempfile::tempdir().unwrap();
    let checkpoint = copied.path().join("checkpoint");
    copy_tree(&source.checkpoint, &checkpoint);

    let mut pointer = source.pointer.clone();
    let current_root = pointer["active_root"]["path"].as_str().unwrap();
    let legacy_root = format!("objects/{}.jsonl", source.generation);
    fs::copy(checkpoint.join(current_root), checkpoint.join(&legacy_root)).unwrap();
    pointer["active_root"]["path"] = Value::String(legacy_root);
    fs::write(
        checkpoint.join("current.json"),
        serde_json::to_vec_pretty(&pointer).unwrap(),
    )
    .unwrap();

    let target = tempfile::tempdir().unwrap();
    let args = [
        "restore",
        "--source",
        checkpoint.to_str().unwrap(),
        "--generation",
        &source.generation,
        "--actor",
        "recovery-operator",
        "--format",
        "json",
    ];
    let report: Value = serde_json::from_slice(&run(target.path(), &args).stdout).unwrap();
    assert_eq!(report["generation_id"], source.generation);
    assert_eq!(
        report["source_root_sha256"],
        source.pointer["active_root"]["sha256"]
    );
}

#[test]
fn sharded_generation_verifies_its_complete_closure_and_restores() {
    let source = source_checkpoint_with_mode(true);
    assert_eq!(source.pointer["mode"], "sharded");
    let target = tempfile::tempdir().unwrap();
    let args = restore_args(&source);
    let report: Value = serde_json::from_slice(
        &run(
            target.path(),
            &args.iter().map(String::as_str).collect::<Vec<_>>(),
        )
        .stdout,
    )
    .unwrap();
    assert_eq!(report["mode"], "sharded");
    assert_eq!(report["issues_restored"], 1);
}

#[test]
fn non_empty_target_is_refused_without_mutation() {
    let source = source_checkpoint();
    let target = tempfile::tempdir().unwrap();
    run(target.path(), &["init", "--prefix", "target"]);
    let target_issue = create_issue(target.path(), "must not be displaced implicitly");
    let before = fs::read(target.path().join(".beads/beads.db")).unwrap();

    let args = restore_args(&source);
    bead(target.path())
        .args(&args)
        .assert()
        .failure()
        .stderr(predicate::str::contains("Target database is not empty"))
        .stderr(predicate::str::contains("--allow-non-empty"));

    // SQLite/WAL bytes are not a stable semantic snapshot, so verify both the
    // target row and absence of a restore receipt. The byte read above also
    // ensures the database existed before the refusal.
    assert!(!before.is_empty());
    let listed = run(target.path(), &["list", "--json", "--limit", "999"]);
    assert!(String::from_utf8(listed.stdout)
        .unwrap()
        .contains(&target_issue));
    let conn = rusqlite::Connection::open(target.path().join(".beads/beads.db")).unwrap();
    let receipts: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM provenance_receipts WHERE kind = 'restore'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(receipts, 0);
}

#[test]
fn explicit_non_empty_override_replaces_native_state_and_reports_displacement() {
    let source = source_checkpoint();
    let target = tempfile::tempdir().unwrap();
    run(target.path(), &["init", "--prefix", "target"]);
    let displaced_id = create_issue(target.path(), "explicitly displaced");
    let conn = rusqlite::Connection::open(target.path().join(".beads/beads.db")).unwrap();
    conn.execute("CREATE TABLE operator_extension (value TEXT NOT NULL)", [])
        .unwrap();
    conn.execute(
        "INSERT INTO operator_extension (value) VALUES ('preserved')",
        [],
    )
    .unwrap();
    drop(conn);

    let mut args = restore_args(&source);
    args.push("--allow-non-empty".into());
    let output = run(
        target.path(),
        &args.iter().map(String::as_str).collect::<Vec<_>>(),
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["non_empty_override"], true);
    assert_eq!(report["displaced"]["issues"], 1);
    assert!(report["displaced"]["events"].as_u64().unwrap() >= 1);

    let listed =
        String::from_utf8(run(target.path(), &["list", "--json", "--limit", "999"]).stdout)
            .unwrap();
    assert!(listed.contains(&source.issue_id));
    assert!(!listed.contains(&displaced_id));

    let conn = rusqlite::Connection::open(target.path().join(".beads/beads.db")).unwrap();
    let extension: String = conn
        .query_row("SELECT value FROM operator_extension", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        extension, "preserved",
        "override must not drop unknown tables"
    );
    drop(conn);

    // The replacement keeps SQLite's live sequence monotonic while export
    // renumbers local origin identities continuously. Prove the resulting
    // generation is itself a valid future recovery source.
    run(target.path(), &["sync", "flush-only"]);
    let pointer: Value = serde_json::from_slice(
        &fs::read(target.path().join(".beads/checkpoint/current.json")).unwrap(),
    )
    .unwrap();
    let generation = pointer["generation_id"].as_str().unwrap();
    let replay = tempfile::tempdir().unwrap();
    run(
        replay.path(),
        &[
            "restore",
            "--source",
            target.path().join(".beads/checkpoint").to_str().unwrap(),
            "--generation",
            generation,
            "--actor",
            "replay-operator",
        ],
    );
}

#[test]
fn simultaneous_empty_target_restores_have_one_winner_and_one_refusal() {
    let source = source_checkpoint();
    let target = tempfile::tempdir().unwrap();
    run(target.path(), &["init", "--prefix", "target"]);
    let args = restore_args(&source);

    let spawn = || {
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_bead"));
        command
            .current_dir(target.path())
            .arg("--skip-foreign-workspace")
            .args(&args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        command.spawn().unwrap()
    };
    let first = spawn();
    let second = spawn();
    let outputs = [
        first.wait_with_output().unwrap(),
        second.wait_with_output().unwrap(),
    ];

    assert_eq!(
        outputs
            .iter()
            .filter(|output| output.status.success())
            .count(),
        1,
        "exactly one concurrent restore should activate the empty target: {outputs:?}"
    );
    let refusal = outputs
        .iter()
        .find(|output| !output.status.success())
        .unwrap();
    assert!(
        String::from_utf8_lossy(&refusal.stderr).contains("Target database is not empty"),
        "losing restore should observe the committed winner: {refusal:?}"
    );

    let conn = rusqlite::Connection::open(target.path().join(".beads/beads.db")).unwrap();
    let receipts: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM provenance_receipts WHERE kind = 'restore'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(receipts, 1);
}

#[test]
fn unverified_generation_artifact_is_refused_before_target_initialization() {
    let source = source_checkpoint();
    let copied = tempfile::tempdir().unwrap();
    let copied_checkpoint = copied.path().join("checkpoint");
    copy_tree(&source.checkpoint, &copied_checkpoint);
    let root = source.pointer["active_root"]["path"].as_str().unwrap();
    fs::write(copied_checkpoint.join(root), b"tampered\n").unwrap();

    let target = tempfile::tempdir().unwrap();
    bead(target.path())
        .args([
            "restore",
            "--source",
            copied_checkpoint.to_str().unwrap(),
            "--generation",
            &source.generation,
            "--actor",
            "recovery-operator",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Unverified restore source"))
        .stderr(predicate::str::contains("hash mismatch"));
    assert!(
        !target.path().join(".beads").exists(),
        "source verification failure must precede target initialization"
    );
}

#[test]
fn r029_archaeology_view_is_explicitly_refused() {
    let view_dir = tempfile::tempdir().unwrap();
    let view = view_dir.path().join("historical-view.json");
    fs::write(
        &view,
        serde_json::to_vec_pretty(&serde_json::json!({
            "artifact_kind": "bead-rs-checkpoint-archaeology-view-v1",
            "importable": false,
            "generation_id": "gen-deadbeef"
        }))
        .unwrap(),
    )
    .unwrap();
    let target = tempfile::tempdir().unwrap();

    bead(target.path())
        .args([
            "restore",
            "--source",
            view.to_str().unwrap(),
            "--generation",
            "gen-deadbeef",
            "--actor",
            "recovery-operator",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("R029 checkpoint archaeology view"))
        .stderr(predicate::str::contains("explicitly non-importable"));
    assert!(!target.path().join(".beads").exists());

    let import_target = tempfile::tempdir().unwrap();
    run(import_target.path(), &["init", "--prefix", "target"]);
    bead(import_target.path())
        .args([
            "sync",
            "import-only",
            "--input",
            view.to_str().unwrap(),
            "--restore-into-empty",
            "--actor",
            "recovery-operator",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("R029 checkpoint archaeology view"))
        .stderr(predicate::str::contains("explicitly non-importable"));
}

#[test]
fn doctor_recommends_named_restore_but_never_performs_it() {
    let source = source_checkpoint();
    let clone = tempfile::tempdir().unwrap();
    let beads = clone.path().join(".beads");
    fs::create_dir_all(&beads).unwrap();
    fs::copy(
        source.workspace.join(".beads/config.json"),
        beads.join("config.json"),
    )
    .unwrap();
    copy_tree(&source.checkpoint, &beads.join("checkpoint"));

    bead(clone.path())
        .arg("doctor")
        .assert()
        .failure()
        .stdout(predicate::str::contains("bead restore"))
        .stdout(predicate::str::contains(&source.generation))
        .stdout(predicate::str::contains(
            "Doctor does not run restore automatically",
        ));

    let conn = rusqlite::Connection::open(beads.join("beads.db")).unwrap();
    let workspace_table: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='workspace'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(workspace_table, 0, "doctor must not initialize or restore");
}

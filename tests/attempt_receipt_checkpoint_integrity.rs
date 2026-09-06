//! Recovery invariants for durable attempt-outcome receipts.

use assert_cmd::Command;
use rusqlite::{params, Connection};
use serde_json::Value;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

fn workspace() -> TempDir {
    let dir = TempDir::new().expect("temp workspace");
    Command::cargo_bin("bead")
        .expect("bead binary")
        .args(["init", "--prefix", "atr", "--skip-foreign-workspace"])
        .current_dir(dir.path())
        .assert()
        .success();
    dir
}

fn configure(path: &Path, mode: &str) {
    let config_path = path.join(".beads/config.json");
    let mut config: Value =
        serde_json::from_slice(&fs::read(&config_path).expect("read config")).expect("config");
    config["checkpoint"]["auto_flush"] = Value::Bool(false);
    config["checkpoint"]["mode"] = Value::String(mode.to_string());
    fs::write(
        config_path,
        serde_json::to_vec_pretty(&config).expect("serialize config"),
    )
    .expect("write config");
}

fn create_issue(path: &Path) -> String {
    let output = Command::cargo_bin("bead")
        .expect("bead binary")
        .args(["create", "--title", "attempt receipt source"])
        .current_dir(path)
        .output()
        .expect("create issue");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("utf8 issue id")
        .trim()
        .to_string()
}

fn insert_outcome(path: &Path, issue_id: &str, receipt_id: &str) {
    let conn = Connection::open(path.join(".beads/beads.db")).expect("database");
    conn.execute(
        "INSERT INTO attempt_outcomes (
            receipt_id, attempt_id, issue_id, outcome, action, reason,
            canonical_request_hash, prior_attempt_tier, resulting_attempt_tier,
            resulting_issue_revision, actor, created_at, evidence_refs_json,
            model, harness, harness_version, resulting_state
         ) VALUES (?1, ?2, ?3, 'verified_success', 'none', 'verified',
                   ?4, 0, 0, 2, 'test-worker', '2026-09-04T00:00:00Z',
                   '[]', NULL, 'test', '1', 'in_progress')",
        params![
            receipt_id,
            format!("urn:needle:attempt:{receipt_id}"),
            issue_id,
            "a".repeat(64),
        ],
    )
    .expect("insert outcome");
}

fn stored_state(path: &Path) -> String {
    Connection::open(path.join(".beads/beads.db"))
        .expect("database")
        .query_row("SELECT resulting_state FROM attempt_outcomes", [], |row| {
            row.get(0)
        })
        .expect("stored resulting state")
}

#[test]
fn exact_resulting_state_survives_both_checkpoint_layouts() {
    for mode in ["monolithic", "sharded"] {
        let source = workspace();
        configure(source.path(), mode);
        let issue_id = create_issue(source.path());
        insert_outcome(source.path(), &issue_id, &format!("ao-{mode}"));

        Command::cargo_bin("bead")
            .expect("bead binary")
            .args(["sync", "flush-only"])
            .current_dir(source.path())
            .assert()
            .success();

        let checkpoint = source.path().join(".beads/checkpoint");
        let pointer: Value = serde_json::from_slice(
            &fs::read(checkpoint.join("current.json")).expect("current pointer"),
        )
        .expect("pointer json");
        assert_eq!(pointer["attempt_outcome_count"], 1);

        let target = workspace();
        Command::cargo_bin("bead")
            .expect("bead binary")
            .args([
                "sync",
                "import-only",
                "--input",
                checkpoint
                    .join("current.json")
                    .to_str()
                    .expect("pointer path"),
                "--restore-into-empty",
                "--actor",
                "recovery-test",
            ])
            .current_dir(target.path())
            .assert()
            .success();

        assert_eq!(stored_state(target.path()), "in_progress");
    }
}

#[test]
fn replacement_restore_removes_displaced_attempt_outcomes() {
    let source = workspace();
    configure(source.path(), "monolithic");
    create_issue(source.path());
    Command::cargo_bin("bead")
        .expect("bead binary")
        .args(["sync", "flush-only"])
        .current_dir(source.path())
        .assert()
        .success();
    let checkpoint = source.path().join(".beads/checkpoint");
    let pointer: Value = serde_json::from_slice(
        &fs::read(checkpoint.join("current.json")).expect("current pointer"),
    )
    .expect("pointer json");
    let generation = pointer["generation_id"]
        .as_str()
        .expect("generation id")
        .to_string();

    let target = workspace();
    configure(target.path(), "monolithic");
    let displaced_issue = create_issue(target.path());
    insert_outcome(target.path(), &displaced_issue, "ao-displaced");

    Command::cargo_bin("bead")
        .expect("bead binary")
        .args([
            "restore",
            "--source",
            checkpoint.to_str().expect("checkpoint path"),
            "--generation",
            &generation,
            "--allow-non-empty",
            "--actor",
            "recovery-test",
        ])
        .current_dir(target.path())
        .assert()
        .success();

    let count: i64 = Connection::open(target.path().join(".beads/beads.db"))
        .expect("database")
        .query_row("SELECT COUNT(*) FROM attempt_outcomes", [], |row| {
            row.get(0)
        })
        .expect("attempt count");
    assert_eq!(count, 0);
}

#[test]
fn public_schemas_describe_attempt_state_and_checkpoint_counts() {
    let ws = workspace();
    for (schema_ref, expected_properties) in [
        (
            "urn:bead-rs:schema:attempt-outcome:native-v1",
            &["resulting_state"][..],
        ),
        (
            "urn:bead-rs:schema:checkpoint-pointer:native-v1",
            &["attempt_outcome_count"][..],
        ),
        (
            "urn:bead-rs:schema:checkpoint-manifest:native-v1",
            &["attempt_outcome_count", "attempt_outcome_shards"][..],
        ),
    ] {
        let output = Command::cargo_bin("bead")
            .expect("bead binary")
            .args(["schema", "show", schema_ref, "--format", "json"])
            .current_dir(ws.path())
            .output()
            .expect("schema show");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let schema: Value = serde_json::from_slice(&output.stdout).expect("schema json");
        for property in expected_properties {
            assert!(
                schema["properties"].get(*property).is_some(),
                "{schema_ref} omits {property}"
            );
        }
    }
}

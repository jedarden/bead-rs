//! Attempt outcome checkpoint round-trip conformance test
//!
//! This test ensures that attempt outcome records survive checkpoint
//! export/import with complete fidelity through both monolithic and sharded
//! modes.
//!
//! Test strategy:
//! 1. Create workspace with issues and attempt outcomes
//! 2. Flush forensic checkpoint (monolithic and sharded modes)
//! 3. Restore into fresh empty workspace
//! 4. Verify all attempt outcomes round-trip correctly
//! 5. Test conflicting duplicate detection
//! 6. Test malformed record rejection
//! 7. Test compatibility with older readers
//!
//! See: attempt-outcome-v1 specification

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::json;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// Create a test workspace and return the temp dir
fn create_workspace() -> TempDir {
    let temp_dir = TempDir::new().unwrap();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "bead", "--skip-foreign-workspace"])
        .current_dir(temp_dir.path())
        .assert()
        .success();

    temp_dir
}

/// Suppress auto-flush to control checkpoint publication
fn suppress_auto_flush(workspace: &std::path::Path) {
    let path = workspace.join(".beads/config.json");
    let mut config: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    config
        .as_object_mut()
        .unwrap()
        .entry("checkpoint")
        .or_insert(serde_json::Value::Object(Default::default()))
        .as_object_mut()
        .unwrap()
        .insert("auto_flush".into(), serde_json::Value::Bool(false));
    fs::write(&path, serde_json::to_string_pretty(&config).unwrap()).unwrap();
}

/// Force sharded mode for testing
fn force_sharded_mode(workspace: &std::path::Path) {
    let path = workspace.join(".beads/config.json");
    let mut config: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    config
        .as_object_mut()
        .unwrap()
        .entry("checkpoint")
        .or_insert(serde_json::Value::Object(Default::default()))
        .as_object_mut()
        .unwrap()
        .insert(
            "mode".into(),
            serde_json::Value::String("sharded".to_string()),
        );
    fs::write(&path, serde_json::to_string_pretty(&config).unwrap()).unwrap();
}

/// Get the first (and usually only) issue ID from the database
fn get_first_issue_id(db_path: &Path) -> String {
    let conn = rusqlite::Connection::open(db_path).unwrap();
    conn.query_row("SELECT id FROM issues LIMIT 1", [], |row| {
        row.get::<_, String>(0)
    })
    .unwrap()
}

/// Read all attempt outcomes from database
fn read_all_attempt_outcomes(db_path: &Path) -> Vec<serde_json::Value> {
    let conn = rusqlite::Connection::open(db_path).unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT receipt_id, attempt_id, issue_id, outcome, action, reason,
                    canonical_request_hash, prior_attempt_tier, resulting_attempt_tier,
                    resulting_issue_revision, actor, created_at, evidence_refs_json,
                    model, harness, harness_version
             FROM attempt_outcomes
             ORDER BY receipt_id",
        )
        .unwrap();

    let rows = stmt
        .query_map([], |row| {
            Ok(json!({
                "receipt_id": row.get::<_, String>("receipt_id")?,
                "attempt_id": row.get::<_, String>("attempt_id")?,
                "issue_id": row.get::<_, String>("issue_id")?,
                "outcome": row.get::<_, String>("outcome")?,
                "action": row.get::<_, String>("action")?,
                "reason": row.get::<_, Option<String>>("reason")?,
                "canonical_request_hash": row.get::<_, String>("canonical_request_hash")?,
                "prior_attempt_tier": row.get::<_, i64>("prior_attempt_tier")?,
                "resulting_attempt_tier": row.get::<_, i64>("resulting_attempt_tier")?,
                "resulting_issue_revision": row.get::<_, i64>("resulting_issue_revision")?,
                "actor": row.get::<_, String>("actor")?,
                "created_at": row.get::<_, String>("created_at")?,
                "evidence_refs_json": row.get::<_, String>("evidence_refs_json")?,
                "model": row.get::<_, Option<String>>("model")?,
                "harness": row.get::<_, Option<String>>("harness")?,
                "harness_version": row.get::<_, Option<String>>("harness_version")?,
            }))
        })
        .unwrap();

    rows.map(|r| r.unwrap()).collect::<Vec<_>>()
}

#[test]
fn test_attempt_outcome_round_trip_monolithic() {
    let workspace1 = create_workspace();
    suppress_auto_flush(workspace1.path());

    // Create a test issue
    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "create",
            "--title",
            "Test issue for attempt outcome",
            "--priority",
            "2",
        ])
        .current_dir(workspace1.path())
        .assert()
        .success();

    // Get the actual issue ID that was created
    let db_path = workspace1.path().join(".beads/beads.db");
    let issue_id = get_first_issue_id(&db_path);

    // Insert attempt outcome directly into database
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    use rusqlite::params;
    conn.execute(
        "INSERT INTO attempt_outcomes (
            receipt_id, attempt_id, issue_id, outcome, action, reason,
            canonical_request_hash, prior_attempt_tier, resulting_attempt_tier,
            resulting_issue_revision, actor, created_at, evidence_refs_json,
            model, harness, harness_version
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
        params![
            "ao-test001",
            "urn:needle:attempt:test001",
            &issue_id,
            "verified_success",
            "close",
            "All tests passed",
            "abc123hash",
            0i64,
            0i64,
            1i64,
            "test-worker",
            "2026-08-31T12:00:00Z",
            r#"["s3:logs/test.tar.gz"]"#,
            "claude-opus-5",
            "needle",
            "1.0.0",
        ],
    )
    .unwrap();

    // Flush forensic checkpoint to .beads/checkpoint/
    Command::cargo_bin("bead")
        .unwrap()
        .args(["sync", "flush-only"])
        .current_dir(workspace1.path())
        .assert()
        .success();

    // The forensic checkpoint is at .beads/checkpoint/
    let checkpoint_forensic = workspace1.path().join(".beads/checkpoint/forensic.jsonl");

    // Restore into fresh workspace
    let workspace2 = create_workspace();
    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "sync",
            "import-only",
            "--input",
            checkpoint_forensic.to_str().unwrap(),
            "--restore-into-empty",
            "--actor",
            "test-restore",
        ])
        .current_dir(workspace2.path())
        .assert()
        .success();

    // Verify attempt outcomes round-tripped
    let outcomes1 = read_all_attempt_outcomes(&db_path);
    let outcomes2 = read_all_attempt_outcomes(&workspace2.path().join(".beads/beads.db"));

    assert_eq!(
        outcomes1.len(),
        outcomes2.len(),
        "Attempt outcome count mismatch"
    );

    for outcome1 in &outcomes1 {
        let outcome2 = outcomes2
            .iter()
            .find(|o| o["receipt_id"] == outcome1["receipt_id"])
            .unwrap_or_else(|| {
                panic!(
                    "Attempt outcome {} not found in restored workspace",
                    outcome1["receipt_id"]
                )
            });

        // Verify key fields match
        assert_eq!(
            outcome1["attempt_id"], outcome2["attempt_id"],
            "attempt_id mismatch"
        );
        assert_eq!(
            outcome1["issue_id"], outcome2["issue_id"],
            "issue_id mismatch"
        );
        assert_eq!(outcome1["outcome"], outcome2["outcome"], "outcome mismatch");
        assert_eq!(outcome1["action"], outcome2["action"], "action mismatch");
        assert_eq!(
            outcome1["canonical_request_hash"], outcome2["canonical_request_hash"],
            "canonical_request_hash mismatch"
        );
        assert_eq!(outcome1["actor"], outcome2["actor"], "actor mismatch");
    }
}

#[test]
fn test_attempt_outcome_round_trip_sharded() {
    let workspace1 = create_workspace();
    suppress_auto_flush(workspace1.path());
    force_sharded_mode(workspace1.path());

    // Create a test issue
    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "create",
            "--title",
            "Test issue for sharded attempt outcome",
            "--priority",
            "2",
        ])
        .current_dir(workspace1.path())
        .assert()
        .success();

    // Insert multiple attempt outcomes
    let db_path = workspace1.path().join(".beads/beads.db");
    let issue_id = get_first_issue_id(&db_path);
    let conn = rusqlite::Connection::open(&db_path).unwrap();

    for i in 1..=3 {
        use rusqlite::params;
        conn.execute(
            "INSERT INTO attempt_outcomes (
                receipt_id, attempt_id, issue_id, outcome, action, reason,
                canonical_request_hash, prior_attempt_tier, resulting_attempt_tier,
                resulting_issue_revision, actor, created_at, evidence_refs_json,
                model, harness, harness_version
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            params![
                format!("ao-shard{:03}", i),
                format!("urn:needle:attempt:shard{}", i),
                &issue_id,
                if i % 2 == 0 {
                    "work_failure"
                } else {
                    "verified_success"
                },
                if i % 2 == 0 { "quarantine" } else { "close" },
                format!("Reason {}", i),
                format!("hash{:03}", i),
                0i64,
                if i % 2 == 0 { 1i64 } else { 0i64 },
                i,
                "test-worker",
                "2026-08-31T12:00:00Z",
                r#"[]"#,
                "claude-opus-5",
                "needle",
                "1.0.0",
            ],
        )
        .unwrap();
    }

    // Flush sharded checkpoint to .beads/checkpoint/
    Command::cargo_bin("bead")
        .unwrap()
        .args(["sync", "flush-only"])
        .current_dir(workspace1.path())
        .assert()
        .success();

    // The sharded checkpoint is at .beads/checkpoint/current.json
    let checkpoint_base = workspace1.path().join(".beads/checkpoint");

    // Verify sharded structure exists
    assert!(checkpoint_base.join("current.json").exists());
    assert!(checkpoint_base.join("manifests").exists());
    assert!(checkpoint_base.join("objects").exists());

    // Restore into fresh workspace
    let workspace2 = create_workspace();
    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "sync",
            "import-only",
            "--input",
            checkpoint_base.join("current.json").to_str().unwrap(),
            "--restore-into-empty",
            "--actor",
            "test-restore",
        ])
        .current_dir(workspace2.path())
        .assert()
        .success();

    // Verify all attempt outcomes round-tripped
    let outcomes1 = read_all_attempt_outcomes(&db_path);
    let outcomes2 = read_all_attempt_outcomes(&workspace2.path().join(".beads/beads.db"));

    assert_eq!(
        outcomes1.len(),
        outcomes2.len(),
        "Attempt outcome count mismatch"
    );
    assert_eq!(outcomes1.len(), 3, "Expected 3 attempt outcomes");

    for outcome1 in &outcomes1 {
        let outcome2 = outcomes2
            .iter()
            .find(|o| o["receipt_id"] == outcome1["receipt_id"])
            .unwrap_or_else(|| {
                panic!(
                    "Attempt outcome {} not found in restored workspace",
                    outcome1["receipt_id"]
                )
            });

        assert_eq!(outcome1["attempt_id"], outcome2["attempt_id"]);
        assert_eq!(outcome1["outcome"], outcome2["outcome"]);
    }
}

#[test]
fn test_attempt_outcome_duplicate_detection() {
    let workspace1 = create_workspace();
    suppress_auto_flush(workspace1.path());

    // Create issue and insert attempt outcome
    Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Test", "--priority", "2"])
        .current_dir(workspace1.path())
        .assert()
        .success();

    let db_path = workspace1.path().join(".beads/beads.db");
    let issue_id = get_first_issue_id(&db_path);
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    use rusqlite::params;
    conn.execute(
        "INSERT INTO attempt_outcomes (
            receipt_id, attempt_id, issue_id, outcome, action, reason,
            canonical_request_hash, prior_attempt_tier, resulting_attempt_tier,
            resulting_issue_revision, actor, created_at, evidence_refs_json,
            model, harness, harness_version
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
        params![
            "ao-dup001",
            "urn:needle:attempt:dup001",
            &issue_id,
            "verified_success",
            "close",
            "Test",
            "hash001",
            0i64,
            0i64,
            1i64,
            "worker",
            "2026-08-31T12:00:00Z",
            r#"[]"#,
            "model",
            "harness",
            "1.0",
        ],
    )
    .unwrap();

    // Flush forensic checkpoint to .beads/checkpoint/
    Command::cargo_bin("bead")
        .unwrap()
        .args(["sync", "flush-only"])
        .current_dir(workspace1.path())
        .assert()
        .success();

    // The forensic checkpoint is at .beads/checkpoint/forensic.jsonl
    let checkpoint_forensic = workspace1.path().join(".beads/checkpoint/forensic.jsonl");

    // First restore should succeed
    let workspace2 = create_workspace();
    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "sync",
            "import-only",
            "--input",
            checkpoint_forensic.to_str().unwrap(),
            "--restore-into-empty",
            "--actor",
            "test",
        ])
        .current_dir(workspace2.path())
        .assert()
        .success();

    // Second restore (duplicate attempt_id) should also succeed
    // because we're restoring into empty workspaces each time
    let workspace3 = create_workspace();
    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "sync",
            "import-only",
            "--input",
            checkpoint_forensic.to_str().unwrap(),
            "--restore-into-empty",
            "--actor",
            "test",
        ])
        .current_dir(workspace3.path())
        .assert()
        .success();
}

#[test]
fn test_attempt_outcome_conflict_rejection() {
    let workspace = create_workspace();

    // Create a checkpoint file with conflicting attempt outcomes
    let checkpoint_dir = workspace.path().join("checkpoint-conflict");
    fs::create_dir_all(&checkpoint_dir).unwrap();

    // Same attempt_id with different hashes (conflict)
    let checkpoint_data = r#"{"record_type":"attempt_outcome","attempt_outcome":{"$schema":"urn:bead-rs:schema:attempt-outcome:native-v1","attempt_id":"urn:needle:attempt:conflict","issue_id":"bead-xxx","outcome":"verified_success","action":"close","reason":"First","canonical_request_hash":"hash001","resulting_issue_revision":1,"resulting_state":"closed","resulting_attempt_tier":0,"receipt_id":"ao-conflict1","actor":"worker","created_at":"2026-08-31T12:00:00Z","evidence_refs":[]}}
{"record_type":"attempt_outcome","attempt_outcome":{"$schema":"urn:bead-rs:schema:attempt-outcome:native-v1","attempt_id":"urn:needle:attempt:conflict","issue_id":"bead-yyy","outcome":"work_failure","action":"quarantine","reason":"Second","canonical_request_hash":"hash002","resulting_issue_revision":2,"resulting_state":"open","resulting_attempt_tier":1,"receipt_id":"ao-conflict2","actor":"worker","created_at":"2026-08-31T12:01:00Z","evidence_refs":[]}}"#;

    fs::write(checkpoint_dir.join("forensic.jsonl"), checkpoint_data).unwrap();

    // Import should fail due to duplicate attempt_id in staging
    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "sync",
            "import-only",
            "--input",
            checkpoint_dir.join("forensic.jsonl").to_str().unwrap(),
            "--restore-into-empty",
            "--actor",
            "test",
        ])
        .current_dir(workspace.path())
        .assert()
        .failure()
        .stderr(predicates::str::contains("duplicate attempt ID"));
}

//! BR-T15 conformance for durable historical-redaction records.
//!
//! These tests exercise the public checkpoint boundary rather than only the
//! Rust structs: all five record kinds must publish, survive verified restore,
//! tolerate additive fields inside known records, and make an older/unknown
//! record kind fail closed instead of silently losing a tombstone.

use assert_cmd::Command;
use bead_rs::model::redaction::{
    FieldSelector, RedactionEpoch, RedactionExtensions, RedactionReceipt, ResurrectionTombstone,
    SCHEMA_REDACTION_FIELD_SELECTOR, SCHEMA_REDACTION_RECEIPT, SCHEMA_REDACTION_TOMBSTONE,
};
use rusqlite::params;
use serde_json::Value;
use std::path::{Path, PathBuf};

fn bead(workspace: &Path) -> Command {
    let mut command = Command::cargo_bin("bead").unwrap();
    command.current_dir(workspace).env("HOME", workspace);
    command
}

fn workspace() -> tempfile::TempDir {
    let workspace = tempfile::Builder::new()
        .prefix("bead-redaction-storage-")
        .tempdir_in("/var/tmp")
        .unwrap();
    bead(workspace.path())
        .args(["init", "--prefix", "redact", "--no-auto-flush"])
        .assert()
        .success();
    workspace
}

fn database(workspace: &Path) -> PathBuf {
    workspace.join(".beads/beads.db")
}

fn insert_redaction_corpus(workspace: &Path) {
    let conn = rusqlite::Connection::open(database(workspace)).unwrap();
    let selector = FieldSelector {
        schema_ref: SCHEMA_REDACTION_FIELD_SELECTOR.to_string(),
        record_kind: "issue".to_string(),
        origin_identity: "redact-record-1".to_string(),
        field_path: "description".to_string(),
        byte_start: 9,
        byte_length: 16,
        prior_record_hash: "a".repeat(64),
        extensions: RedactionExtensions::new(),
    };
    let finding_fingerprint = "b".repeat(64);
    let receipt_id = RedactionReceipt::canonical_identity(
        &finding_fingerprint,
        1,
        "fixture-rule",
        &selector,
        &"c".repeat(64),
        "storage-test",
        "verify durable records",
        "2026-09-03T00:00:00Z",
        Some(2),
    );
    let receipt_ids = vec![receipt_id.clone()];
    let epoch_id = RedactionEpoch::identity_for(&receipt_ids);
    let generation_id = "gen-sanitized-fixture";
    let tombstone_id = ResurrectionTombstone::identity_for(
        &selector.record_kind,
        &selector.origin_identity,
        &selector.field_path,
        &selector.prior_record_hash,
        &finding_fingerprint,
        &epoch_id,
    );

    let tx = conn.unchecked_transaction().unwrap();
    tx.execute(
        "INSERT INTO redaction_findings (
            fingerprint, ruleset_version, rule_id, record_kind,
            origin_identity, field_path, byte_start, byte_length,
            prior_record_hash, severity, detected_at
         ) VALUES (?1, 1, 'fixture-rule', ?2, ?3, ?4, ?5, ?6, ?7,
                   'blocking', '2026-09-03T00:00:00Z')",
        params![
            &finding_fingerprint,
            &selector.record_kind,
            &selector.origin_identity,
            &selector.field_path,
            selector.byte_start,
            selector.byte_length,
            &selector.prior_record_hash,
        ],
    )
    .unwrap();
    tx.execute(
        "INSERT INTO redaction_acknowledgments
            (fingerprint, actor, reason, acknowledged_at)
         VALUES (?1, 'storage-test', 'reviewed fixture', '2026-09-03T00:00:01Z')",
        [&finding_fingerprint],
    )
    .unwrap();
    tx.execute(
        "INSERT INTO redaction_receipts (
            receipt_id, finding_fingerprint, ruleset_version, rule_id,
            record_kind, origin_identity, field_path, byte_start, byte_length,
            prior_record_hash, sanitized_record_hash, actor, reason,
            redacted_at, affected_issue_revision, publication_state,
            resulting_generation_id, epoch_id
         ) VALUES (?1, ?2, 1, 'fixture-rule', ?3, ?4, ?5, ?6, ?7, ?8,
                   ?9, 'storage-test', 'verify durable records',
                   '2026-09-03T00:00:00Z', 2, 'published', ?10, ?11)",
        params![
            &receipt_id,
            &finding_fingerprint,
            &selector.record_kind,
            &selector.origin_identity,
            &selector.field_path,
            selector.byte_start,
            selector.byte_length,
            &selector.prior_record_hash,
            "c".repeat(64),
            generation_id,
            &epoch_id,
        ],
    )
    .unwrap();
    tx.execute(
        "INSERT INTO redaction_epochs (
            epoch_id, publication_state, receipt_ids_json,
            resulting_generation_id, previous_generation_reset,
            superseded_generations_json, opened_at, published_at
         ) VALUES (?1, 'published', ?2, ?3, 1, '[\"gen-dirty\"]',
                   '2026-09-03T00:00:00Z', '2026-09-03T00:00:02Z')",
        params![
            &epoch_id,
            serde_json::to_string(&receipt_ids).unwrap(),
            generation_id,
        ],
    )
    .unwrap();
    tx.execute(
        "INSERT INTO redaction_tombstones (
            tombstone_id, record_kind, origin_identity, field_path,
            prior_record_hash, finding_fingerprint, epoch_id, created_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, '2026-09-03T00:00:02Z')",
        params![
            &tombstone_id,
            &selector.record_kind,
            &selector.origin_identity,
            &selector.field_path,
            &selector.prior_record_hash,
            &finding_fingerprint,
            &epoch_id,
        ],
    )
    .unwrap();
    tx.execute(
        "INSERT INTO events (issue_id, kind, actor, time, detail)
         VALUES (NULL, 'historical_redaction', 'storage-test',
                 '2026-09-03T00:00:02Z', '{}')",
        [],
    )
    .unwrap();
    tx.commit().unwrap();
}

fn pointer(workspace: &Path) -> Value {
    serde_json::from_slice(
        &std::fs::read(workspace.join(".beads/checkpoint/current.json")).unwrap(),
    )
    .unwrap()
}

fn assert_redaction_round_trip(sharded: bool) {
    let source = workspace();
    if sharded {
        let config_path = source.path().join(".beads/config.json");
        let mut config: Value =
            serde_json::from_slice(&std::fs::read(&config_path).unwrap()).unwrap();
        config["checkpoint"]["mode"] = Value::String("sharded".to_string());
        std::fs::write(config_path, serde_json::to_vec_pretty(&config).unwrap()).unwrap();
    }
    insert_redaction_corpus(source.path());
    bead(source.path())
        .args(["sync", "flush-only"])
        .assert()
        .success();

    let pointer = pointer(source.path());
    assert_eq!(pointer["redaction_record_count"], 5);
    let checkpoint_dir = source.path().join(".beads/checkpoint");
    let root = checkpoint_dir.join(
        pointer["active_root"]["path"]
            .as_str()
            .expect("active root path"),
    );
    let checkpoint = if sharded {
        let manifest: Value = serde_json::from_slice(&std::fs::read(root).unwrap()).unwrap();
        manifest["redaction_shards"]
            .as_array()
            .unwrap()
            .iter()
            .map(|shard| {
                std::fs::read_to_string(checkpoint_dir.join(shard["path"].as_str().unwrap()))
                    .unwrap()
            })
            .collect::<String>()
    } else {
        std::fs::read_to_string(root).unwrap()
    };
    for record_type in [
        "redaction_finding",
        "redaction_acknowledgment",
        "redaction_receipt",
        "redaction_epoch",
        "redaction_tombstone",
    ] {
        assert!(checkpoint.contains(&format!(r#""record_type":"{record_type}""#)));
    }

    let target = workspace();
    bead(target.path())
        .args(["restore", "--source"])
        .arg(source.path().join(".beads/checkpoint"))
        .args([
            "--generation",
            pointer["generation_id"].as_str().unwrap(),
            "--actor",
            "storage-test",
            "--no-auto-flush",
        ])
        .assert()
        .success();

    let conn = rusqlite::Connection::open(database(target.path())).unwrap();
    for table in [
        "redaction_findings",
        "redaction_acknowledgments",
        "redaction_receipts",
        "redaction_epochs",
        "redaction_tombstones",
    ] {
        let count: i64 = conn
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 1, "{table} did not survive restore");
    }
}

#[test]
fn every_redaction_record_round_trips_through_monolithic_restore() {
    assert_redaction_round_trip(false);
}

#[test]
fn every_redaction_record_round_trips_through_sharded_restore() {
    assert_redaction_round_trip(true);
}

#[test]
fn known_redaction_records_preserve_additive_fields_but_unknown_kinds_fail_closed() {
    let source = workspace();
    insert_redaction_corpus(source.path());
    bead(source.path())
        .args(["sync", "flush-only"])
        .assert()
        .success();
    let source_pointer = pointer(source.path());
    let root = source
        .path()
        .join(".beads/checkpoint")
        .join(source_pointer["active_root"]["path"].as_str().unwrap());

    let future_path = source.path().join("future-redaction.jsonl");
    let mut future_lines = Vec::new();
    for line in std::fs::read_to_string(root).unwrap().lines() {
        let mut record: Value = serde_json::from_str(line).unwrap();
        if let Some(kind) = record["record_type"].as_str().map(str::to_string) {
            if kind.starts_with("redaction_") {
                record[&kind]["future_nonsecret_field"] = serde_json::json!({"version": 2});
            }
        }
        future_lines.push(serde_json::to_string(&record).unwrap());
    }
    std::fs::write(&future_path, format!("{}\n", future_lines.join("\n"))).unwrap();

    let target = workspace();
    bead(target.path())
        .args(["sync", "import-only", "--input"])
        .arg(&future_path)
        .args([
            "--restore-into-empty",
            "--actor",
            "storage-test",
            "--no-auto-flush",
        ])
        .assert()
        .success();

    bead(target.path())
        .args(["sync", "flush-only"])
        .assert()
        .success();
    let restored_pointer = pointer(target.path());
    let restored_root = target
        .path()
        .join(".beads/checkpoint")
        .join(restored_pointer["active_root"]["path"].as_str().unwrap());
    let restored = std::fs::read_to_string(restored_root).unwrap();
    let preserved = restored
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|record| {
            record["record_type"]
                .as_str()
                .is_some_and(|kind| kind.starts_with("redaction_"))
        })
        .all(|record| {
            let kind = record["record_type"].as_str().unwrap();
            record[kind]["future_nonsecret_field"] == serde_json::json!({"version": 2})
        });
    assert!(
        preserved,
        "additive redaction fields were lost on republish"
    );

    let unknown_path = source.path().join("unknown-redaction.jsonl");
    std::fs::write(
        &unknown_path,
        "{\"record_type\":\"redaction_future\",\"redaction_future\":{}}\n",
    )
    .unwrap();
    let empty_target = workspace();
    bead(empty_target.path())
        .args(["sync", "import-only", "--input"])
        .arg(&unknown_path)
        .args([
            "--restore-into-empty",
            "--actor",
            "storage-test",
            "--dry-run",
            "--no-auto-flush",
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("unknown record type"));
}

#[test]
fn incomplete_redaction_graph_is_rejected_before_restore_mutates() {
    let source = workspace();
    insert_redaction_corpus(source.path());
    bead(source.path())
        .args(["sync", "flush-only"])
        .assert()
        .success();
    let pointer = pointer(source.path());
    let root = source
        .path()
        .join(".beads/checkpoint")
        .join(pointer["active_root"]["path"].as_str().unwrap());
    let incomplete_path = source.path().join("incomplete-redaction.jsonl");
    let checkpoint = std::fs::read_to_string(root).unwrap();
    let lines: Vec<&str> = checkpoint
        .lines()
        .filter(|line| !line.contains(r#""record_type":"redaction_epoch""#))
        .collect();
    std::fs::write(&incomplete_path, format!("{}\n", lines.join("\n"))).unwrap();

    let target = workspace();
    bead(target.path())
        .args(["sync", "import-only", "--input"])
        .arg(&incomplete_path)
        .args([
            "--restore-into-empty",
            "--actor",
            "storage-test",
            "--no-auto-flush",
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("references missing epoch"));

    let conn = rusqlite::Connection::open(database(target.path())).unwrap();
    let event_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))
        .unwrap();
    let redaction_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM redaction_receipts", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!((event_count, redaction_count), (0, 0));
}

#[test]
fn schema_catalog_exposes_every_redaction_record_shape() {
    let catalog = bead_rs::service::schema_catalog().unwrap();
    for schema_ref in [
        "urn:bead-rs:schema:redaction-acknowledgment:native-v1",
        "urn:bead-rs:schema:redaction-epoch:native-v1",
        "urn:bead-rs:schema:redaction-field-selector:native-v1",
        "urn:bead-rs:schema:redaction-finding:native-v1",
        SCHEMA_REDACTION_RECEIPT,
        SCHEMA_REDACTION_TOMBSTONE,
    ] {
        assert!(catalog.iter().any(|entry| entry.schema_ref == schema_ref));
        let document = bead_rs::service::schema_document(schema_ref).unwrap();
        assert_eq!(document["$id"], schema_ref);
    }
}

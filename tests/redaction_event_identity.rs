//! Native-event identity conformance for historical redaction.

use assert_cmd::Command;
use bead_rs::model::redaction::REDACTION_MARKER;
use bead_rs::service::redaction::redact_finding;
use bead_rs::service::secret_diagnostics::scan_live_findings;
use bead_rs::store::{open_configured_connection, SqliteStore};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

fn bead(workspace: &Path) -> Command {
    let mut command = Command::cargo_bin("bead").unwrap();
    command.current_dir(workspace).env("HOME", workspace);
    command
}

fn workspace(name: &str) -> tempfile::TempDir {
    let workspace = tempfile::Builder::new()
        .prefix(&format!("bead-redaction-event-{name}-"))
        .tempdir_in("/var/tmp")
        .unwrap();
    bead(workspace.path())
        .args([
            "init",
            "--skip-foreign-workspace",
            "--prefix",
            "redact",
            "--no-auto-flush",
        ])
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

fn use_monolithic_checkpoint(root: &Path) {
    let path = root.join(".beads/config.json");
    let mut config: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    config["checkpoint"]["mode"] = Value::String("monolithic".to_string());
    fs::write(path, serde_json::to_vec_pretty(&config).unwrap()).unwrap();
}

fn current_checkpoint(root: &Path) -> (Value, Vec<Value>) {
    let checkpoint = root.join(".beads/checkpoint");
    let pointer: Value =
        serde_json::from_slice(&fs::read(checkpoint.join("current.json")).unwrap()).unwrap();
    assert_eq!(pointer["mode"], "monolithic");
    let object_path = checkpoint.join(pointer["active_root"]["path"].as_str().unwrap());
    let records = fs::read_to_string(object_path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    (pointer, records)
}

fn redact(root: &Path, fingerprint: &str, dry_run: bool) -> Value {
    let mut arguments = vec![
        "redact",
        "--finding",
        fingerprint,
        "--actor",
        "event-identity-test",
        "--reason",
        "remove historical fixture",
    ];
    if dry_run {
        arguments.push("--dry-run");
    }
    arguments.push("--json");
    let output = bead(root).args(arguments).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn duplicate_native_events_redact_by_stable_wire_identity_and_round_trip() {
    let source = workspace("source");
    let secret = shaped_value();
    let detail = format!(r#"{{"credential":"{secret}"}}"#);
    let conn = rusqlite::Connection::open(database(source.path())).unwrap();
    for _ in 0..2 {
        conn.execute(
            "INSERT INTO events (kind, actor, time, detail)
             VALUES ('fixture', 'worker', '2026-09-03T00:00:00Z', ?1)",
            [&detail],
        )
        .unwrap();
    }
    let store_uuid: String = conn
        .query_row("SELECT uuid FROM workspace WHERE id = 1", [], |row| {
            row.get(0)
        })
        .unwrap();

    let findings = scan_live_findings(&conn)
        .unwrap()
        .into_iter()
        .filter(|finding| {
            finding.selector.starts_with("live:events:")
                && finding.field_path == "detail"
                && finding.rule_id == "aws-access-key-id"
                && finding.is_blocking_match()
        })
        .collect::<Vec<_>>();
    assert_eq!(findings.len(), 2);
    assert_ne!(findings[0].selector, findings[1].selector);
    assert_ne!(findings[0].fingerprint, findings[1].fingerprint);
    let first_fingerprint = findings[0].fingerprint.clone();
    let second_fingerprint = findings[1].fingerprint.clone();
    drop(conn);

    let preview = redact(source.path(), &second_fingerprint, true);
    let conn = rusqlite::Connection::open(database(source.path())).unwrap();
    let after_preview: (i64, i64) = conn
        .query_row(
            "SELECT
                (SELECT COUNT(*) FROM events
                 WHERE origin_store_uuid IS NULL AND origin_event_sequence IS NULL),
                (SELECT COUNT(*) FROM redaction_receipts)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(after_preview, (2, 0));
    drop(conn);

    let second_receipt = redact(source.path(), &second_fingerprint, false);
    assert_eq!(
        preview["sanitized_record_hash"],
        second_receipt["sanitized_record_hash"]
    );
    let conn = rusqlite::Connection::open(database(source.path())).unwrap();
    let original_events = conn
        .prepare(
            "SELECT origin_store_uuid, origin_event_sequence, detail, event_sha256
             FROM events WHERE kind = 'fixture' ORDER BY sequence",
        )
        .unwrap()
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    assert_eq!(original_events.len(), 2);
    assert_eq!(
        (&original_events[0].0, original_events[0].1),
        (&store_uuid, 1)
    );
    assert_eq!(
        (&original_events[1].0, original_events[1].1),
        (&store_uuid, 2)
    );
    assert_eq!(
        original_events
            .iter()
            .filter(|event| event.2 == detail)
            .count(),
        1
    );
    let sanitized_event = original_events
        .iter()
        .find(|event| {
            serde_json::from_str::<Value>(&event.2).unwrap()["credential"] == REDACTION_MARKER
        })
        .unwrap();
    assert_eq!(
        sanitized_event.3.as_deref(),
        second_receipt["sanitized_record_hash"].as_str()
    );
    let pending_audit_identity: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM events
             WHERE kind = 'historical_redaction'
               AND origin_store_uuid IS NULL
               AND origin_event_sequence IS NULL",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(pending_audit_identity, 1);
    drop(conn);

    let replay = redact(source.path(), &second_fingerprint, false);
    assert_eq!(replay["receipt_id"], second_receipt["receipt_id"]);
    let conn = rusqlite::Connection::open(database(source.path())).unwrap();
    let pending_after_replay: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM events
             WHERE kind = 'historical_redaction'
               AND origin_store_uuid IS NULL
               AND origin_event_sequence IS NULL",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(pending_after_replay, 1);
    drop(conn);

    let first_receipt = redact(source.path(), &first_fingerprint, false);
    let conn = rusqlite::Connection::open(database(source.path())).unwrap();
    let finalized = conn
        .prepare(
            "SELECT origin_store_uuid, origin_event_sequence, detail, event_sha256
             FROM events WHERE kind = 'fixture' ORDER BY sequence",
        )
        .unwrap()
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    assert_eq!(finalized.len(), 2);
    for event in &finalized {
        assert_eq!(
            serde_json::from_str::<Value>(&event.2).unwrap()["credential"],
            REDACTION_MARKER
        );
    }
    let finalized_hashes = finalized
        .iter()
        .map(|event| event.3.as_str())
        .collect::<Vec<_>>();
    assert!(finalized_hashes.contains(&first_receipt["sanitized_record_hash"].as_str().unwrap()));
    assert!(finalized_hashes.contains(&second_receipt["sanitized_record_hash"].as_str().unwrap()));
    drop(conn);

    let restored = workspace("restored");
    bead(restored.path())
        .args([
            "sync",
            "import-only",
            "--input",
            source
                .path()
                .join(".beads/checkpoint/forensic.jsonl")
                .to_str()
                .unwrap(),
            "--restore-into-empty",
            "--actor",
            "event-identity-test",
            "--no-auto-flush",
        ])
        .assert()
        .success();
    let conn = rusqlite::Connection::open(database(restored.path())).unwrap();
    let restored_events = conn
        .prepare(
            "SELECT origin_store_uuid, origin_event_sequence, detail
             FROM events WHERE kind = 'fixture'
             ORDER BY origin_store_uuid, origin_event_sequence",
        )
        .unwrap()
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    assert_eq!(restored_events.len(), 2);
    assert_eq!(restored_events[0].0, store_uuid);
    assert_eq!(restored_events[0].1, 1);
    assert_eq!(restored_events[1].1, 2);
    assert!(restored_events
        .iter()
        .all(
            |event| serde_json::from_str::<Value>(&event.2).unwrap()["credential"]
                == REDACTION_MARKER
        ));
    assert!(!scan_live_findings(&conn)
        .unwrap()
        .iter()
        .any(|finding| finding.rule_id == "aws-access-key-id" && finding.is_blocking_match()));
}

#[test]
fn event_redaction_preserves_unknown_fields_in_checkpoint_records_and_pointer() {
    let source = workspace("unknown-fields");
    use_monolithic_checkpoint(source.path());

    let event_sentinel = serde_json::json!({
        "shape": ["event", 23, {"opaque": true}],
        "version": 7
    });
    let pointer_sentinel = serde_json::json!({
        "shape": {"pointer": [null, false, "stable"]},
        "version": 11
    });
    let receipt_sentinel = serde_json::json!({
        "shape": ["receipt", {"opaque": "retained"}],
        "version": 5
    });
    let event_extensions = serde_json::json!({
        "future_event_fixture_sentinel": event_sentinel
    });
    let receipt_extensions = serde_json::json!({
        "future_redaction_receipt_fixture_sentinel": receipt_sentinel
    });

    let secret = shaped_value();
    let detail_value = serde_json::json!({
        "credential": secret,
        "context": {"label": "stable fixture", "values": [1, true, null]}
    });
    let detail = serde_json::to_string(&detail_value).unwrap();
    let mut store =
        SqliteStore::from_conn(open_configured_connection(&database(source.path())).unwrap());
    store
        .conn()
        .execute(
            "INSERT INTO events (kind, actor, time, detail, extensions_json)
             VALUES ('fixture', 'worker', '2026-09-03T00:00:00Z', ?1, ?2)",
            rusqlite::params![detail, event_extensions.to_string()],
        )
        .unwrap();
    let checkpoint_base = source.path().join(".beads");
    let checkpoint_config = bead_rs::service::load_checkpoint_config(&checkpoint_base).unwrap();
    bead_rs::service::publish_forensic_checkpoint(&mut store, &checkpoint_config, &checkpoint_base)
        .unwrap();
    let pointer_path = source.path().join(".beads/checkpoint/current.json");
    let mut seeded_pointer: Value =
        serde_json::from_slice(&fs::read(&pointer_path).unwrap()).unwrap();
    seeded_pointer.as_object_mut().unwrap().insert(
        "future_pointer_fixture_sentinel".to_string(),
        pointer_sentinel.clone(),
    );
    fs::write(
        &pointer_path,
        serde_json::to_vec_pretty(&seeded_pointer).unwrap(),
    )
    .unwrap();
    let (before_pointer, before_records) = current_checkpoint(source.path());
    assert_eq!(
        before_pointer["future_pointer_fixture_sentinel"],
        pointer_sentinel
    );
    let before_event = before_records
        .iter()
        .find(|record| record["record_type"] == "event")
        .unwrap()["event"]
        .clone();
    assert_eq!(
        before_event["future_event_fixture_sentinel"],
        event_sentinel
    );
    assert_eq!(before_event["detail"], detail_value);

    let finding = scan_live_findings(store.conn())
        .unwrap()
        .into_iter()
        .find(|finding| {
            finding.selector.starts_with("live:events:")
                && finding.field_path == "detail"
                && finding.rule_id == "aws-access-key-id"
                && finding.is_blocking_match()
        })
        .expect("the synthetic event credential must produce a blocking finding");
    let outcome = redact_finding(
        &mut store,
        source.path(),
        &finding.fingerprint,
        "event-extension-test",
        "remove synthetic fixture credential",
    )
    .unwrap();
    assert_eq!(
        outcome.receipt.publication_state,
        bead_rs::model::redaction::PublicationState::Committed
    );

    let changed = store
        .conn()
        .execute(
            "UPDATE redaction_receipts SET extensions_json = ?1 WHERE receipt_id = ?2",
            rusqlite::params![receipt_extensions.to_string(), outcome.receipt.receipt_id],
        )
        .unwrap();
    assert_eq!(changed, 1);
    let rewritten_detail: String = store
        .conn()
        .query_row(
            "SELECT detail FROM events WHERE kind = 'fixture'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(rewritten_detail, detail.replace(&secret, REDACTION_MARKER));
    assert!(!rewritten_detail.contains(&secret));
    drop(store);

    bead(source.path())
        .args(["redact", "--resume", &outcome.receipt.receipt_id, "--json"])
        .assert()
        .success();

    let (after_pointer, after_records) = current_checkpoint(source.path());
    assert_eq!(
        after_pointer["future_pointer_fixture_sentinel"],
        before_pointer["future_pointer_fixture_sentinel"]
    );
    let after_event = after_records
        .iter()
        .find(|record| record["record_type"] == "event")
        .unwrap()["event"]
        .clone();
    let mut expected_event = before_event;
    expected_event["detail"]["credential"] = Value::String(REDACTION_MARKER.to_string());
    assert_eq!(after_event, expected_event);
    assert_eq!(after_event["future_event_fixture_sentinel"], event_sentinel);
    assert!(!after_event.to_string().contains(&secret));

    let receipt = after_records
        .iter()
        .find(|record| record["record_type"] == "redaction_receipt")
        .unwrap()["redaction_receipt"]
        .clone();
    assert_eq!(
        receipt["future_redaction_receipt_fixture_sentinel"],
        receipt_sentinel
    );
}

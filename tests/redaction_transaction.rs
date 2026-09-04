//! BR-T16 transactional historical-redaction conformance.

use assert_cmd::Command;
use bead_rs::model::redaction::{PublicationState, RedactionError, REDACTION_MARKER};
use bead_rs::service::redaction::redact_finding;
use bead_rs::service::secret_diagnostics::scan_live_findings;
use bead_rs::store::{open_configured_connection, SqliteStore};
use rusqlite::params;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Barrier};

fn workspace() -> tempfile::TempDir {
    let workspace = tempfile::Builder::new()
        .prefix("bead-redaction-transaction-")
        .tempdir_in("/var/tmp")
        .unwrap();
    Command::cargo_bin("bead")
        .unwrap()
        .current_dir(workspace.path())
        .env("HOME", workspace.path())
        .args(["init", "--prefix", "redact", "--no-auto-flush"])
        .assert()
        .success();
    workspace
}

fn database(root: &Path) -> PathBuf {
    root.join(".beads/beads.db")
}

fn store(root: &Path) -> SqliteStore {
    SqliteStore::from_conn(open_configured_connection(&database(root)).unwrap())
}

fn shaped_value() -> String {
    ["AK", "IA", "7Q9W2E4R6T8Y1U3I"].concat()
}

fn insert_issue(conn: &rusqlite::Connection, id: &str, description: &str, assignee: Option<&str>) {
    conn.execute(
        "INSERT INTO issues (
            id, title, description, notes, priority, issue_type, base_status,
            assignee, created_at, updated_at, revision
         ) VALUES (?1, 'stable title', ?2, 'stable notes', 2, 'task', 'open',
                   ?3, '2026-09-03T00:00:00Z', '2026-09-03T00:00:00Z', 1)",
        params![id, description, assignee],
    )
    .unwrap();
}

#[test]
fn issue_redaction_is_exact_atomic_and_idempotent() {
    let workspace = workspace();
    let mut store = store(workspace.path());
    let value = shaped_value();
    let description = format!("é first {value} middle {value} tail");
    insert_issue(store.conn(), "redact-1", &description, None);

    let live_findings = scan_live_findings(store.conn()).unwrap();
    let findings: Vec<_> = live_findings
        .iter()
        .filter(|finding| {
            finding.field_path == "description"
                && finding.rule_id == "aws-access-key-id"
                && finding.is_blocking_match()
        })
        .collect();
    assert_eq!(findings.len(), 2);
    let fingerprint = findings[1].fingerprint.clone();
    let first_start = findings[0].start;
    let selected_start = findings[1].start;

    let outcome = redact_finding(
        &mut store,
        workspace.path(),
        &fingerprint,
        "operator",
        "remove exposed credential",
    )
    .unwrap();
    assert!(!outcome.is_replay);
    assert_eq!(
        outcome.receipt.publication_state,
        PublicationState::Committed
    );
    assert_eq!(outcome.receipt.affected_issue_revision, Some(2));
    assert_eq!(outcome.receipt.selector.byte_start, selected_start as i64);

    let (after, title, notes, revision, updated_at): (String, String, String, i64, String) = store
        .conn()
        .query_row(
            "SELECT description, title, notes, revision, updated_at
             FROM issues WHERE id = 'redact-1'",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .unwrap();
    let expected = format!("é first {value} middle {REDACTION_MARKER} tail");
    assert_eq!(after, expected);
    assert_eq!(&after[first_start..first_start + value.len()], value);
    assert_eq!(
        (title.as_str(), notes.as_str()),
        ("stable title", "stable notes")
    );
    assert_eq!(revision, 2);
    assert_eq!(updated_at, "2026-09-03T00:00:00Z");

    let rendered = serde_json::to_string(&outcome).unwrap();
    assert!(!rendered.contains(&value));
    assert!(rendered.contains(&fingerprint));

    let replay = redact_finding(
        &mut store,
        workspace.path(),
        &fingerprint,
        "operator",
        "remove exposed credential",
    )
    .unwrap();
    assert!(replay.is_replay);
    assert_eq!(replay.receipt.receipt_id, outcome.receipt.receipt_id);

    let conflict = redact_finding(
        &mut store,
        workspace.path(),
        &fingerprint,
        "operator",
        "different request",
    )
    .unwrap_err();
    assert!(matches!(conflict, RedactionError::Conflict(_)));

    let counts: (i64, i64, i64, i64, i64) = store
        .conn()
        .query_row(
            "SELECT
                (SELECT COUNT(*) FROM redaction_findings),
                (SELECT COUNT(*) FROM redaction_receipts),
                (SELECT COUNT(*) FROM redaction_epochs),
                (SELECT COUNT(*) FROM redaction_tombstones),
                (SELECT COUNT(*) FROM events WHERE kind = 'historical_redaction')",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(counts, (1, 1, 1, 1, 1));
}

#[test]
fn unsupported_scanner_field_conflicts_without_mutation() {
    let workspace = workspace();
    let mut store = store(workspace.path());
    let value = shaped_value();
    insert_issue(store.conn(), "redact-unsupported", "clean", Some(&value));
    let live_findings = scan_live_findings(store.conn()).unwrap();
    let finding = live_findings
        .iter()
        .find(|finding| {
            finding.field_path == "assignee"
                && finding.rule_id == "aws-access-key-id"
                && finding.is_blocking_match()
        })
        .unwrap();
    let error = redact_finding(
        &mut store,
        workspace.path(),
        &finding.fingerprint,
        "operator",
        "unsupported field check",
    )
    .unwrap_err();
    assert!(matches!(error, RedactionError::Conflict(_)));
    let stored: String = store
        .conn()
        .query_row(
            "SELECT assignee FROM issues WHERE id = 'redact-unsupported'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(stored, value);
    let receipts: i64 = store
        .conn()
        .query_row("SELECT COUNT(*) FROM redaction_receipts", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(receipts, 0);
}

#[test]
fn every_supported_nonissue_record_preserves_identity_and_integrity() {
    let workspace = workspace();
    let mut store = store(workspace.path());
    let value = shaped_value();
    insert_issue(store.conn(), "redact-related", "clean", None);
    let conn = store.conn();
    conn.execute(
        "INSERT INTO comments
            (id, issue_id, author, body, created_at)
         VALUES ('comment-1', 'redact-related', 'author', ?1, '2026-09-03T00:00:00Z')",
        [&value],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO issue_data (issue_id, namespace, schema_ref, value)
         VALUES ('redact-related', 'fixture', 'urn:test', ?1)",
        [format!(r#"{{"credential":"{value}"}}"#)],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO external_references (issue_id, namespace, key, value)
         VALUES ('redact-related', 'fixture', 'key', ?1)",
        [&value],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO attempt_outcomes (
            receipt_id, attempt_id, issue_id, outcome, action, reason,
            canonical_request_hash, prior_attempt_tier, resulting_attempt_tier,
            resulting_issue_revision, actor, created_at, evidence_refs_json,
            resulting_state
         ) VALUES ('attempt-receipt', 'attempt-1', 'redact-related',
                   'verified_success', 'none', 'reason', ?1, 0, 0, 1, 'worker',
                   '2026-09-03T00:00:00Z', ?2, 'open')",
        params!["a".repeat(64), format!(r#"["fixture:{value}"]"#)],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO recurrence_templates (
            id, title, description, base_title_template, base_description,
            priority, issue_type, labels_json, created_at
         ) VALUES ('template-1', 'title', ?1, 'base title', 'base description',
                   2, 'task', '[]', '2026-09-03T00:00:00Z')",
        [&value],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO events (
            issue_id, kind, actor, time, detail, origin_store_uuid,
            origin_event_sequence, event_sha256, local_ingestion_sequence
         ) VALUES ('redact-related', 'fixture', 'actor', '2026-09-03T00:00:00Z',
                   ?1, 'origin-store', 7, ?2, 1)",
        params![format!(r#"{{"credential":"{value}"}}"#), "b".repeat(64)],
    )
    .unwrap();
    conn.execute(
        r#"INSERT INTO provenance_receipts (
            receipt_id, schema_ref, kind, source_store_uuid, target_store_uuid,
            source_root_sha256, actor, created_at, counts_json, result,
            summary_event_identity, receipt_sha256
         ) VALUES ('provenance-1',
                   'urn:bead-rs:schema:provenance-receipt:native-v1',
                   'restore', 'source', 'target', ?1, ?2,
                   '2026-09-03T00:00:00Z',
                   '{"issues":0,"events":0,"provenance_receipts":0}',
                   'success', NULL, ?3)"#,
        params!["c".repeat(64), &value, "d".repeat(64)],
    )
    .unwrap();

    let live_findings = scan_live_findings(store.conn()).unwrap();
    let supported = live_findings
        .iter()
        .filter(|finding| {
            finding.rule_id == "aws-access-key-id"
                && finding.is_blocking_match()
                && [
                    ("live:comments:", "body"),
                    ("live:issue_data:", "value"),
                    ("live:external_references:", "value"),
                    ("live:attempt_outcomes:", "evidence_refs_json"),
                    ("live:recurrence_templates:", "description"),
                    ("live:events:", "detail"),
                    ("live:provenance_receipts:", "actor"),
                ]
                .iter()
                .any(|(prefix, field)| {
                    finding.selector.starts_with(prefix) && finding.field_path == *field
                })
        })
        .map(|finding| finding.fingerprint.clone())
        .collect::<Vec<_>>();
    assert_eq!(supported.len(), 7);

    let mut outcomes = Vec::new();
    for fingerprint in supported {
        outcomes.push(
            redact_finding(
                &mut store,
                workspace.path(),
                &fingerprint,
                "operator",
                "remove exposed credential",
            )
            .unwrap(),
        );
    }

    let conn = store.conn();
    let marker_count: i64 = conn
        .query_row(
            "SELECT
                (SELECT COUNT(*) FROM comments WHERE body = ?1) +
                (SELECT COUNT(*) FROM external_references WHERE value = ?1) +
                (SELECT COUNT(*) FROM recurrence_templates WHERE description = ?1) +
                (SELECT COUNT(*) FROM provenance_receipts WHERE actor = ?1)",
            [REDACTION_MARKER],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(marker_count, 4);
    let data: String = conn
        .query_row(
            "SELECT value FROM issue_data WHERE issue_id = 'redact-related'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let detail: String = conn
        .query_row(
            "SELECT detail FROM events WHERE origin_store_uuid = 'origin-store' AND origin_event_sequence = 7",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let evidence: String = conn
        .query_row(
            "SELECT evidence_refs_json FROM attempt_outcomes WHERE attempt_id = 'attempt-1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&data).unwrap()["credential"],
        REDACTION_MARKER
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&detail).unwrap()["credential"],
        REDACTION_MARKER
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&evidence).unwrap()[0],
        format!("fixture:{REDACTION_MARKER}")
    );

    let (origin, origin_sequence, event_hash): (String, i64, String) = conn
        .query_row(
            "SELECT origin_store_uuid, origin_event_sequence, event_sha256
             FROM events WHERE origin_store_uuid = 'origin-store' AND origin_event_sequence = 7",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!((origin.as_str(), origin_sequence), ("origin-store", 7));
    assert_eq!(event_hash.len(), 64);
    assert_ne!(event_hash, "b".repeat(64));
    let event_outcome = outcomes
        .iter()
        .find(|outcome| outcome.receipt.selector.record_kind == "events")
        .unwrap();
    assert_eq!(event_hash, event_outcome.receipt.sanitized_record_hash);

    let (revision, receipts, tombstones): (i64, i64, i64) = conn
        .query_row(
            "SELECT
                (SELECT revision FROM issues WHERE id = 'redact-related'),
                (SELECT COUNT(*) FROM redaction_receipts),
                (SELECT COUNT(*) FROM redaction_tombstones)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(revision, 5);
    assert_eq!((receipts, tombstones), (7, 7));
}

#[test]
fn concurrent_identical_requests_commit_once_and_replay_once() {
    let workspace = workspace();
    let root = workspace.path().to_path_buf();
    let mut initial = store(&root);
    let value = shaped_value();
    insert_issue(initial.conn(), "redact-concurrent", &value, None);
    let fingerprint = scan_live_findings(initial.conn())
        .unwrap()
        .into_iter()
        .find(|finding| finding.rule_id == "aws-access-key-id" && finding.is_blocking_match())
        .unwrap()
        .fingerprint;
    drop(initial);

    let barrier = Arc::new(Barrier::new(3));
    let mut handles = Vec::new();
    for _ in 0..2 {
        let root = root.clone();
        let fingerprint = fingerprint.clone();
        let barrier = barrier.clone();
        handles.push(std::thread::spawn(move || {
            let mut store = store(&root);
            barrier.wait();
            redact_finding(
                &mut store,
                &root,
                &fingerprint,
                "operator",
                "remove exposed credential",
            )
            .unwrap()
        }));
    }
    barrier.wait();
    let outcomes = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        outcomes.iter().filter(|outcome| outcome.is_replay).count(),
        1
    );
    assert_eq!(
        outcomes[0].receipt.receipt_id,
        outcomes[1].receipt.receipt_id
    );

    let mut store = store(&root);
    let counts: (i64, i64) = store
        .conn()
        .query_row(
            "SELECT
                (SELECT COUNT(*) FROM redaction_receipts),
                (SELECT COUNT(*) FROM events WHERE kind = 'historical_redaction')",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(counts, (1, 1));
}

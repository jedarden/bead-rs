//! Independent controlled-identity fixtures; synthetic candidates only.
use assert_cmd::Command;
use bead_rs::service::{redaction, secret_diagnostics};
use bead_rs::store::{open_configured_connection, SqliteStore};
use rusqlite::params;
use serde_json::Value;
use std::path::Path;

fn bead(root: &Path) -> Command {
    let mut command = Command::cargo_bin("bead").unwrap();
    command.current_dir(root);
    command
}
fn workspace() -> (tempfile::TempDir, SqliteStore) {
    let root = tempfile::Builder::new()
        .prefix("bead-identity-rekey-")
        .tempdir_in("/var/tmp")
        .unwrap();
    bead(root.path())
        .args(["init", "--prefix", "keys", "--no-auto-flush"])
        .assert()
        .success();
    let store = SqliteStore::from_conn(
        open_configured_connection(&root.path().join(".beads/beads.db")).unwrap(),
    );
    (root, store)
}
fn candidate() -> String {
    ["AK", "IA", "7Q9W2E4R6T8Y1U3I"].concat()
}
fn seed(store: &mut SqliteStore) {
    for id in ["keys-1", "keys-2", "keys-3"] {
        store.conn().execute("INSERT INTO issues (id,title,priority,base_status,created_at,updated_at,revision) VALUES (?1,'stable title',2,'open','2026-10-03T00:00:00Z','2026-10-03T00:00:00Z',1)",[id]).unwrap();
    }
}
fn selected(store: &mut SqliteStore, field: &str) -> String {
    secret_diagnostics::scan_live_findings(store.conn())
        .unwrap()
        .into_iter()
        .find(|finding| {
            finding.rule_id == "aws-access-key-id"
                && finding.field_path == field
                && finding.is_blocking_match()
        })
        .unwrap()
        .fingerprint
}
fn key(preview: &redaction::RedactionPreview) -> String {
    preview.identity_rekey.as_ref().unwrap()["new_value"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn identity_catalog_dry_run_publication_and_restore_preserve_related_rows() {
    for (case, field) in [
        ("label", "label"),
        ("data", "namespace"),
        ("reference_namespace", "namespace"),
        ("reference_key", "key"),
        ("unique_key", "key"),
        ("unique_namespace", "namespace"),
        ("view", "name"),
    ] {
        let (root, mut store) = workspace();
        seed(&mut store);
        let value = candidate();
        match case {
            "label" => {
                store
                    .conn()
                    .execute(
                        "INSERT INTO labels (issue_id,label) VALUES ('keys-1',?1)",
                        [&value],
                    )
                    .unwrap();
            }
            "data" => {
                store.conn().execute("INSERT INTO issue_data (issue_id,namespace,schema_ref,value) VALUES ('keys-1',?1,'urn:fixture:data','{}')",[&value]).unwrap();
            }
            "reference_namespace" => {
                store.conn().execute("INSERT INTO external_references (issue_id,namespace,key,value) VALUES ('keys-1',?1,'ticket','stable value')",[&value]).unwrap();
            }
            "reference_key" => {
                store.conn().execute("INSERT INTO external_references (issue_id,namespace,key,value) VALUES ('keys-1','tracker',?1,'stable value')",[&value]).unwrap();
            }
            "unique_key" => {
                store.conn().execute("INSERT INTO external_references (issue_id,namespace,key,value) VALUES ('keys-1','tracker','unique-ref',?1)",[&value]).unwrap();
                store.conn().execute("INSERT INTO unique_reference_bindings (issue_id,namespace,key) VALUES ('keys-1','tracker',?1)",[&value]).unwrap();
            }
            "unique_namespace" => {
                store.conn().execute("INSERT INTO external_references (issue_id,namespace,key,value) VALUES ('keys-1',?1,'unique-ref','stable binding'),('keys-1',?1,'ticket','stable value')",[&value]).unwrap();
                store.conn().execute("INSERT INTO unique_reference_bindings (issue_id,namespace,key) VALUES ('keys-1',?1,'stable binding')",[&value]).unwrap();
            }
            "view" => {
                store.conn().execute("INSERT INTO saved_views (id,name,description,query_json,created_at,updated_at) VALUES ('fixture-view',?1,'stable description','{}','2026-10-03T00:00:00Z','2026-10-03T00:00:00Z')",[&value]).unwrap();
            }
            _ => unreachable!(),
        }
        let fingerprint = selected(&mut store, field);
        let locks = redaction::acquire_redaction_locks(root.path()).unwrap();
        let preview = redaction::preview_redaction_holding(
            &mut store,
            &locks,
            &fingerprint,
            "tester",
            "identity fixture",
        )
        .unwrap();
        let replacement = key(&preview);
        assert!(replacement.starts_with("redacted-"));
        assert_eq!(replacement.len(), 41);
        assert_eq!(
            selected(&mut store, field),
            fingerprint,
            "dry run changed the identity"
        );
        let outcome = redaction::redact_finding_holding(
            &mut store,
            &locks,
            &fingerprint,
            "tester",
            "identity fixture",
        )
        .unwrap();
        assert_eq!(
            preview.sanitized_record_hash,
            outcome.receipt.sanitized_record_hash
        );
        assert!(!serde_json::to_string(&outcome).unwrap().contains(&value));
        assert!(!secret_diagnostics::scan_live_findings(store.conn())
            .unwrap()
            .iter()
            .any(|finding| finding.rule_id == "aws-access-key-id" && finding.is_blocking_match()));
        if case == "unique_key" {
            let pair:(String,String)=store.conn().query_row("SELECT b.key,r.value FROM unique_reference_bindings b JOIN external_references r USING(issue_id,namespace) WHERE r.key='unique-ref'",[],|row|Ok((row.get(0)?,row.get(1)?))).unwrap();
            assert_eq!(pair, (replacement.clone(), replacement.clone()));
        }
        if case == "unique_namespace" {
            let count: i64 = store
                .conn()
                .query_row(
                    "SELECT COUNT(*) FROM external_references WHERE namespace=?1",
                    [&replacement],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 2);
            let namespace: String = store
                .conn()
                .query_row(
                    "SELECT namespace FROM unique_reference_bindings",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(namespace, replacement);
        }
        let revision: i64 = store
            .conn()
            .query_row("SELECT revision FROM issues WHERE id='keys-1'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(revision, if case == "view" { 1 } else { 2 });
        drop(locks);
        drop(store);
        bead(root.path())
            .args(["redact", "--resume", &outcome.receipt.receipt_id, "--json"])
            .assert()
            .success();
        let (restored, _store) = workspace();
        let checkpoint = root.path().join(".beads/checkpoint");
        bead(restored.path())
            .args([
                "sync",
                "import-only",
                "--input",
                checkpoint.to_str().unwrap(),
                "--restore-into-empty",
                "--actor",
                "tester",
            ])
            .assert()
            .success();
        let conn = open_configured_connection(&restored.path().join(".beads/beads.db")).unwrap();
        assert!(!secret_diagnostics::scan_live_findings(&conn)
            .unwrap()
            .iter()
            .any(|finding| finding.rule_id == "aws-access-key-id" && finding.is_blocking_match()));
    }
}

#[test]
fn shared_resource_rekey_preserves_contention_and_refuses_recovery_resurrection() {
    let (root, mut store) = workspace();
    seed(&mut store);
    let value = candidate();
    for id in ["keys-1", "keys-2"] {
        store
            .conn()
            .execute(
                "INSERT INTO issue_resource_keys (issue_id,resource_key) VALUES (?1,?2)",
                params![id, value],
            )
            .unwrap();
    }
    store.conn().execute("INSERT INTO resource_locks (resource_key,issue_id,lease_fencing_token,acquired_at) VALUES (?1,'keys-1',7,'2026-10-03T00:00:00Z')",[&value]).unwrap();
    let input = root.path().join("older.jsonl");
    bead_rs::service::flush_checkpoint(&mut store, &input).unwrap();
    let fingerprint = selected(&mut store, "resource_key");
    let locks = redaction::acquire_redaction_locks(root.path()).unwrap();
    let preview = redaction::preview_redaction_holding(
        &mut store,
        &locks,
        &fingerprint,
        "tester",
        "resource fixture",
    )
    .unwrap();
    let replacement = key(&preview);
    let outcome = redaction::redact_finding_holding(
        &mut store,
        &locks,
        &fingerprint,
        "tester",
        "resource fixture",
    )
    .unwrap();
    let locks_row: (String, String, i64, String) = store
        .conn()
        .query_row(
            "SELECT resource_key,issue_id,lease_fencing_token,acquired_at FROM resource_locks",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(
        locks_row,
        (
            replacement.clone(),
            "keys-1".into(),
            7,
            "2026-10-03T00:00:00Z".into()
        )
    );
    let declarations: i64 = store
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM issue_resource_keys WHERE resource_key=?1",
            [&replacement],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(declarations, 2);
    let revisions: Vec<i64> = store
        .conn()
        .prepare("SELECT revision FROM issues ORDER BY id")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(revisions, vec![2, 2, 1]);
    drop(locks);
    drop(store);
    bead(root.path())
        .args(["redact", "--resume", &outcome.receipt.receipt_id])
        .assert()
        .success();
    let old = std::fs::read_to_string(&input).unwrap();
    let advancing = old
        .lines()
        .map(|line| {
            let mut issue: Value = serde_json::from_str(line).unwrap();
            issue["revision"] = serde_json::json!(100);
            issue["updated_at"] = serde_json::json!("2030-01-01T00:00:00Z");
            serde_json::to_string(&issue).unwrap()
        })
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(&input, advancing).unwrap();
    let output = bead(root.path())
        .args([
            "sync",
            "import-only",
            "--input",
            "older.jsonl",
            "--merge",
            "--actor",
            "tester",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!String::from_utf8_lossy(&output.stderr).contains(&value));
    let conn = open_configured_connection(&root.path().join(".beads/beads.db")).unwrap();
    let declarations: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM issue_resource_keys WHERE resource_key=?1",
            [&replacement],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(declarations, 2);
}

#[test]
fn collision_and_coupled_batch_refuse_without_partial_rekey() {
    let (root, mut store) = workspace();
    seed(&mut store);
    let value = candidate();
    for id in ["keys-1", "keys-2"] {
        store
            .conn()
            .execute(
                "INSERT INTO issue_resource_keys (issue_id,resource_key) VALUES (?1,?2)",
                params![id, value],
            )
            .unwrap();
    }
    let fingerprints: Vec<String> = secret_diagnostics::scan_live_findings(store.conn())
        .unwrap()
        .into_iter()
        .filter(|finding| {
            finding.rule_id == "aws-access-key-id" && finding.field_path == "resource_key"
        })
        .map(|finding| finding.fingerprint)
        .collect();
    assert_eq!(fingerprints.len(), 2);
    let locks = redaction::acquire_redaction_locks(root.path()).unwrap();
    assert!(redaction::redact_findings_holding(
        &mut store,
        &locks,
        &fingerprints,
        "tester",
        "coupled selection",
        false
    )
    .is_err());
    let preview = redaction::preview_redaction_holding(
        &mut store,
        &locks,
        &fingerprints[0],
        "tester",
        "collision fixture",
    )
    .unwrap();
    store
        .conn()
        .execute(
            // No composite primary-key conflict: keys-3 does not declare the
            // old key. Global identity still makes this a collision.
            "INSERT INTO issue_resource_keys (issue_id,resource_key) VALUES ('keys-3',?1)",
            [key(&preview)],
        )
        .unwrap();
    let error = redaction::redact_finding_holding(
        &mut store,
        &locks,
        &fingerprints[0],
        "tester",
        "collision fixture",
    )
    .err()
    .unwrap();
    assert!(error.to_string().contains("identity_rekey_collision"));
    assert!(!error.to_string().contains(&value));
    let old: i64 = store
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM issue_resource_keys WHERE resource_key=?1",
            [&value],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(old, 2);
    let receipts: i64 = store
        .conn()
        .query_row("SELECT COUNT(*) FROM redaction_receipts", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(receipts, 0);
    let revisions: Vec<i64> = store
        .conn()
        .prepare("SELECT revision FROM issues ORDER BY id")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(revisions, vec![1, 1, 1]);
}

#[test]
fn reference_alias_collisions_refuse_even_without_primary_key_overlap() {
    for family in ["namespace", "key", "unique-key"] {
        let (root, mut store) = workspace();
        seed(&mut store);
        let value = candidate();
        match family {
            "namespace" => {
                store.conn().execute("INSERT INTO external_references (issue_id,namespace,key,value) VALUES ('keys-1',?1,'original','stable')",[&value]).unwrap();
            }
            "key" => {
                store.conn().execute("INSERT INTO external_references (issue_id,namespace,key,value) VALUES ('keys-1','original',?1,'stable')",[&value]).unwrap();
            }
            _ => {
                store.conn().execute("INSERT INTO unique_reference_bindings (issue_id,namespace,key) VALUES ('keys-1','original',?1)",[&value]).unwrap();
                store.conn().execute("INSERT INTO external_references (issue_id,namespace,key,value) VALUES ('keys-1','original','unique-ref',?1)",[&value]).unwrap();
            }
        }
        let fingerprint = selected(
            &mut store,
            if family == "namespace" {
                "namespace"
            } else {
                "key"
            },
        );
        let locks = redaction::acquire_redaction_locks(root.path()).unwrap();
        let preview = redaction::preview_redaction_holding(
            &mut store,
            &locks,
            &fingerprint,
            "tester",
            "alias collision",
        )
        .unwrap();
        let replacement = key(&preview);
        match family {
            "namespace" => {
                store.conn().execute("INSERT INTO external_references (issue_id,namespace,key,value) VALUES ('keys-1',?1,'unrelated','stable')",[&replacement]).unwrap();
            }
            "key" => {
                store.conn().execute("INSERT INTO external_references (issue_id,namespace,key,value) VALUES ('keys-1','unrelated',?1,'stable')",[&replacement]).unwrap();
            }
            _ => {
                store.conn().execute("INSERT INTO unique_reference_bindings (issue_id,namespace,key) VALUES ('keys-1','unrelated',?1)",[&replacement]).unwrap();
                store.conn().execute("INSERT INTO external_references (issue_id,namespace,key,value) VALUES ('keys-1','unrelated','unique-ref',?1)",[&replacement]).unwrap();
            }
        }
        let error = redaction::redact_finding_holding(
            &mut store,
            &locks,
            &fingerprint,
            "tester",
            "alias collision",
        )
        .err()
        .unwrap();
        assert!(error.to_string().contains("identity_rekey_collision"));
        assert!(!error.to_string().contains(&value));
        assert_eq!(
            selected(
                &mut store,
                if family == "namespace" {
                    "namespace"
                } else {
                    "key"
                }
            ),
            fingerprint
        );
        let counts: (i64, i64) = store.conn().query_row("SELECT (SELECT COUNT(*) FROM redaction_receipts), (SELECT revision FROM issues WHERE id='keys-1')", [], |row| Ok((row.get(0)?,row.get(1)?))).unwrap();
        assert_eq!(counts, (0, 1));
    }
}

#[test]
fn independent_key_and_prose_batch_uses_one_epoch_and_one_revision() {
    let (root, mut store) = workspace();
    seed(&mut store);
    let value = candidate();
    store
        .conn()
        .execute(
            "INSERT INTO labels (issue_id,label) VALUES ('keys-1',?1)",
            [&value],
        )
        .unwrap();
    store
        .conn()
        .execute(
            "UPDATE issues SET description=?1 WHERE id='keys-1'",
            [&value],
        )
        .unwrap();
    let fingerprints: Vec<String> = secret_diagnostics::scan_live_findings(store.conn())
        .unwrap()
        .into_iter()
        .filter(|finding| finding.rule_id == "aws-access-key-id")
        .map(|finding| finding.fingerprint)
        .collect();
    assert_eq!(fingerprints.len(), 2);
    let locks = redaction::acquire_redaction_locks(root.path()).unwrap();
    let receipts = redaction::redact_findings_holding(
        &mut store,
        &locks,
        &fingerprints,
        "tester",
        "independent batch",
        false,
    )
    .unwrap();
    assert_eq!(receipts[0].receipt.epoch_id, receipts[1].receipt.epoch_id);
    assert!(receipts
        .iter()
        .all(|outcome| outcome.receipt.affected_issue_revision == Some(2)));
    let revision: i64 = store
        .conn()
        .query_row("SELECT revision FROM issues WHERE id='keys-1'", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(revision, 2);
}

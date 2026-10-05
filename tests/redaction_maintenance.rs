//! Interrupted maintenance and local byte-erasure regression coverage.
use assert_cmd::Command;
use bead_rs::service::redaction::redact_finding;
use bead_rs::service::secret_diagnostics::scan_live_findings;
use bead_rs::store::{open_configured_connection, SqliteStore};
use rusqlite::Connection;
use serde_json::Value;
use std::path::Path;

fn bead(root: &Path) -> Command {
    let mut command = Command::cargo_bin("bead").unwrap();
    command.current_dir(root);
    command
}

fn fixture() -> (tempfile::TempDir, SqliteStore, String) {
    let root = tempfile::Builder::new()
        .prefix("bead-redaction-maintenance-")
        .tempdir_in("/var/tmp")
        .unwrap();
    bead(root.path())
        .args([
            "init",
            "--skip-foreign-workspace",
            "--prefix",
            "maint",
            "--no-auto-flush",
        ])
        .assert()
        .success();
    let mut store = SqliteStore::from_conn(
        open_configured_connection(&root.path().join(".beads/beads.db")).unwrap(),
    );
    let synthetic = ["AK", "IA", "7Q9W2E4R6T8Y1U3I"].concat();
    store.conn().execute(
        "INSERT INTO issues (id,title,description,priority,issue_type,base_status,created_at,updated_at,revision)
         VALUES ('maint-1','stable title',?1,2,'task','open','2026-10-03T00:00:00Z','2026-10-03T00:00:00Z',1)",
        [&synthetic],
    ).unwrap();
    // This intentionally secret-bearing fixture predates the public flush
    // quarantine boundary, so construct its historical checkpoint directly.
    let checkpoint_base = root.path().join(".beads");
    let config = bead_rs::service::load_checkpoint_config(&checkpoint_base).unwrap();
    bead_rs::service::publish_forensic_checkpoint(&mut store, &config, &checkpoint_base).unwrap();
    (root, store, synthetic)
}

fn semantic_redaction(root: &Path, store: &mut SqliteStore) -> String {
    let finding = scan_live_findings(store.conn())
        .unwrap()
        .into_iter()
        .find(|finding| finding.is_blocking_match())
        .unwrap();
    redact_finding(
        store,
        root,
        &finding.fingerprint,
        "maintenance-test",
        "synthetic fixture cleanup",
    )
    .unwrap()
    .receipt
    .receipt_id
}

#[test]
fn interrupted_redaction_blocks_ordinary_flush_and_commit_until_resume() {
    let (root, mut store, _) = fixture();
    let before = std::fs::read(root.path().join(".beads/checkpoint/current.json")).unwrap();
    let receipt = semantic_redaction(root.path(), &mut store);
    drop(store);
    for args in [
        vec!["sync", "flush-only"],
        vec!["sync", "commit", "--dry-run"],
    ] {
        let output = bead(root.path()).args(args).output().unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("secret_redaction_pending"));
        assert_eq!(
            std::fs::read(root.path().join(".beads/checkpoint/current.json")).unwrap(),
            before
        );
    }
    bead(root.path())
        .args(["redact", "--resume", &receipt, "--json"])
        .assert()
        .success();
    bead(root.path())
        .args(["sync", "flush-only"])
        .assert()
        .success();
}

#[test]
fn completed_redaction_removes_synthetic_bytes_from_database_and_wal() {
    let (root, mut store, synthetic) = fixture();
    let receipt = semantic_redaction(root.path(), &mut store);
    let output = bead(root.path())
        .args(["redact", "--resume", &receipt, "--json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["publication_state"], "published");
    for filename in ["beads.db", "beads.db-wal"] {
        if let Ok(bytes) = std::fs::read(root.path().join(".beads").join(filename)) {
            assert!(
                !bytes
                    .windows(synthetic.len())
                    .any(|window| window == synthetic.as_bytes()),
                "removed bytes remain in {filename}"
            );
        }
    }
}

#[test]
fn reader_holds_cleanup_pending_and_resume_finishes_after_reader_closes() {
    let (root, mut store, _) = fixture();
    let reader = Connection::open(root.path().join(".beads/beads.db")).unwrap();
    reader.execute_batch("BEGIN").unwrap();
    let _: i64 = reader
        .query_row("SELECT COUNT(*) FROM issues", [], |row| row.get(0))
        .unwrap();
    let receipt = semantic_redaction(root.path(), &mut store);
    let blocked = bead(root.path())
        .args(["redact", "--resume", &receipt, "--json"])
        .output()
        .unwrap();
    assert!(!blocked.status.success());
    let state: String = store
        .conn()
        .query_row(
            "SELECT publication_state FROM redaction_receipts WHERE receipt_id = ?1",
            [&receipt],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(state, "committed");
    reader.execute_batch("ROLLBACK").unwrap();
    drop(reader);
    bead(root.path())
        .args(["redact", "--resume", &receipt, "--json"])
        .assert()
        .success();
}

use assert_cmd::Command;
use bead_rs::service::redaction::redact_finding;
use bead_rs::service::secret_diagnostics::scan_live_findings;
use bead_rs::store::{open_configured_connection, SqliteStore};
use serde_json::Value;
use std::path::Path;

fn bead(root: &Path) -> Command {
    let mut command = Command::cargo_bin("bead").unwrap();
    command
        .current_dir(root)
        .env("BEAD_ORG_SECRET_SCANNER", "off")
        .args(["--skip-foreign-workspace", "--no-auto-flush"]);
    command
}

fn workspace() -> (tempfile::TempDir, String) {
    let root = tempfile::Builder::new()
        .prefix("bead-history-")
        .tempdir_in("/var/tmp")
        .unwrap();
    bead(root.path()).arg("init").assert().success();
    let output = bead(root.path())
        .args(["create", "--title", "History probe"])
        .output()
        .unwrap();
    assert!(output.status.success());
    (
        root,
        String::from_utf8(output.stdout).unwrap().trim().to_owned(),
    )
}

fn history(root: &Path, issue_id: &str) -> Vec<Value> {
    let output = bead(root)
        .args(["history", issue_id, "--json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn closed_issue_history_is_ordered_and_reports_changed_fields() {
    let (root, issue_id) = workspace();
    bead(root.path())
        .args(["update", &issue_id, "--notes", "reviewed"])
        .assert()
        .success();
    bead(root.path())
        .args(["close", &issue_id, "--reason", "done"])
        .assert()
        .success();

    let rows = history(root.path(), &issue_id);
    assert_eq!(rows.len(), 3);
    assert_eq!(
        rows.iter()
            .map(|row| row["kind"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["created", "updated", "closed"]
    );
    assert_eq!(rows[0]["revision"], 1);
    assert_eq!(rows[1]["revision"], 2);
    assert_eq!(rows[1]["changed_fields"], serde_json::json!(["notes"]));
    assert_eq!(rows[2]["revision"], 3);
    for pair in rows.windows(2) {
        assert!(pair[0]["sequence"].as_i64().unwrap() < pair[1]["sequence"].as_i64().unwrap());
    }
    for row in &rows {
        assert!(row["time"].is_string());
        assert!(row["actor"].is_string());
    }
    let text = bead(root.path())
        .args(["history", &issue_id])
        .output()
        .unwrap();
    assert!(text.status.success());
    assert_eq!(String::from_utf8(text.stdout).unwrap().lines().count(), 3);
}

#[test]
fn missing_issue_exits_three_and_issue_without_events_returns_empty_array() {
    let (root, _) = workspace();
    let missing = bead(root.path())
        .args(["history", "missing-0000", "--json"])
        .output()
        .unwrap();
    assert_eq!(missing.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&missing.stderr).contains("Issue missing-0000 not found"));

    let conn = open_configured_connection(&root.path().join(".beads/beads.db")).unwrap();
    conn.execute(
        "INSERT INTO issues (id, title, priority, issue_type, base_status, created_at, updated_at, revision)
         VALUES ('history-no-events', 'Imported without events', 2, 'task', 'open',
                 '2026-10-15T00:00:00Z', '2026-10-15T00:00:00Z', 1)",
        [],
    )
    .unwrap();
    assert!(history(root.path(), "history-no-events").is_empty());
}

#[test]
fn redacted_note_history_shows_marker_without_old_value_or_fingerprint() {
    let (root, issue_id) = workspace();
    let secret = ["AK", "IA", "7Q9W2E4R6T8Y1U3I"].concat();
    let mut store = SqliteStore::from_conn(
        open_configured_connection(&root.path().join(".beads/beads.db")).unwrap(),
    );
    store
        .conn()
        .execute(
            "UPDATE issues SET notes = ?1 WHERE id = ?2",
            [&secret, &issue_id],
        )
        .unwrap();
    let finding = scan_live_findings(store.conn())
        .unwrap()
        .into_iter()
        .find(|finding| finding.field_path == "notes" && finding.is_blocking_match())
        .unwrap();
    let fingerprint = finding.fingerprint;
    let receipt = redact_finding(
        &mut store,
        root.path(),
        &fingerprint,
        "operator",
        "remove synthetic fixture",
    )
    .unwrap()
    .receipt
    .receipt_id;
    drop(store);
    Command::cargo_bin("bead")
        .unwrap()
        .current_dir(root.path())
        .env("BEAD_ORG_SECRET_SCANNER", "off")
        .arg("--skip-foreign-workspace")
        .args(["redact", "--resume", &receipt])
        .assert()
        .success();

    let output = bead(root.path())
        .args(["history", &issue_id, "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let rendered = String::from_utf8(output.stdout).unwrap();
    assert!(rendered.contains("[REDACTED:bead-rs]"));
    assert!(!rendered.contains(&secret));
    assert!(!rendered.contains(&fingerprint));
    let rows: Vec<Value> = serde_json::from_str(&rendered).unwrap();
    assert_eq!(rows.last().unwrap()["kind"], "historical_redaction");
    assert_eq!(
        rows.last().unwrap()["changed_fields"],
        serde_json::json!(["notes"])
    );
}

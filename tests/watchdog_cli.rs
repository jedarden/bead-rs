use assert_cmd::Command;
use rusqlite::{params, Connection};
use serde_json::Value;
use std::path::Path;

fn bead(workspace: &Path, args: &[&str]) -> std::process::Output {
    Command::cargo_bin("bead")
        .unwrap()
        .args(args)
        .current_dir(workspace)
        .output()
        .unwrap()
}

fn run(workspace: &Path, args: &[&str]) -> std::process::Output {
    let output = bead(workspace, args);
    assert!(
        output.status.success(),
        "bead {args:?} failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    output
}

fn stale_claim(workspace: &Path) -> String {
    run(workspace, &["init", "--prefix", "watchdog"]);
    let id = String::from_utf8(run(workspace, &["create", "--title", "watchdog target"]).stdout)
        .unwrap()
        .trim()
        .to_string();
    let claim = run(
        workspace,
        &["claim", "--assignee", "watchdog-test-worker", "--json"],
    );
    let claimed: Value = serde_json::from_slice(&claim.stdout).unwrap();
    assert_eq!(claimed["bead_id"], id);

    let conn = Connection::open(workspace.join(".beads/beads.db")).unwrap();
    conn.execute(
        "UPDATE issues SET updated_at = '2020-01-01T00:00:00Z' WHERE id = ?1",
        [&id],
    )
    .unwrap();
    id
}

fn issue_state(workspace: &Path, id: &str) -> (String, Option<String>) {
    let conn = Connection::open(workspace.join(".beads/beads.db")).unwrap();
    conn.query_row(
        "SELECT base_status, assignee FROM issues WHERE id = ?1",
        [id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .unwrap()
}

fn event_count(workspace: &Path) -> i64 {
    let conn = Connection::open(workspace.join(".beads/beads.db")).unwrap();
    conn.query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))
        .unwrap()
}

#[test]
fn dry_run_reports_a_forced_release_without_mutating() {
    let temp = tempfile::tempdir().unwrap();
    let id = stale_claim(temp.path());
    let before = event_count(temp.path());

    let output = run(
        temp.path(),
        &[
            "watchdog",
            "--threshold",
            "1m",
            "--force",
            "--dry-run",
            "--json",
        ],
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();

    assert_eq!(report["released_beads"].as_array().unwrap().len(), 1);
    assert!(report["released_beads"][0]["reason"]
        .as_str()
        .unwrap()
        .contains("DRY-RUN"));
    assert_eq!(
        issue_state(temp.path(), &id),
        (
            "in_progress".to_string(),
            Some("watchdog-test-worker".to_string())
        )
    );
    assert_eq!(event_count(temp.path()), before);
    assert!(!temp.path().join(".beads/watchdog-releases.jsonl").exists());
}

#[test]
fn force_releases_a_stale_claim_and_audits_the_override() {
    let temp = tempfile::tempdir().unwrap();
    let id = stale_claim(temp.path());
    let before = event_count(temp.path());

    let output = run(
        temp.path(),
        &["watchdog", "--threshold", "1m", "--force", "--json"],
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();

    assert_eq!(report["released_beads"].as_array().unwrap().len(), 1);
    assert_eq!(issue_state(temp.path(), &id), ("open".to_string(), None));
    assert!(event_count(temp.path()) >= before + 2);

    let conn = Connection::open(temp.path().join(".beads/beads.db")).unwrap();
    let override_events: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM events WHERE issue_id = ?1 AND kind = 'claim_override'",
            params![id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(override_events, 1);
    assert!(temp.path().join(".beads/watchdog-releases.jsonl").exists());
}

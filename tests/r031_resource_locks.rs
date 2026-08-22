//! R031 atomic workspace-local resource lock conformance tests.

use assert_cmd::Command;
use rusqlite::Connection;
use serial_test::serial;
use std::path::Path;
use tempfile::TempDir;

fn run(workspace: &Path, args: &[&str]) -> std::process::Output {
    Command::cargo_bin("bead")
        .unwrap()
        .args(args)
        .current_dir(workspace)
        .output()
        .unwrap()
}

fn create(workspace: &Path, title: &str, extra: &[&str]) -> String {
    let mut args = vec!["create", "--title", title];
    args.extend_from_slice(extra);
    let output = run(workspace, &args);
    assert!(
        output.status.success(),
        "create failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

fn workspace() -> TempDir {
    let temp = tempfile::tempdir().unwrap();
    let output = run(temp.path(), &["init", "--prefix", "r031"]);
    assert!(output.status.success());
    temp
}

#[test]
#[serial]
fn conflicting_ready_work_is_skipped_and_why_reports_reason() {
    let temp = workspace();
    let holder = create(
        temp.path(),
        "holder",
        &["--priority", "0", "--resource-key", " gpu "],
    );
    let conflict = create(
        temp.path(),
        "conflict",
        &["--priority", "1", "--resource-key", "gpu"],
    );
    let free = create(temp.path(), "free", &["--priority", "2"]);

    let first = run(temp.path(), &["claim", "--assignee", "worker-1", "--json"]);
    assert!(first.status.success());
    let first: serde_json::Value = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(first["bead_id"].as_str(), Some(holder.as_str()));

    let why = run(temp.path(), &["why", "--id", &conflict, "--json"]);
    assert!(why.status.success());
    let why: serde_json::Value = serde_json::from_slice(&why.stdout).unwrap();
    assert_eq!(why["is_ready"], false);
    assert!(why["reasons"]
        .as_array()
        .unwrap()
        .iter()
        .any(|reason| reason == "resource_conflict"));

    let second = run(temp.path(), &["claim", "--assignee", "worker-2", "--json"]);
    assert!(second.status.success());
    let second: serde_json::Value = serde_json::from_slice(&second.stdout).unwrap();
    assert_eq!(second["bead_id"].as_str(), Some(free.as_str()));

    let release = run(temp.path(), &["release", &holder]);
    assert!(release.status.success());
    let third = run(temp.path(), &["claim", "--assignee", "worker-3", "--json"]);
    assert!(third.status.success());
    let third: serde_json::Value = serde_json::from_slice(&third.stdout).unwrap();
    assert_eq!(third["bead_id"].as_str(), Some(conflict.as_str()));
}

#[test]
#[serial]
fn resource_commands_normalize_and_close_releases_keys() {
    let temp = workspace();
    let issue = create(temp.path(), "resource lifecycle", &[]);

    let add = run(
        temp.path(),
        &["resource", "add", &issue, "--key", " z ", "--key", "a"],
    );
    assert!(
        add.status.success(),
        "{}",
        String::from_utf8_lossy(&add.stderr)
    );
    let keys = run(temp.path(), &["resource", "list", &issue, "--json"]);
    let keys: serde_json::Value = serde_json::from_slice(&keys.stdout).unwrap();
    assert_eq!(keys, serde_json::json!(["a", "z"]));

    let claimed = run(temp.path(), &["claim", "--assignee", "worker", "--json"]);
    assert!(claimed.status.success());
    let closed = run(temp.path(), &["close", &issue, "--reason", "done"]);
    assert!(closed.status.success());

    let other = create(temp.path(), "reuse", &["--resource-key", "a"]);
    let claimed_other = run(temp.path(), &["claim", "--assignee", "other", "--json"]);
    assert!(claimed_other.status.success());
    let claimed_other: serde_json::Value = serde_json::from_slice(&claimed_other.stdout).unwrap();
    assert_eq!(claimed_other["bead_id"].as_str(), Some(other.as_str()));
}

#[test]
#[serial]
fn expired_lease_returns_resource_keys_on_next_claim() {
    let temp = workspace();
    let holder = create(
        temp.path(),
        "leased holder",
        &["--priority", "0", "--resource-key", "gpu"],
    );
    let waiting = create(
        temp.path(),
        "waiting",
        &["--priority", "1", "--resource-key", "gpu"],
    );

    let claim = run(
        temp.path(),
        &[
            "claim",
            "--assignee",
            "worker",
            "--lease-ttl",
            "30",
            "--json",
        ],
    );
    assert!(claim.status.success());

    let conn = Connection::open(temp.path().join(".beads/beads.db")).unwrap();
    conn.execute(
        "UPDATE leases SET expires_at = '2000-01-01T00:00:00Z' WHERE issue_id = ?1",
        [&holder],
    )
    .unwrap();
    drop(conn);

    let next = run(temp.path(), &["claim", "--assignee", "next", "--json"]);
    assert!(
        next.status.success(),
        "{}",
        String::from_utf8_lossy(&next.stderr)
    );
    let next: serde_json::Value = serde_json::from_slice(&next.stdout).unwrap();
    assert_eq!(next["bead_id"].as_str(), Some(waiting.as_str()));
}

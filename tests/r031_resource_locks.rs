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
    let output = run(
        temp.path(),
        &["--skip-foreign-workspace", "init", "--prefix", "r031"],
    );
    assert!(output.status.success());
    temp
}

fn resource_state(workspace: &Path, issue: &str) -> (String, String, i64, Vec<String>, i64) {
    let shown = run(workspace, &["show", issue, "--json"]);
    assert!(shown.status.success());
    let shown: serde_json::Value = serde_json::from_slice(&shown.stdout).unwrap();
    let issue_json = &shown.as_array().unwrap()[0];

    let listed = run(workspace, &["resource", "list", issue, "--json"]);
    assert!(listed.status.success());
    let keys: Vec<String> = serde_json::from_slice(&listed.stdout).unwrap();

    let conn = Connection::open(workspace.join(".beads/beads.db")).unwrap();
    let event_count = conn
        .query_row(
            "SELECT COUNT(*) FROM events WHERE issue_id = ?1",
            [issue],
            |row| row.get(0),
        )
        .unwrap();

    (
        issue_json["status"].as_str().unwrap().to_string(),
        issue_json["assignee"].as_str().unwrap().to_string(),
        issue_json["revision"].as_i64().unwrap(),
        keys,
        event_count,
    )
}

fn assert_resource_fence_conflict(
    workspace: &Path,
    args: &[&str],
    expected: &(String, String, i64, Vec<String>, i64),
    issue: &str,
) {
    let output = run(workspace, args);
    assert_eq!(
        output.status.code(),
        Some(4),
        "expected credential conflict, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("Claim-epoch credential"),
        "resource mutation must be rejected by the claim fence"
    );
    assert_eq!(
        &resource_state(workspace, issue),
        expected,
        "a rejected resource mutation must not change issue, keys, revision, or events"
    );
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

    // Each hand-back presents the epoch its own claim issued: release and
    // close are claimant-owned mutations, fenced by the same credential.
    let release = run(
        temp.path(),
        &[
            "release",
            &holder,
            "--fencing-token",
            &first["claim_epoch"].as_i64().unwrap().to_string(),
        ],
    );
    assert!(release.status.success());
    let third = run(temp.path(), &["claim", "--assignee", "worker-3", "--json"]);
    assert!(third.status.success());
    let third: serde_json::Value = serde_json::from_slice(&third.stdout).unwrap();
    assert_eq!(third["bead_id"].as_str(), Some(holder.as_str()));

    let close = run(
        temp.path(),
        &[
            "close",
            &holder,
            "--reason",
            "done",
            "--fencing-token",
            &third["claim_epoch"].as_i64().unwrap().to_string(),
        ],
    );
    assert!(close.status.success());
    let fourth = run(temp.path(), &["claim", "--assignee", "worker-4", "--json"]);
    assert!(fourth.status.success());
    let fourth: serde_json::Value = serde_json::from_slice(&fourth.stdout).unwrap();
    assert_eq!(fourth["bead_id"].as_str(), Some(conflict.as_str()));
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
    let claimed: serde_json::Value = serde_json::from_slice(&claimed.stdout).unwrap();
    let closed = run(
        temp.path(),
        &[
            "close",
            &issue,
            "--reason",
            "done",
            "--fencing-token",
            &claimed["claim_epoch"].as_i64().unwrap().to_string(),
        ],
    );
    assert!(closed.status.success());

    let other = create(temp.path(), "reuse", &["--resource-key", "a"]);
    let claimed_other = run(temp.path(), &["claim", "--assignee", "other", "--json"]);
    assert!(claimed_other.status.success());
    let claimed_other: serde_json::Value = serde_json::from_slice(&claimed_other.stdout).unwrap();
    assert_eq!(claimed_other["bead_id"].as_str(), Some(other.as_str()));
}

#[test]
#[serial]
fn claimed_resource_mutations_require_the_current_claim_epoch() {
    let temp = workspace();
    let issue = create(temp.path(), "fenced resource mutation", &[]);

    let first_claim = run(temp.path(), &["claim", "--assignee", "worker-1", "--json"]);
    assert!(first_claim.status.success());
    let first_claim: serde_json::Value = serde_json::from_slice(&first_claim.stdout).unwrap();
    let first_epoch = first_claim["claim_epoch"].as_i64().unwrap().to_string();

    let before_add = resource_state(temp.path(), &issue);
    assert_resource_fence_conflict(
        temp.path(),
        &["resource", "add", &issue, "--key", "gpu:0"],
        &before_add,
        &issue,
    );

    let add = run(
        temp.path(),
        &[
            "resource",
            "add",
            &issue,
            "--key",
            "gpu:0",
            "--fencing-token",
            &first_epoch,
        ],
    );
    assert!(add.status.success());

    let before_remove = resource_state(temp.path(), &issue);
    assert_eq!(before_remove.3, vec!["gpu:0"]);
    assert_resource_fence_conflict(
        temp.path(),
        &["resource", "remove", &issue, "--key", "gpu:0"],
        &before_remove,
        &issue,
    );

    let released = run(
        temp.path(),
        &["release", &issue, "--fencing-token", &first_epoch],
    );
    assert!(released.status.success());
    let second_claim = run(temp.path(), &["claim", "--assignee", "worker-2", "--json"]);
    assert!(second_claim.status.success());
    let second_claim: serde_json::Value = serde_json::from_slice(&second_claim.stdout).unwrap();
    let second_epoch = second_claim["claim_epoch"].as_i64().unwrap().to_string();
    assert!(second_epoch.parse::<i64>().unwrap() > first_epoch.parse::<i64>().unwrap());

    let before_stale_remove = resource_state(temp.path(), &issue);
    assert_resource_fence_conflict(
        temp.path(),
        &[
            "resource",
            "remove",
            &issue,
            "--key",
            "gpu:0",
            "--fencing-token",
            &first_epoch,
        ],
        &before_stale_remove,
        &issue,
    );

    let remove = run(
        temp.path(),
        &[
            "resource",
            "remove",
            &issue,
            "--key",
            "gpu:0",
            "--fencing-token",
            &second_epoch,
        ],
    );
    assert!(remove.status.success());

    let before_stale_add = resource_state(temp.path(), &issue);
    assert_resource_fence_conflict(
        temp.path(),
        &[
            "resource",
            "add",
            &issue,
            "--key",
            "gpu:0",
            "--fencing-token",
            &first_epoch,
        ],
        &before_stale_add,
        &issue,
    );

    let add_current = run(
        temp.path(),
        &[
            "resource",
            "add",
            &issue,
            "--key",
            "gpu:0",
            "--fencing-token",
            &second_epoch,
        ],
    );
    assert!(add_current.status.success());
    assert_eq!(resource_state(temp.path(), &issue).3, vec!["gpu:0"]);
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

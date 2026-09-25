//! Attempt evidence projected by `show --json` and `list --json`.

use assert_cmd::Command;
use serde_json::Value;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

fn bead() -> Command {
    Command::cargo_bin("bead").expect("the bead binary must be built for the test")
}

fn workspace() -> TempDir {
    let workspace = TempDir::new().unwrap();
    bead()
        .current_dir(workspace.path())
        .args([
            "init",
            "--prefix",
            "summary",
            "--skip-foreign-workspace",
            "--no-auto-flush",
        ])
        .assert()
        .success();
    workspace
}

fn create_issue(workspace: &Path, title: &str) -> String {
    let output = bead()
        .current_dir(workspace)
        .args(["create", "--title", title, "--no-auto-flush"])
        .assert()
        .success();
    String::from_utf8(output.get_output().stdout.clone())
        .unwrap()
        .trim()
        .to_string()
}

fn resolve(workspace: &Path, issue_id: &str, attempt_id: &str, outcome: &str, actor: &str) {
    bead()
        .current_dir(workspace)
        .args([
            "resolve",
            issue_id,
            "--attempt-id",
            attempt_id,
            "--outcome",
            outcome,
            "--action",
            "none",
            "--actor",
            actor,
            "--no-auto-flush",
        ])
        .assert()
        .success();
}

fn show_json(workspace: &Path, issue_id: &str) -> Value {
    let output = bead()
        .current_dir(workspace)
        .args(["show", issue_id, "--json"])
        .assert()
        .success();
    let value: Value = serde_json::from_slice(&output.get_output().stdout).unwrap();
    value
        .as_array()
        .and_then(|issues| issues.first())
        .cloned()
        .expect("show --json must return a nonempty issue array")
}

fn list_json(workspace: &Path, issue_id: &str) -> Value {
    let output = bead()
        .current_dir(workspace)
        .args(["list", "--json", "--limit", "999999"])
        .assert()
        .success();
    String::from_utf8(output.get_output().stdout.clone())
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .find(|issue| issue["id"] == issue_id)
        .expect("list --json must include the created issue")
}

fn assert_empty_summary(issue: &Value) {
    assert_eq!(issue["attempts"]["count"], 0);
    assert_eq!(issue["attempts"]["consecutive_failures"], 0);
    assert_eq!(issue["attempts"]["last_outcomes"], serde_json::json!([]));
}

fn assert_resolved_summary(issue: &Value) {
    let attempts = &issue["attempts"];
    assert_eq!(attempts["count"], 3);
    assert_eq!(
        attempts["consecutive_failures"], 0,
        "verified success must reset the derived failure run"
    );

    let last = attempts["last_outcomes"].as_array().unwrap();
    assert_eq!(last.len(), 3);
    let expected = [
        ("attempt-1", "work_failure", "worker-a"),
        ("attempt-2", "work_failure", "worker-b"),
        ("attempt-3", "verified_success", "verifier"),
    ];
    for (actual, (attempt_id, outcome, actor)) in last.iter().zip(expected) {
        assert_eq!(actual["attempt_id"], attempt_id);
        assert_eq!(actual["outcome"], outcome);
        assert_eq!(actual["actor"], actor);
        assert!(
            actual["resolved_at"]
                .as_str()
                .is_some_and(|time| !time.is_empty()),
            "each retained outcome must expose its resolution time"
        );
        assert_eq!(
            actual.as_object().unwrap().len(),
            4,
            "the compact outcome projection must expose only its contracted fields"
        );
    }
}

#[test]
fn show_and_list_include_attempt_evidence() {
    let workspace = workspace();
    let empty_id = create_issue(workspace.path(), "no attempts");
    let attempted_id = create_issue(workspace.path(), "three attempts");

    assert_empty_summary(&show_json(workspace.path(), &empty_id));
    assert_empty_summary(&list_json(workspace.path(), &empty_id));

    resolve(
        workspace.path(),
        &attempted_id,
        "attempt-1",
        "work_failure",
        "worker-a",
    );
    resolve(
        workspace.path(),
        &attempted_id,
        "attempt-2",
        "work_failure",
        "worker-b",
    );
    resolve(
        workspace.path(),
        &attempted_id,
        "attempt-3",
        "verified_success",
        "verifier",
    );

    assert_resolved_summary(&show_json(workspace.path(), &attempted_id));
    assert_resolved_summary(&list_json(workspace.path(), &attempted_id));
}

#[test]
fn attempt_summary_is_advertised_by_native_v1() {
    let nowhere = TempDir::new().unwrap();
    let output = bead()
        .current_dir(nowhere.path())
        .args(["capabilities", "--profile", "native-v1"])
        .assert()
        .success();
    let capabilities: Value = serde_json::from_slice(&output.get_output().stdout).unwrap();
    assert_eq!(capabilities["attempt_summary"], true);
}

#[test]
fn attempt_summary_retains_only_the_latest_five_outcomes() {
    let workspace = workspace();
    let issue_id = create_issue(workspace.path(), "bounded attempt history");
    for number in 1..=6 {
        resolve(
            workspace.path(),
            &issue_id,
            &format!("bounded-{number}"),
            "infrastructure_failure",
            "fleet-worker",
        );
    }

    let issue = show_json(workspace.path(), &issue_id);
    assert_eq!(issue["attempts"]["count"], 6);
    let retained = issue["attempts"]["last_outcomes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|outcome| outcome["attempt_id"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        retained,
        [
            "bounded-2",
            "bounded-3",
            "bounded-4",
            "bounded-5",
            "bounded-6"
        ],
        "the bounded tail must stay in chronological order"
    );
}

#[test]
fn attempt_summary_does_not_change_checkpoint_issue_records() {
    let workspace = workspace();
    let issue_id = create_issue(workspace.path(), "checkpoint shape");
    resolve(
        workspace.path(),
        &issue_id,
        "checkpoint-attempt",
        "work_failure",
        "checkpoint-worker",
    );

    bead()
        .current_dir(workspace.path())
        .args(["sync", "flush-only"])
        .assert()
        .success();

    let checkpoint = fs::read_to_string(workspace.path().join(".beads/checkpoint/forensic.jsonl"))
        .expect("the small workspace must use a monolithic checkpoint");
    let records: Vec<Value> = checkpoint
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let issue = records
        .iter()
        .find(|record| record["record_type"] == "issue" && record["issue"]["id"] == issue_id)
        .expect("checkpoint must contain the issue record");
    assert!(
        issue["issue"].get("attempts").is_none(),
        "attempts is a CLI projection, not persisted issue state"
    );
    assert_eq!(
        records
            .iter()
            .filter(|record| record["record_type"] == "attempt_outcome")
            .count(),
        1,
        "the underlying attempt outcome checkpoint record remains unchanged"
    );
}

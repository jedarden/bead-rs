//! Ready-frontier ordering from native consecutive-failure evidence.

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
            "attemptsort",
            "--skip-foreign-workspace",
            "--no-auto-flush",
        ])
        .assert()
        .success();
    workspace
}

fn create_issue(workspace: &Path, title: &str, priority: &str) -> String {
    let output = bead()
        .current_dir(workspace)
        .args([
            "create",
            "--title",
            title,
            "--priority",
            priority,
            "--no-auto-flush",
        ])
        .assert()
        .success();
    String::from_utf8(output.get_output().stdout.clone())
        .unwrap()
        .trim()
        .to_string()
}

fn record_failures(workspace: &Path, issue_id: &str, count: usize) {
    for number in 1..=count {
        bead()
            .current_dir(workspace)
            .args([
                "resolve",
                issue_id,
                "--attempt-id",
                &format!("{issue_id}-failure-{number}"),
                "--outcome",
                "work_failure",
                "--action",
                "none",
                "--no-auto-flush",
            ])
            .assert()
            .success();
    }
}

fn ready_output(workspace: &Path, sort_attempts: bool) -> Vec<u8> {
    let mut command = bead();
    command
        .current_dir(workspace)
        .args(["list", "--ready", "--json", "--limit", "999999"]);
    if sort_attempts {
        command.args(["--sort", "attempts"]);
    }
    command.assert().success().get_output().stdout.clone()
}

fn listed_ids(output: &[u8]) -> Vec<String> {
    String::from_utf8(output.to_vec())
        .unwrap()
        .lines()
        .map(|line| {
            let issue: Value = serde_json::from_str(line).unwrap();
            issue["id"].as_str().unwrap().to_string()
        })
        .collect()
}

fn configure_claim_attempt_sort(workspace: &Path) {
    let path = workspace.join(".beads/config.json");
    let mut config: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    config["claim"] = serde_json::json!({"sort": "attempts"});
    fs::write(path, serde_json::to_vec_pretty(&config).unwrap()).unwrap();
}

#[test]
fn attempts_sort_prefers_fresh_work_within_priority_but_priority_still_wins() {
    let workspace = workspace();
    let failed_equal = create_issue(workspace.path(), "older P2 with failures", "2");
    let fresh_equal = create_issue(workspace.path(), "fresh P2", "2");
    let failed_higher_priority = create_issue(workspace.path(), "P1 with failures", "1");

    record_failures(workspace.path(), &failed_equal, 2);
    record_failures(workspace.path(), &failed_higher_priority, 2);

    assert_eq!(
        listed_ids(&ready_output(workspace.path(), true)),
        [failed_higher_priority, fresh_equal, failed_equal],
        "declared priority must remain primary, while attempts reorder equal-priority beads"
    );
}

#[test]
fn configured_claim_uses_attempt_ordering() {
    let workspace = workspace();
    let failed = create_issue(workspace.path(), "older failed work", "2");
    let fresh = create_issue(workspace.path(), "fresh work", "2");
    record_failures(workspace.path(), &failed, 2);
    configure_claim_attempt_sort(workspace.path());

    let output = bead()
        .current_dir(workspace.path())
        .args([
            "claim",
            "--assignee",
            "attempt-aware-worker",
            "--json",
            "--no-auto-flush",
        ])
        .assert()
        .success();
    let claim: Value = serde_json::from_slice(&output.get_output().stdout).unwrap();

    assert_eq!(claim["bead_id"], fresh);
}

#[test]
fn absent_list_flag_preserves_default_output_bytes_and_fifo_order() {
    let workspace = workspace();
    let failed = create_issue(workspace.path(), "older failed work", "2");
    let fresh = create_issue(workspace.path(), "fresh work", "2");
    record_failures(workspace.path(), &failed, 2);

    let before_config = ready_output(workspace.path(), false);
    assert_eq!(listed_ids(&before_config), [failed, fresh]);

    configure_claim_attempt_sort(workspace.path());
    let after_config = ready_output(workspace.path(), false);
    assert_eq!(
        after_config, before_config,
        "claim configuration must not change unflagged list output, including its bytes"
    );
}

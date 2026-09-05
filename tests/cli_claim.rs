//! Integration tests for `bead claim` command

use assert_cmd::Command;
use serial_test::serial;

#[test]
#[serial]
fn test_claim_empty_workspace() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Try to claim from empty workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["claim", "--assignee", "worker-1", "--json"])
        .assert()
        .success()
        .stdout(predicates::str::contains("\"bead_id\":null"));
}

#[test]
#[serial]
fn test_claim_basic() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Create an issue
    Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Test Issue"])
        .assert()
        .success();

    // Claim the issue
    let result = Command::cargo_bin("bead")
        .unwrap()
        .args(["claim", "--assignee", "worker-1", "--json"])
        .assert()
        .success();

    let output = std::str::from_utf8(&result.get_output().stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(output).unwrap();

    assert!(parsed["bead_id"].is_string());
    assert_eq!(parsed["assignee"], "worker-1");
}

#[test]
#[serial]
fn test_claim_priority_ordering() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Create issues with different priorities
    Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Low Priority", "--priority", "3"])
        .assert()
        .success();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "High Priority", "--priority", "0"])
        .assert()
        .success();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Medium Priority", "--priority", "2"])
        .assert()
        .success();

    // First claim should get the high priority issue
    let result1 = Command::cargo_bin("bead")
        .unwrap()
        .args(["claim", "--assignee", "worker-1", "--json"])
        .assert()
        .success();

    let output1 = std::str::from_utf8(&result1.get_output().stdout).unwrap();
    let parsed1: serde_json::Value = serde_json::from_str(output1).unwrap();
    let first_id = parsed1["bead_id"].as_str().unwrap();

    // Show the first claimed issue to verify it's the high priority one
    let show_result = Command::cargo_bin("bead")
        .unwrap()
        .args(["show", first_id, "--json"])
        .assert()
        .success();

    let show_output = std::str::from_utf8(&show_result.get_output().stdout).unwrap();
    let show_parsed: serde_json::Value = serde_json::from_str(show_output).unwrap();
    let issue = &show_parsed[0];

    assert_eq!(issue["title"], "High Priority");
    assert_eq!(issue["priority"], 0);

    // Second claim should get the medium priority issue
    let result2 = Command::cargo_bin("bead")
        .unwrap()
        .args(["claim", "--assignee", "worker-2", "--json"])
        .assert()
        .success();

    let output2 = std::str::from_utf8(&result2.get_output().stdout).unwrap();
    let parsed2: serde_json::Value = serde_json::from_str(output2).unwrap();
    let second_id = parsed2["bead_id"].as_str().unwrap();

    let show_result2 = Command::cargo_bin("bead")
        .unwrap()
        .args(["show", second_id, "--json"])
        .assert()
        .success();

    let show_output2 = std::str::from_utf8(&show_result2.get_output().stdout).unwrap();
    let show_parsed2: serde_json::Value = serde_json::from_str(show_output2).unwrap();
    let issue2 = &show_parsed2[0];

    assert_eq!(issue2["title"], "Medium Priority");
    assert_eq!(issue2["priority"], 2);

    // Third claim should get the low priority issue
    let result3 = Command::cargo_bin("bead")
        .unwrap()
        .args(["claim", "--assignee", "worker-3", "--json"])
        .assert()
        .success();

    let output3 = std::str::from_utf8(&result3.get_output().stdout).unwrap();
    let parsed3: serde_json::Value = serde_json::from_str(output3).unwrap();
    let third_id = parsed3["bead_id"].as_str().unwrap();

    let show_result3 = Command::cargo_bin("bead")
        .unwrap()
        .args(["show", third_id, "--json"])
        .assert()
        .success();

    let show_output3 = std::str::from_utf8(&show_result3.get_output().stdout).unwrap();
    let show_parsed3: serde_json::Value = serde_json::from_str(show_output3).unwrap();
    let issue3 = &show_parsed3[0];

    assert_eq!(issue3["title"], "Low Priority");
    assert_eq!(issue3["priority"], 3);

    // Fourth claim should get nothing
    let result4 = Command::cargo_bin("bead")
        .unwrap()
        .args(["claim", "--assignee", "worker-4", "--json"])
        .assert()
        .success();

    let output4 = std::str::from_utf8(&result4.get_output().stdout).unwrap();
    let parsed4: serde_json::Value = serde_json::from_str(output4).unwrap();

    assert!(parsed4["bead_id"].is_null());
}

#[test]
#[serial]
fn test_claim_without_workspace() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Try to claim without workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["claim", "--assignee", "worker-1"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("No workspace found"));
}

#[test]
#[serial]
fn test_twenty_simultaneous_claimers_no_duplicates() {
    let temp = tempfile::tempdir().unwrap();
    let workspace_root = temp.path().to_path_buf();
    std::env::set_current_dir(&workspace_root).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Create 25 issues (more than the number of claimers)
    for i in 1..=25 {
        Command::cargo_bin("bead")
            .unwrap()
            .args(["create", "--title", &format!("Issue {}", i)])
            .assert()
            .success();
    }

    // Claim issues sequentially to verify no duplicates occur
    let mut claimed_ids = std::collections::HashSet::new();

    for worker_id in 1..=20 {
        let result = Command::cargo_bin("bead")
            .unwrap()
            .args([
                "claim",
                "--assignee",
                &format!("worker-{}", worker_id),
                "--json",
            ])
            .assert()
            .success();

        let output = std::str::from_utf8(&result.get_output().stdout).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(output).unwrap();

        if parsed["bead_id"].is_string() {
            let bead_id = parsed["bead_id"].as_str().unwrap().to_string();
            assert!(
                claimed_ids.insert(bead_id.clone()),
                "Duplicate claim ID detected: {}",
                bead_id
            );
        }
    }

    // All 20 claims should have been successful
    assert_eq!(claimed_ids.len(), 20, "Expected 20 successful claims");
}

// ---------------------------------------------------------------------------
// --single-claim guard
//
// Opt-in claim-time guard (beadrs-b412cc80): `bead claim --single-claim`
// refuses the claim when the assignee already holds an in_progress issue in
// this workspace, failing with exit code 4 and the machine-readable reason
// code `assignee_has_active_claim` naming the blocking issue ID. Default
// behavior (no flag) is unchanged.
// ---------------------------------------------------------------------------

/// Create an initialized workspace for the single-claim tests
///
/// Tests in this section pass an explicit `current_dir` to every command
/// instead of mutating the process working directory, so they do not need
/// `#[serial]` and cannot be disturbed by the chdir-based tests above.
fn single_claim_workspace() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    Command::cargo_bin("bead")
        .unwrap()
        .current_dir(dir.path())
        .args(["init", "--prefix", "sc"])
        .assert()
        .success();
    dir
}

/// Create an issue and return its ID
fn create_issue(dir: &std::path::Path, title: &str) -> String {
    let out = Command::cargo_bin("bead")
        .unwrap()
        .current_dir(dir)
        .args(["create", "--title", title])
        .output()
        .unwrap();
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

/// Read one field of an issue from `bead list --json` output
///
/// `list --json` emits one issue object per line (`[]` when empty).
fn issue_field(dir: &std::path::Path, id: &str, field: &str) -> serde_json::Value {
    let out = Command::cargo_bin("bead")
        .unwrap()
        .current_dir(dir)
        .args(["list", "--json"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed == "[]" {
            continue;
        }
        let issue: serde_json::Value = serde_json::from_str(trimmed).unwrap();
        if issue["id"].as_str() == Some(id) {
            return issue[field].clone();
        }
    }
    panic!("issue {} not found in list output", id);
}

/// Run a claim command and return its parsed `--json` result
fn claim(dir: &std::path::Path, extra_args: &[&str]) -> serde_json::Value {
    let mut args = vec!["claim", "--assignee", "worker-1", "--json"];
    args.extend_from_slice(extra_args);
    let out = Command::cargo_bin("bead")
        .unwrap()
        .current_dir(dir)
        .args(&args)
        .output()
        .unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!(
            "claim {:?} produced non-JSON output {:?}: {}",
            args, stdout, e
        )
    })
}

#[test]
fn test_single_claim_refuses_when_assignee_holds_in_progress() {
    let dir = single_claim_workspace();
    let first = create_issue(dir.path(), "First issue");
    let second = create_issue(dir.path(), "Second issue");

    // Fresh assignee: the flag does not block a first claim
    let claimed = claim(dir.path(), &["--single-claim"]);
    assert_eq!(claimed["bead_id"].as_str(), Some(first.as_str()));

    // Second claim under the guard: refused with the machine-readable reason
    // code, exit code 4, and the blocking issue ID named
    let args = vec!["claim", "--assignee", "worker-1", "--single-claim"];
    Command::cargo_bin("bead")
        .unwrap()
        .current_dir(dir.path())
        .args(&args)
        .assert()
        .failure()
        .code(4)
        .stderr(predicates::str::contains("assignee_has_active_claim"))
        .stderr(predicates::str::contains(&first));

    // The refusal assigned nothing: the second issue is still open and
    // unassigned on the ready frontier
    assert_eq!(issue_field(dir.path(), &second, "status"), "open");
    assert!(issue_field(dir.path(), &second, "assignee").is_null());

    // The guard is per assignee: a different worker claiming under the flag
    // still gets the remaining issue
    let out = Command::cargo_bin("bead")
        .unwrap()
        .current_dir(dir.path())
        .args([
            "claim",
            "--assignee",
            "worker-2",
            "--single-claim",
            "--json",
        ])
        .output()
        .unwrap();
    let claimed: serde_json::Value =
        serde_json::from_str(&String::from_utf8(out.stdout).unwrap()).unwrap();
    assert_eq!(claimed["bead_id"].as_str(), Some(second.as_str()));
}

#[test]
fn test_single_claim_allows_claim_again_after_release() {
    let dir = single_claim_workspace();
    let first = create_issue(dir.path(), "First issue");
    create_issue(dir.path(), "Second issue");

    let claimed = claim(dir.path(), &["--single-claim"]);
    assert_eq!(claimed["bead_id"].as_str(), Some(first.as_str()));

    // Guard refuses while the claim is held
    Command::cargo_bin("bead")
        .unwrap()
        .current_dir(dir.path())
        .args(["claim", "--assignee", "worker-1", "--single-claim"])
        .assert()
        .failure()
        .code(4)
        .stderr(predicates::str::contains("assignee_has_active_claim"));

    // After releasing the held issue (which takes the claim's epoch
    // credential), the same assignee can claim again
    let epoch = claimed["claim_epoch"].as_i64().unwrap().to_string();
    Command::cargo_bin("bead")
        .unwrap()
        .current_dir(dir.path())
        .args(["release", &first, "--fencing-token", &epoch])
        .assert()
        .success();

    let reclaimed = claim(dir.path(), &["--single-claim"]);
    assert!(reclaimed["bead_id"].is_string());
}

#[test]
fn test_single_claim_allows_claim_again_after_close() {
    let dir = single_claim_workspace();
    let first = create_issue(dir.path(), "First issue");
    create_issue(dir.path(), "Second issue");

    let claimed = claim(dir.path(), &["--single-claim"]);
    assert_eq!(claimed["bead_id"].as_str(), Some(first.as_str()));

    // Guard refuses while the claim is held
    Command::cargo_bin("bead")
        .unwrap()
        .current_dir(dir.path())
        .args(["claim", "--assignee", "worker-1", "--single-claim"])
        .assert()
        .failure()
        .code(4);

    // After closing the held issue (which takes the claim's epoch
    // credential), the same assignee can claim again
    let epoch = claimed["claim_epoch"].as_i64().unwrap().to_string();
    Command::cargo_bin("bead")
        .unwrap()
        .current_dir(dir.path())
        .args([
            "close",
            &first,
            "--reason",
            "work complete",
            "--fencing-token",
            &epoch,
        ])
        .assert()
        .success();

    let reclaimed = claim(dir.path(), &["--single-claim"]);
    assert!(reclaimed["bead_id"].is_string());
}

#[test]
fn test_claim_without_single_claim_allows_multiple_in_progress() {
    let dir = single_claim_workspace();
    let first = create_issue(dir.path(), "First issue");
    let second = create_issue(dir.path(), "Second issue");

    // Default behavior is unchanged: no flag, no limit on held claims
    let claimed_first = claim(dir.path(), &[]);
    assert_eq!(claimed_first["bead_id"].as_str(), Some(first.as_str()));

    let claimed_second = claim(dir.path(), &[]);
    assert_eq!(claimed_second["bead_id"].as_str(), Some(second.as_str()));

    assert_eq!(issue_field(dir.path(), &first, "status"), "in_progress");
    assert_eq!(issue_field(dir.path(), &second, "status"), "in_progress");
}

#[test]
fn test_single_claim_does_not_guard_update_assignee() {
    let dir = single_claim_workspace();
    let first = create_issue(dir.path(), "First issue");
    let second = create_issue(dir.path(), "Second issue");

    let claimed = claim(dir.path(), &["--single-claim"]);
    assert_eq!(claimed["bead_id"].as_str(), Some(first.as_str()));

    // The guard scopes to the claim action only: assigning an already-open
    // issue via `bead update --assignee` still works while holding a claim
    Command::cargo_bin("bead")
        .unwrap()
        .current_dir(dir.path())
        .args(["update", &second, "--assignee", "worker-1"])
        .assert()
        .success();

    assert_eq!(issue_field(dir.path(), &second, "assignee"), "worker-1");
}

#[test]
fn test_single_claim_permits_lease_renewal() {
    let dir = single_claim_workspace();
    let first = create_issue(dir.path(), "First issue");

    let claimed = claim(dir.path(), &["--single-claim", "--lease-ttl", "120"]);
    assert_eq!(claimed["bead_id"].as_str(), Some(first.as_str()));

    // Renewal operates on the issue the assignee already holds, so it is not
    // guarded even with --single-claim set
    let out = Command::cargo_bin("bead")
        .unwrap()
        .current_dir(dir.path())
        .args([
            "claim",
            "--assignee",
            "worker-1",
            "--renew-lease",
            "--single-claim",
            "--lease-ttl",
            "120",
            "--json",
        ])
        .output()
        .unwrap();
    let renewed: serde_json::Value =
        serde_json::from_str(&String::from_utf8(out.stdout).unwrap()).unwrap();
    assert_eq!(renewed["bead_id"].as_str(), Some(first.as_str()));
    assert_eq!(renewed["lease"]["issue_id"].as_str(), Some(first.as_str()));
}

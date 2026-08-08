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

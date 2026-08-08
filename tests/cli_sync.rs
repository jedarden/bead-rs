//! Integration tests for `bead sync` commands

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use std::fs;
use tempfile::TempDir;

fn create_workspace() -> TempDir {
    let temp_dir = TempDir::new().unwrap();
    let bead_dir = temp_dir.path().join(".beads");
    fs::create_dir(&bead_dir).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "bead"])
        .current_dir(temp_dir.path())
        .assert()
        .success();

    temp_dir
}

#[test]
fn test_sync_flush_only_basic() {
    let temp_dir = create_workspace();

    // Create a test issue
    Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Test Issue"])
        .current_dir(temp_dir.path())
        .assert()
        .success()
        .stdout(predicate::str::is_match("bead-[a-f0-9]{16}").unwrap());

    // Flush checkpoint
    Command::cargo_bin("bead")
        .unwrap()
        .args(["sync", "flush-only"])
        .current_dir(temp_dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("Flushed checkpoint:"));

    // Verify checkpoint file exists
    let checkpoint_path = temp_dir.path().join(".beads/issues.jsonl");
    assert!(checkpoint_path.exists());

    // Verify checkpoint contains one issue
    let content = fs::read_to_string(&checkpoint_path).unwrap();
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines.len(), 1);

    // Verify issue JSON structure
    let issue: Value = serde_json::from_str(lines[0]).unwrap();
    assert!(issue["id"].is_string());
    assert_eq!(issue["title"], "Test Issue");
    assert!(issue["priority"].is_number());
    assert!(issue["base_status"].is_string());
}

#[test]
fn test_sync_flush_only_empty_workspace() {
    let temp_dir = create_workspace();

    // Flush empty checkpoint
    Command::cargo_bin("bead")
        .unwrap()
        .args(["sync", "flush-only"])
        .current_dir(temp_dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("Issues: 0"));

    // Verify checkpoint file exists and is empty
    let checkpoint_path = temp_dir.path().join(".beads/issues.jsonl");
    assert!(checkpoint_path.exists());

    let content = fs::read_to_string(&checkpoint_path).unwrap();
    assert!(content.is_empty());
}

#[test]
fn test_sync_flush_only_with_custom_output() {
    let temp_dir = create_workspace();

    // Create a test issue
    Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Custom Output Test"])
        .current_dir(temp_dir.path())
        .assert()
        .success();

    // Flush to custom output
    let custom_output = temp_dir.path().join("custom.jsonl");
    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "sync",
            "flush-only",
            "--output",
            custom_output.to_str().unwrap(),
        ])
        .current_dir(temp_dir.path())
        .assert()
        .success();

    // Verify custom file exists
    assert!(custom_output.exists());

    // Verify it contains the issue
    let content = fs::read_to_string(&custom_output).unwrap();
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines.len(), 1);
}

#[test]
fn test_sync_flush_only_rejects_invalid_profile() {
    let temp_dir = create_workspace();

    // Try with invalid profile
    Command::cargo_bin("bead")
        .unwrap()
        .args(["sync", "flush-only", "--profile", "needle-v1"])
        .current_dir(temp_dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("is not supported"));
}

#[test]
fn test_sync_flush_only_rejects_checkpoint_path() {
    let temp_dir = create_workspace();

    // Try to output to checkpoint directory (reserved for F017)
    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "sync",
            "flush-only",
            "--output",
            ".beads/checkpoint/test.jsonl",
        ])
        .current_dir(temp_dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("reserved for F017"));
}

#[test]
fn test_sync_flush_only_deterministic_ordering() {
    let temp_dir = create_workspace();

    // Create multiple issues (IDs will be generated in order)
    for i in 1..=3 {
        Command::cargo_bin("bead")
            .unwrap()
            .args(["create", "--title", &format!("Issue {}", i)])
            .current_dir(temp_dir.path())
            .assert()
            .success();
    }

    // Flush checkpoint
    Command::cargo_bin("bead")
        .unwrap()
        .args(["sync", "flush-only"])
        .current_dir(temp_dir.path())
        .assert()
        .success();

    // Verify issues are in deterministic order (by ID)
    let checkpoint_path = temp_dir.path().join(".beads/issues.jsonl");
    let content = fs::read_to_string(&checkpoint_path).unwrap();
    let lines: Vec<&str> = content.lines().collect();

    let mut prev_id = String::new();
    for line in lines {
        let issue: Value = serde_json::from_str(line).unwrap();
        let id = issue["id"].as_str().unwrap().to_string();
        if !prev_id.is_empty() {
            assert!(id > prev_id, "Issues should be sorted by ID");
        }
        prev_id = id;
    }
}

#[test]
fn test_sync_flush_only_without_workspace() {
    let temp_dir = TempDir::new().unwrap();

    // Try to flush without workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["sync", "flush-only"])
        .current_dir(temp_dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("No workspace found"));
}

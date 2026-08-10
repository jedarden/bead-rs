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

    // Flush forensic checkpoint
    Command::cargo_bin("bead")
        .unwrap()
        .args(["sync", "flush-only"])
        .current_dir(temp_dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("Flushed forensic checkpoint:"))
        .stderr(predicate::str::contains("Mode: monolithic"))
        .stderr(predicate::str::contains("Issues: 1"))
        .stderr(predicate::str::contains("Root hash:"));

    // Verify forensic checkpoint structure exists
    let checkpoint_base = temp_dir.path().join(".beads/checkpoint");
    assert!(checkpoint_base.exists());

    let current_pointer = checkpoint_base.join("current.json");
    assert!(current_pointer.exists());

    let objects_dir = checkpoint_base.join("objects");
    assert!(objects_dir.exists());

    // Verify forensic view exists
    let forensic_view = checkpoint_base.join("forensic.jsonl");
    assert!(forensic_view.exists());

    // Verify checkpoint contains one issue
    let content = fs::read_to_string(&forensic_view).unwrap();
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines.len(), 1);

    // Verify issue JSON structure (wrapped in record_type envelope)
    let record: Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(record["record_type"], "issue");
    let issue = &record["issue"];
    assert!(issue["id"].is_string());
    assert_eq!(issue["title"], "Test Issue");
    assert!(issue["priority"].is_number());
    assert!(issue["base_status"].is_string());
}

#[test]
fn test_sync_flush_only_empty_workspace() {
    let temp_dir = create_workspace();

    // Flush empty forensic checkpoint
    Command::cargo_bin("bead")
        .unwrap()
        .args(["sync", "flush-only"])
        .current_dir(temp_dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("Flushed forensic checkpoint:"))
        .stderr(predicate::str::contains("Issues: 0"));

    // Verify forensic checkpoint structure exists
    let checkpoint_base = temp_dir.path().join(".beads/checkpoint");
    assert!(checkpoint_base.exists());

    let current_pointer = checkpoint_base.join("current.json");
    assert!(current_pointer.exists());

    let forensic_view = checkpoint_base.join("forensic.jsonl");
    assert!(forensic_view.exists());

    // Verify forensic view is empty (zero bytes for empty workspace)
    let content = fs::read_to_string(&forensic_view).unwrap();
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

    // Try with invalid profile for export (should fail when using explicit output)
    let custom_output = temp_dir.path().join("custom.jsonl");
    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "sync",
            "flush-only",
            "--profile",
            "needle-v1",
            "--output",
            custom_output.to_str().unwrap(),
        ])
        .current_dir(temp_dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("is not supported"));
}

#[test]
fn test_sync_flush_only_external_profile_emits_loss_report() {
    let temp_dir = create_workspace();
    let blocker = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Blocker"])
        .current_dir(temp_dir.path())
        .output()
        .unwrap();
    let blocker = String::from_utf8(blocker.stdout)
        .unwrap()
        .trim()
        .to_string();
    let blocked = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Blocked"])
        .current_dir(temp_dir.path())
        .output()
        .unwrap();
    let blocked = String::from_utf8(blocked.stdout)
        .unwrap()
        .trim()
        .to_string();
    Command::cargo_bin("bead")
        .unwrap()
        .args(["label", "add", &blocked, "--label", "zeta"])
        .current_dir(temp_dir.path())
        .assert()
        .success();
    Command::cargo_bin("bead")
        .unwrap()
        .args(["dep", "add", &blocked, &blocker])
        .current_dir(temp_dir.path())
        .assert()
        .success();

    let output_path = temp_dir.path().join("br.jsonl");
    let output = Command::cargo_bin("bead")
        .unwrap()
        .args([
            "sync",
            "flush-only",
            "--profile",
            "br-v1",
            "--output",
            output_path.to_str().unwrap(),
        ])
        .current_dir(temp_dir.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        report["schema_ref"],
        "urn:bead-rs:schema:profile-loss-report:v1"
    );
    assert_eq!(report["profile"], "br-v1");
    assert_eq!(report["direction"], "export");

    let records: Vec<Value> = fs::read_to_string(output_path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let blocked_record = records
        .iter()
        .find(|record| record["id"] == blocked)
        .unwrap();
    assert_eq!(blocked_record["labels"], serde_json::json!(["zeta"]));
    assert_eq!(blocked_record["dependencies"][0]["depends_on_id"], blocker);
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
        .stderr(predicate::str::contains(
            "use default for forensic checkpoints",
        ));
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

    // Flush forensic checkpoint
    Command::cargo_bin("bead")
        .unwrap()
        .args(["sync", "flush-only"])
        .current_dir(temp_dir.path())
        .assert()
        .success();

    // Verify issues are in deterministic order (by ID)
    let forensic_view = temp_dir.path().join(".beads/checkpoint/forensic.jsonl");
    let content = fs::read_to_string(&forensic_view).unwrap();
    let lines: Vec<&str> = content.lines().collect();

    let mut prev_id = String::new();
    for line in lines {
        let record: Value = serde_json::from_str(line).unwrap();
        assert_eq!(record["record_type"], "issue");
        let issue = &record["issue"];
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

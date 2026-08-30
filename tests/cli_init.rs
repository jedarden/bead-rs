//! Integration tests for bead CLI init command

use assert_cmd::Command;

#[test]
fn test_init_creates_workspace() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    let mut cmd = Command::cargo_bin("bead").unwrap();
    cmd.current_dir(root).arg("init").assert().success();

    // Check that workspace was created
    let beads_dir = root.join(".beads");
    assert!(beads_dir.exists());

    let db_path = root.join(".beads/beads.db");
    assert!(db_path.exists());
}

#[test]
fn test_init_with_custom_prefix() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    let mut cmd = Command::cargo_bin("bead").unwrap();
    cmd.current_dir(root)
        .arg("init")
        .arg("--prefix")
        .arg("custom")
        .assert()
        .success();

    // Check that prefix was used
    let config_path = root.join(".beads/config.json");
    let config_content = std::fs::read_to_string(config_path).unwrap();
    let config: serde_json::Value = serde_json::from_str(&config_content).unwrap();
    assert_eq!(config["prefix"], "custom");
}

#[test]
fn test_init_invalid_prefix() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    let mut cmd = Command::cargo_bin("bead").unwrap();
    cmd.current_dir(root)
        .arg("init")
        .arg("--prefix")
        .arg("INVALID")
        .assert()
        .failure();
}

#[test]
fn test_init_idempotent() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    // First init
    let mut cmd = Command::cargo_bin("bead").unwrap();
    cmd.current_dir(root).arg("init").assert().success();

    // Second init should succeed (idempotent)
    let mut cmd = Command::cargo_bin("bead").unwrap();
    cmd.current_dir(root).arg("init").assert().success();
}

#[test]
fn test_init_creates_checkpoint_and_receipts_directories() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    let mut cmd = Command::cargo_bin("bead").unwrap();
    cmd.current_dir(root).arg("init").assert().success();

    // Check checkpoint directory
    let checkpoint_dir = root.join(".beads/checkpoint");
    assert!(checkpoint_dir.exists());

    // Check receipts directory
    let receipts_dir = root.join(".beads/receipts");
    assert!(receipts_dir.exists());
}

#[test]
fn test_init_creates_gitignore() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    let mut cmd = Command::cargo_bin("bead").unwrap();
    cmd.current_dir(root).arg("init").assert().success();

    // Check .gitignore was created
    let gitignore_path = root.join(".beads/.gitignore");
    assert!(gitignore_path.exists());

    let content = std::fs::read_to_string(&gitignore_path).unwrap();
    assert!(content.contains("*.db"));
    assert!(content.contains("*.db-wal"));
    assert!(content.contains("*.lock"));
}

#[test]
fn test_init_creates_initial_checkpoint() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    let mut cmd = Command::cargo_bin("bead").unwrap();
    cmd.current_dir(root).arg("init").assert().success();

    // Check that initial checkpoint was created
    let checkpoint_current = root.join(".beads/checkpoint/current.json");
    assert!(
        checkpoint_current.exists(),
        "Initial checkpoint current.json should exist after init"
    );

    // Verify the checkpoint points to a zero-issue generation
    let checkpoint_content = std::fs::read_to_string(&checkpoint_current).unwrap();
    let checkpoint: serde_json::Value = serde_json::from_str(&checkpoint_content).unwrap();

    assert!(checkpoint["generation_id"].is_string());
    assert_eq!(checkpoint["mode"], "monolithic");
    assert_eq!(checkpoint["issue_count"], 0);
    assert_eq!(checkpoint["event_count"], 0);
    assert_eq!(checkpoint["receipt_count"], 0);

    // Verify forensic.jsonl exists (monolithic mode)
    let forensic_path = root.join(".beads/checkpoint/forensic.jsonl");
    assert!(
        forensic_path.exists(),
        "Forensic checkpoint should exist in monolithic mode"
    );

    // Verify it's empty (no records)
    let forensic_content = std::fs::read_to_string(&forensic_path).unwrap();
    assert!(
        forensic_content.lines().count() == 0,
        "Initial checkpoint should contain zero records"
    );
}

#[test]
fn test_init_with_no_auto_flush_skips_checkpoint() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    let mut cmd = Command::cargo_bin("bead").unwrap();
    cmd.current_dir(root)
        .arg("init")
        .arg("--no-auto-flush")
        .assert()
        .success();

    // Check that initial checkpoint was NOT created
    let checkpoint_current = root.join(".beads/checkpoint/current.json");
    assert!(
        !checkpoint_current.exists(),
        "Initial checkpoint should not exist with --no-auto-flush"
    );
}

#[test]
fn test_init_idempotent_does_not_overwrite_checkpoint() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    // First init
    let mut cmd = Command::cargo_bin("bead").unwrap();
    cmd.current_dir(root).arg("init").assert().success();

    // Read the initial checkpoint generation
    let checkpoint_current = root.join(".beads/checkpoint/current.json");
    let first_checkpoint = std::fs::read_to_string(&checkpoint_current).unwrap();

    // Second init should succeed (idempotent)
    let mut cmd = Command::cargo_bin("bead").unwrap();
    cmd.current_dir(root).arg("init").assert().success();

    // Verify checkpoint was not overwritten
    let second_checkpoint = std::fs::read_to_string(&checkpoint_current).unwrap();
    assert_eq!(
        first_checkpoint, second_checkpoint,
        "Second init should not overwrite existing checkpoint"
    );
}

#[test]
fn test_unimplemented_command() {
    let mut cmd = Command::cargo_bin("bead").unwrap();
    cmd.arg("create").assert().failure();
}

#[test]
fn test_help_command() {
    let mut cmd = Command::cargo_bin("bead").unwrap();
    cmd.arg("--help").assert().success();
}

#[test]
fn test_version_short() {
    let mut cmd = Command::cargo_bin("bead").unwrap();
    cmd.arg("-V").assert().success();
}

#[test]
fn test_version_long() {
    let mut cmd = Command::cargo_bin("bead").unwrap();
    cmd.arg("--version").assert().success();
}

#[test]
fn test_version_command() {
    let mut cmd = Command::cargo_bin("bead").unwrap();
    cmd.arg("--version").assert().success();
}

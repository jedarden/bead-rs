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

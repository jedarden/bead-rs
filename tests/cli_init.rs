//! Integration tests for bead CLI init command

use assert_cmd::Command;

#[test]
fn test_init_creates_workspace() {
    // Use /var/tmp to avoid conflicts with /tmp/.beads
    let temp = tempfile::Builder::new()
        .prefix("bead-test-")
        .tempdir_in("/var/tmp")
        .unwrap();
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
    let temp = tempfile::Builder::new()
        .prefix("bead-test-")
        .tempdir_in("/var/tmp")
        .unwrap();
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
    let temp = tempfile::Builder::new()
        .prefix("bead-test-")
        .tempdir_in("/var/tmp")
        .unwrap();
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
    let temp = tempfile::Builder::new()
        .prefix("bead-test-")
        .tempdir_in("/var/tmp")
        .unwrap();
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
    let temp = tempfile::Builder::new()
        .prefix("bead-test-")
        .tempdir_in("/var/tmp")
        .unwrap();
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
    let temp = tempfile::Builder::new()
        .prefix("bead-test-")
        .tempdir_in("/var/tmp")
        .unwrap();
    let root = temp.path();

    let mut cmd = Command::cargo_bin("bead").unwrap();
    cmd.current_dir(root).arg("init").assert().success();

    // Check .gitignore was created
    let gitignore_path = root.join(".beads/.gitignore");
    assert!(gitignore_path.exists());

    let content = std::fs::read_to_string(&gitignore_path).unwrap();

    // Runtime database artifacts should be excluded
    assert!(content.contains("*.db"), "Should exclude *.db files");
    assert!(
        content.contains("*.db-shm"),
        "Should exclude *.db-shm files"
    );
    assert!(
        content.contains("*.db-wal"),
        "Should exclude *.db-wal files"
    );
    assert!(
        content.contains("*.db.backup.*"),
        "Should exclude database backups"
    );

    // Lock files should be excluded
    assert!(content.contains("*.lock"), "Should exclude lock files");

    // Temporary files should be excluded
    assert!(content.contains("*.tmp"), "Should exclude *.tmp files");
    assert!(content.contains("*.temp"), "Should exclude *.temp files");

    // Journals should be excluded
    assert!(
        content.contains("*.journal"),
        "Should exclude journal files"
    );

    // Runtime directories should be excluded
    assert!(
        content.contains("traces/"),
        "Should exclude traces/ directory"
    );
    assert!(
        content.contains("diagnostics/"),
        "Should exclude diagnostics/ directory"
    );
    assert!(
        content.contains("receipts/"),
        "Should exclude receipts/ directory"
    );

    // Runtime event logs should be excluded
    assert!(
        content.contains("events.jsonl"),
        "Should exclude events.jsonl"
    );
    assert!(
        content.contains("heartbeats.jsonl"),
        "Should exclude heartbeats.jsonl"
    );
}

#[test]
fn test_init_creates_initial_checkpoint() {
    let temp = tempfile::Builder::new()
        .prefix("bead-test-")
        .tempdir_in("/var/tmp")
        .unwrap();
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
    let temp = tempfile::Builder::new()
        .prefix("bead-test-")
        .tempdir_in("/var/tmp")
        .unwrap();
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
    let temp = tempfile::Builder::new()
        .prefix("bead-test-")
        .tempdir_in("/var/tmp")
        .unwrap();
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

#[test]
fn test_gitignore_trackable_files_not_excluded() {
    let temp = tempfile::Builder::new()
        .prefix("bead-test-")
        .tempdir_in("/var/tmp")
        .unwrap();
    let root = temp.path();

    let mut cmd = Command::cargo_bin("bead").unwrap();
    cmd.current_dir(root).arg("init").assert().success();

    let gitignore_path = root.join(".beads/.gitignore");
    let content = std::fs::read_to_string(&gitignore_path).unwrap();

    // Trackable files should NOT be excluded
    assert!(
        !content.contains("config.json"),
        "config.json should be trackable"
    );
    assert!(
        !content.contains("checkpoint/"),
        "checkpoint/ directory should be trackable"
    );

    // Verify trackable files actually exist
    assert!(
        root.join(".beads/config.json").exists(),
        "config.json should exist"
    );
    assert!(
        root.join(".beads/checkpoint").exists(),
        "checkpoint/ directory should exist"
    );
}

#[test]
fn test_init_preserves_existing_custom_gitignore() {
    let temp = tempfile::Builder::new()
        .prefix("bead-test-")
        .tempdir_in("/var/tmp")
        .unwrap();
    let root = temp.path();

    // Create .beads directory with a custom .gitignore and config.json BEFORE init
    // This simulates an existing workspace with custom ignore rules
    let beads_dir = root.join(".beads");
    std::fs::create_dir_all(&beads_dir).unwrap();

    // Create a minimal config.json to make this a valid existing workspace
    let config_path = beads_dir.join("config.json");
    let config_content = r#"{"version": 1, "uuid": "test-uuid", "prefix": "test"}"#;
    std::fs::write(&config_path, config_content).unwrap();

    let custom_gitignore = beads_dir.join(".gitignore");
    let custom_content = "# Custom gitignore\n*.log\n*.tmp\n";
    std::fs::write(&custom_gitignore, custom_content).unwrap();

    // Now run init (should preserve the custom .gitignore)
    let mut cmd = Command::cargo_bin("bead").unwrap();
    cmd.current_dir(root).arg("init").assert().success();

    // Verify the custom .gitignore was preserved byte-for-byte
    let preserved_content = std::fs::read_to_string(&custom_gitignore).unwrap();
    assert_eq!(
        custom_content, preserved_content,
        "Custom .gitignore should be preserved exactly"
    );
}

#[test]
fn test_init_existing_workspace_no_gitignore() {
    let temp = tempfile::Builder::new()
        .prefix("bead-test-")
        .tempdir_in("/var/tmp")
        .unwrap();
    let root = temp.path();

    // First init: creates workspace with .gitignore
    let mut cmd = Command::cargo_bin("bead").unwrap();
    cmd.current_dir(root).arg("init").assert().success();

    let gitignore_path = root.join(".beads/.gitignore");
    let _first_content = std::fs::read_to_string(&gitignore_path).unwrap();

    // Remove the .gitignore to simulate an existing workspace without one
    std::fs::remove_file(&gitignore_path).unwrap();

    // Second init: should NOT recreate the .gitignore (workspace already existed)
    let mut cmd = Command::cargo_bin("bead").unwrap();
    cmd.current_dir(root).arg("init").assert().success();

    // Verify .gitignore was NOT recreated
    assert!(
        !gitignore_path.exists(),
        ".gitignore should not be recreated for existing workspace"
    );
}

#[test]
fn test_init_fresh_clone_recovery_preserves_gitignore() {
    let temp = tempfile::Builder::new()
        .prefix("bead-test-")
        .tempdir_in("/var/tmp")
        .unwrap();
    let root = temp.path();

    // First init: creates complete workspace
    let mut cmd = Command::cargo_bin("bead").unwrap();
    cmd.current_dir(root).arg("init").assert().success();

    let gitignore_path = root.join(".beads/.gitignore");
    let config_path = root.join(".beads/config.json");
    let original_config = std::fs::read_to_string(&config_path).unwrap();

    // Simulate fresh clone: remove database but keep config and .gitignore
    std::fs::remove_file(root.join(".beads/beads.db")).unwrap();

    // Modify the .gitignore to test preservation
    let modified_gitignore = "# Modified gitignore\n*.custom\n";
    std::fs::write(&gitignore_path, modified_gitignore).unwrap();

    // Run init again (fresh-clone recovery)
    let mut cmd = Command::cargo_bin("bead").unwrap();
    cmd.current_dir(root).arg("init").assert().success();

    // Verify database was recreated
    assert!(
        root.join(".beads/beads.db").exists(),
        "Database should be recreated"
    );

    // Verify .gitignore was preserved (not overwritten)
    let preserved_gitignore = std::fs::read_to_string(&gitignore_path).unwrap();
    assert_eq!(
        modified_gitignore, preserved_gitignore,
        "Modified .gitignore should be preserved during recovery"
    );

    // Verify config was preserved (not overwritten)
    let preserved_config = std::fs::read_to_string(&config_path).unwrap();
    assert_eq!(
        original_config, preserved_config,
        "config.json should be preserved during recovery"
    );
}

#[test]
fn test_gitignore_excludes_all_runtime_artifacts() {
    let temp = tempfile::Builder::new()
        .prefix("bead-test-")
        .tempdir_in("/var/tmp")
        .unwrap();
    let root = temp.path();

    let mut cmd = Command::cargo_bin("bead").unwrap();
    cmd.current_dir(root).arg("init").assert().success();

    let gitignore_path = root.join(".beads/.gitignore");
    let content = std::fs::read_to_string(&gitignore_path).unwrap();

    // All runtime artifact patterns should be present
    let expected_patterns = [
        "*.db",
        "*.db-shm",
        "*.db-wal",
        "*.db.backup.*",
        "*.lock",
        "*.tmp",
        "*.temp",
        "*.journal",
        "traces/",
        "diagnostics/",
        "receipts/",
        "events.jsonl",
        "heartbeats.jsonl",
    ];

    for pattern in &expected_patterns {
        assert!(
            content.contains(pattern),
            "Should exclude pattern: {}",
            pattern
        );
    }

    // Should have reasonable structure with comments
    assert!(
        content.contains("#"),
        "Should have comments for organization"
    );
}

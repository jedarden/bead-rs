//! Integration tests for bead CLI init command

use assert_cmd::Command;

#[test]
fn test_init_creates_workspace() {
    let temp = temp_workspace_dir();
    let root = temp.path();

    let mut cmd = Command::cargo_bin("bead").unwrap();
    cmd.current_dir(root)
        .args(["init", "--skip-foreign-workspace"])
        .assert()
        .success();
    force_monolithic_mode(root);

    // Check that workspace was created
    let beads_dir = root.join(".beads");
    assert!(beads_dir.exists());

    let db_path = root.join(".beads/beads.db");
    assert!(db_path.exists());
}

#[test]
fn test_init_with_custom_prefix() {
    let temp = temp_workspace_dir();
    let root = temp.path();

    let mut cmd = Command::cargo_bin("bead").unwrap();
    cmd.current_dir(root)
        .args(["init", "--skip-foreign-workspace"])
        .arg("--prefix")
        .arg("custom")
        .assert()
        .success();
    force_monolithic_mode(root);

    // Check that prefix was used
    let config_path = root.join(".beads/config.json");
    let config_content = std::fs::read_to_string(config_path).unwrap();
    let config: serde_json::Value = serde_json::from_str(&config_content).unwrap();
    assert_eq!(config["prefix"], "custom");
}

#[test]
fn test_init_invalid_prefix() {
    let temp = temp_workspace_dir();
    let root = temp.path();

    let mut cmd = Command::cargo_bin("bead").unwrap();
    cmd.current_dir(root)
        .args(["init", "--skip-foreign-workspace"])
        .arg("--prefix")
        .arg("INVALID")
        .assert()
        .failure();
}

#[test]
fn test_init_idempotent() {
    let temp = temp_workspace_dir();
    let root = temp.path();

    // First init
    let mut cmd = Command::cargo_bin("bead").unwrap();
    cmd.current_dir(root)
        .args(["init", "--skip-foreign-workspace"])
        .assert()
        .success();
    force_monolithic_mode(root);

    // Second init should succeed (idempotent)
    let mut cmd = Command::cargo_bin("bead").unwrap();
    cmd.current_dir(root)
        .args(["init", "--skip-foreign-workspace"])
        .assert()
        .success();
    force_monolithic_mode(root);
}

#[test]
fn test_init_creates_checkpoint_and_receipts_directories() {
    let temp = temp_workspace_dir();
    let root = temp.path();

    let mut cmd = Command::cargo_bin("bead").unwrap();
    cmd.current_dir(root)
        .args(["init", "--skip-foreign-workspace"])
        .assert()
        .success();
    force_monolithic_mode(root);

    // Check checkpoint directory
    let checkpoint_dir = root.join(".beads/checkpoint");
    assert!(checkpoint_dir.exists());

    // Check receipts directory
    let receipts_dir = root.join(".beads/receipts");
    assert!(receipts_dir.exists());
}

#[test]
fn test_init_creates_gitignore() {
    let temp = temp_workspace_dir();
    let root = temp.path();

    let mut cmd = Command::cargo_bin("bead").unwrap();
    cmd.current_dir(root)
        .args(["init", "--skip-foreign-workspace"])
        .assert()
        .success();
    force_monolithic_mode(root);

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
    // 2026-09-18 leak set: recovery trees and bf-era exports carried worker
    // stdout and credentials into public history in four repos.
    assert!(
        content.contains(".br_recovery/"),
        "Should exclude .br_recovery/ directory"
    );
    assert!(
        content.contains(".bf_history/"),
        "Should exclude .bf_history/ directory"
    );
    assert!(
        content.contains("recovery/"),
        "Should exclude recovery/ directory"
    );
    assert!(content.contains("logs/"), "Should exclude logs/ directory");
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
    let temp = temp_workspace_dir();
    let root = temp.path();

    let mut cmd = Command::cargo_bin("bead").unwrap();
    cmd.current_dir(root)
        .args(["init", "--skip-foreign-workspace"])
        .assert()
        .success();
    force_monolithic_mode(root);

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
    // 5d4c08a6 (checkpoint-set-v2 spec 9): the adaptive default on a fresh
    // workspace resolves to set-v2, which retires the forensic view.
    assert_eq!(checkpoint["mode"], "set-v2");
    assert_eq!(checkpoint["issue_count"], 0);
    assert_eq!(checkpoint["event_count"], 0);
    assert_eq!(checkpoint["receipt_count"], 0);

    let forensic_path = root.join(".beads/checkpoint/forensic.jsonl");
    assert!(
        !forensic_path.exists(),
        "the set-v2 default retires the forensic view"
    );
}

#[test]
fn test_init_with_no_auto_flush_skips_checkpoint() {
    let temp = temp_workspace_dir();
    let root = temp.path();

    let mut cmd = Command::cargo_bin("bead").unwrap();
    cmd.current_dir(root)
        .args(["init", "--skip-foreign-workspace"])
        .arg("--no-auto-flush")
        .assert()
        .success();
    force_monolithic_mode(root);

    // Check that initial checkpoint was NOT created
    let checkpoint_current = root.join(".beads/checkpoint/current.json");
    assert!(
        !checkpoint_current.exists(),
        "Initial checkpoint should not exist with --no-auto-flush"
    );
}

#[test]
fn test_init_idempotent_does_not_overwrite_checkpoint() {
    let temp = temp_workspace_dir();
    let root = temp.path();

    // First init
    let mut cmd = Command::cargo_bin("bead").unwrap();
    cmd.current_dir(root)
        .args(["init", "--skip-foreign-workspace"])
        .assert()
        .success();
    force_monolithic_mode(root);

    // Read the initial checkpoint generation
    let checkpoint_current = root.join(".beads/checkpoint/current.json");
    let first_checkpoint = std::fs::read_to_string(&checkpoint_current).unwrap();

    // Second init should succeed (idempotent)
    let mut cmd = Command::cargo_bin("bead").unwrap();
    cmd.current_dir(root)
        .args(["init", "--skip-foreign-workspace"])
        .assert()
        .success();
    force_monolithic_mode(root);

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
    let temp = temp_workspace_dir();
    let root = temp.path();

    let mut cmd = Command::cargo_bin("bead").unwrap();
    cmd.current_dir(root)
        .args(["init", "--skip-foreign-workspace"])
        .assert()
        .success();
    force_monolithic_mode(root);

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
    let temp = temp_workspace_dir();
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
    cmd.current_dir(root)
        .args(["init", "--skip-foreign-workspace"])
        .assert()
        .success();
    force_monolithic_mode(root);

    // Verify the custom .gitignore was preserved byte-for-byte
    let preserved_content = std::fs::read_to_string(&custom_gitignore).unwrap();
    assert_eq!(
        custom_content, preserved_content,
        "Custom .gitignore should be preserved exactly"
    );
}

#[test]
fn test_init_existing_workspace_no_gitignore() {
    let temp = temp_workspace_dir();
    let root = temp.path();

    // First init: creates workspace with .gitignore
    let mut cmd = Command::cargo_bin("bead").unwrap();
    cmd.current_dir(root)
        .args(["init", "--skip-foreign-workspace"])
        .assert()
        .success();
    force_monolithic_mode(root);

    let gitignore_path = root.join(".beads/.gitignore");
    let _first_content = std::fs::read_to_string(&gitignore_path).unwrap();

    // Remove the .gitignore to simulate an existing workspace without one
    std::fs::remove_file(&gitignore_path).unwrap();

    // Second init: should NOT recreate the .gitignore (workspace already existed)
    let mut cmd = Command::cargo_bin("bead").unwrap();
    cmd.current_dir(root)
        .args(["init", "--skip-foreign-workspace"])
        .assert()
        .success();
    force_monolithic_mode(root);

    // Verify .gitignore was NOT recreated
    assert!(
        !gitignore_path.exists(),
        ".gitignore should not be recreated for existing workspace"
    );
}

#[test]
fn test_init_fresh_clone_recovery_preserves_gitignore() {
    let temp = temp_workspace_dir();
    let root = temp.path();

    // First init: creates complete workspace
    let mut cmd = Command::cargo_bin("bead").unwrap();
    cmd.current_dir(root)
        .args(["init", "--skip-foreign-workspace"])
        .assert()
        .success();
    force_monolithic_mode(root);

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
    cmd.current_dir(root)
        .args(["init", "--skip-foreign-workspace"])
        .assert()
        .success();
    force_monolithic_mode(root);

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
    let temp = temp_workspace_dir();
    let root = temp.path();

    let mut cmd = Command::cargo_bin("bead").unwrap();
    cmd.current_dir(root)
        .args(["init", "--skip-foreign-workspace"])
        .assert()
        .success();
    force_monolithic_mode(root);

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
        ".br_recovery/",
        ".bf_history/",
        "recovery/",
        "logs/",
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

/// The adaptive default resolves to set-v2 since 5d4c08a6 (checkpoint-set-v2
/// spec 9); these legs assert the monolith layout, so pin the mode they were
/// written against.
fn force_monolithic_mode(workspace: &std::path::Path) {
    let path = workspace.join(".beads/config.json");
    let mut config: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    config
        .as_object_mut()
        .unwrap()
        .entry("checkpoint")
        .or_insert(serde_json::Value::Object(Default::default()))
        .as_object_mut()
        .unwrap()
        .insert(
            "mode".into(),
            serde_json::Value::String("monolithic".to_string()),
        );
    std::fs::write(&path, serde_json::to_string_pretty(&config).unwrap()).unwrap();
}

/// Create the tempdir backing a test workspace.
///
/// `bead init` fails closed when discovery finds an enclosing workspace, so
/// the base must not itself sit below a stray `.beads` directory: hosts carry
/// them (a shared smoke-test store lives at `/var/tmp/.beads` on some
/// machines), and a stray ancestor would turn every init in this file into a
/// refusal naming a workspace the test never created. `/var/tmp` stays the
/// first choice (the historical base, chosen to dodge a stray `/tmp/.beads`),
/// falling back to TMPDIR and then /dev/shm when an earlier base is occupied.
fn temp_workspace_dir() -> tempfile::TempDir {
    let mut bases = vec![
        std::path::PathBuf::from("/var/tmp"),
        std::env::temp_dir(),
        std::path::PathBuf::from("/dev/shm"),
    ];
    bases.retain(|base| base.exists() && !has_beads_ancestor(base));
    tempfile::Builder::new()
        .prefix("bead-test-")
        .tempdir_in(bases.first().expect(
            "no tempdir base with a .beads-free ancestor chain: clean up the stray .beads \
             above /var/tmp, TMPDIR, or /dev/shm",
        ))
        .unwrap()
}

/// True when the directory or any ancestor carries a `.beads` directory.
fn has_beads_ancestor(start: &std::path::Path) -> bool {
    let mut current = Some(start);
    while let Some(dir) = current {
        if dir.join(".beads").exists() {
            return true;
        }
        current = dir.parent();
    }
    false
}

/// Read a single integer from the workspace database (issue count, schema
/// version) so a test can assert the ancestor was not modified.
fn scalar(db_path: &std::path::Path, sql: &str) -> i64 {
    let conn =
        rusqlite::Connection::open_with_flags(db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    conn.query_row(sql, [], |row| row.get(0)).unwrap()
}

#[test]
fn test_init_below_workspace_fails_closed_without_nested() {
    let temp = temp_workspace_dir();
    let outer = temp.path();

    let mut cmd = Command::cargo_bin("bead").unwrap();
    cmd.current_dir(outer)
        .args(["init", "--skip-foreign-workspace", "--prefix", "outer"])
        .assert()
        .success();
    force_monolithic_mode(outer);

    // Seed a real issue so "the ancestor was untouched" is observable as
    // unchanged state, not as absence of anything to change.
    let mut cmd = Command::cargo_bin("bead").unwrap();
    cmd.current_dir(outer)
        .args(["create", "--title", "outer fixture issue"])
        .assert()
        .success();

    let outer_db = outer.join(".beads/beads.db");
    let issues_before = scalar(&outer_db, "SELECT COUNT(*) FROM issues");
    let schema_before = scalar(
        &outer_db,
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
    );
    assert!(issues_before >= 1);

    let inner = outer.join("inner");
    std::fs::create_dir(&inner).unwrap();

    // Without --nested, init below the workspace refuses with the conflict
    // exit code and names the enclosing root.
    let mut cmd = Command::cargo_bin("bead").unwrap();
    cmd.current_dir(&inner)
        .args(["init", "--skip-foreign-workspace", "--prefix", "inner"])
        .assert()
        .failure()
        .code(4)
        .stderr(predicates::str::contains(outer.display().to_string()))
        .stderr(predicates::str::contains("--nested"));

    // The refusal touched nothing: no workspace created here, the ancestor's
    // issue count and schema version unchanged.
    assert!(
        !inner.join(".beads").exists(),
        "refused init must not create a workspace in the subdirectory"
    );
    assert_eq!(
        issues_before,
        scalar(&outer_db, "SELECT COUNT(*) FROM issues")
    );
    assert_eq!(
        schema_before,
        scalar(
            &outer_db,
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations"
        )
    );
}

#[test]
fn test_init_nested_creates_separate_workspace() {
    let temp = temp_workspace_dir();
    let outer = temp.path();

    let mut cmd = Command::cargo_bin("bead").unwrap();
    cmd.current_dir(outer)
        .args(["init", "--skip-foreign-workspace", "--prefix", "outer"])
        .assert()
        .success();
    force_monolithic_mode(outer);

    let outer_issues_before = scalar(
        &outer.join(".beads/beads.db"),
        "SELECT COUNT(*) FROM issues",
    );

    let inner = outer.join("inner");
    std::fs::create_dir(&inner).unwrap();

    let mut cmd = Command::cargo_bin("bead").unwrap();
    cmd.current_dir(&inner)
        .args([
            "init",
            "--skip-foreign-workspace",
            "--prefix",
            "inner",
            "--nested",
        ])
        .assert()
        .success()
        .stderr(predicates::str::contains(format!(
            "Root: {}",
            inner.display()
        )));
    force_monolithic_mode(&inner);

    // A separate workspace exists here, carrying the requested prefix.
    let config_path = inner.join(".beads/config.json");
    let config: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
    assert_eq!(config["prefix"], "inner");

    // And it really is separate: an issue created here lands in the nested
    // store, not the enclosing one.
    let mut cmd = Command::cargo_bin("bead").unwrap();
    cmd.current_dir(&inner)
        .args(["create", "--title", "nested fixture issue"])
        .assert()
        .success();
    assert_eq!(
        1,
        scalar(
            &inner.join(".beads/beads.db"),
            "SELECT COUNT(*) FROM issues"
        ),
        "issue created in the nested workspace must live in its own store"
    );
    assert_eq!(
        outer_issues_before,
        scalar(
            &outer.join(".beads/beads.db"),
            "SELECT COUNT(*) FROM issues"
        ),
        "nested init must not route issues into the enclosing workspace"
    );
}

#[test]
fn test_init_at_root_still_idempotent_with_nested_child() {
    let temp = temp_workspace_dir();
    let outer = temp.path();

    let mut cmd = Command::cargo_bin("bead").unwrap();
    cmd.current_dir(outer)
        .args(["init", "--skip-foreign-workspace", "--prefix", "outer"])
        .assert()
        .success();
    force_monolithic_mode(outer);

    // A nested child workspace exists below the root; discovery from the
    // root stops at the root's own .beads before ever seeing it.
    let inner = outer.join("inner");
    std::fs::create_dir(&inner).unwrap();
    let mut cmd = Command::cargo_bin("bead").unwrap();
    cmd.current_dir(&inner)
        .args([
            "init",
            "--skip-foreign-workspace",
            "--prefix",
            "inner",
            "--nested",
        ])
        .assert()
        .success();
    force_monolithic_mode(&inner);

    // Re-init at the workspace root itself stays the idempotent reuse path.
    let mut cmd = Command::cargo_bin("bead").unwrap();
    cmd.current_dir(outer)
        .args(["init", "--skip-foreign-workspace", "--prefix", "outer"])
        .assert()
        .success()
        .stderr(predicates::str::contains("Workspace already exists"))
        .stderr(predicates::str::contains(outer.display().to_string()));
}

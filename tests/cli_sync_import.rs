//! Integration tests for `bead sync --import-only`

use assert_cmd::Command;
use predicates::prelude::*;
use serial_test::serial;
use std::fs;
use tempfile::TempDir;

#[test]
#[serial]
fn test_sync_import_only_basic() {
    let temp_dir = TempDir::new().unwrap();
    let _workspace_path = temp_dir.path().join(".beads");

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .current_dir(temp_dir.path())
        .assert()
        .success();

    // Create import file with two issues
    let import_path = temp_dir.path().join("import.jsonl");
    let import_content = r#"{"id":"test-0000000000000001","title":"Test Issue 1","description":"Description 1","priority":2,"issue_type":"task","base_status":"open","manual_blocked":false,"created_at":"2026-08-08T12:00:00Z","updated_at":"2026-08-08T12:00:00Z","profile":"native-v1","schema_ref":"urn:bead-rs:schema:issue:native-v1"}
{"id":"test-0000000000000002","title":"Test Issue 2","description":"Description 2","priority":1,"issue_type":"bug","base_status":"open","manual_blocked":false,"created_at":"2026-08-08T12:00:00Z","updated_at":"2026-08-08T12:00:00Z","profile":"native-v1","schema_ref":"urn:bead-rs:schema:issue:native-v1"}"#;
    fs::write(&import_path, import_content).unwrap();

    // Import issues
    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "sync",
            "import-only",
            "--input",
            import_path.to_str().unwrap(),
            "--profile",
            "native-v1",
        ])
        .current_dir(temp_dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("Imported checkpoint:"))
        .stderr(predicate::str::contains("Inserted: 2"));

    // Verify issues were imported
    Command::cargo_bin("bead")
        .unwrap()
        .args(["list", "--json"])
        .current_dir(temp_dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("test-0000000000000001"))
        .stdout(predicate::str::contains("test-0000000000000002"));
}

#[test]
#[serial]
fn test_sync_import_only_dry_run() {
    let temp_dir = TempDir::new().unwrap();
    let _workspace_path = temp_dir.path().join(".beads");

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .current_dir(temp_dir.path())
        .assert()
        .success();

    // Create import file
    let import_path = temp_dir.path().join("import.jsonl");
    let import_content = r#"{"id":"test-0000000000000001","title":"Test Issue 1","description":"Description 1","priority":2,"issue_type":"task","base_status":"open","manual_blocked":false,"created_at":"2026-08-08T12:00:00Z","updated_at":"2026-08-08T12:00:00Z","profile":"native-v1","schema_ref":"urn:bead-rs:schema:issue:native-v1"}"#;
    fs::write(&import_path, import_content).unwrap();

    // Run dry-run import
    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "sync",
            "import-only",
            "--input",
            import_path.to_str().unwrap(),
            "--profile",
            "native-v1",
            "--dry-run",
        ])
        .current_dir(temp_dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("Dry-run import analysis:"))
        .stderr(predicate::str::contains("Prospective: true"));

    // Verify no issues were actually imported (workspace should still be empty)
    Command::cargo_bin("bead")
        .unwrap()
        .args(["list", "--json"])
        .current_dir(temp_dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("[]")); // Empty array
}

#[test]
#[serial]
fn test_sync_import_only_malformed_json() {
    let temp_dir = TempDir::new().unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .current_dir(temp_dir.path())
        .assert()
        .success();

    // Create import file with malformed JSON
    let import_path = temp_dir.path().join("import.jsonl");
    fs::write(&import_path, "not a json object").unwrap();

    // Import should fail
    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "sync",
            "import-only",
            "--input",
            import_path.to_str().unwrap(),
        ])
        .current_dir(temp_dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("Line 1: malformed JSON"));
}

#[test]
#[serial]
fn test_sync_import_only_duplicate_id() {
    let temp_dir = TempDir::new().unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .current_dir(temp_dir.path())
        .assert()
        .success();

    // Create import file with duplicate IDs
    let import_path = temp_dir.path().join("import.jsonl");
    let import_content = r#"{"id":"test-0000000000000001","title":"Test Issue 1","description":"Description 1","priority":2,"issue_type":"task","base_status":"open","manual_blocked":false,"created_at":"2026-08-08T12:00:00Z","updated_at":"2026-08-08T12:00:00Z","profile":"native-v1","schema_ref":"urn:bead-rs:schema:issue:native-v1"}
{"id":"test-0000000000000001","title":"Test Issue 1 Duplicate","description":"Description 1","priority":2,"issue_type":"task","base_status":"open","manual_blocked":false,"created_at":"2026-08-08T12:00:00Z","updated_at":"2026-08-08T12:00:00Z","profile":"native-v1","schema_ref":"urn:bead-rs:schema:issue:native-v1"}"#;
    fs::write(&import_path, import_content).unwrap();

    // Import should fail
    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "sync",
            "import-only",
            "--input",
            import_path.to_str().unwrap(),
        ])
        .current_dir(temp_dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("duplicate issue ID"));
}

#[test]
#[serial]
fn test_sync_import_only_missing_id() {
    let temp_dir = TempDir::new().unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .current_dir(temp_dir.path())
        .assert()
        .success();

    // Create import file without ID field
    let import_path = temp_dir.path().join("import.jsonl");
    let import_content = r#"{"title":"Test Issue 1","description":"Description 1","priority":2,"issue_type":"task","base_status":"open","manual_blocked":false,"created_at":"2026-08-08T12:00:00Z","updated_at":"2026-08-08T12:00:00Z","profile":"native-v1","schema_ref":"urn:bead-rs:schema:issue:native-v1"}"#;
    fs::write(&import_path, import_content).unwrap();

    // Import should fail
    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "sync",
            "import-only",
            "--input",
            import_path.to_str().unwrap(),
        ])
        .current_dir(temp_dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("missing or invalid 'id' field"));
}

#[test]
#[serial]
fn test_sync_import_only_self_edge() {
    let temp_dir = TempDir::new().unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .current_dir(temp_dir.path())
        .assert()
        .success();

    // Create import file with self-edge dependency
    let import_path = temp_dir.path().join("import.jsonl");
    let import_content = r#"{"id":"test-0000000000000001","title":"Test Issue 1","description":"Description 1","priority":2,"issue_type":"task","base_status":"open","manual_blocked":false,"created_at":"2026-08-08T12:00:00Z","updated_at":"2026-08-08T12:00:00Z","profile":"native-v1","schema_ref":"urn:bead-rs:schema:issue:native-v1","dependencies":[{"blocker":"test-0000000000000001","kind":"blocks"}]}"#;
    fs::write(&import_path, import_content).unwrap();

    // Import should fail
    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "sync",
            "import-only",
            "--input",
            import_path.to_str().unwrap(),
        ])
        .current_dir(temp_dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("Self-edge detected"));
}

#[test]
#[serial]
fn test_sync_import_only_cycle() {
    let temp_dir = TempDir::new().unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .current_dir(temp_dir.path())
        .assert()
        .success();

    // Create import file with cyclic dependency
    let import_path = temp_dir.path().join("import.jsonl");
    let import_content = r#"{"id":"test-0000000000000001","title":"Test Issue 1","description":"Description 1","priority":2,"issue_type":"task","base_status":"open","manual_blocked":false,"created_at":"2026-08-08T12:00:00Z","updated_at":"2026-08-08T12:00:00Z","profile":"native-v1","schema_ref":"urn:bead-rs:schema:issue:native-v1","dependencies":[{"blocker":"test-0000000000000002","kind":"blocks"}]}
{"id":"test-0000000000000002","title":"Test Issue 2","description":"Description 2","priority":2,"issue_type":"task","base_status":"open","manual_blocked":false,"created_at":"2026-08-08T12:00:00Z","updated_at":"2026-08-08T12:00:00Z","profile":"native-v1","schema_ref":"urn:bead-rs:schema:issue:native-v1","dependencies":[{"blocker":"test-0000000000000001","kind":"blocks"}]}"#;
    fs::write(&import_path, import_content).unwrap();

    // Import should fail
    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "sync",
            "import-only",
            "--input",
            import_path.to_str().unwrap(),
        ])
        .current_dir(temp_dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("Cycle detected"));
}

#[test]
#[serial]
fn test_sync_import_only_dangling_dependency() {
    let temp_dir = TempDir::new().unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .current_dir(temp_dir.path())
        .assert()
        .success();

    // Create import file with dangling dependency
    let import_path = temp_dir.path().join("import.jsonl");
    let import_content = r#"{"id":"test-0000000000000001","title":"Test Issue 1","description":"Description 1","priority":2,"issue_type":"task","base_status":"open","manual_blocked":false,"created_at":"2026-08-08T12:00:00Z","updated_at":"2026-08-08T12:00:00Z","profile":"native-v1","schema_ref":"urn:bead-rs:schema:issue:native-v1","dependencies":[{"blocker":"test-0000000000000002","kind":"blocks"}]}"#;
    fs::write(&import_path, import_content).unwrap();

    // Import should fail
    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "sync",
            "import-only",
            "--input",
            import_path.to_str().unwrap(),
        ])
        .current_dir(temp_dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown blocker issue"));
}

#[test]
#[serial]
fn test_sync_import_only_invalid_profile() {
    let temp_dir = TempDir::new().unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .current_dir(temp_dir.path())
        .assert()
        .success();

    // Create import file
    let import_path = temp_dir.path().join("import.jsonl");
    let import_content = r#"{"id":"test-0000000000000001","title":"Test Issue 1","description":"Description 1","priority":2,"issue_type":"task","base_status":"open","manual_blocked":false,"created_at":"2026-08-08T12:00:00Z","updated_at":"2026-08-08T12:00:00Z","profile":"native-v1","schema_ref":"urn:bead-rs:schema:issue:native-v1"}"#;
    fs::write(&import_path, import_content).unwrap();

    // Import with invalid profile should fail
    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "sync",
            "import-only",
            "--input",
            import_path.to_str().unwrap(),
            "--profile",
            "invalid-profile",
        ])
        .current_dir(temp_dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("is not supported before F017"));
}

#[test]
#[serial]
fn test_sync_import_only_empty_target() {
    let temp_dir = TempDir::new().unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .current_dir(temp_dir.path())
        .assert()
        .success();

    // Create an issue first
    Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Existing Issue"])
        .current_dir(temp_dir.path())
        .assert()
        .success();

    // Create import file
    let import_path = temp_dir.path().join("import.jsonl");
    let import_content = r#"{"id":"test-0000000000000001","title":"Test Issue 1","description":"Description 1","priority":2,"issue_type":"task","base_status":"open","manual_blocked":false,"created_at":"2026-08-08T12:00:00Z","updated_at":"2026-08-08T12:00:00Z","profile":"native-v1","schema_ref":"urn:bead-rs:schema:issue:native-v1"}"#;
    fs::write(&import_path, import_content).unwrap();

    // Import should fail because target is not empty
    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "sync",
            "import-only",
            "--input",
            import_path.to_str().unwrap(),
        ])
        .current_dir(temp_dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("Target database is not empty"));
}

#[test]
#[serial]
fn test_sync_import_only_with_dependencies() {
    let temp_dir = TempDir::new().unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .current_dir(temp_dir.path())
        .assert()
        .success();

    // Create import file with valid dependency
    let import_path = temp_dir.path().join("import.jsonl");
    let import_content = r#"{"id":"test-0000000000000001","title":"Test Issue 1","description":"Description 1","priority":2,"issue_type":"task","base_status":"open","manual_blocked":false,"created_at":"2026-08-08T12:00:00Z","updated_at":"2026-08-08T12:00:00Z","profile":"native-v1","schema_ref":"urn:bead-rs:schema:issue:native-v1","dependencies":[{"blocker":"test-0000000000000002","kind":"blocks"}]}
{"id":"test-0000000000000002","title":"Test Issue 2","description":"Description 2","priority":2,"issue_type":"task","base_status":"open","manual_blocked":false,"created_at":"2026-08-08T12:00:00Z","updated_at":"2026-08-08T12:00:00Z","profile":"native-v1","schema_ref":"urn:bead-rs:schema:issue:native-v1"}"#;
    fs::write(&import_path, import_content).unwrap();

    // Import should succeed
    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "sync",
            "import-only",
            "--input",
            import_path.to_str().unwrap(),
        ])
        .current_dir(temp_dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("Imported checkpoint:"))
        .stderr(predicate::str::contains("Inserted: 2"));

    // Verify dependency was created
    Command::cargo_bin("bead")
        .unwrap()
        .args(["show", "test-0000000000000001", "--json"])
        .current_dir(temp_dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("test-0000000000000002"));
}

#[test]
#[serial]
fn test_sync_import_only_with_labels() {
    let temp_dir = TempDir::new().unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .current_dir(temp_dir.path())
        .assert()
        .success();

    // Create import file with labels
    let import_path = temp_dir.path().join("import.jsonl");
    let import_content = r#"{"id":"test-0000000000000001","title":"Test Issue 1","description":"Description 1","priority":2,"issue_type":"task","base_status":"open","manual_blocked":false,"created_at":"2026-08-08T12:00:00Z","updated_at":"2026-08-08T12:00:00Z","profile":"native-v1","schema_ref":"urn:bead-rs:schema:issue:native-v1","labels":["bug","urgent"]}"#;
    fs::write(&import_path, import_content).unwrap();

    // Import should succeed
    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "sync",
            "import-only",
            "--input",
            import_path.to_str().unwrap(),
        ])
        .current_dir(temp_dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("Inserted: 1"));

    // Verify labels were imported
    Command::cargo_bin("bead")
        .unwrap()
        .args(["show", "test-0000000000000001", "--json"])
        .current_dir(temp_dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("bug"))
        .stdout(predicate::str::contains("urgent"));
}

#[test]
#[serial]
fn test_sync_import_only_blank_lines() {
    let temp_dir = TempDir::new().unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .current_dir(temp_dir.path())
        .assert()
        .success();

    // Create import file with blank lines
    let import_path = temp_dir.path().join("import.jsonl");
    let import_content = r#"
{"id":"test-0000000000000001","title":"Test Issue 1","description":"Description 1","priority":2,"issue_type":"task","base_status":"open","manual_blocked":false,"created_at":"2026-08-08T12:00:00Z","updated_at":"2026-08-08T12:00:00Z","profile":"native-v1","schema_ref":"urn:bead-rs:schema:issue:native-v1"}

{"id":"test-0000000000000002","title":"Test Issue 2","description":"Description 2","priority":2,"issue_type":"task","base_status":"open","manual_blocked":false,"created_at":"2026-08-08T12:00:00Z","updated_at":"2026-08-08T12:00:00Z","profile":"native-v1","schema_ref":"urn:bead-rs:schema:issue:native-v1"}
"#;
    fs::write(&import_path, import_content).unwrap();

    // Import should succeed (blank lines are skipped)
    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "sync",
            "import-only",
            "--input",
            import_path.to_str().unwrap(),
        ])
        .current_dir(temp_dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("Inserted: 2"));
}

#[test]
#[serial]
fn test_sync_import_only_unknown_field_preservation() {
    let temp_dir = TempDir::new().unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .current_dir(temp_dir.path())
        .assert()
        .success();

    // Create import file with unknown fields
    let import_path = temp_dir.path().join("import.jsonl");
    let import_content = r#"{"id":"test-0000000000000001","title":"Test Issue 1","description":"Description 1","priority":2,"issue_type":"task","base_status":"open","manual_blocked":false,"created_at":"2026-08-08T12:00:00Z","updated_at":"2026-08-08T12:00:00Z","profile":"native-v1","schema_ref":"urn:bead-rs:schema:issue:native-v1","custom_field":"custom_value","another_unknown":123}"#;
    fs::write(&import_path, import_content).unwrap();

    // Import should succeed
    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "sync",
            "import-only",
            "--input",
            import_path.to_str().unwrap(),
        ])
        .current_dir(temp_dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("Inserted: 1"));

    // Flush to export and verify unknown fields are preserved
    let export_path = temp_dir.path().join("export.jsonl");
    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "sync",
            "flush-only",
            "--output",
            export_path.to_str().unwrap(),
        ])
        .current_dir(temp_dir.path())
        .assert()
        .success();

    // Verify unknown fields in export
    let export_content = fs::read_to_string(&export_path).unwrap();
    assert!(export_content.contains("custom_field"));
    assert!(export_content.contains("custom_value"));
    assert!(export_content.contains("another_unknown"));
}

#[test]
#[serial]
fn test_sync_import_only_without_workspace() {
    let temp_dir = TempDir::new().unwrap();

    // Create import file
    let import_path = temp_dir.path().join("import.jsonl");
    let import_content = r#"{"id":"test-0000000000000001","title":"Test Issue 1","description":"Description 1","priority":2,"issue_type":"task","base_status":"open","manual_blocked":false,"created_at":"2026-08-08T12:00:00Z","updated_at":"2026-08-08T12:00:00Z","profile":"native-v1","schema_ref":"urn:bead-rs:schema:issue:native-v1"}"#;
    fs::write(&import_path, import_content).unwrap();

    // Import should fail without workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "sync",
            "import-only",
            "--input",
            import_path.to_str().unwrap(),
        ])
        .current_dir(temp_dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("No workspace found"));
}

#[test]
#[serial]
fn test_sync_import_only_nonexistent_input() {
    let temp_dir = TempDir::new().unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .current_dir(temp_dir.path())
        .assert()
        .success();

    // Try to import nonexistent file
    Command::cargo_bin("bead")
        .unwrap()
        .args(["sync", "import-only", "--input", "nonexistent.jsonl"])
        .current_dir(temp_dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("Input file not found"));
}

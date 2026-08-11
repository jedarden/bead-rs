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
            "--restore-into-empty",
            "--actor",
            "testuser",
        ])
        .current_dir(temp_dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("Forensic import completed:"))
        .stderr(predicate::str::contains("Mode: restore-into-empty"))
        .stderr(predicate::str::contains("Restored 2 issues"));

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
            "--restore-into-empty",
            "--actor",
            "testuser",
            "--input",
            import_path.to_str().unwrap(),
            "--profile",
            "native-v1",
            "--dry-run",
        ])
        .current_dir(temp_dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "Dry-run forensic import analysis:",
        ))
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
            "--restore-into-empty",
            "--actor",
            "testuser",
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
            "--restore-into-empty",
            "--actor",
            "testuser",
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
            "--restore-into-empty",
            "--actor",
            "testuser",
            "--input",
            import_path.to_str().unwrap(),
        ])
        .current_dir(temp_dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("missing field `id`"));
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
            "--restore-into-empty",
            "--actor",
            "testuser",
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
            "--restore-into-empty",
            "--actor",
            "testuser",
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
            "--restore-into-empty",
            "--actor",
            "testuser",
            "--input",
            import_path.to_str().unwrap(),
        ])
        .current_dir(temp_dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Dependency references non-existent blocker issue",
        ));
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
            "--restore-into-empty",
            "--actor",
            "testuser",
            "--input",
            import_path.to_str().unwrap(),
            "--profile",
            "invalid-profile",
        ])
        .current_dir(temp_dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("is not supported for import"));
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
            "--restore-into-empty",
            "--actor",
            "testuser",
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
            "--restore-into-empty",
            "--actor",
            "testuser",
            "--input",
            import_path.to_str().unwrap(),
        ])
        .current_dir(temp_dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("Forensic import completed:"))
        .stderr(predicate::str::contains("Restored 2"));

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
            "--restore-into-empty",
            "--actor",
            "testuser",
            "--input",
            import_path.to_str().unwrap(),
        ])
        .current_dir(temp_dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("Restored 1"));

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
            "--restore-into-empty",
            "--actor",
            "testuser",
            "--input",
            import_path.to_str().unwrap(),
        ])
        .current_dir(temp_dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("Restored 2"));
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
            "--restore-into-empty",
            "--actor",
            "testuser",
            "--input",
            import_path.to_str().unwrap(),
        ])
        .current_dir(temp_dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("Restored 1"));

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
            "--restore-into-empty",
            "--actor",
            "testuser",
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
        .args([
            "sync",
            "import-only",
            "--restore-into-empty",
            "--actor",
            "testuser",
            "--input",
            "nonexistent.jsonl",
        ])
        .current_dir(temp_dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("Input not found"));
}

#[test]
#[serial]
fn test_sync_import_only_external_profile_merge() {
    let temp_dir = TempDir::new().unwrap();
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .current_dir(temp_dir.path())
        .assert()
        .success();

    let import_path = temp_dir.path().join("bf.jsonl");
    let content = r#"{"id":"test-0000000000000001","title":"Blocker","description":"","design":"","acceptance_criteria":"","notes":"","status":"open","priority":2,"issue_type":"task","created_at":"2026-08-08T12:00:00Z","updated_at":"2026-08-08T12:00:00Z","events":[]}
{"id":"test-0000000000000002","title":"Blocked","description":"","design":"","acceptance_criteria":"","notes":"","status":"blocked","priority":2,"issue_type":"task","created_at":"2026-08-08T12:00:00Z","updated_at":"2026-08-08T12:00:00Z","labels":["alpha"],"dependencies":[{"issue_id":"test-0000000000000002","depends_on_id":"test-0000000000000001","type":"blocks"}],"events":[]}"#;
    fs::write(&import_path, content).unwrap();

    let output = Command::cargo_bin("bead")
        .unwrap()
        .args([
            "sync",
            "import-only",
            "--merge",
            "--actor",
            "testuser",
            "--input",
            import_path.to_str().unwrap(),
            "--profile",
            "bf-v1",
        ])
        .current_dir(temp_dir.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["profile"], "bf-v1");
    assert_eq!(report["direction"], "import");

    Command::cargo_bin("bead")
        .unwrap()
        .args(["show", "test-0000000000000002", "--json"])
        .current_dir(temp_dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("test-0000000000000001"))
        .stdout(predicate::str::contains("alpha"));
}

#[test]
#[serial]
fn accepted_observed_profile_corpora_round_trip_operationally() {
    for profile in ["br-v1", "bf-v1"] {
        let temp_dir = TempDir::new().unwrap();
        Command::cargo_bin("bead")
            .unwrap()
            .args(["init", "--prefix", "test"])
            .current_dir(temp_dir.path())
            .assert()
            .success();
        let input = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("research/fixtures")
            .join(profile)
            .join("observed-valid.jsonl");
        Command::cargo_bin("bead")
            .unwrap()
            .args([
                "sync",
                "import-only",
                "--merge",
                "--actor",
                "conformance",
                "--input",
                input.to_str().unwrap(),
                "--profile",
                profile,
            ])
            .current_dir(temp_dir.path())
            .assert()
            .success();

        let output = temp_dir.path().join("round-trip.jsonl");
        Command::cargo_bin("bead")
            .unwrap()
            .args([
                "sync",
                "flush-only",
                "--output",
                output.to_str().unwrap(),
                "--profile",
                profile,
            ])
            .current_dir(temp_dir.path())
            .assert()
            .success();

        let parse =
            |path: &std::path::Path| -> std::collections::BTreeMap<String, serde_json::Value> {
                fs::read_to_string(path)
                    .unwrap()
                    .lines()
                    .map(|line| {
                        let record: serde_json::Value = serde_json::from_str(line).unwrap();
                        (record["id"].as_str().unwrap().to_string(), record)
                    })
                    .collect()
            };
        assert_eq!(parse(&input), parse(&output), "{} observed corpus", profile);
    }
}

#[test]
#[serial]
fn external_profile_reserved_state_collision_is_atomic() {
    for profile in ["br-v1", "bf-v1"] {
        let temp_dir = TempDir::new().unwrap();
        Command::cargo_bin("bead")
            .unwrap()
            .args(["init", "--prefix", "test"])
            .current_dir(temp_dir.path())
            .assert()
            .success();
        let database = temp_dir.path().join(".beads/beads.db");
        let before = fs::read(&database).unwrap();
        let input = temp_dir.path().join("collision.jsonl");
        let mut record = serde_json::json!({
            "id": "test-0000000000000001",
            "title": "Collision",
            "status": "open",
            "priority": 2,
            "issue_type": "task",
            "created_at": "2030-01-01T00:00:00Z",
            "updated_at": "2030-01-01T00:00:00Z",
            "__profile_status__": "closed"
        });
        if profile == "bf-v1" {
            let object = record.as_object_mut().unwrap();
            for field in ["description", "design", "acceptance_criteria", "notes"] {
                object.insert(field.to_string(), serde_json::json!(""));
            }
            object.insert("events".to_string(), serde_json::json!([]));
        }
        fs::write(&input, format!("{}\n", record)).unwrap();

        Command::cargo_bin("bead")
            .unwrap()
            .args([
                "sync",
                "import-only",
                "--merge",
                "--actor",
                "conformance",
                "--input",
                input.to_str().unwrap(),
                "--profile",
                profile,
            ])
            .current_dir(temp_dir.path())
            .assert()
            .failure()
            .stderr(predicate::str::contains("known_extension_collision"));
        assert_eq!(
            fs::read(database).unwrap(),
            before,
            "{profile} mutated store"
        );
    }
}

fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) {
    fs::create_dir_all(dst).unwrap();
    for entry in fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let file_type = entry.file_type().unwrap();
        let dst_path = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(&entry.path(), &dst_path);
        } else {
            fs::copy(entry.path(), &dst_path).unwrap();
        }
    }
}

/// Regression test for two bugs found reconstituting a workspace from a
/// real `git clone`, where `.beads/config.json` is tracked but
/// `.beads/beads.db` is (correctly) gitignored:
///   1. `bead init` used to hard-error instead of self-healing when
///      config.json exists without a matching db ("Failed to load
///      workspace UUID: no such table: workspace").
///   2. `bead sync import-only --input <checkpoint-dir>` used to
///      unconditionally treat any directory checkpoint as sharded, but a
///      monolithic `flush-only` writes the same pointer+objects layout
///      with the raw JSONL data directly at active_root.path, not a shard
///      manifest -- parsing it as one failed with "trailing characters".
#[test]
#[serial]
fn test_restore_from_flushed_checkpoint_after_fresh_clone() {
    let origin = TempDir::new().unwrap();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "gol"])
        .current_dir(origin.path())
        .assert()
        .success();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "First issue"])
        .current_dir(origin.path())
        .assert()
        .success();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Second issue"])
        .current_dir(origin.path())
        .assert()
        .success();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["sync", "flush-only"])
        .current_dir(origin.path())
        .assert()
        .success();

    // Simulate a fresh `git clone`: only the tracked files survive
    // (config.json, checkpoint/*) -- beads.db does not.
    let clone = TempDir::new().unwrap();
    let clone_beads = clone.path().join(".beads");
    fs::create_dir_all(&clone_beads).unwrap();
    fs::copy(
        origin.path().join(".beads/config.json"),
        clone_beads.join("config.json"),
    )
    .unwrap();
    copy_dir_recursive(
        &origin.path().join(".beads/checkpoint"),
        &clone_beads.join("checkpoint"),
    );

    // Bug 1: bead init must self-heal, not error, given config.json
    // without a matching beads.db.
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "gol"])
        .current_dir(clone.path())
        .assert()
        .success();

    // Bug 2: restoring from the flushed checkpoint directory must
    // correctly parse the monolithic generation file, not misread it as a
    // sharded manifest.
    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "sync",
            "import-only",
            "--input",
            clone_beads.join("checkpoint").to_str().unwrap(),
            "--restore-into-empty",
            "--actor",
            "test",
        ])
        .current_dir(clone.path())
        .assert()
        .success();

    let result = Command::cargo_bin("bead")
        .unwrap()
        .args(["list"])
        .current_dir(clone.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let output = std::str::from_utf8(&result).unwrap();
    assert_eq!(output.matches("ID: gol-").count(), 2);
    assert!(output.contains("First issue"));
    assert!(output.contains("Second issue"));
}

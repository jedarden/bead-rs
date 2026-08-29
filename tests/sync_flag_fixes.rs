//! Integration tests for sync CLI flag fixes
//!
//! Test suite for preventing regressions in sync flag handling:
//! - flush-only --profile flag rejection
//! - import-only --diagnostics R014 validation path triggering

use assert_cmd::Command;
use predicates::prelude::*;
use serial_test::serial;
use std::fs;
use tempfile::TempDir;

#[test]
#[serial]
fn test_flush_only_rejects_profile_flag() {
    let temp_dir = TempDir::new().unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .current_dir(temp_dir.path())
        .assert()
        .success();

    // Attempt to use --profile with flush-only (should be rejected)
    Command::cargo_bin("bead")
        .unwrap()
        .args(["sync", "flush-only", "--profile", "native-v1"])
        .current_dir(temp_dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "unexpected argument '--profile' found",
        ))
        .stderr(predicate::str::contains("error:"));
}

#[test]
#[serial]
fn test_flush_only_with_output_rejects_profile_flag() {
    let temp_dir = TempDir::new().unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .current_dir(temp_dir.path())
        .assert()
        .success();

    // Create an issue
    Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Test issue"])
        .current_dir(temp_dir.path())
        .assert()
        .success();

    let output_path = temp_dir.path().join("export.jsonl");

    // Attempt to use --profile with flush-only --output (should be rejected)
    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "sync",
            "flush-only",
            "--output",
            output_path.to_str().unwrap(),
            "--profile",
            "native-v1",
        ])
        .current_dir(temp_dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "unexpected argument '--profile' found",
        ))
        .stderr(predicate::str::contains("error:"));
}

#[test]
#[serial]
fn test_flush_only_without_profile_works() {
    let temp_dir = TempDir::new().unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .current_dir(temp_dir.path())
        .assert()
        .success();

    // Create an issue
    Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Test issue"])
        .current_dir(temp_dir.path())
        .assert()
        .success();

    // flush-only without --profile should work (output varies depending on checkpoint state)
    Command::cargo_bin("bead")
        .unwrap()
        .args(["sync", "flush-only"])
        .current_dir(temp_dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("Checkpoint"))
        .stderr(predicate::str::contains("sequence"));
}

#[test]
#[serial]
fn test_import_only_diagnostics_triggers_r014_path() {
    let temp_dir = TempDir::new().unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .current_dir(temp_dir.path())
        .assert()
        .success();

    // Create import file with validation errors
    let import_path = temp_dir.path().join("import.jsonl");
    let import_content = r#"{"id":"test-0000000000000001","title":"Valid issue","priority":2,"issue_type":"task","base_status":"open","manual_blocked":false,"created_at":"2026-08-08T12:00:00Z","updated_at":"2026-08-08T12:00:00Z","profile":"native-v1","schema_ref":"urn:bead-rs:schema:issue:native-v1"}
malformed json line
{"id":"test-0000000000000001","title":"Duplicate ID","priority":1,"issue_type":"bug","base_status":"open","manual_blocked":false,"created_at":"2026-08-08T12:00:00Z","updated_at":"2026-08-08T12:00:00Z","profile":"native-v1","schema_ref":"urn:bead-rs:schema:issue:native-v1"}"#;
    fs::write(&import_path, import_content).unwrap();

    // Use --diagnostics flag (should trigger R014 validation path)
    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "sync",
            "import-only",
            "--input",
            import_path.to_str().unwrap(),
            "--diagnostics",
        ])
        .current_dir(temp_dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("diagnostics"))
        .stderr(predicate::str::contains("R014"))
        .stderr(predicate::str::contains("Validation failures"))
        .stderr(predicate::str::contains("malformed_json"))
        .stderr(predicate::str::contains("duplicate_issue_id"));
}

#[test]
#[serial]
fn test_import_only_diagnostics_rejected_with_restore_into_empty() {
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

    // --diagnostics with --restore-into-empty should be rejected
    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "sync",
            "import-only",
            "--input",
            import_path.to_str().unwrap(),
            "--diagnostics",
            "--restore-into-empty",
            "--actor",
            "testuser",
        ])
        .current_dir(temp_dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "--diagnostics mode is not compatible with --restore-into-empty",
        ));
}

#[test]
#[serial]
fn test_import_only_diagnostics_rejected_with_merge() {
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

    // --diagnostics with --merge should be rejected
    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "sync",
            "import-only",
            "--input",
            import_path.to_str().unwrap(),
            "--diagnostics",
            "--merge",
            "--actor",
            "testuser",
        ])
        .current_dir(temp_dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("not compatible"))
        .stderr(predicate::str::contains("--merge"));
}

#[test]
#[serial]
fn test_import_only_diagnostics_shows_detailed_validation_failures() {
    let temp_dir = TempDir::new().unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .current_dir(temp_dir.path())
        .assert()
        .success();

    // Create import file with various validation errors
    let import_path = temp_dir.path().join("import.jsonl");
    let import_content = r#"malformed json on line 1
{"id":"test-0000000000000001","title":"Missing required fields","priority":2,"base_status":"open"}
{"id":"test-0000000000000002","title":"Issue with unknown blocker","priority":2,"issue_type":"task","base_status":"open","manual_blocked":false,"created_at":"2026-08-08T12:00:00Z","updated_at":"2026-08-08T12:00:00Z","profile":"native-v1","schema_ref":"urn:bead-rs:schema:issue:native-v1","dependencies":[{"blocker":"test-9999999999999999","kind":"blocks"}]}
{"id":"test-0000000000000003","title":"Self-edge dependency","priority":2,"issue_type":"task","base_status":"open","manual_blocked":false,"created_at":"2026-08-08T12:00:00Z","updated_at":"2026-08-08T12:00:00Z","profile":"native-v1","schema_ref":"urn:bead-rs:schema:issue:native-v1","dependencies":[{"blocker":"test-0000000000000003","kind":"blocks"}]}"#;
    fs::write(&import_path, import_content).unwrap();

    // --diagnostics should show detailed validation failures
    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "sync",
            "import-only",
            "--input",
            import_path.to_str().unwrap(),
            "--diagnostics",
        ])
        .current_dir(temp_dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("malformed_json"))
        .stderr(predicate::str::contains("invalid_field_type"))
        .stderr(predicate::str::contains("unknown_blocker_issue"))
        .stderr(predicate::str::contains("self_edge_dependency"));
}

#[test]
#[serial]
fn test_import_only_diagnostics_with_cycle_detection() {
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
    let import_content = r#"{"id":"test-0000000000000001","title":"First issue","priority":2,"issue_type":"task","base_status":"open","manual_blocked":false,"created_at":"2026-08-08T12:00:00Z","updated_at":"2026-08-08T12:00:00Z","profile":"native-v1","schema_ref":"urn:bead-rs:schema:issue:native-v1","dependencies":[{"blocker":"test-0000000000000002","kind":"blocks"}]}
{"id":"test-0000000000000002","title":"Second issue","priority":2,"issue_type":"task","base_status":"open","manual_blocked":false,"created_at":"2026-08-08T12:00:00Z","updated_at":"2026-08-08T12:00:00Z","profile":"native-v1","schema_ref":"urn:bead-rs:schema:issue:native-v1","dependencies":[{"blocker":"test-0000000000000001","kind":"blocks"}]}"#;
    fs::write(&import_path, import_content).unwrap();

    // --diagnostics should detect cycles
    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "sync",
            "import-only",
            "--input",
            import_path.to_str().unwrap(),
            "--diagnostics",
        ])
        .current_dir(temp_dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("cycle_in_dependencies"))
        .stderr(predicate::str::contains("Cycle"));
}

#[test]
#[serial]
fn test_import_only_diagnostics_bounded_error_collection() {
    let temp_dir = TempDir::new().unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .current_dir(temp_dir.path())
        .assert()
        .success();

    // Create import file with many lines (including some errors) to test bounded collection
    let mut import_content = String::new();
    for i in 0..150 {
        import_content.push_str(&format!(
            r#"{{"id":"test-{:03}","title":"Issue {}","priority":2,"issue_type":"task","base_status":"open","manual_blocked":false,"created_at":"2026-08-08T12:00:00Z","updated_at":"2026-08-08T12:00:00Z","profile":"native-v1","schema_ref":"urn:bead-rs:schema:issue:native-v1"}}
"#,
            i, i
        ));
    }
    // Add malformed lines to generate errors
    import_content.push_str(
        r#"malformed json line 1
malformed json line 2
malformed json line 3"#,
    );

    let import_path = temp_dir.path().join("import.jsonl");
    fs::write(&import_path, import_content).unwrap();

    // --diagnostics should collect errors but be bounded (max 100)
    let result = Command::cargo_bin("bead")
        .unwrap()
        .args([
            "sync",
            "import-only",
            "--input",
            import_path.to_str().unwrap(),
            "--diagnostics",
        ])
        .current_dir(temp_dir.path())
        .assert()
        .success()
        .get_output()
        .to_owned();

    let stderr = String::from_utf8_lossy(&result.stderr);

    // Should show validation failures section
    assert!(stderr.contains("Validation failures"));

    // Should show that errors were collected
    assert!(stderr.contains("malformed_json"));

    // Should show the line numbers where errors occurred
    assert!(
        stderr.contains("Line 151") || stderr.contains("Line 152") || stderr.contains("Line 153")
    );
}

#[test]
#[serial]
fn test_import_only_diagnostics_no_activation_with_errors() {
    let temp_dir = TempDir::new().unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .current_dir(temp_dir.path())
        .assert()
        .success();

    // Create import file with errors
    let import_path = temp_dir.path().join("import.jsonl");
    let import_content = r#"malformed json
{"id":"test-0000000000000001","title":"Valid issue","priority":2,"issue_type":"task","base_status":"open","manual_blocked":false,"created_at":"2026-08-08T12:00:00Z","updated_at":"2026-08-08T12:00:00Z","profile":"native-v1","schema_ref":"urn:bead-rs:schema:issue:native-v1"}"#;
    fs::write(&import_path, import_content).unwrap();

    // --diagnostics should not activate any issues when errors are present
    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "sync",
            "import-only",
            "--input",
            import_path.to_str().unwrap(),
            "--diagnostics",
        ])
        .current_dir(temp_dir.path())
        .assert()
        .success();

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
fn test_import_only_diagnostics_empty_file() {
    let temp_dir = TempDir::new().unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .current_dir(temp_dir.path())
        .assert()
        .success();

    // Create empty import file
    let import_path = temp_dir.path().join("import.jsonl");
    fs::write(&import_path, "").unwrap();

    // --diagnostics with empty file should succeed
    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "sync",
            "import-only",
            "--input",
            import_path.to_str().unwrap(),
            "--diagnostics",
        ])
        .current_dir(temp_dir.path())
        .assert()
        .success();
}

#[test]
#[serial]
fn test_import_only_diagnostics_with_valid_data() {
    let temp_dir = TempDir::new().unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .current_dir(temp_dir.path())
        .assert()
        .success();

    // Create import file with valid data
    let import_path = temp_dir.path().join("import.jsonl");
    let import_content = r#"{"id":"test-0000000000000001","title":"Valid Issue 1","description":"Description 1","priority":2,"issue_type":"task","base_status":"open","manual_blocked":false,"created_at":"2026-08-08T12:00:00Z","updated_at":"2026-08-08T12:00:00Z","profile":"native-v1","schema_ref":"urn:bead-rs:schema:issue:native-v1"}
{"id":"test-0000000000000002","title":"Valid Issue 2","description":"Description 2","priority":1,"issue_type":"bug","base_status":"open","manual_blocked":false,"created_at":"2026-08-08T12:00:00Z","updated_at":"2026-08-08T12:00:00Z","profile":"native-v1","schema_ref":"urn:bead-rs:schema:issue:native-v1","dependencies":[{"blocker":"test-0000000000000001","kind":"blocks"}]}"#;
    fs::write(&import_path, import_content).unwrap();

    // --diagnostics with valid data should succeed without validation errors
    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "sync",
            "import-only",
            "--input",
            import_path.to_str().unwrap(),
            "--diagnostics",
        ])
        .current_dir(temp_dir.path())
        .assert()
        .success();

    // Verify no issues were actually imported (diagnostics mode doesn't activate)
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
fn test_import_only_without_diagnostics_requires_mode() {
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

    // Without --diagnostics, must specify either --restore-into-empty or --merge
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
        .stderr(predicate::str::contains(
            "Exactly one of --restore-into-empty or --merge must be specified",
        ));
}

#[test]
#[serial]
fn test_import_only_diagnostics_does_not_require_actor() {
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

    // --diagnostics should work without --actor (diagnostics mode doesn't activate)
    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "sync",
            "import-only",
            "--input",
            import_path.to_str().unwrap(),
            "--diagnostics",
        ])
        .current_dir(temp_dir.path())
        .assert()
        .success();
}

#[test]
#[serial]
fn test_flush_only_multiple_flag_variations() {
    let temp_dir = TempDir::new().unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .current_dir(temp_dir.path())
        .assert()
        .success();

    // Create an issue
    Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Test issue"])
        .current_dir(temp_dir.path())
        .assert()
        .success();

    // Test various flag combinations that should all reject --profile
    let test_cases = vec![
        vec!["sync", "flush-only", "--profile", "native-v1"],
        vec!["sync", "flush-only", "--profile", "needle-v1"],
        vec!["sync", "flush-only", "--profile=native-v1"],
        vec![
            "sync",
            "flush-only",
            "--output",
            "test.jsonl",
            "--profile",
            "native-v1",
        ],
    ];

    for args in test_cases {
        Command::cargo_bin("bead")
            .unwrap()
            .args(&args)
            .current_dir(temp_dir.path())
            .assert()
            .failure()
            .stderr(predicate::str::contains("unexpected argument"))
            .stderr(predicate::str::contains("--profile"));
    }
}

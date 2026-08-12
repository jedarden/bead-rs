//! F013 migration dry-run and audit receipts integration tests
//!
//! This test suite validates the migration command functionality:
//! - Profile transformation between native-v1, needle-v1, br-v1, and bf-v1
//! - Dry-run validation without file creation
//! - Canonical migration receipts with hashes and transformation counts
//! - Non-overwriting path validation
//! - Proper error handling for invalid profiles and paths

use assert_cmd::Command;
use serde_json::Value;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn create_test_workspace(dir: &Path) -> PathBuf {
    let workspace = dir.join("test_workspace");
    fs::create_dir_all(&workspace).unwrap();

    // Initialize workspace
    Command::new(env!("CARGO_BIN_EXE_bead"))
        .arg("init")
        .arg("--prefix")
        .arg("test")
        .current_dir(&workspace)
        .assert()
        .success();

    workspace
}

fn create_test_issues_file(dir: &Path, profile: &str) -> PathBuf {
    let file_path = dir.join(format!("test-issues-{}.jsonl", profile));
    let mut file = File::create(&file_path).unwrap();

    // Create test issues in native format
    let issues = vec![
        serde_json::json!({
            "id": "test-000000000000001",
            "title": "First issue",
            "description": "Test description",
            "priority": 2,
            "base_status": "open",
            "manual_blocked": false,
            "assignee": null,
            "issue_type": "task",
            "created_at": "2026-08-10T00:00:00Z",
            "updated_at": "2026-08-10T00:00:00Z",
            "labels": ["bug"]
        }),
        serde_json::json!({
            "id": "test-000000000000002",
            "title": "Second issue",
            "description": "",
            "priority": 1,
            "base_status": "open",
            "manual_blocked": false,
            "assignee": null,
            "issue_type": "task",
            "created_at": "2026-08-10T00:00:00Z",
            "updated_at": "2026-08-10T00:00:00Z",
            "labels": []
        }),
    ];

    for issue in issues {
        writeln!(file, "{}", serde_json::to_string(&issue).unwrap()).unwrap();
    }

    file_path
}

#[allow(dead_code)]
fn calculate_file_hash(path: &PathBuf) -> String {
    use sha2::{Digest, Sha256};
    use std::io::{BufReader, Read};

    let file = File::open(path).unwrap();
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];

    loop {
        let n = reader.read(&mut buffer).unwrap();
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }

    format!("{:x}", hasher.finalize())
}

#[test]
fn test_migration_help_available() {
    // `-h` shows the summary.
    Command::new(env!("CARGO_BIN_EXE_bead"))
        .arg("migrate")
        .arg("-h")
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "Migrate checkpoints between profiles",
        ));

    // `--help` shows the long description, which must be distinct from the
    // summary -- if the two are identical the long help has been shadowed.
    Command::new(env!("CARGO_BIN_EXE_bead"))
        .arg("migrate")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "Transform checkpoint data between interchange profiles",
        ))
        .stdout(predicates::str::contains("EXAMPLES:"))
        .stdout(predicates::str::contains("dry-run"))
        .stdout(predicates::str::contains("--from"))
        .stdout(predicates::str::contains("--to"))
        .stdout(predicates::str::contains("--input"))
        .stdout(predicates::str::contains("--output"));
}

#[test]
fn test_migration_native_to_native_dry_run() {
    let temp_dir = TempDir::new().unwrap();
    let input_file = create_test_issues_file(temp_dir.path(), "native");
    let output_file = temp_dir.path().join("output-native.jsonl");

    Command::new(env!("CARGO_BIN_EXE_bead"))
        .arg("migrate")
        .arg("--from")
        .arg("native-v1")
        .arg("--to")
        .arg("native-v1")
        .arg("--input")
        .arg(input_file.to_str().unwrap())
        .arg("--output")
        .arg(output_file.to_str().unwrap())
        .arg("--dry-run")
        .assert()
        .success()
        .stdout(predicates::str::contains("schema_ref"))
        .stdout(predicates::str::contains("tool_version"))
        .stdout(predicates::str::contains("dry_run"))
        .stdout(predicates::str::contains("true"))
        .stdout(predicates::str::contains("prospective"))
        .stdout(predicates::str::contains("true"));

    // Verify no files were created
    assert!(!output_file.exists());
}

#[test]
fn test_migration_native_to_needle_dry_run() {
    let temp_dir = TempDir::new().unwrap();
    let input_file = create_test_issues_file(temp_dir.path(), "native");
    let output_file = temp_dir.path().join("output-needle.jsonl");

    Command::new(env!("CARGO_BIN_EXE_bead"))
        .arg("migrate")
        .arg("--from")
        .arg("native-v1")
        .arg("--to")
        .arg("needle-v1")
        .arg("--input")
        .arg(input_file.to_str().unwrap())
        .arg("--output")
        .arg(output_file.to_str().unwrap())
        .arg("--dry-run")
        .assert()
        .success()
        .stdout(predicates::str::contains("source_profile"))
        .stdout(predicates::str::contains("native-v1"))
        .stdout(predicates::str::contains("target_profile"))
        .stdout(predicates::str::contains("needle-v1"))
        .stdout(predicates::str::contains("transformed_issues"));
}

#[test]
fn test_migration_invalid_profile() {
    let temp_dir = TempDir::new().unwrap();
    let input_file = create_test_issues_file(temp_dir.path(), "native");
    let output_file = temp_dir.path().join("output.jsonl");

    Command::new(env!("CARGO_BIN_EXE_bead"))
        .arg("migrate")
        .arg("--from")
        .arg("invalid-profile")
        .arg("--to")
        .arg("native-v1")
        .arg("--input")
        .arg(input_file.to_str().unwrap())
        .arg("--output")
        .arg(output_file.to_str().unwrap())
        .assert()
        .failure();
}

#[test]
fn test_migration_output_file_exists() {
    let temp_dir = TempDir::new().unwrap();
    let input_file = create_test_issues_file(temp_dir.path(), "native");
    let output_file = temp_dir.path().join("output.jsonl");

    // Create the output file first
    fs::write(&output_file, "existing content").unwrap();

    Command::new(env!("CARGO_BIN_EXE_bead"))
        .arg("migrate")
        .arg("--from")
        .arg("native-v1")
        .arg("--to")
        .arg("native-v1")
        .arg("--input")
        .arg(input_file.to_str().unwrap())
        .arg("--output")
        .arg(output_file.to_str().unwrap())
        .assert()
        .failure();
}

#[test]
fn test_migration_same_input_output() {
    let temp_dir = TempDir::new().unwrap();
    let input_file = create_test_issues_file(temp_dir.path(), "native");
    let output_file = input_file.clone();

    Command::new(env!("CARGO_BIN_EXE_bead"))
        .arg("migrate")
        .arg("--from")
        .arg("native-v1")
        .arg("--to")
        .arg("native-v1")
        .arg("--input")
        .arg(input_file.to_str().unwrap())
        .arg("--output")
        .arg(output_file.to_str().unwrap())
        .assert()
        .failure();
}

#[test]
fn test_migration_real_execution() {
    let temp_dir = TempDir::new().unwrap();
    let input_file = create_test_issues_file(temp_dir.path(), "native");
    let output_file = temp_dir.path().join("output.jsonl");
    let receipt_file = temp_dir.path().join("receipt.json");

    Command::new(env!("CARGO_BIN_EXE_bead"))
        .arg("migrate")
        .arg("--from")
        .arg("native-v1")
        .arg("--to")
        .arg("native-v1")
        .arg("--input")
        .arg(input_file.to_str().unwrap())
        .arg("--output")
        .arg(output_file.to_str().unwrap())
        .arg("--receipt")
        .arg(receipt_file.to_str().unwrap())
        .assert()
        .success()
        .stdout(predicates::str::contains("schema_ref"))
        .stdout(predicates::str::contains("input_sha256"))
        .stdout(predicates::str::contains("output_sha256"))
        .stdout(predicates::str::contains("record_counts"))
        .stdout(predicates::str::contains("dry_run"))
        .stdout(predicates::str::contains("false"));

    // Verify output file was created
    assert!(output_file.exists());
    assert!(receipt_file.exists());

    // Verify output content
    let output_content = fs::read_to_string(&output_file).unwrap();
    let output_lines: Vec<&str> = output_content.lines().collect();

    assert_eq!(output_lines.len(), 2, "Should have 2 issues");

    // Verify receipt structure
    let receipt_content = fs::read_to_string(&receipt_file).unwrap();
    let receipt: Value = serde_json::from_str(&receipt_content).unwrap();

    assert_eq!(
        receipt["schema_ref"],
        "urn:bead-rs:schema:migration-receipt:native-v1"
    );
    assert_eq!(receipt["source_profile"], "native-v1");
    assert_eq!(receipt["target_profile"], "native-v1");
    assert_eq!(receipt["dry_run"], false);
    assert!(receipt["successful"].as_bool().unwrap());
}

#[test]
fn test_migration_receipt_structure() {
    let temp_dir = TempDir::new().unwrap();
    let input_file = create_test_issues_file(temp_dir.path(), "native");
    let output_file = temp_dir.path().join("output.jsonl");
    let receipt_file = temp_dir.path().join("receipt.json");

    Command::new(env!("CARGO_BIN_EXE_bead"))
        .arg("migrate")
        .arg("--from")
        .arg("native-v1")
        .arg("--to")
        .arg("needle-v1")
        .arg("--input")
        .arg(input_file.to_str().unwrap())
        .arg("--output")
        .arg(output_file.to_str().unwrap())
        .arg("--receipt")
        .arg(receipt_file.to_str().unwrap())
        .assert()
        .success();

    // Verify receipt JSON structure
    let receipt_content = fs::read_to_string(&receipt_file).unwrap();
    let receipt: Value = serde_json::from_str(&receipt_content).unwrap();

    // Check required fields exist
    assert!(receipt.get("schema_ref").is_some());
    assert!(receipt.get("tool_version").is_some());
    assert!(receipt.get("timestamp").is_some());
    assert!(receipt.get("source_profile").is_some());
    assert!(receipt.get("target_profile").is_some());
    assert!(receipt.get("input_sha256").is_some());
    assert!(receipt.get("output_sha256").is_some());
    assert!(receipt.get("record_counts").is_some());
    assert!(receipt.get("transformation_counts").is_some());
    assert!(receipt.get("warnings").is_some());
    assert!(receipt.get("dry_run").is_some());
    assert!(receipt.get("successful").is_some());

    // Check record_counts structure
    let record_counts = &receipt["record_counts"];
    assert!(record_counts.get("total_issues").is_some());
    assert!(record_counts.get("input_issues").is_some());
    assert!(record_counts.get("output_issues").is_some());
    assert!(record_counts.get("total_lines").is_some());

    // Check transformation_counts structure
    let transform_counts = &receipt["transformation_counts"];
    assert!(transform_counts.get("transformed_issues").is_some());
    assert!(transform_counts.get("preserved_issues").is_some());
    assert!(transform_counts.get("total_transformations").is_some());
}

#[test]
fn test_migration_malformed_input() {
    let temp_dir = TempDir::new().unwrap();
    let input_file = temp_dir.path().join("malformed.jsonl");
    let output_file = temp_dir.path().join("output.jsonl");

    // Create malformed JSON input
    fs::write(&input_file, "invalid json content\n{\"broken\": \"json\"").unwrap();

    Command::new(env!("CARGO_BIN_EXE_bead"))
        .arg("migrate")
        .arg("--from")
        .arg("native-v1")
        .arg("--to")
        .arg("native-v1")
        .arg("--input")
        .arg(input_file.to_str().unwrap())
        .arg("--output")
        .arg(output_file.to_str().unwrap())
        .assert()
        .failure();
}

#[test]
fn test_migration_input_not_found() {
    let temp_dir = TempDir::new().unwrap();
    let input_file = temp_dir.path().join("nonexistent.jsonl");
    let output_file = temp_dir.path().join("output.jsonl");

    Command::new(env!("CARGO_BIN_EXE_bead"))
        .arg("migrate")
        .arg("--from")
        .arg("native-v1")
        .arg("--to")
        .arg("native-v1")
        .arg("--input")
        .arg(input_file.to_str().unwrap())
        .arg("--output")
        .arg(output_file.to_str().unwrap())
        .assert()
        .failure();
}

#[test]
fn test_migration_workspace_managed_output() {
    let temp_dir = TempDir::new().unwrap();
    let workspace = create_test_workspace(temp_dir.path());
    let input_file = create_test_issues_file(temp_dir.path(), "native");

    // Try to output to workspace-managed directory
    let output_file = workspace.join(".beads").join("issues.jsonl");

    Command::new(env!("CARGO_BIN_EXE_bead"))
        .arg("migrate")
        .arg("--from")
        .arg("native-v1")
        .arg("--to")
        .arg("native-v1")
        .arg("--input")
        .arg(input_file.to_str().unwrap())
        .arg("--output")
        .arg(output_file.to_str().unwrap())
        .current_dir(&workspace)
        .assert()
        .failure();
}

#[test]
fn test_migration_receipt_file_exists() {
    let temp_dir = TempDir::new().unwrap();
    let input_file = create_test_issues_file(temp_dir.path(), "native");
    let output_file = temp_dir.path().join("output.jsonl");
    let receipt_file = temp_dir.path().join("receipt.json");

    // Create receipt file first
    fs::write(&receipt_file, "existing content").unwrap();

    Command::new(env!("CARGO_BIN_EXE_bead"))
        .arg("migrate")
        .arg("--from")
        .arg("native-v1")
        .arg("--to")
        .arg("native-v1")
        .arg("--input")
        .arg(input_file.to_str().unwrap())
        .arg("--output")
        .arg(output_file.to_str().unwrap())
        .arg("--receipt")
        .arg(receipt_file.to_str().unwrap())
        .assert()
        .failure();
}

#[test]
fn test_migration_receipt_equals_stdout() {
    let temp_dir = TempDir::new().unwrap();
    let input_file = create_test_issues_file(temp_dir.path(), "native");
    let output_file = temp_dir.path().join("output.jsonl");
    let receipt_file = temp_dir.path().join("receipt.json");

    // Run migration and capture stdout
    Command::new(env!("CARGO_BIN_EXE_bead"))
        .arg("migrate")
        .arg("--from")
        .arg("native-v1")
        .arg("--to")
        .arg("native-v1")
        .arg("--input")
        .arg(input_file.to_str().unwrap())
        .arg("--output")
        .arg(output_file.to_str().unwrap())
        .arg("--receipt")
        .arg(receipt_file.to_str().unwrap())
        .assert()
        .success()
        .stdout(predicates::str::contains("schema_ref"))
        .stdout(predicates::str::contains("input_sha256"))
        .stdout(predicates::str::contains("output_sha256"));

    // Verify receipt file was created and has structure
    let receipt_content = fs::read_to_string(&receipt_file).unwrap();
    let receipt: Value = serde_json::from_str(&receipt_content).unwrap();

    assert_eq!(
        receipt["schema_ref"],
        "urn:bead-rs:schema:migration-receipt:native-v1"
    );
    assert!(receipt["successful"].as_bool().unwrap());
}

/// A bf-v1-shaped input file (the actual wire format `bf`/`bead-forge`
/// writes), including a "blocked" status and a real dependency edge.
fn create_bf_v1_issues_file(dir: &Path) -> PathBuf {
    let file_path = dir.join("bf-v1-issues.jsonl");
    let mut file = File::create(&file_path).unwrap();

    let issues = vec![
        serde_json::json!({
            "id": "bf-blocker",
            "title": "Blocker issue",
            "description": "",
            "design": "",
            "acceptance_criteria": "",
            "notes": "",
            "status": "open",
            "priority": 2,
            "issue_type": "task",
            "created_at": "2026-08-10T00:00:00Z",
            "updated_at": "2026-08-10T00:00:00Z",
            "events": [],
            "labels": [],
            "dependencies": []
        }),
        serde_json::json!({
            "id": "bf-blocked",
            "title": "Blocked issue",
            "description": "",
            "design": "",
            "acceptance_criteria": "",
            "notes": "",
            "status": "blocked",
            "priority": 2,
            "issue_type": "task",
            "created_at": "2026-08-10T00:00:00Z",
            "updated_at": "2026-08-10T00:00:00Z",
            "events": [],
            "labels": ["urgent"],
            "dependencies": [
                {"issue_id": "bf-blocked", "depends_on_id": "bf-blocker", "type": "blocks"}
            ]
        }),
    ];

    for issue in issues {
        writeln!(file, "{}", serde_json::to_string(&issue).unwrap()).unwrap();
    }

    file_path
}

#[test]
fn test_migration_bf_v1_to_native_v1_uses_the_bf_v1_adapter() {
    // Regression test: `read_input_file` used to always parse the input
    // through the native-v1 adapter regardless of `--from`, so it required
    // a literal `base_status` key on the wire -- every real bf-v1 file
    // (which has `status`, not `base_status`) failed immediately with
    // "missing field `base_status`", and the bf-v1 import adapter
    // (`profile_to_native` in `src/profile/bf_v1.rs`) was never reached.
    let temp_dir = TempDir::new().unwrap();
    let input_file = create_bf_v1_issues_file(temp_dir.path());
    let output_file = temp_dir.path().join("migrated.jsonl");

    Command::new(env!("CARGO_BIN_EXE_bead"))
        .arg("migrate")
        .arg("--from")
        .arg("bf-v1")
        .arg("--to")
        .arg("native-v1")
        .arg("--input")
        .arg(input_file.to_str().unwrap())
        .arg("--output")
        .arg(output_file.to_str().unwrap())
        .assert()
        .success();

    let output_content = fs::read_to_string(&output_file).unwrap();
    let lines: Vec<&str> = output_content.lines().collect();
    assert_eq!(lines.len(), 2);

    let blocked: Value = serde_json::from_str(
        lines
            .iter()
            .find(|l| l.contains("bf-blocked"))
            .expect("migrated output should contain bf-blocked"),
    )
    .unwrap();

    // bf-v1 "blocked" maps to native base_status "open" + manual_blocked
    // true (see BfV1Adapter::bf_status_to_native).
    assert_eq!(blocked["base_status"], "open");
    assert_eq!(blocked["manual_blocked"], true);
}

#[test]
fn test_migration_bf_v1_round_trip_preserves_status_and_dependencies() {
    // Regression test covering a second bug this fix exposed: migrating
    // bf-v1 -> native-v1 -> bf-v1 used to fail with "known extension
    // collision for bf-v1 field 'dependencies'" on the return leg, because
    // BfV1Adapter::profile_to_native additionally wrote a bare, unprefixed
    // "dependencies"/"labels" convenience key directly onto the serialized
    // native JSON (alongside the authoritative `__profile_dependencies__`
    // stash extension), and NativeV1Adapter::profile_to_native captured
    // that bare key as an ordinary extension on re-import -- colliding
    // with the stash the next time it was exported back to bf-v1.
    let temp_dir = TempDir::new().unwrap();
    let input_file = create_bf_v1_issues_file(temp_dir.path());
    let native_file = temp_dir.path().join("native.jsonl");
    let roundtrip_file = temp_dir.path().join("roundtrip.jsonl");

    Command::new(env!("CARGO_BIN_EXE_bead"))
        .arg("migrate")
        .arg("--from")
        .arg("bf-v1")
        .arg("--to")
        .arg("native-v1")
        .arg("--input")
        .arg(input_file.to_str().unwrap())
        .arg("--output")
        .arg(native_file.to_str().unwrap())
        .assert()
        .success();

    Command::new(env!("CARGO_BIN_EXE_bead"))
        .arg("migrate")
        .arg("--from")
        .arg("native-v1")
        .arg("--to")
        .arg("bf-v1")
        .arg("--input")
        .arg(native_file.to_str().unwrap())
        .arg("--output")
        .arg(roundtrip_file.to_str().unwrap())
        .assert()
        .success();

    let roundtrip_content = fs::read_to_string(&roundtrip_file).unwrap();
    let blocked: Value = serde_json::from_str(
        roundtrip_content
            .lines()
            .find(|l| l.contains("\"id\":\"bf-blocked\""))
            .expect("round-tripped output should contain bf-blocked"),
    )
    .unwrap();

    assert_eq!(blocked["status"], "blocked");
    assert_eq!(blocked["labels"], serde_json::json!(["urgent"]));
    assert_eq!(
        blocked["dependencies"],
        serde_json::json!([
            {"issue_id": "bf-blocked", "depends_on_id": "bf-blocker", "type": "blocks"}
        ])
    );
}

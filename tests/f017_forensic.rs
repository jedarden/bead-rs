//! F017 forensic checkpoint integration tests
//!
//! These tests verify the forensic checkpoint system including:
//! - Monolithic checkpoint publication
//! - Content-addressed object storage
//! - Pointer metadata and path tracking
//! - Atomic operations and crash safety
//! - Doctor validation for forensic checkpoints

use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

/// Get the path to the bead binary for testing
fn bead_binary() -> String {
    // Try CARGO_BIN_EXE_bead first (set by cargo test)
    if let Ok(bin) = env::var("CARGO_BIN_EXE_bead") {
        return bin;
    }

    // Fallback to target/debug/bead
    let mut cargo_target_dir = env::current_dir().unwrap();
    cargo_target_dir.push("target");
    cargo_target_dir.push("debug");
    cargo_target_dir.push("bead");

    if cargo_target_dir.exists() {
        return cargo_target_dir.to_str().unwrap().to_string();
    }

    // Another fallback: use target/debug/bead
    "target/debug/bead".to_string()
}

#[test]
fn test_f017_monolithic_checkpoint_basic() {
    let test_dir = format!("/tmp/test-f017-basic-{}", std::process::id());
    let _ = fs::remove_dir_all(&test_dir);
    fs::create_dir_all(&test_dir).unwrap();

    let workspace = Path::new(&test_dir);
    let bead = bead_binary();

    // Initialize workspace
    Command::new(&bead)
        .args(["init"])
        .current_dir(workspace)
        .output()
        .expect("Failed to init workspace");

    // Create some issues
    for i in 1..=3 {
        Command::new(&bead)
            .args(["create", "--title", &format!("Issue {}", i)])
            .current_dir(workspace)
            .output()
            .expect("Failed to create issue");
    }

    // Flush forensic checkpoint
    let output = Command::new(&bead)
        .args(["sync", "flush-only"])
        .current_dir(workspace)
        .output()
        .expect("Failed to flush checkpoint");

    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Flushed forensic checkpoint"));
    assert!(stderr.contains("Issues: 3"));

    // Verify checkpoint structure
    let checkpoint_dir = workspace.join(".beads/checkpoint");
    assert!(checkpoint_dir.exists());

    let current_json = checkpoint_dir.join("current.json");
    assert!(current_json.exists());

    let forensic_jsonl = checkpoint_dir.join("forensic.jsonl");
    assert!(forensic_jsonl.exists());

    let objects_dir = checkpoint_dir.join("objects");
    assert!(objects_dir.exists());

    // Verify current.json contains proper metadata
    let current_content = fs::read_to_string(&current_json).unwrap();
    let current: serde_json::Value = serde_json::from_str(&current_content).unwrap();

    assert!(current.get("generation_id").unwrap().is_string());
    assert_eq!(current.get("mode").unwrap().as_str().unwrap(), "monolithic");
    assert_eq!(current.get("issue_count").unwrap().as_i64().unwrap(), 3);
    assert!(current.get("active_root").unwrap().is_object());

    // Verify content-addressed object file exists
    let active_root = current
        .get("active_root")
        .unwrap()
        .get("path")
        .unwrap()
        .as_str()
        .unwrap();

    let object_path = checkpoint_dir.join(active_root);
    assert!(object_path.exists());

    // Cleanup
    let _ = fs::remove_dir_all(&test_dir);
}

#[test]
fn test_f017_content_addressed_paths() {
    let test_dir = format!("/tmp/test-f017-content-{}", std::process::id());
    let _ = fs::remove_dir_all(&test_dir);
    fs::create_dir_all(&test_dir).unwrap();

    let workspace = Path::new(&test_dir);
    let bead = bead_binary();

    // Initialize workspace
    Command::new(&bead)
        .args(["init"])
        .current_dir(workspace)
        .output()
        .expect("Failed to init workspace");

    // Create issues
    Command::new(&bead)
        .args(["create", "--title", "Test issue"])
        .current_dir(workspace)
        .output()
        .expect("Failed to create issue");

    // Flush checkpoint
    Command::new(&bead)
        .args(["sync", "flush-only"])
        .current_dir(workspace)
        .output()
        .expect("Failed to flush checkpoint");

    // Read current.json
    let current_json = workspace.join(".beads/checkpoint/current.json");
    let current_content = fs::read_to_string(&current_json).unwrap();
    let current: serde_json::Value = serde_json::from_str(&current_content).unwrap();

    // Verify content-addressed path format
    let active_root = current
        .get("active_root")
        .unwrap()
        .get("path")
        .unwrap()
        .as_str()
        .unwrap();

    assert!(active_root.ends_with(".jsonl"));
    assert!(active_root.contains("/"));
    assert!(active_root.starts_with("objects/"));

    // Cleanup
    let _ = fs::remove_dir_all(&test_dir);
}

#[test]
fn test_f017_pointer_metadata_tracking() {
    let test_dir = format!("/tmp/test-f017-metadata-{}", std::process::id());
    let _ = fs::remove_dir_all(&test_dir);
    fs::create_dir_all(&test_dir).unwrap();

    let workspace = Path::new(&test_dir);
    let bead = bead_binary();

    // Initialize workspace
    Command::new(&bead)
        .args(["init"])
        .current_dir(workspace)
        .output()
        .expect("Failed to init workspace");

    // Create first issue
    Command::new(&bead)
        .args(["create", "--title", "First issue"])
        .current_dir(workspace)
        .output()
        .expect("Failed to create issue");

    // First checkpoint
    Command::new(&bead)
        .args(["sync", "flush-only"])
        .current_dir(workspace)
        .output()
        .expect("Failed to flush first checkpoint");

    // Read first generation
    let current_json = workspace.join(".beads/checkpoint/current.json");
    let first_gen = {
        let content = fs::read_to_string(&current_json).unwrap();
        let json: serde_json::Value = serde_json::from_str(&content).unwrap();
        json.get("generation_id")
            .unwrap()
            .as_str()
            .unwrap()
            .to_string()
    };

    // Create second issue
    Command::new(&bead)
        .args(["create", "--title", "Second issue"])
        .current_dir(workspace)
        .output()
        .expect("Failed to create second issue");

    // Second checkpoint
    Command::new(&bead)
        .args(["sync", "flush-only"])
        .current_dir(workspace)
        .output()
        .expect("Failed to flush second checkpoint");

    // Verify generation changed
    let content = fs::read_to_string(&current_json).unwrap();
    let json: serde_json::Value = serde_json::from_str(&content).unwrap();
    let second_gen = json.get("generation_id").unwrap().as_str().unwrap();

    assert_ne!(first_gen, second_gen);

    // Verify added_paths tracking
    let added_paths = json.get("added_paths").unwrap().as_array().unwrap();

    assert!(!added_paths.is_empty());

    // Verify previous.json exists
    let previous_json = workspace.join(".beads/checkpoint/previous.json");
    assert!(previous_json.exists());

    // Cleanup
    let _ = fs::remove_dir_all(&test_dir);
}

#[test]
fn test_f017_doctor_validation() {
    let test_dir = format!("/tmp/test-f017-doctor-{}", std::process::id());
    let _ = fs::remove_dir_all(&test_dir);
    fs::create_dir_all(&test_dir).unwrap();

    let workspace = Path::new(&test_dir);
    let bead = bead_binary();

    // Initialize workspace
    Command::new(&bead)
        .args(["init"])
        .current_dir(workspace)
        .output()
        .expect("Failed to init workspace");

    // Create issues
    Command::new(&bead)
        .args(["create", "--title", "Doctor test issue"])
        .current_dir(workspace)
        .output()
        .expect("Failed to create issue");

    // Flush checkpoint
    Command::new(&bead)
        .args(["sync", "flush-only"])
        .current_dir(workspace)
        .output()
        .expect("Failed to flush checkpoint");

    // Run doctor
    let output = Command::new(&bead)
        .args(["doctor"])
        .current_dir(workspace)
        .output()
        .expect("Failed to run doctor");

    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should validate forensic checkpoint (R016: checkpoint_state replaced with checkpoint_freshness)
    assert!(stderr.contains("OK checkpoint_freshness") || stderr.contains("Forensic checkpoint valid"));

    // Cleanup
    let _ = fs::remove_dir_all(&test_dir);
}

#[test]
fn test_f017_crash_safety_and_atomicity() {
    let test_dir = format!("/tmp/test-f017-atomicity-{}", std::process::id());
    let _ = fs::remove_dir_all(&test_dir);
    fs::create_dir_all(&test_dir).unwrap();

    let workspace = Path::new(&test_dir);
    let bead = bead_binary();

    // Initialize workspace
    Command::new(&bead)
        .args(["init"])
        .current_dir(workspace)
        .output()
        .expect("Failed to init workspace");

    // Create multiple issues
    for i in 1..=5 {
        Command::new(&bead)
            .args(["create", "--title", &format!("Issue {}", i)])
            .current_dir(workspace)
            .output()
            .expect("Failed to create issue");
    }

    // Perform multiple rapid checkpoints
    for _ in 0..3 {
        Command::new(&bead)
            .args(["sync", "flush-only"])
            .current_dir(workspace)
            .output()
            .expect("Failed to flush checkpoint");

        // Verify checkpoint is valid after each flush
        let current_json = workspace.join(".beads/checkpoint/current.json");
        assert!(current_json.exists());

        let content = fs::read_to_string(&current_json).unwrap();
        let json: serde_json::Value = serde_json::from_str(&content).unwrap();

        // Verify all required fields present
        assert!(json.get("generation_id").is_some());
        assert!(json.get("active_root").is_some());
        assert!(json.get("issue_count").is_some());
        assert!(json.get("schema_version").is_some());

        // Verify no temporary files left behind
        let checkpoint_dir = workspace.join(".beads/checkpoint");
        let entries = fs::read_dir(&checkpoint_dir).unwrap();
        for entry in entries {
            let path = entry.unwrap().path();
            if let Some(ext) = path.extension() {
                assert_ne!(ext.to_str().unwrap(), "tmp");
            }
        }
    }

    // Cleanup
    let _ = fs::remove_dir_all(&test_dir);
}

#[test]
fn test_f017_forensic_jsonl_record_types() {
    let test_dir = format!("/tmp/test-f017-record-types-{}", std::process::id());
    let _ = fs::remove_dir_all(&test_dir);
    fs::create_dir_all(&test_dir).unwrap();

    let workspace = Path::new(&test_dir);
    let bead = bead_binary();

    // Initialize workspace
    Command::new(&bead)
        .args(["init"])
        .current_dir(workspace)
        .output()
        .expect("Failed to init workspace");

    // Create an issue
    Command::new(&bead)
        .args(["create", "--title", "Record type test"])
        .current_dir(workspace)
        .output()
        .expect("Failed to create issue");

    // Flush checkpoint
    Command::new(&bead)
        .args(["sync", "flush-only"])
        .current_dir(workspace)
        .output()
        .expect("Failed to flush checkpoint");

    // Read forensic.jsonl and verify record structure
    let forensic_jsonl = workspace.join(".beads/checkpoint/forensic.jsonl");
    let content = fs::read_to_string(&forensic_jsonl).unwrap();

    // Each line should be a valid JSON object with record_type field
    for line in content.lines() {
        if !line.trim().is_empty() {
            let record: serde_json::Value = serde_json::from_str(line).unwrap();
            assert!(record.get("record_type").is_some());

            let record_type = record.get("record_type").unwrap().as_str().unwrap();
            assert!(
                record_type == "issue"
                    || record_type == "event"
                    || record_type == "provenance_receipt"
            );
        }
    }

    // Cleanup
    let _ = fs::remove_dir_all(&test_dir);
}

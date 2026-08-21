//! F017 forensic checkpoint integration tests
//!
//! These tests verify the forensic checkpoint system including:
//! - Monolithic checkpoint publication
//! - Content-addressed object storage
//! - Pointer metadata and path tracking
//! - Atomic operations and crash safety
//! - Doctor validation for forensic checkpoints

use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

// R030: workspace discovery stops at the first featureless `.beads` above the
// working directory, so a test workspace must not live under one. The base is
// `std::env::temp_dir()` (TMPDIR-aware, like every other test file here)
// instead of a hardcoded `/tmp`, which a machine may share with unrelated
// `.beads` debris.
use std::process::Command;

/// Get the path to the bead binary for testing
fn bead_binary() -> String {
    env!("CARGO_BIN_EXE_bead").to_string()
}

/// Run `bead sync flush-only` in the workspace, asserting success
fn run_flush(workspace: &Path, bead: &str) {
    let output = Command::new(bead)
        .args(["sync", "flush-only"])
        .current_dir(workspace)
        .output()
        .expect("Failed to flush checkpoint");
    assert!(
        output.status.success(),
        "flush-only failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Read and parse `current.json` from a checkpoint directory
fn read_current_pointer(checkpoint_dir: &Path) -> serde_json::Value {
    let content = fs::read_to_string(checkpoint_dir.join("current.json")).unwrap();
    serde_json::from_str(&content).unwrap()
}

#[test]
fn test_f017_monolithic_checkpoint_basic() {
    let test_dir = format!(
        "{}/test-f017-basic-{}",
        std::env::temp_dir().display(),
        std::process::id()
    );
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
    let test_dir = format!(
        "{}/test-f017-content-{}",
        std::env::temp_dir().display(),
        std::process::id()
    );
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

    // The object name must be its own content SHA-256 (plan 6.1.1 / 6.2.1 P1),
    // not the generation ID.
    let root_sha = current
        .get("active_root")
        .unwrap()
        .get("sha256")
        .unwrap()
        .as_str()
        .unwrap();
    assert_eq!(active_root, format!("objects/{}.jsonl", root_sha));

    let object_bytes = fs::read(workspace.join(".beads/checkpoint").join(active_root)).unwrap();
    let actual_hash = format!("{:x}", Sha256::digest(&object_bytes));
    assert_eq!(actual_hash, root_sha);

    let generation_id = current.get("generation_id").unwrap().as_str().unwrap();
    assert_ne!(
        active_root,
        format!("objects/{}.jsonl", generation_id),
        "monolithic root must not be named by generation ID"
    );

    // Cleanup
    let _ = fs::remove_dir_all(&test_dir);
}

#[test]
fn test_f017_identical_flushes_reuse_one_object() {
    let test_dir = format!(
        "{}/test-f017-reuse-{}",
        std::env::temp_dir().display(),
        std::process::id()
    );
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
        .args(["create", "--title", "Reuse test issue"])
        .current_dir(workspace)
        .output()
        .expect("Failed to create issue");

    let checkpoint_dir = workspace.join(".beads/checkpoint");

    // First flush
    run_flush(workspace, &bead);
    let first = read_current_pointer(&checkpoint_dir);

    // A clean re-flush publishes nothing now (plan 6.2.1 item 8:
    // flush-only is idempotent against a clean checkpoint), so the
    // republication this test needs is triggered the way a real one
    // arises: the pointer is lost. Publication commits `current.json`
    // last, so an interrupted first publication leaves exactly this
    // state -- objects on disk, no pointer -- and the retry must publish.
    fs::remove_file(checkpoint_dir.join("current.json")).unwrap();

    // Second flush with no intervening mutation: the monolith bytes are
    // identical, so the content-addressed object must be reused, not
    // duplicated.
    run_flush(workspace, &bead);
    let second = read_current_pointer(&checkpoint_dir);

    // A new generation was still published...
    assert_ne!(
        first.get("generation_id").unwrap().as_str().unwrap(),
        second.get("generation_id").unwrap().as_str().unwrap()
    );

    // ...but it points at the same content-addressed object.
    assert_eq!(
        first
            .get("active_root")
            .unwrap()
            .get("path")
            .unwrap()
            .as_str()
            .unwrap(),
        second
            .get("active_root")
            .unwrap()
            .get("path")
            .unwrap()
            .as_str()
            .unwrap()
    );

    // Exactly one monolithic object exists after both publications.
    let objects_dir = checkpoint_dir.join("objects");
    let jsonl_count = fs::read_dir(&objects_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "jsonl").unwrap_or(false))
        .count();
    assert_eq!(
        jsonl_count, 1,
        "two byte-identical publications must reuse one object"
    );

    // Cleanup
    let _ = fs::remove_dir_all(&test_dir);
}

#[test]
fn test_f017_legacy_generation_named_object_importable() {
    let test_dir = format!(
        "{}/test-f017-legacy-gen-{}",
        std::env::temp_dir().display(),
        std::process::id()
    );
    let _ = fs::remove_dir_all(&test_dir);
    fs::create_dir_all(&test_dir).unwrap();

    let source_workspace = Path::new(&test_dir).join("source");
    let target_workspace = Path::new(&test_dir).join("target");
    fs::create_dir_all(&source_workspace).unwrap();
    fs::create_dir_all(&target_workspace).unwrap();

    let bead = bead_binary();

    // Build a real checkpoint in the source workspace.
    Command::new(&bead)
        .args(["init"])
        .current_dir(&source_workspace)
        .output()
        .expect("Failed to init source workspace");

    let create_output = Command::new(&bead)
        .args(["create", "--title", "Legacy object test issue"])
        .current_dir(&source_workspace)
        .output()
        .expect("Failed to create issue");
    assert!(
        create_output.status.success(),
        "create failed: {}",
        String::from_utf8_lossy(&create_output.stderr)
    );

    run_flush(&source_workspace, &bead);

    // Reshape the checkpoint into the legacy generation-named layout: rename
    // the content-addressed object to `objects/gen-*.jsonl` and repoint
    // current.json at it, as pre-P1 checkpoints on disk still look.
    let checkpoint_dir = source_workspace.join(".beads/checkpoint");
    let mut pointer = read_current_pointer(&checkpoint_dir);
    let old_path = pointer
        .get("active_root")
        .unwrap()
        .get("path")
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();
    let legacy_name = "objects/gen-legacy0123456789.jsonl";
    fs::rename(
        checkpoint_dir.join(&old_path),
        checkpoint_dir.join(legacy_name),
    )
    .unwrap();
    *pointer
        .get_mut("active_root")
        .unwrap()
        .get_mut("path")
        .unwrap() = serde_json::Value::String(legacy_name.to_string());
    fs::write(
        checkpoint_dir.join("current.json"),
        serde_json::to_vec_pretty(&pointer).unwrap(),
    )
    .unwrap();

    // Restore the legacy-shaped checkpoint into a fresh workspace -- the
    // pointer's active_root.path is authoritative, so the generation-named
    // object must still be readable.
    Command::new(&bead)
        .args(["init"])
        .current_dir(&target_workspace)
        .output()
        .expect("Failed to init target workspace");

    let import_output = Command::new(&bead)
        .args([
            "sync",
            "import-only",
            "--input",
            checkpoint_dir.to_str().unwrap(),
            "--profile",
            "native-v1",
            "--restore-into-empty",
            "--actor",
            "test",
        ])
        .current_dir(&target_workspace)
        .output()
        .expect("Failed to run import-only");
    assert!(
        import_output.status.success(),
        "import of legacy generation-named checkpoint failed: {}",
        String::from_utf8_lossy(&import_output.stderr)
    );

    // The restored workspace contains the source issue.
    let list_output = Command::new(&bead)
        .args(["list", "--json"])
        .current_dir(&target_workspace)
        .output()
        .expect("Failed to list issues");
    assert!(list_output.status.success());
    let list_stdout = String::from_utf8_lossy(&list_output.stdout);
    assert!(
        list_stdout.contains("Legacy object test issue"),
        "restored workspace is missing the source issue"
    );

    // Cleanup
    let _ = fs::remove_dir_all(&test_dir);
}

#[test]
fn test_f017_pointer_metadata_tracking() {
    let test_dir = format!(
        "{}/test-f017-metadata-{}",
        std::env::temp_dir().display(),
        std::process::id()
    );
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
    let test_dir = format!(
        "{}/test-f017-doctor-{}",
        std::env::temp_dir().display(),
        std::process::id()
    );
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
    assert!(
        stderr.contains("OK checkpoint_freshness") || stderr.contains("Forensic checkpoint valid")
    );

    // Cleanup
    let _ = fs::remove_dir_all(&test_dir);
}

#[test]
fn test_f017_crash_safety_and_atomicity() {
    let test_dir = format!(
        "{}/test-f017-atomicity-{}",
        std::env::temp_dir().display(),
        std::process::id()
    );
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
    let test_dir = format!(
        "{}/test-f017-record-types-{}",
        std::env::temp_dir().display(),
        std::process::id()
    );
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

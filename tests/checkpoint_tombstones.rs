//! Pointer-declared checkpoint tombstone tests (plan 6.2 step 6, 6.2.1 P2)
//!
//! These tests verify that a publication applies the tombstones the new
//! pointer declares:
//! - every path in `deleted_paths` is absent from disk after a successful
//!   publication
//! - `current.json` is never declared in its own `deleted_paths` (it belongs
//!   in `replaced_paths`)
//! - the retained object set is bounded by the generations `current.json`
//!   and `previous.json` reference
//! - `sync status` reports unresolved tombstones and is not ready to commit
//!   while any remain
//! - cleanup is crash-safe and repeatable: files left behind by an
//!   interrupted cleanup are re-declared and removed by the next publication

use serde_json::Value;
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

fn run(workspace: &Path, args: &[&str]) -> std::process::Output {
    Command::new(bead_binary())
        .args(args)
        .current_dir(workspace)
        .output()
        .expect("Failed to run bead")
}

fn run_flush(workspace: &Path) -> std::process::Output {
    run(workspace, &["sync", "flush-only"])
}

/// Read and parse `current.json` (or `previous.json`) from a checkpoint
fn read_pointer(checkpoint_dir: &Path, name: &str) -> Value {
    let content = fs::read_to_string(checkpoint_dir.join(name)).unwrap();
    serde_json::from_str(&content).unwrap()
}

fn pointer_array(pointer: &Value, key: &str) -> Vec<String> {
    pointer
        .get(key)
        .and_then(|v| v.as_array())
        .map(|entries| {
            entries
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

fn active_root(pointer: &Value) -> String {
    pointer
        .get("active_root")
        .unwrap()
        .get("path")
        .unwrap()
        .as_str()
        .unwrap()
        .to_string()
}

/// Names of the generation objects currently on disk
fn objects_on_disk(checkpoint_dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(checkpoint_dir.join("objects"))
        .unwrap()
        .map(|entry| format!("objects/{}", entry.unwrap().file_name().to_str().unwrap()))
        .collect();
    names.sort();
    names
}

fn run_status_json(workspace: &Path) -> Value {
    let output = run(workspace, &["sync", "status", "--format", "json"]);
    assert!(
        output.status.success(),
        "sync status failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

/// Mutate-and-flush cycles apply tombstones, never delete `current.json`,
/// and bound the retained object set to the two referenced generations
#[test]
fn tombstones_applied_and_object_set_bounded() {
    let test_dir = format!(
        "{}/test-tombstones-bounded-{}",
        std::env::temp_dir().display(),
        std::process::id()
    );
    let _ = fs::remove_dir_all(&test_dir);
    fs::create_dir_all(&test_dir).unwrap();

    let workspace = Path::new(&test_dir);
    run(workspace, &["init"]);

    let checkpoint_dir = workspace.join(".beads/checkpoint");
    let mut roots = Vec::new();

    for i in 1..=8 {
        run(workspace, &["create", "--title", &format!("Issue {}", i)]);
        let output = run_flush(workspace);
        assert!(
            output.status.success(),
            "flush {} failed: {}",
            i,
            String::from_utf8_lossy(&output.stderr)
        );

        let current = read_pointer(&checkpoint_dir, "current.json");
        let root = active_root(&current);
        let deleted = pointer_array(&current, "deleted_paths");

        // Every tombstone the new pointer declares is absent from disk
        for path in &deleted {
            assert!(
                !checkpoint_dir.join(path).exists(),
                "flush {} declares deleted path {} but it is still on disk",
                i,
                path
            );
        }

        // current.json is rewritten by every publication, so it is replaced,
        // never deleted
        assert!(
            !deleted.iter().any(|p| p == "current.json"),
            "flush {} declares current.json deleted",
            i
        );
        if i >= 2 {
            assert!(
                pointer_array(&current, "replaced_paths")
                    .iter()
                    .any(|p| p == "current.json"),
                "flush {} does not list current.json in replaced_paths",
                i
            );
        }

        // The retained object set never exceeds the generations current.json
        // and previous.json reference
        let retained: Vec<String> = {
            let mut set = vec![root.clone()];
            if i >= 2 {
                set.push(active_root(&read_pointer(&checkpoint_dir, "previous.json")));
            }
            set.sort();
            set.dedup();
            set
        };
        assert_eq!(
            objects_on_disk(&checkpoint_dir),
            retained,
            "object set after flush {} is not bounded by the referenced generations",
            i
        );

        roots.push(root);
    }

    // Successive flushes with changing content retire their predecessors:
    // after 8 cycles only the two newest roots remain and every older root
    // was tombstoned away
    assert_eq!(objects_on_disk(&checkpoint_dir).len(), 2);
    assert!(roots
        .iter()
        .take(6)
        .all(|root| { !checkpoint_dir.join(root).exists() }));

    let _ = fs::remove_dir_all(&test_dir);
}

/// Objects left behind by an older writer or an interrupted cleanup are
/// re-declared as tombstones and removed by the next publication, and the
/// deletions appear in the reported changed-path set
#[test]
fn stray_objects_are_reclaimed_and_reported() {
    let test_dir = format!(
        "{}/test-tombstones-stray-{}",
        std::env::temp_dir().display(),
        std::process::id()
    );
    let _ = fs::remove_dir_all(&test_dir);
    fs::create_dir_all(&test_dir).unwrap();

    let workspace = Path::new(&test_dir);
    run(workspace, &["init"]);
    run(workspace, &["create", "--title", "First"]);
    assert!(run_flush(workspace).status.success());

    let checkpoint_dir = workspace.join(".beads/checkpoint");

    // Simulate the observed backlog: stale generation objects an older
    // writer declared but never removed, plus a publication temporary,
    // which is doctor's cleanup business, not a tombstone's
    let stray = checkpoint_dir.join("objects/gen-0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e.jsonl");
    fs::write(&stray, b"stale object from an interrupted cleanup\n").unwrap();
    let temp = checkpoint_dir.join("objects/gen-ffffffffffffffffffffffffffffffff.tmp");
    fs::write(&temp, b"publication temporary\n").unwrap();

    run(workspace, &["create", "--title", "Second"]);
    let output = run_flush(workspace);
    assert!(output.status.success());

    let current = read_pointer(&checkpoint_dir, "current.json");
    let deleted = pointer_array(&current, "deleted_paths");

    // The stray was declared and removed; the temporary was left alone
    assert!(deleted
        .iter()
        .any(|p| p == "objects/gen-0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e.jsonl"));
    assert!(!stray.exists());
    assert!(temp.exists());

    // Deletions are part of the changed-path set one external Git commit
    // must carry (asserted here through the recorded checkpoint state)
    let status = run_status_json(workspace);
    let changed: Vec<String> = status
        .get("changed_paths")
        .and_then(|v| v.as_array())
        .map(|entries| {
            entries
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    assert!(changed
        .iter()
        .any(|p| p == "objects/gen-0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e.jsonl"));

    let _ = fs::remove_dir_all(&test_dir);
}

/// `sync status` reports tombstones declared but not yet applied and is not
/// ready to commit until a repeat publication applies them
#[test]
fn status_reports_unresolved_tombstones_until_reapplied() {
    let test_dir = format!(
        "{}/test-tombstones-status-{}",
        std::env::temp_dir().display(),
        std::process::id()
    );
    let _ = fs::remove_dir_all(&test_dir);
    fs::create_dir_all(&test_dir).unwrap();

    let workspace = Path::new(&test_dir);
    run(workspace, &["init"]);

    let checkpoint_dir = workspace.join(".beads/checkpoint");

    // Before any flush there is no checkpoint to commit
    let status = run_status_json(workspace);
    assert_eq!(
        status.get("checkpoint_present").unwrap(),
        &Value::Bool(false)
    );
    assert_eq!(status.get("ready_to_commit").unwrap(), &Value::Bool(false));
    assert!(status
        .get("not_ready_reasons")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r.as_str().unwrap().contains("no checkpoint published")));

    run(workspace, &["create", "--title", "First"]);
    assert!(run_flush(workspace).status.success());
    run(workspace, &["create", "--title", "Second"]);
    assert!(run_flush(workspace).status.success());
    run(workspace, &["create", "--title", "Third"]);
    assert!(run_flush(workspace).status.success());

    // A healthy checkpoint is ready to commit
    let status = run_status_json(workspace);
    assert_eq!(status.get("ready_to_commit").unwrap(), &Value::Bool(true));
    assert!(status
        .get("unresolved_tombstones")
        .unwrap()
        .as_array()
        .unwrap()
        .is_empty());

    // Reproduce the crash window this bead fixes: the pointer committed but
    // the cleanup never ran, so a declared-deleted object is still on disk
    let current = read_pointer(&checkpoint_dir, "current.json");
    let unresolved: Vec<String> = pointer_array(&current, "deleted_paths");
    assert!(!unresolved.is_empty());
    for path in &unresolved {
        fs::write(checkpoint_dir.join(path), b"resurrected by a crash\n").unwrap();
    }

    let status = run_status_json(workspace);
    assert_eq!(
        status
            .get("unresolved_tombstones")
            .unwrap()
            .as_array()
            .map(|a| a.len()),
        Some(unresolved.len())
    );
    assert_eq!(status.get("ready_to_commit").unwrap(), &Value::Bool(false));
    assert!(status
        .get("not_ready_reasons")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r.as_str().unwrap().contains("unresolved tombstones")));

    // The human-readable report says the same thing
    let text = run(workspace, &["sync", "status"]);
    assert!(text.status.success());
    let stdout = String::from_utf8_lossy(&text.stdout);
    assert!(stdout.contains(&format!("Unresolved tombstones: {}", unresolved.len())));
    assert!(stdout.contains("Ready to commit: NO"));

    // Cleanup is repeatable: the next publication re-declares and removes
    // the leftovers without needing a mutation to make it safe
    assert!(run_flush(workspace).status.success());
    for path in &unresolved {
        assert!(
            !checkpoint_dir.join(path).exists(),
            "repeat publication did not reapply tombstone {}",
            path
        );
    }

    let status = run_status_json(workspace);
    assert_eq!(status.get("ready_to_commit").unwrap(), &Value::Bool(true));
    assert!(status
        .get("unresolved_tombstones")
        .unwrap()
        .as_array()
        .unwrap()
        .is_empty());

    // An unparseable pointer is reported, not a crash
    fs::write(checkpoint_dir.join("current.json"), b"{ not json").unwrap();
    let status = run_status_json(workspace);
    assert_eq!(status.get("ready_to_commit").unwrap(), &Value::Bool(false));
    assert!(status
        .get("not_ready_reasons")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r.as_str().unwrap().contains("unparseable")));

    let _ = fs::remove_dir_all(&test_dir);
}

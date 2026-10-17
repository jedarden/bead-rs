//! Checkpoint publication must verify a content-addressed object against its
//! own name before reusing it (beadrs-fb3100ce).
//!
//! A root object hand-edited on disk no longer hashes to its name, but an
//! unchanged live store recomputes the identical content address, so a flush
//! that trusted the name would republish a pointer to the corrupt bytes
//! forever -- the failure observed in ~/utilities, where `sync status`
//! reported "root hash mismatch ... Ready to commit: NO" for 18 days while
//! every `sync flush-only` "succeeded". These tests pin the contract:
//! flush-only either repairs the object from the live serialization or fails
//! loudly; it never reports success while the root is unverified.

use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn bead_binary() -> String {
    env!("CARGO_BIN_EXE_bead").to_string()
}

fn run(workspace: &Path, args: &[&str]) -> Output {
    Command::new(bead_binary())
        .args(args)
        .current_dir(workspace)
        .output()
        .expect("Failed to run bead")
}

/// `sync status --format json` parsed from stdout.
fn status(workspace: &Path) -> Value {
    let output = run(workspace, &["sync", "status", "--format", "json"]);
    assert!(
        output.status.success(),
        "sync status failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

/// Record `checkpoint.auto_flush = VALUE` in the workspace's
/// `.beads/config.json`, preserving every other key.
fn set_auto_flush(workspace: &Path, value: bool) {
    let path = workspace.join(".beads/config.json");
    let mut config: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    config
        .as_object_mut()
        .unwrap()
        .entry("checkpoint")
        .or_insert(Value::Object(Default::default()))
        .as_object_mut()
        .unwrap()
        .insert("auto_flush".into(), Value::Bool(value));
    fs::write(&path, serde_json::to_string_pretty(&config).unwrap()).unwrap();
}

/// A fresh workspace with publication controlled entirely by the test.
fn workspace(prefix: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("workspace tempdir");
    let path = dir.path().to_path_buf();
    let output = run(
        &path,
        &["init", "--skip-foreign-workspace", "--prefix", prefix],
    );
    assert!(
        output.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    set_auto_flush(&path, false);
    (dir, path)
}

#[test]
fn flush_only_repairs_a_root_planted_at_the_next_publications_address() {
    let (_dir, workspace) = workspace("cor");

    // Generation 1: a verified root whose bytes hash to their name.
    let flush = run(&workspace, &["sync", "flush-only"]);
    assert!(
        flush.status.success(),
        "initial flush failed: {}",
        String::from_utf8_lossy(&flush.stderr)
    );
    let pointer: Value = serde_json::from_str(
        &fs::read_to_string(workspace.join(".beads/checkpoint/current.json")).unwrap(),
    )
    .unwrap();
    let root_path = pointer["active_root"]["path"].as_str().unwrap().to_string();
    let root_hash = pointer["active_root"]["sha256"]
        .as_str()
        .unwrap()
        .to_string();
    let root_file = workspace.join(".beads/checkpoint").join(&root_path);
    assert_eq!(status(&workspace)["root_verified"], true);

    // Plant a byte-modified object at the exact content address the next
    // publication recomputes for unchanged live state.
    let tampered = b"{}\n".to_vec();
    fs::write(&root_file, &tampered).unwrap();

    // The incident's 18-day state: status refuses and names the mismatch.
    let before = status(&workspace);
    assert_eq!(before["root_verified"], false);
    assert_eq!(before["ready_to_commit"], false);
    let reasons = before["not_ready_reasons"].as_array().unwrap();
    assert!(
        reasons
            .iter()
            .any(|reason| reason.as_str().unwrap().contains("root hash mismatch")),
        "expected a root hash mismatch reason, got: {reasons:?}"
    );

    // flush-only must not report success while the root is unverified: it
    // rewrites the object from the live serialization, restoring bytes that
    // hash to their name at the same address.
    let repair = run(&workspace, &["sync", "flush-only"]);
    assert!(
        repair.status.success(),
        "flush-only failed: {}",
        String::from_utf8_lossy(&repair.stderr)
    );
    let repaired = fs::read(&root_file).unwrap();
    assert_ne!(repaired, tampered, "the corrupt bytes must not survive");
    let mut hasher = Sha256::new();
    hasher.update(&repaired);
    assert_eq!(
        format!("{:x}", hasher.finalize()),
        root_hash,
        "the flushed root must hash to the address publication reused"
    );

    // After the flush both pointers select verified bytes: the internals
    // verdict is clean and no mismatch reason remains. (`ready_to_commit`
    // additionally folds in git reachability, out of scope in a tempdir.)
    let after = status(&workspace);
    assert_eq!(after["root_verified"], true);
    assert_eq!(after["checkpoint_consistent"], true);
    let reasons = after["not_ready_reasons"].as_array().unwrap();
    assert!(
        !reasons
            .iter()
            .any(|reason| reason.as_str().unwrap().contains("root hash mismatch")),
        "the mismatch reason must be gone, got: {reasons:?}"
    );
    assert_eq!(after["root_path"], Value::String(root_path));
}

#[test]
fn flush_only_republication_of_unchanged_state_keeps_a_verified_root_reused() {
    let (_dir, workspace) = workspace("reu");

    let first = run(&workspace, &["sync", "flush-only"]);
    assert!(first.status.success());
    let pointer: Value = serde_json::from_str(
        &fs::read_to_string(workspace.join(".beads/checkpoint/current.json")).unwrap(),
    )
    .unwrap();
    let root_file = workspace
        .join(".beads/checkpoint")
        .join(pointer["active_root"]["path"].as_str().unwrap());
    let verified_bytes = fs::read(&root_file).unwrap();

    // A second flush over unchanged, already-consistent state short-circuits
    // and must leave the verified object untouched on disk.
    let second = run(&workspace, &["sync", "flush-only"]);
    assert!(
        second.status.success(),
        "second flush failed: {}",
        String::from_utf8_lossy(&second.stderr)
    );
    assert_eq!(fs::read(&root_file).unwrap(), verified_bytes);
    assert_eq!(status(&workspace)["root_verified"], true);
}

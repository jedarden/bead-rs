//! Checkpoint publication lock tests (plan 6.2.1 item 4, ADR-003, R026).
//!
//! Publication is serialized by a file lock on
//! `.beads/checkpoint/publish.lock`, distinct from the SQLite write path:
//!
//! - the lock is exclusive across separately opened files, so two publisher
//!   processes never interleave object writes, pointer replacement, and
//!   tombstone application -- a lost race leaves a superseded generation,
//!   never a torn pointer or a partially applied tombstone set;
//! - a worker that finds a newer generation already published for a
//!   sequence at or beyond its own treats that as success and exits 0 --
//!   the coverage check that decides this runs *under* the lock, because
//!   the pointer it reads may be the one a concurrent publisher just wrote;
//! - ordinary mutations validate the checkpoint under this lock before
//!   committing, so they wait while another publisher replaces a generation;
//! - at quiesce, after bounded concurrent workers finish, the single
//!   surviving pointer covers the live event sequence, so every worker's
//!   committed sequence is covered by some published generation.

use bead_rs::service::acquire_checkpoint_publication_lock;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

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

fn spawn(workspace: &Path, args: &[&str]) -> Child {
    Command::new(bead_binary())
        .args(args)
        .current_dir(workspace)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn bead")
}

/// Wait for a child, failing the test rather than hanging forever.
fn wait_with_timeout(mut child: Child, context: &str) -> std::process::Output {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        match child.try_wait().expect("child is running") {
            // The process has exited; wait_with_output now just drains the
            // buffered pipes and returns immediately.
            Some(_) => return child.wait_with_output().expect("child reaped"),
            None if Instant::now() > deadline => {
                let _ = child.kill();
                panic!("{context}: child did not finish within 60s");
            }
            None => std::thread::sleep(Duration::from_millis(10)),
        }
    }
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

/// A fresh workspace with automatic publication armed.
fn workspace_with_auto_flush(prefix: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("workspace tempdir");
    let workspace = dir.path().to_path_buf();
    let output = run(
        &workspace,
        &["init", "--skip-foreign-workspace", "--prefix", prefix],
    );
    assert!(output.status.success(), "init failed");
    set_auto_flush(&workspace, true);
    (dir, workspace)
}

/// Every generation object on disk under `objects/` and `manifests/`.
fn objects_on_disk(checkpoint_dir: &Path) -> Vec<String> {
    let mut paths = Vec::new();
    for dir_name in ["objects", "manifests"] {
        if let Ok(entries) = fs::read_dir(checkpoint_dir.join(dir_name)) {
            for entry in entries {
                let entry = entry.unwrap();
                if !entry.file_type().unwrap().is_file() {
                    continue;
                }
                let name = entry.file_name();
                let name = name.to_str().unwrap();
                if !name.ends_with(".tmp") {
                    paths.push(format!("{dir_name}/{name}"));
                }
            }
        }
    }
    paths.sort();
    paths
}

/// The generation objects a pointer still references: its active root plus
/// its added and replaced paths.
fn referenced_objects(pointer: &Value) -> Vec<String> {
    let mut files = Vec::new();
    if let Some(path) = pointer
        .get("active_root")
        .and_then(|root| root.get("path"))
        .and_then(Value::as_str)
    {
        files.push(path.to_string());
    }
    for key in ["added_paths", "replaced_paths"] {
        if let Some(paths) = pointer.get(key).and_then(Value::as_array) {
            for path in paths {
                if let Some(path) = path.as_str() {
                    files.push(path.to_string());
                }
            }
        }
    }
    files
        .into_iter()
        .filter(|p| p.starts_with("objects/") || p.starts_with("manifests/"))
        .collect()
}

fn read_pointer(checkpoint_dir: &Path, name: &str) -> Value {
    serde_json::from_str(&fs::read_to_string(checkpoint_dir.join(name)).unwrap()).unwrap()
}

/// Every file under the checkpoint directory with its bytes, keyed by
/// checkpoint-relative path: byte-identical maps mean nothing was written.
fn snapshot_checkpoint(checkpoint_dir: &Path) -> std::collections::BTreeMap<String, Vec<u8>> {
    fn walk(dir: &Path, prefix: String, out: &mut std::collections::BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_str().unwrap().to_string();
            if path.is_dir() {
                walk(&path, format!("{prefix}{name}/"), out);
            } else {
                out.insert(format!("{prefix}{name}"), fs::read(&path).unwrap());
            }
        }
    }
    let mut out = std::collections::BTreeMap::new();
    walk(checkpoint_dir, String::new(), &mut out);
    out
}

/// Assert the whole-checkpoint invariants a torn or half-cleaned
/// publication would break: the pointer selects a root that verifies, the
/// checkpoint is ready to commit, no tombstone is unresolved, and the
/// on-disk object set is exactly what the two retained generations
/// reference -- the tombstone set fully applied and the retained set
/// bounded by `current.json` and `previous.json`.
fn assert_intact_checkpoint(workspace: &Path, context: &str) {
    let report = status(workspace);
    assert_eq!(
        report["root_verified"],
        Value::Bool(true),
        "{context}: the pointer does not verify against its root object"
    );
    assert_eq!(
        report["ready_to_commit"],
        Value::Bool(true),
        "{context}: the checkpoint is not ready to commit (torn publication)"
    );
    assert_eq!(
        report["unresolved_tombstones"].as_array().map(Vec::len),
        Some(0),
        "{context}: a partially applied tombstone set remains"
    );

    let checkpoint_dir = workspace.join(".beads/checkpoint");
    let mut retained: Vec<String> =
        referenced_objects(&read_pointer(&checkpoint_dir, "current.json"));
    retained.extend(referenced_objects(&read_pointer(
        &checkpoint_dir,
        "previous.json",
    )));
    retained.sort();
    retained.dedup();
    assert_eq!(
        objects_on_disk(&checkpoint_dir),
        retained,
        "{context}: the on-disk object set is not exactly the retained set -- \
         a tombstone was applied to a live object or left unapplied"
    );
}

/// Bounded concurrent publishers: several workers commit while the lock is
/// held, so every one of them is blocked wanting to publish, and releasing
/// the lock turns them loose on the checkpoint at the same instant. The
/// serialized result must be one intact generation covering every worker's
/// committed sequence -- never a torn pointer or a partially applied
/// tombstone set.
///
/// The workers are started one at a time (each observed to commit before
/// the next starts) because concurrent *mutations* contend on the SQLite
/// write path, whose behavior is not this lock's to fix -- plan 6.2.1
/// item 4 serializes publication precisely so that path stays separate.
#[test]
fn concurrent_workers_leave_one_intact_covering_generation() {
    let (_dir, workspace) = workspace_with_auto_flush("lock");
    assert!(run(&workspace, &["create", "--title", "setup"])
        .status
        .success());
    let held = acquire_checkpoint_publication_lock(&workspace.join(".beads/checkpoint")).unwrap();
    let children: Vec<_> = (0..8)
        .map(|i| spawn(&workspace, &["create", "--title", &format!("worker {i}")]))
        .collect();
    drop(held);
    let outputs: Vec<_> = children
        .into_iter()
        .map(|child| wait_with_timeout(child, "worker"))
        .collect();
    for output in outputs {
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let id = String::from_utf8(output.stdout).unwrap();
        assert!(!id.trim().is_empty());
        assert!(run(&workspace, &["show", id.trim()]).status.success());
    }
    let report = status(&workspace);
    assert_eq!(report["covered_sequence"], report["live_sequence"]);
    assert_intact_checkpoint(&workspace, "after concurrent guarded writers");
}

#[test]
fn mutation_waits_for_a_stable_checkpoint_before_committing() {
    let (_dir, workspace) = workspace_with_auto_flush("held");
    assert!(run(&workspace, &["create", "--title", "setup"])
        .status
        .success());
    let before = status(&workspace)["live_sequence"].clone();
    let held = acquire_checkpoint_publication_lock(&workspace.join(".beads/checkpoint")).unwrap();
    let mut child = spawn(&workspace, &["create", "--title", "guarded write"]);
    std::thread::sleep(Duration::from_millis(200));
    assert!(child.try_wait().unwrap().is_none());
    assert_eq!(status(&workspace)["live_sequence"], before);
    drop(held);
    let output = wait_with_timeout(child, "guarded writer");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_intact_checkpoint(&workspace, "after guard admission");
}

#[test]
fn higher_sequence_without_local_publication_proof_cannot_admit_a_mutation() {
    let (_dir, workspace) = workspace_with_auto_flush("raced");
    assert!(run(&workspace, &["create", "--title", "setup"])
        .status
        .success());
    let before_live = status(&workspace)["live_sequence"].as_i64().unwrap();
    let checkpoint_dir = workspace.join(".beads/checkpoint");
    let held = acquire_checkpoint_publication_lock(&checkpoint_dir).unwrap();
    let child = spawn(&workspace, &["create", "--title", "must not commit"]);
    let path = checkpoint_dir.join("current.json");
    let mut pointer = read_pointer(&checkpoint_dir, "current.json");
    pointer["snapshot_sequence"] = Value::from(before_live + 1);
    fs::write(&path, serde_json::to_vec_pretty(&pointer).unwrap()).unwrap();
    let before_files = snapshot_checkpoint(&checkpoint_dir);
    drop(held);
    let output = wait_with_timeout(child, "guarded writer");
    assert!(
        matches!(output.status.code(), Some(4 | 5)),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    assert_eq!(status(&workspace)["live_sequence"], before_live);
    assert_eq!(snapshot_checkpoint(&checkpoint_dir), before_files);
}

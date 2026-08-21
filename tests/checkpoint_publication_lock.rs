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
//! - the lock never blocks mutations: a worker commits to SQLite while
//!   another publisher holds the lock, and only its own post-commit
//!   publication waits;
//! - at quiesce, after bounded concurrent workers finish, the single
//!   surviving pointer covers the live event sequence, so every worker's
//!   committed sequence is covered by some published generation.

use bead_rs::service::acquire_checkpoint_publication_lock;
use serde_json::Value;
use std::fs;
use std::io::{BufRead, BufReader, Read};
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
    let output = run(&workspace, &["init", "--prefix", prefix]);
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
    // One setup mutation establishes an initial pointer and retained
    // generation for the burst to supersede and shrink.
    let setup = run(&workspace, &["create", "--title", "burst setup"]);
    assert!(setup.status.success());

    // Hold the lock as a slow publisher would, so every worker commits its
    // mutation (distinct from the SQLite write path), prints its ID, and
    // then queues wanting to publish.
    let held = acquire_checkpoint_publication_lock(&workspace.join(".beads/checkpoint")).unwrap();

    let workers = 8;
    // Each worker keeps its stdout reader alive so the test can drain
    // whatever follows the ID line after the child exits.
    let mut children: Vec<(Child, BufReader<std::process::ChildStdout>)> = Vec::new();
    let mut ids = Vec::new();
    for i in 0..workers {
        let mut child = spawn(
            &workspace,
            &["create", "--title", &format!("concurrent worker {i}")],
        );
        let stdout = child.stdout.take().expect("piped stdout");
        let mut reader = BufReader::new(stdout);
        let mut id_line = String::new();
        reader
            .read_line(&mut id_line)
            .expect("the create printed its ID");
        let id = id_line.trim().to_string();
        assert!(!id.is_empty(), "worker {i} printed no ID: {id_line:?}");
        ids.push(id);
        children.push((child, reader));
    }

    // Every worker committed (its ID is printed only after its transaction
    // commits) and every worker is still alive, blocked acquiring the lock.
    for (i, (child, _)) in children.iter_mut().enumerate() {
        assert!(
            child.try_wait().expect("child is alive").is_none(),
            "worker {i} finished without the publication lock; it must be \
             queued wanting to publish"
        );
    }

    // Turn the queued publishers loose on the checkpoint together.
    drop(held);
    for (i, (child, mut reader)) in children.into_iter().enumerate() {
        let output = wait_with_timeout(child, &format!("worker {i}"));
        assert!(
            output.status.success(),
            "worker {i} exited {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
        let mut beyond_id = String::new();
        reader.read_to_string(&mut beyond_id).expect("drain stdout");
        assert!(
            beyond_id.is_empty(),
            "publication contention must stay silent (item 6); worker {i} \
             printed beyond its ID: {beyond_id:?}"
        );
    }

    // Quiesce: the pointer covers the live sequence, which covers every
    // worker's committed sequence, and every worker's issue is committed.
    let report = status(&workspace);
    assert_eq!(
        report["covered_sequence"], report["live_sequence"],
        "after the burst the checkpoint covers {} but live is {} -- a \
         worker's committed sequence is not covered by any generation",
        report["covered_sequence"], report["live_sequence"]
    );
    for id in &ids {
        let shown = run(&workspace, &["show", id]);
        assert!(
            shown.status.success(),
            "worker issue {id} did not survive the burst: {}",
            String::from_utf8_lossy(&shown.stderr)
        );
    }

    assert_intact_checkpoint(&workspace, "after the concurrent burst");
}

/// The publication lock is distinct from the SQLite write path: a worker
/// whose mutation commits while another publisher holds the lock proceeds
/// through its own transaction, prints its normal output, and only its
/// post-commit publication waits. Releasing the lock lets it publish and
/// exit 0.
#[test]
fn mutation_commits_while_publication_is_locked() {
    let (_dir, workspace) = workspace_with_auto_flush("held");
    let setup = run(&workspace, &["create", "--title", "before held lock"]);
    assert!(setup.status.success());

    // Hold the publication lock as another publisher would.
    let held = acquire_checkpoint_publication_lock(&workspace.join(".beads/checkpoint")).unwrap();

    let mut child = spawn(
        &workspace,
        &["create", "--title", "commits under held lock"],
    );
    let stdout = child.stdout.take().expect("piped stdout");
    let mut reader = BufReader::new(stdout);
    let mut id_line = String::new();
    reader
        .read_line(&mut id_line)
        .expect("the create printed its ID");
    let id = id_line.trim().to_string();
    assert!(!id.is_empty(), "create printed no ID: {id_line:?}");

    // The child has printed its success output, so its transaction is
    // committed -- SQLite was never blocked by the publication lock. Prove
    // it against the database: a read-only command sees the issue while
    // the lock is still held and the child still waiting to publish.
    let shown = run(&workspace, &["show", &id]);
    assert!(
        shown.status.success(),
        "the mutation did not commit while the publication lock was held -- \
         the lock is blocking the SQLite write path: {}",
        String::from_utf8_lossy(&shown.stderr)
    );
    assert!(
        child.try_wait().expect("child is alive").is_none(),
        "the child must still be waiting to publish while the lock is held"
    );

    drop(held);
    let output = wait_with_timeout(child, "worker");
    assert!(
        output.status.success(),
        "the worker failed once the lock was released: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let report = status(&workspace);
    assert_eq!(
        report["covered_sequence"], report["live_sequence"],
        "after release the checkpoint must cover the waiting worker's \
         sequence, covers {} live {}",
        report["covered_sequence"], report["live_sequence"]
    );
    assert_intact_checkpoint(&workspace, "after the held-lock worker publishes");
}

/// A lost publication race is success: a worker that committed, passed the
/// lock-free coverage check, and then finds -- under the lock -- a pointer
/// another publisher carried to its sequence or beyond publishes nothing
/// and exits 0. The pointer is forged to the exact sequence the blocked
/// worker's own commit reached, which is what a concurrent publisher that
/// won the race would have left behind.
#[test]
fn lost_publication_race_is_success_under_the_lock() {
    let (_dir, workspace) = workspace_with_auto_flush("raced");
    let setup = run(&workspace, &["create", "--title", "race setup"]);
    assert!(setup.status.success());
    let before_live = status(&workspace)["live_sequence"].as_i64().unwrap();

    // Hold the lock so the spawned worker commits, passes its lock-free
    // coverage check (the pointer still covers the old sequence), and then
    // blocks acquiring the lock.
    let held = acquire_checkpoint_publication_lock(&workspace.join(".beads/checkpoint")).unwrap();

    let mut child = spawn(
        &workspace,
        &["create", "--title", "loses the publication race"],
    );
    let stdout = child.stdout.take().expect("piped stdout");
    let mut reader = BufReader::new(stdout);
    let mut id_line = String::new();
    reader
        .read_line(&mut id_line)
        .expect("the create printed its ID");
    assert!(
        !id_line.trim().is_empty(),
        "create printed no ID: {id_line:?}"
    );

    // Give the worker time to reach the lock acquisition: between printing
    // its ID and blocking it performs only two local reads (live sequence,
    // pointer). Still holding the lock, it cannot have published.
    std::thread::sleep(Duration::from_millis(400));
    assert!(
        child.try_wait().expect("child is alive").is_none(),
        "the worker must be blocked acquiring the publication lock"
    );

    // The state the race winner leaves: a pointer already covering the
    // blocked worker's committed sequence (one `created` event beyond
    // setup).
    let pointer_path = workspace.join(".beads/checkpoint/current.json");
    let mut pointer: Value =
        serde_json::from_str(&fs::read_to_string(&pointer_path).unwrap()).unwrap();
    pointer
        .as_object_mut()
        .unwrap()
        .insert("snapshot_sequence".into(), Value::from(before_live + 1));
    fs::write(
        &pointer_path,
        serde_json::to_string_pretty(&pointer).unwrap(),
    )
    .unwrap();

    let checkpoint_dir = workspace.join(".beads/checkpoint");
    let before_files = snapshot_checkpoint(&checkpoint_dir);
    let before_generation = status(&workspace)["generation_id"]
        .as_str()
        .unwrap()
        .to_string();

    drop(held);
    let output = wait_with_timeout(child, "worker");

    // The lost race is success: exit 0, nothing published over the winner.
    assert!(
        output.status.success(),
        "a lost publication race must exit 0, got {:?}: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    // The command's own output was the ID line read above; nothing may
    // follow it (publication is silent, item 6). Drain the surviving
    // reader: wait_with_output could not, the pipe was already taken.
    let mut beyond_id = String::new();
    reader.read_to_string(&mut beyond_id).expect("drain stdout");
    assert!(
        beyond_id.is_empty(),
        "the lost race must print nothing beyond the ID it already printed, got {beyond_id:?}"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).is_empty(),
        "a lost race must not warn on stderr, got {:?}",
        String::from_utf8_lossy(&output.stderr)
    );

    let report = status(&workspace);
    assert_eq!(
        report["generation_id"].as_str().unwrap(),
        before_generation,
        "the loser published a generation over the race winner"
    );
    let after_files = snapshot_checkpoint(&checkpoint_dir);
    assert_eq!(
        before_files, after_files,
        "the lost race changed the checkpoint set -- nothing may be written"
    );
    assert_eq!(
        report["covered_sequence"].as_i64().unwrap(),
        before_live + 1,
        "the winner's pointer must still cover the loser's committed sequence"
    );
}

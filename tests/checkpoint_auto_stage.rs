//! Post-publication auto-staging of the verified checkpoint fileset
//! (ADR-018).
//!
//! Publication has been automatic since R026, but assembling the Git
//! commit that carries the published set stayed manual, and two ways that
//! goes wrong are structural:
//!
//! - `git commit <pathspec>` stages tracked modifications but not freshly
//!   written untracked objects, so committing `.beads/checkpoint` by
//!   pathspec omitted a freshly written object and NEEDLE's
//!   checkpoint-verification hook rejected the commit;
//! - compaction renames objects, so a hand-written pathspec is stale the
//!   moment it is typed.
//!
//! These tests pin the fix at the CLI boundary: every successful
//! publication leaves exactly the fileset it made authoritative in the
//! index -- pointers, referenced and retained objects, the compatibility
//! view when written, tracked tombstones as removals -- with runtime files
//! carved out, the `checkpoint.auto_stage` opt-out honored, and a
//! workspace outside any repository untouched.

use assert_cmd::Command;
use serde_json::Value;
use std::fs;
use std::path::Path;

fn bead(workspace: &Path) -> Command {
    let mut cmd = Command::cargo_bin("bead").unwrap();
    cmd.current_dir(workspace);
    cmd
}

fn run(workspace: &Path, args: &[&str]) -> std::process::Output {
    bead(workspace)
        .args(args)
        .assert()
        .success()
        .get_output()
        .clone()
}

fn git(workspace: &Path, args: &[&str]) -> std::process::Output {
    Command::new("git")
        .current_dir(workspace)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .args([
            "-c",
            "user.name=beadrs-test",
            "-c",
            "user.email=beadrs-test@invalid",
        ])
        .args(args)
        .output()
        .expect("git should be runnable in tests")
}

fn git_init(workspace: &Path) {
    let init = git(workspace, &["init", "-q"]);
    assert!(
        init.status.success(),
        "git init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );
}

/// Paths in the index for the given pathspec, from `git diff --cached
/// --name-only` (staged-but-never-committed content included).
fn staged_paths(workspace: &Path, pathspec: &str) -> Vec<String> {
    let out = git(
        workspace,
        &["diff", "--cached", "--name-only", "--", pathspec],
    );
    assert!(
        out.status.success(),
        "git diff --cached failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_string)
        .collect()
}

/// `git status --porcelain` lines for the given pathspec.
fn porcelain(workspace: &Path, pathspec: &str) -> Vec<String> {
    let out = git(workspace, &["status", "--porcelain", "--", pathspec]);
    assert!(
        out.status.success(),
        "git status failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_string)
        .collect()
}

/// Whether a porcelain line still has unstaged or untracked content:
/// a staged-only line (`A `, `M `, `D `, `AM`'s first column...) reads
/// `false`, anything with a second-column letter or an `??` reads `true`.
fn has_unstaged_content(line: &str) -> bool {
    let bytes = line.as_bytes();
    if line.starts_with("??") {
        return true;
    }
    bytes.len() > 1 && bytes[1] != b' '
}

/// Runtime files (`RUNTIME_CHECKPOINT_FILES`) are synchronization
/// metadata, deliberately never staged, so they may linger untracked.
fn is_runtime_path(porcelain_line: &str) -> bool {
    porcelain_line
        .split_whitespace()
        .next_back()
        .is_some_and(|p| p.ends_with("/publish.lock") || p == "publish.lock")
}

fn checkpoint_lines(workspace: &Path) -> Vec<String> {
    porcelain(workspace, ".beads/checkpoint")
        .into_iter()
        .filter(|line| !is_runtime_path(line))
        .collect()
}

/// Record `checkpoint.auto_stage = VALUE` in the workspace's
/// `.beads/config.json`, preserving every other key.
fn set_auto_stage(workspace: &Path, value: bool) {
    let path = workspace.join(".beads/config.json");
    let mut config: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    config
        .as_object_mut()
        .unwrap()
        .entry("checkpoint")
        .or_insert(Value::Object(Default::default()))
        .as_object_mut()
        .unwrap()
        .insert("auto_stage".into(), Value::Bool(value));
    fs::write(&path, serde_json::to_string_pretty(&config).unwrap()).unwrap();
}

fn create_issue(workspace: &Path, title: &str) -> String {
    String::from_utf8(run(workspace, &["create", "--title", title]).stdout)
        .unwrap()
        .trim()
        .to_string()
}

/// The current pointer's active root object path, read straight from the
/// pointer.
fn root_path(workspace: &Path) -> String {
    let pointer: Value = serde_json::from_str(
        &fs::read_to_string(workspace.join(".beads/checkpoint/current.json")).unwrap(),
    )
    .unwrap();
    pointer["active_root"]["path"]
        .as_str()
        .expect("active_root.path")
        .to_string()
}

fn status(workspace: &Path) -> Value {
    let output = bead(workspace)
        .args(["sync", "status", "--format", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice(&output).unwrap()
}

#[test]
fn every_publication_leaves_the_verified_fileset_staged() {
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path();
    git_init(workspace);

    run(workspace, &["init", "--prefix", "stage"]);
    create_issue(workspace, "first mutation");
    create_issue(workspace, "second mutation");

    // No published checkpoint file may remain unstaged or untracked: the
    // staged set must be exactly what the publications made authoritative.
    let lines = checkpoint_lines(workspace);
    let leftover: Vec<&String> = lines
        .iter()
        .filter(|line| has_unstaged_content(line))
        .collect();
    assert!(
        leftover.is_empty(),
        "published checkpoint files left unstaged: {:?}",
        leftover
    );

    // Both pointers and every on-disk object are in the index.
    let staged = staged_paths(workspace, ".beads/checkpoint");
    assert!(
        staged.iter().any(|p| p.ends_with("current.json")),
        "current.json not staged: {:?}",
        staged
    );
    assert!(
        staged.iter().any(|p| p.ends_with("previous.json")),
        "previous.json not staged: {:?}",
        staged
    );
    let objects_dir = workspace.join(".beads/checkpoint/objects");
    if objects_dir.exists() {
        for entry in fs::read_dir(&objects_dir).unwrap() {
            let entry = entry.unwrap();
            let rel = format!(
                ".beads/checkpoint/objects/{}",
                entry.file_name().to_str().unwrap()
            );
            assert!(
                staged.contains(&rel),
                "on-disk object {} missing from the index: {:?}",
                rel,
                staged
            );
        }
    }
}

#[test]
fn a_fresh_untracked_object_is_staged_without_a_manual_pathspec() {
    // The 2026-09-02 regression: `git commit <pathspec>` takes tracked
    // modifications but not freshly written untracked objects. The object
    // the second publication writes did not exist when the first staged;
    // it must be in the index without anyone typing its path.
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path();
    git_init(workspace);

    run(workspace, &["init", "--prefix", "fresh"]);
    let first_root = root_path(workspace);

    create_issue(workspace, "the mutation that mints a new object");
    let second_root = root_path(workspace);
    assert_ne!(
        first_root, second_root,
        "the second publication should mint a new root object"
    );

    let staged = staged_paths(workspace, ".beads/checkpoint");
    assert!(
        staged.contains(&format!(".beads/checkpoint/{}", second_root)),
        "the freshly written object {} is not staged; a pathspec commit would \
         omit it exactly as the 2026-09-02 incident: {:?}",
        second_root,
        staged
    );
}

#[test]
fn tracked_tombstones_stage_as_removals_and_never_tracked_ones_do_not_abort() {
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path();
    git_init(workspace);

    run(workspace, &["init", "--prefix", "tomb"]);
    let first_root = root_path(workspace);

    // Carry the first generation into history: a staged removal is visible
    // as `D` only against a HEAD that contains the tombstoned path, and the
    // publication's auto-stage of the first generation is exactly what put
    // its object in the index. Committing here freezes that tracked state
    // into HEAD so the third publication's tombstone can be observed.
    assert!(
        git(
            workspace,
            &["commit", "-q", "-m", "first generation", "--allow-empty"]
        )
        .status
        .success(),
        "committing the first staged generation failed"
    );
    create_issue(workspace, "second generation");
    create_issue(workspace, "third generation retires the first object");

    // The first generation's object is two generations back: nothing
    // current.json or previous.json references it, so the third
    // publication tombstoned it. It was staged when it was written, so it
    // is tracked, and its removal must be staged with it.
    assert!(
        !workspace
            .join(".beads/checkpoint")
            .join(&first_root)
            .exists(),
        "the tombstoned object should be gone from the worktree"
    );
    let staged = git(
        workspace,
        &[
            "diff",
            "--cached",
            "--name-status",
            "--",
            ".beads/checkpoint",
        ],
    );
    let staged_text = String::from_utf8_lossy(&staged.stdout).to_string();
    assert!(
        staged_text.contains(&format!("D\t.beads/checkpoint/{}", first_root)),
        "the tracked tombstone's removal is not staged: {}",
        staged_text
    );

    // A never-tracked deletion candidate must not abort the invocation.
    // A stray object left before the next publication is the documented
    // legacy/interrupted-cleanup shape: the publication tombstones it, but
    // only tracked paths stage as removals.
    let stray = workspace.join(".beads/checkpoint/objects/stray.jsonl");
    fs::write(&stray, "left by an interrupted cleanup\n").unwrap();
    create_issue(workspace, "fourth generation sweeps the stray");
    assert!(
        !stray.exists(),
        "the stray object should be tombstoned away from the worktree"
    );
    let staged_after = git(
        workspace,
        &[
            "diff",
            "--cached",
            "--name-status",
            "--",
            ".beads/checkpoint",
        ],
    );
    let text_after = String::from_utf8_lossy(&staged_after.stdout).to_string();
    assert!(
        !text_after.contains("stray"),
        "a never-tracked deletion must not enter the index: {}",
        text_after
    );
    let final_lines = checkpoint_lines(workspace);
    let leftover: Vec<&String> = final_lines
        .iter()
        .filter(|line| has_unstaged_content(line))
        .collect();
    assert!(
        leftover.is_empty(),
        "the stray-sweep publication left published files unstaged: {:?}",
        leftover
    );
}

#[test]
fn auto_stage_false_leaves_the_fileset_unstaged() {
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path();
    git_init(workspace);

    run(workspace, &["init", "--prefix", "optout"]);
    set_auto_stage(workspace, false);

    create_issue(workspace, "published but not staged");

    // The publication ran (the checkpoint covers the mutation)...
    let status = status(workspace);
    assert_eq!(status["covered_sequence"], status["live_sequence"]);

    // ...but nothing new was staged: the pointer is rewritten by every
    // publication, so an unstaged modification on it means staging was
    // suppressed.
    let lines = checkpoint_lines(workspace);
    assert!(
        lines.iter().any(|line| has_unstaged_content(line)),
        "checkpoint.auto_stage=false did not suppress staging: {:?}",
        lines
    );
    let current = lines
        .iter()
        .find(|line| line.contains("current.json"))
        .expect("current.json appears in git status");
    assert!(
        has_unstaged_content(current),
        "current.json is staged despite the opt-out: {}",
        current
    );
}

#[test]
fn capability_document_advertises_auto_stage() {
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path();
    let output = bead(workspace)
        .args(["capabilities"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let caps: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(
        caps.get("auto_stage").and_then(|v| v.as_bool()),
        Some(true),
        "auto_stage should advertise the compiled default (true)"
    );
}

#[test]
fn a_workspace_outside_any_repository_publishes_without_staging() {
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path();

    run(workspace, &["init", "--prefix", "repoless"]);
    create_issue(workspace, "no index to stage into");

    let status = status(workspace);
    assert_eq!(status["checkpoint_present"], Value::Bool(true));
    assert_eq!(status["covered_sequence"], status["live_sequence"]);
    assert!(
        !workspace.join(".git").exists(),
        "staging must not create a repository as a side effect"
    );
}

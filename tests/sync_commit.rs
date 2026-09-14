//! The explicit checkpoint commit: `bead sync commit` (ADR-019).
//!
//! Auto-staging (ADR-018) leaves the verified fileset in the index; this
//! command assembles the commit that carries it. These tests pin the
//! contract at the CLI boundary:
//!
//! - the commit records **exactly** the verified fileset -- both pointers,
//!   the root object the pointer selects (the 2026-09-02 omitted-object
//!   shape), the compatibility view -- and **nothing else**: another
//!   worker's unrelated staged file survives the command still staged;
//! - the gates refuse before anything is touched: a dirty checkpoint (live
//!   store ahead), a remote-advanced checkpoint (pull ahead of the store),
//!   a damaged checkpoint (missing referenced file), and a detached HEAD;
//! - an up-to-date workspace commits nothing and exits 0;
//! - `--dry-run` reports without touching the index or history;
//! - a rejecting pre-commit hook rejects the commit: the command never
//!   bypasses the gates (no `--no-verify`).

use assert_cmd::Command;
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::Output;

fn bead(workspace: &Path) -> Command {
    let mut cmd = Command::cargo_bin("bead").unwrap();
    cmd.current_dir(workspace);
    cmd
}

fn run(workspace: &Path, args: &[&str]) -> Output {
    bead(workspace)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .args(args)
        .assert()
        .success()
        .get_output()
        .clone()
}

/// A hermetic `git` invocation: no global or system config leaks in, and
/// the identity never depends on the host.
fn git(workspace: &Path, args: &[&str]) -> Output {
    std::process::Command::new("git")
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

fn git_ok(workspace: &Path, args: &[&str]) {
    let out = git(workspace, args);
    assert!(
        out.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
}

/// The files one commit (or the index) carries.
fn changed_files(workspace: &Path, args: &[&str]) -> Vec<String> {
    let out = git(workspace, args);
    assert!(
        out.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_string)
        .collect()
}

fn head(workspace: &Path) -> String {
    String::from_utf8_lossy(&git(workspace, &["rev-parse", "HEAD"]).stdout)
        .trim()
        .to_string()
}

fn status(workspace: &Path) -> Value {
    let output = run(workspace, &["sync", "status", "--format", "json"]);
    serde_json::from_slice(&output.stdout).unwrap()
}

fn create_issue(workspace: &Path, title: &str) -> String {
    String::from_utf8(run(workspace, &["create", "--title", title]).stdout)
        .unwrap()
        .trim()
        .to_string()
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

/// A git repository with an initialized, current bead workspace inside it:
/// one issue created, so auto-flush published a generation and auto-stage
/// staged its fileset.
fn repo_with_current_checkpoint(prefix: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path();
    git_ok(workspace, &["init", "-q"]);
    run(workspace, &["init", "--prefix", prefix]);
    create_issue(workspace, "the mutation that publishes");
    dir
}

#[test]
fn commit_carries_exactly_the_verified_fileset_and_nothing_else() {
    let dir = repo_with_current_checkpoint("scmt");
    let workspace = dir.path();

    // Another worker's in-flight staging on the shared index: an unrelated
    // file staged before the commit runs.
    fs::write(workspace.join("rival.txt"), "a rival worker's WIP\n").unwrap();
    git_ok(workspace, &["add", "rival.txt"]);

    let out = run(workspace, &["sync", "commit"]);
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        out.status.success(),
        "sync commit failed: {} {}",
        stderr,
        String::from_utf8_lossy(&out.stdout)
    );

    // The commit carries the pointer, the previous pointer, the
    // compatibility view, and every object the pointer references --
    // including the freshly written untracked root object whose omission
    // produced the 2026-09-02 rejection.
    let committed = changed_files(workspace, &["show", "--name-only", "--format=", "HEAD"]);
    let pointer: Value = serde_json::from_str(
        &fs::read_to_string(workspace.join(".beads/checkpoint/current.json")).unwrap(),
    )
    .unwrap();
    let root = pointer["active_root"]["path"].as_str().unwrap();
    for expected in [
        ".beads/checkpoint/current.json",
        ".beads/checkpoint/previous.json",
        ".beads/checkpoint/forensic.jsonl",
        format!(".beads/checkpoint/{}", root).as_str(),
    ] {
        assert!(
            committed.iter().any(|path| path == expected),
            "{} not in the commit: {:?}",
            expected,
            committed
        );
    }

    // And nothing else: the rival's staged file is neither committed nor
    // unstaged -- a pathspec commit leaves the rest of the index alone.
    assert!(
        !committed.iter().any(|path| path == "rival.txt"),
        "an unrelated staged file was swept into checkpoint history: {:?}",
        committed
    );
    let still_staged = changed_files(workspace, &["diff", "--cached", "--name-only"]);
    assert_eq!(
        still_staged,
        vec!["rival.txt".to_string()],
        "the rival's staging did not survive the checkpoint commit"
    );

    // The reported commit is HEAD.
    let report = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        report.contains(&head(workspace)[..8]),
        "the report does not name the new commit: {}",
        report
    );
}

#[test]
fn commit_stages_a_fresh_untracked_object_when_auto_stage_is_off() {
    let dir = repo_with_current_checkpoint("ufresh");
    let workspace = dir.path();

    // Land the first generation in history, then suppress auto-staging and
    // mint a second generation: its object is fresh, untracked, and would
    // be omitted by a hand-typed pathspec -- the exact ADR-018 incident.
    git_ok(workspace, &["add", ".beads/checkpoint"]);
    git_ok(workspace, &["commit", "-q", "-m", "first generation"]);
    let config_path = workspace.join(".beads/config.json");
    let mut config: Value =
        serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
    config
        .as_object_mut()
        .unwrap()
        .entry("checkpoint")
        .or_insert(Value::Object(Default::default()))
        .as_object_mut()
        .unwrap()
        .insert("auto_stage".into(), Value::Bool(false));
    fs::write(&config_path, serde_json::to_string_pretty(&config).unwrap()).unwrap();
    create_issue(workspace, "the mutation that mints an untracked object");

    let before = head(workspace);
    run(workspace, &["sync", "commit"]);

    let committed = changed_files(workspace, &["show", "--name-only", "--format=", "HEAD"]);
    let pointer: Value = serde_json::from_str(
        &fs::read_to_string(workspace.join(".beads/checkpoint/current.json")).unwrap(),
    )
    .unwrap();
    let root = pointer["active_root"]["path"].as_str().unwrap();
    assert!(
        committed
            .iter()
            .any(|path| path == &format!(".beads/checkpoint/{}", root)),
        "the freshly written object {} was omitted from the commit -- the 2026-09-02 \
         incident reproduced: {:?}",
        root,
        committed
    );
    assert_ne!(before, head(workspace), "no commit was created");
}

#[test]
fn commit_refuses_a_dirty_checkpoint_and_names_flush_only() {
    let dir = repo_with_current_checkpoint("sdirt");
    let workspace = dir.path();

    git_ok(workspace, &["add", ".beads/checkpoint"]);
    git_ok(workspace, &["commit", "-q", "-m", "checkpoint"]);
    let before = head(workspace);

    // Suppress auto-flush, then mutate: the store advances past the
    // checkpoint and the workspace reads dirty.
    set_auto_flush(workspace, false);
    create_issue(workspace, "unpublished mutation");
    assert_eq!(status(workspace)["relationship"], "behind");

    let out = bead(workspace)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .args(["sync", "commit"])
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "sync commit accepted a dirty checkpoint"
    );
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        stderr.contains("dirty") && stderr.contains("flush-only"),
        "the refusal must name the state and the remedy: {}",
        stderr
    );
    assert_eq!(before, head(workspace), "a refused commit created history");
}

/// Recursive directory copy -- the filesystem equivalent of what `git
/// pull` delivers (the r027 fixture's `copy_tree`).
fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).unwrap();
        }
    }
}

#[test]
fn commit_refuses_a_remote_advanced_checkpoint() {
    // One store cloned the README way: the clone advances and publishes,
    // the pull (a checkpoint-dir copy) leaves the origin's store behind.
    let origin_dir = tempfile::tempdir().unwrap();
    let origin = origin_dir.path();
    git_ok(origin, &["init", "-q"]);
    run(origin, &["init", "--prefix", "srem"]);
    create_issue(origin, "shared history");
    run(origin, &["sync", "flush-only"]);
    let before = head(origin);

    let clone_dir = tempfile::tempdir().unwrap();
    let clone = clone_dir.path().join("clone");
    fs::create_dir_all(clone.join(".beads")).unwrap();
    fs::copy(
        origin.join(".beads/config.json"),
        clone.join(".beads/config.json"),
    )
    .unwrap();
    copy_tree(
        &origin.join(".beads/checkpoint"),
        &clone.join(".beads/checkpoint"),
    );
    run(&clone, &["init"]);
    let generation = {
        let pointer: Value = serde_json::from_str(
            &fs::read_to_string(clone.join(".beads/checkpoint/current.json")).unwrap(),
        )
        .unwrap();
        pointer["generation_id"].as_str().unwrap().to_string()
    };
    run(
        &clone,
        &[
            "restore",
            "--source",
            ".beads/checkpoint",
            "--generation",
            &generation,
            "--actor",
            "clone-operator",
        ],
    );
    create_issue(&clone, "advancement the origin lacks");

    // The pull: the clone's checkpoint replaces the origin's, pointer and
    // all. The origin's store never saw the advancement.
    fs::remove_dir_all(origin.join(".beads/checkpoint")).unwrap();
    copy_tree(
        &clone.join(".beads/checkpoint"),
        &origin.join(".beads/checkpoint"),
    );
    assert_eq!(status(origin)["relationship"], "remote-advanced");

    let out = bead(origin)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .args(["sync", "commit"])
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "sync commit accepted a remote-advanced checkpoint"
    );
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        stderr.contains("remote-advanced") && stderr.contains("reconcile"),
        "the refusal must name the state and the remedy: {}",
        stderr
    );
    assert_eq!(before, head(origin), "a refused commit created history");
}

#[test]
fn commit_refuses_a_damaged_checkpoint_with_missing_referenced_files() {
    let dir = repo_with_current_checkpoint("sdmg");
    let workspace = dir.path();

    // Remove a non-root object the pointer references: the status gate
    // only verifies the root, so this shape reaches the commit command,
    // which must refuse rather than enshrine the damage in history.
    git_ok(workspace, &["add", ".beads/checkpoint"]);
    git_ok(workspace, &["commit", "-q", "-m", "checkpoint"]);
    let pointer: Value = serde_json::from_str(
        &fs::read_to_string(workspace.join(".beads/checkpoint/current.json")).unwrap(),
    )
    .unwrap();
    let root = pointer["active_root"]["path"].as_str().unwrap();
    let objects: Vec<String> = fs::read_dir(workspace.join(".beads/checkpoint/objects"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_str().unwrap().to_string())
        .filter(|name| name.ends_with(".jsonl") && !name.starts_with('.'))
        .collect();
    let victim = objects
        .iter()
        .find(|name| !name.starts_with(&root.replace("objects/", "").replace(".jsonl", "")))
        .or_else(|| objects.first())
        .unwrap()
        .clone();
    // Never delete the root itself -- the status gate owns that shape.
    if format!("objects/{}", victim) != root {
        fs::remove_file(workspace.join(".beads/checkpoint/objects").join(&victim)).unwrap();

        let before = head(workspace);
        let out = bead(workspace)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .args(["sync", "commit"])
            .output()
            .unwrap();
        assert!(
            !out.status.success(),
            "sync commit accepted a checkpoint whose referenced object {} is missing",
            victim
        );
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        assert!(
            stderr.contains("missing from disk"),
            "the refusal must name the damage: {}",
            stderr
        );
        assert_eq!(before, head(workspace), "a refused commit created history");
    }
}

#[test]
fn commit_refuses_a_detached_head() {
    let dir = repo_with_current_checkpoint("sdet");
    let workspace = dir.path();
    git_ok(workspace, &["add", ".beads/checkpoint"]);
    git_ok(workspace, &["commit", "-q", "-m", "checkpoint"]);
    git_ok(workspace, &["checkout", "-q", "--detach", "HEAD"]);
    let before = head(workspace);

    let out = bead(workspace)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .args(["sync", "commit"])
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "sync commit accepted a detached HEAD"
    );
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        stderr.contains("detached") && stderr.contains("branch"),
        "the refusal must name the state and the remedy: {}",
        stderr
    );
    assert_eq!(before, head(workspace), "a refused commit created history");
}

#[test]
fn an_up_to_date_workspace_commits_nothing_and_exits_zero() {
    let dir = repo_with_current_checkpoint("sidle");
    let workspace = dir.path();

    // First commit lands the generation.
    run(workspace, &["sync", "commit"]);
    let after_first = head(workspace);

    // Second run: nothing changed, so nothing is committed and the exit
    // is still 0 -- the same idempotence `sync flush-only` offers.
    let out = run(workspace, &["sync", "commit"]);
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        stdout.contains("nothing to do"),
        "the idempotent run must say so: {}",
        stdout
    );
    assert_eq!(after_first, head(workspace), "a duplicate commit landed");
}

#[test]
fn dry_run_reports_without_touching_the_index_or_history() {
    let dir = repo_with_current_checkpoint("sdry");
    let workspace = dir.path();
    let before = head(workspace);
    let index_before = changed_files(workspace, &["diff", "--cached", "--name-only"]);

    let out = run(workspace, &["sync", "commit", "--dry-run"]);
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        stdout.contains("Dry-run") && stdout.contains("would be created"),
        "the dry run must report a prospective commit: {}",
        stdout
    );
    assert!(
        stdout.contains(".beads/checkpoint/current.json"),
        "{}",
        stdout
    );
    assert_eq!(before, head(workspace), "a dry run created a commit");
    assert_eq!(
        index_before,
        changed_files(workspace, &["diff", "--cached", "--name-only"]),
        "a dry run staged files"
    );

    // On an up-to-date workspace the dry run reports the no-op too.
    run(workspace, &["sync", "commit"]);
    let out = run(workspace, &["sync", "commit", "--dry-run"]);
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("nothing to commit"),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
}

#[test]
fn a_rejecting_pre_commit_hook_rejects_the_commit() {
    let dir = repo_with_current_checkpoint("shook");
    let workspace = dir.path();
    let hook = workspace.join(".git/hooks/pre-commit");
    fs::create_dir_all(hook.parent().unwrap()).unwrap();
    fs::write(
        &hook,
        "#!/bin/sh\necho 'Definition of Done gate: rejected' >&2\nexit 1\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let before = head(workspace);

    let out = bead(workspace)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .args(["sync", "commit"])
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "sync commit bypassed a rejecting pre-commit hook"
    );
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        stderr.contains("Definition of Done gate: rejected"),
        "the gate's own output must surface verbatim: {}",
        stderr
    );
    assert_eq!(before, head(workspace), "the commit landed anyway");
}

#[test]
fn the_default_message_names_the_generation_and_message_override_wins() {
    let dir = repo_with_current_checkpoint("smsg");
    let workspace = dir.path();
    run(workspace, &["sync", "commit"]);
    let subject = String::from_utf8_lossy(&git(workspace, &["log", "-1", "--format=%s"]).stdout)
        .trim()
        .to_string();
    let pointer: Value = serde_json::from_str(
        &fs::read_to_string(workspace.join(".beads/checkpoint/current.json")).unwrap(),
    )
    .unwrap();
    let generation = pointer["generation_id"].as_str().unwrap();
    assert_eq!(
        subject,
        format!("chore(beads): checkpoint {} [bead-rs]", generation),
        "the default message must follow the checkpoint convention"
    );

    create_issue(workspace, "one more mutation");
    run(
        workspace,
        &["sync", "commit", "--message", "operator's own words"],
    );
    let subject = String::from_utf8_lossy(&git(workspace, &["log", "-1", "--format=%s"]).stdout)
        .trim()
        .to_string();
    assert_eq!(subject, "operator's own words");
}

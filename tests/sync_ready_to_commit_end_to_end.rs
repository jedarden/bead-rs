//! End-to-end `sync status` readiness coverage for the ADR-017 gate.
//!
//! The pure gate in `service::git::commit_readiness` pins its three
//! contract cases with unit tests; the fixtures here drive the real
//! binary over three workspaces so the wiring -- probe, JSON report,
//! `gate_readiness`, text render -- is exercised as one path:
//!
//! 1. **committed** -- a flushed checkpoint committed to Git reads
//!    `Ready to commit: yes` with every pending bucket empty;
//! 2. **uncommitted** -- a flushed checkpoint nothing has committed (the
//!    live 2026-09-02 commitgraph reproduction: `current.json`,
//!    `forensic.jsonl` and `previous.json` modified, fresh `objects/`
//!    files untracked) never reads a bare yes, and the not-ready reason
//!    names the pending paths;
//! 3. **repo-less** -- no `.git` above the workspace reads the explicit
//!    unavailable form and still exits 0, with the unavailability folded
//!    into `ready_to_commit` rather than silently passing the gate.

use std::path::Path;
use std::process::{Command, Output};

fn run_bead(workspace: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_bead"))
        .arg("--skip-foreign-workspace")
        .args(args)
        .current_dir(workspace)
        .output()
        .expect("run bead")
}

fn run_git(workspace: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(workspace)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn init_bead(workspace: &Path, prefix: &str) {
    let output = run_bead(workspace, &["init", "--prefix", prefix]);
    assert!(
        output.status.success(),
        "bead init failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Text `sync status`, asserting the invocation itself succeeded.
fn text_status(workspace: &Path) -> String {
    let output = run_bead(workspace, &["sync", "status"]);
    assert!(
        output.status.success(),
        "sync status failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("UTF-8 status output")
}

fn status_json(workspace: &Path) -> serde_json::Value {
    let output = run_bead(workspace, &["sync", "status", "--format", "json"]);
    assert!(
        output.status.success(),
        "JSON sync status failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("JSON status output")
}

fn commit_checkpoint(root: &Path, message: &str) {
    run_git(root, &["add", ".beads"]);
    run_git(root, &["commit", "--quiet", "-m", message]);
}

/// Fixture 1: a flushed checkpoint fully reachable from HEAD.
#[test]
fn committed_checkpoint_reads_ready_to_commit_yes() {
    let workspace = tempfile::tempdir().expect("temp workspace");
    let root = workspace.path();

    run_git(root, &["init", "--quiet"]);
    run_git(root, &["config", "user.name", "bead-rs test"]);
    run_git(
        root,
        &["config", "user.email", "bead-rs-test@example.invalid"],
    );
    init_bead(root, "readyes");
    // `bead init` publishes the initial checkpoint; make the published
    // state explicit before Git ever sees it.
    let flush = run_bead(root, &["sync", "flush-only"]);
    assert!(
        flush.status.success(),
        "flush-only failed: {}",
        String::from_utf8_lossy(&flush.stderr)
    );
    commit_checkpoint(root, "checkpoint baseline");

    let stdout = text_status(root);
    assert!(stdout.contains("  Git: committed\n"), "{stdout}");
    for bucket in ["staged", "unstaged", "untracked", "ignored"] {
        assert!(
            stdout.contains(&format!("    {bucket}: 0\n")),
            "expected empty {bucket} bucket:\n{stdout}"
        );
    }
    assert!(stdout.contains("  Ready to commit: yes\n"), "{stdout}");
    assert!(!stdout.contains("Ready to commit: NO"), "{stdout}");

    let report = status_json(root);
    assert_eq!(report["git_reachability"]["status"], "committed");
    assert_eq!(report["ready_to_commit"], true);
    assert_eq!(
        report["not_ready_reasons"].as_array().map(Vec::len),
        Some(0),
        "{report}"
    );
    let committed = report["git_reachability"]["committed"]
        .as_array()
        .unwrap_or_else(|| panic!("missing committed bucket: {report}"));
    assert!(
        committed
            .iter()
            .any(|path| path == ".beads/checkpoint/current.json"),
        "current.json not committed: {report}"
    );
}

/// Fixture 2: the commitgraph reproduction -- checkpoint flushed, nothing
/// committed. Not a bare yes, and the reason names the pending paths.
#[test]
fn uncommitted_checkpoint_is_never_a_bare_yes_and_names_paths() {
    let workspace = tempfile::tempdir().expect("temp workspace");
    let root = workspace.path();

    run_git(root, &["init", "--quiet"]);
    run_git(root, &["config", "user.name", "bead-rs test"]);
    run_git(
        root,
        &["config", "user.email", "bead-rs-test@example.invalid"],
    );
    init_bead(root, "rdyfl");
    commit_checkpoint(root, "checkpoint baseline");

    // Mutate, then flush: the re-publication makes the new checkpoint
    // authoritative and stages its fileset (ADR-018), so the pending
    // paths are staged-but-uncommitted rather than modified-unstaged
    // with untracked objects.
    let create = run_bead(
        root,
        &[
            "create",
            "--title",
            "uncommitted probe",
            "--issue-type",
            "task",
        ],
    );
    assert!(
        create.status.success(),
        "bead create failed: {}",
        String::from_utf8_lossy(&create.stderr)
    );
    let flush = run_bead(root, &["sync", "flush-only"]);
    assert!(
        flush.status.success(),
        "flush-only failed: {}",
        String::from_utf8_lossy(&flush.stderr)
    );

    let stdout = text_status(root);
    assert!(stdout.contains("  Ready to commit: NO\n"), "{stdout}");
    assert!(!stdout.contains("Ready to commit: yes"), "{stdout}");
    assert!(
        stdout.contains(
            "    - checkpoint not fully committed (Git cannot reach every published file): "
        ),
        "{stdout}"
    );
    assert!(
        stdout.contains(".beads/checkpoint/current.json"),
        "{stdout}"
    );
    assert!(stdout.contains(".beads/checkpoint/objects/"), "{stdout}");

    let report = status_json(root);
    assert_eq!(report["ready_to_commit"], false);
    assert_eq!(report["checkpoint_consistent"], true, "{report}");
    let reasons = report["not_ready_reasons"]
        .as_array()
        .unwrap_or_else(|| panic!("missing not_ready_reasons: {report}"));
    assert!(
        reasons.iter().any(|reason| reason
            .as_str()
            .unwrap_or("")
            .starts_with("checkpoint not fully committed")),
        "{report}"
    );

    // Since ADR-018 every publication stages exactly the fileset it made
    // authoritative, so the still-unreachable pending paths sit in the
    // staged bucket -- staged counts as "Git cannot reach" for the verdict.
    // Pinning the staged bucket (rather than accepting any pending bucket)
    // also fails loudly if publication regresses to the pre-ADR-018
    // modified-unstaged-with-untracked-objects shape.
    let reach = &report["git_reachability"];
    let staged = reach["staged"].as_array().expect("staged bucket");
    assert!(
        staged
            .iter()
            .any(|path| path == ".beads/checkpoint/current.json"),
        "current.json not staged: {report}"
    );
    assert!(
        staged.iter().any(|path| path
            .as_str()
            .unwrap_or("")
            .starts_with(".beads/checkpoint/objects/")),
        "no staged objects/ file: {report}"
    );
}

/// Fixture 3: no repository above the workspace. The explicit
/// unavailable form, exit 0, and the unavailability folded into the
/// readiness verdict (ADR-017 case (c)) instead of a silent yes.
#[test]
fn repo_less_workspace_names_unavailability_in_the_readiness_gate() {
    let workspace = tempfile::tempdir().expect("temp workspace");
    let root = workspace.path();

    init_bead(root, "rdyno");

    let stdout = text_status(root);
    assert!(
        stdout.contains("  Git: unavailable: no Git repository above "),
        "{stdout}"
    );
    assert!(stdout.contains("  Ready to commit: NO\n"), "{stdout}");
    assert!(
        stdout.contains("    - git reachability unavailable: no Git repository above "),
        "{stdout}"
    );

    let report = status_json(root);
    assert_eq!(report["git_reachability"]["status"], "unavailable");
    assert!(
        report["git_reachability"]["unavailable_reason"]
            .as_str()
            .unwrap()
            .starts_with("no Git repository above "),
        "{report}"
    );
    assert_eq!(report["ready_to_commit"], false);
    let reasons = report["not_ready_reasons"]
        .as_array()
        .unwrap_or_else(|| panic!("missing not_ready_reasons: {report}"));
    assert!(
        reasons.iter().any(|reason| reason
            .as_str()
            .unwrap_or("")
            .starts_with("git reachability unavailable:")),
        "{report}"
    );
    // The file this fixture probes must actually exist, or the report
    // would carry `git_reachability: null` and the gate would pass
    // through unconsulted.
    assert!(root.join(".beads/checkpoint/current.json").is_file());
}

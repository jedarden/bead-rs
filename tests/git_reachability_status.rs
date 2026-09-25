//! Human-readable `sync status` coverage for ADR-013 Git reachability.

use std::fs;
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

fn init_bead(workspace: &Path) {
    let output = run_bead(workspace, &["init", "--prefix", "reach"]);
    assert!(
        output.status.success(),
        "bead init failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
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

fn assert_json_bucket(report: &serde_json::Value, bucket: &str, expected_path: &str) {
    let paths = report["git_reachability"][bucket]
        .as_array()
        .unwrap_or_else(|| panic!("missing JSON {bucket} bucket: {report}"));
    assert!(
        paths.iter().any(|path| path == expected_path),
        "missing {expected_path} from JSON {bucket} bucket: {report}"
    );
}

#[test]
fn text_status_prints_counts_and_paths_for_every_git_disposition() {
    let workspace = tempfile::tempdir().expect("temp workspace");
    let root = workspace.path();

    run_git(root, &["init", "--quiet"]);
    run_git(root, &["config", "user.name", "bead-rs test"]);
    run_git(
        root,
        &["config", "user.email", "bead-rs-test@example.invalid"],
    );
    init_bead(root);

    let checkpoint = root.join(".beads/checkpoint");
    fs::write(checkpoint.join("committed.txt"), "committed\n").unwrap();
    fs::write(checkpoint.join("unstaged.txt"), "baseline\n").unwrap();
    run_git(root, &["add", ".beads/checkpoint"]);
    run_git(root, &["commit", "--quiet", "-m", "checkpoint baseline"]);

    fs::write(checkpoint.join("unstaged.txt"), "working tree change\n").unwrap();
    fs::write(checkpoint.join("staged.txt"), "staged\n").unwrap();
    run_git(root, &["add", ".beads/checkpoint/staged.txt"]);
    fs::write(checkpoint.join("untracked.txt"), "untracked\n").unwrap();
    fs::write(checkpoint.join("ignored.txt"), "ignored\n").unwrap();
    // git's default template normally ships .git/info/exclude, but an
    // init.templatedir override may not — create it rather than depend on
    // the host template.
    fs::create_dir_all(root.join(".git/info")).unwrap();
    fs::write(
        root.join(".git/info/exclude"),
        ".beads/checkpoint/ignored.txt\n",
    )
    .unwrap();

    let output = run_bead(root, &["sync", "status"]);
    assert!(
        output.status.success(),
        "sync status failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("UTF-8 status output");

    assert!(stdout.contains("  Git: ignored\n"), "{stdout}");
    for (bucket, path) in [
        ("staged", ".beads/checkpoint/staged.txt"),
        ("unstaged", ".beads/checkpoint/unstaged.txt"),
        ("untracked", ".beads/checkpoint/untracked.txt"),
    ] {
        let heading = format!("    {bucket}: 1\n");
        assert!(
            stdout.contains(&heading),
            "missing {bucket} count:\n{stdout}"
        );
        assert!(
            stdout.contains(&format!("      {path}\n")),
            "missing {bucket} path:\n{stdout}"
        );
    }
    for bucket in ["committed", "ignored"] {
        let count = stdout
            .lines()
            .find_map(|line| line.strip_prefix(&format!("    {bucket}: ")))
            .unwrap_or_else(|| panic!("missing {bucket} count:\n{stdout}"));
        assert!(count.parse::<usize>().unwrap() > 0, "{stdout}");
    }
    assert!(
        stdout.contains("      .beads/checkpoint/committed.txt\n"),
        "{stdout}"
    );
    assert!(
        stdout.contains("      .beads/checkpoint/ignored.txt\n"),
        "{stdout}"
    );

    let report = status_json(root);
    assert_eq!(report["git_reachability"]["status"], "ignored");
    for (bucket, path) in [
        ("committed", ".beads/checkpoint/committed.txt"),
        ("staged", ".beads/checkpoint/staged.txt"),
        ("unstaged", ".beads/checkpoint/unstaged.txt"),
        ("untracked", ".beads/checkpoint/untracked.txt"),
        ("ignored", ".beads/checkpoint/ignored.txt"),
    ] {
        assert_json_bucket(&report, bucket, path);
    }
}

#[test]
fn text_status_reports_repo_less_git_as_unavailable_and_succeeds() {
    let workspace = tempfile::tempdir().expect("temp workspace");
    init_bead(workspace.path());

    let output = run_bead(workspace.path(), &["sync", "status"]);
    assert!(
        output.status.success(),
        "repo-less sync status failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("UTF-8 status output");
    assert!(
        stdout.contains("  Git: unavailable: no Git repository above "),
        "{stdout}"
    );
    assert!(!stdout.contains("    committed:"), "{stdout}");

    let report = status_json(workspace.path());
    assert_eq!(report["git_reachability"]["status"], "unavailable");
    assert!(
        report["git_reachability"]["unavailable_reason"]
            .as_str()
            .unwrap()
            .starts_with("no Git repository above "),
        "{report}"
    );
}

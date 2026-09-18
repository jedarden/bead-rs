//! End-to-end coverage for the 2026-09-18 history-leak follow-up at the CLI
//! boundary: `bead doctor` must advise when the runtime directories that
//! carried worker stdout, database backups and bf-era exports into public
//! history (AgentScribe, screenferry, miroir, drawrace; a live Postgres
//! password rode along in `.bf_history` exports) are not named by the
//! workspace's ignore files, and stay quiet when they are.
//!
//! `bead init` writes `.beads/.gitignore` only for brand-new workspaces, so
//! the warning case is simulated by rewriting the file with the pre-leak
//! template — exactly what every workspace initialized before 2026-09-18
//! carries on disk.

use assert_cmd::Command;
use serde_json::Value;
use std::path::Path;

/// The `.beads/.gitignore` as `bead init` wrote it before the leak set was
/// extended: no `.br_recovery/`, `.bf_history/` or `recovery/` entries.
const PRE_LEAK_GITIGNORE: &str = "\
# Runtime database artifacts (SQLite files and backups)
*.db
*.db-shm
*.db-wal
*.db.backup.*

# Lock files
*.lock

# Temporary files
*.tmp
*.temp

# Journals
*.journal

# Runtime directories (traces, diagnostics, logs, receipts)
traces/
diagnostics/
logs/
receipts/

# Runtime event logs (root-level JSONL files)
events.jsonl
heartbeats.jsonl
";

fn workspace() -> tempfile::TempDir {
    let workspace = tempfile::Builder::new()
        .prefix("bead-doctor-runtime-dirs-")
        .tempdir_in("/var/tmp")
        .unwrap();
    Command::cargo_bin("bead")
        .unwrap()
        .current_dir(workspace.path())
        .args(["init", "--no-auto-flush"])
        .assert()
        .success();
    workspace
}

fn coverage_check(root: &Path) -> Value {
    let output = Command::cargo_bin("bead")
        .unwrap()
        .current_dir(root)
        .args(["doctor", "--scope", "secrets", "--format", "json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "advisory warnings never fail doctor: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let diagnostics: Value = serde_json::from_str(&String::from_utf8(output.stdout).unwrap())
        .expect("doctor --format json emits a JSON report");
    diagnostics["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|check| check["name"] == "runtime_dir_ignore_coverage")
        .cloned()
        .expect("runtime_dir_ignore_coverage check present in secrets scope")
}

#[test]
fn fresh_init_ignore_file_covers_every_runtime_dir() {
    let workspace = workspace();
    let check = coverage_check(workspace.path());
    assert_eq!(check["status"], "ok");
    let message = check["message"].as_str().unwrap();
    for dir in [
        "traces/",
        "diagnostics/",
        ".br_recovery/",
        ".bf_history/",
        "recovery/",
    ] {
        assert!(
            message.contains(dir),
            "coverage message should name {dir}: {message}"
        );
    }
}

#[test]
fn pre_leak_ignore_file_is_advised() {
    let workspace = workspace();
    std::fs::write(
        workspace.path().join(".beads/.gitignore"),
        PRE_LEAK_GITIGNORE,
    )
    .unwrap();

    let check = coverage_check(workspace.path());
    assert_eq!(check["status"], "warning");
    let message = check["message"].as_str().unwrap();
    for dir in [".br_recovery/", ".bf_history/", "recovery/"] {
        assert!(
            message.contains(dir),
            "warning should name the uncovered directory {dir}: {message}"
        );
    }
    assert!(
        !message.contains("traces/"),
        "traces/ is covered by the pre-leak file and must not be listed: {message}"
    );
    assert!(
        !message.contains("currently present"),
        "no directory exists on disk yet, so no leak-in-progress framing: {message}"
    );
}

#[test]
fn present_uncovered_directory_is_named_as_leak_in_progress() {
    let workspace = workspace();
    std::fs::write(
        workspace.path().join(".beads/.gitignore"),
        PRE_LEAK_GITIGNORE,
    )
    .unwrap();
    std::fs::create_dir_all(workspace.path().join(".beads/.bf_history")).unwrap();

    let check = coverage_check(workspace.path());
    assert_eq!(check["status"], "warning");
    let message = check["message"].as_str().unwrap();
    assert!(
        message.contains(".bf_history/ currently present"),
        "warning should flag the on-disk directory: {message}"
    );
}

#[test]
fn missing_ignore_file_with_no_directories_stays_quiet() {
    let workspace = workspace();
    std::fs::remove_file(workspace.path().join(".beads/.gitignore")).unwrap();

    let check = coverage_check(workspace.path());
    assert_eq!(check["status"], "ok");
    assert!(
        check["message"]
            .as_str()
            .unwrap()
            .contains("No runtime directories present"),
        "nothing on disk and nothing to verify: {:?}",
        check["message"]
    );
}

#[test]
fn missing_ignore_file_with_present_directory_warns() {
    let workspace = workspace();
    std::fs::remove_file(workspace.path().join(".beads/.gitignore")).unwrap();
    std::fs::create_dir_all(workspace.path().join(".beads/recovery")).unwrap();

    let check = coverage_check(workspace.path());
    assert_eq!(check["status"], "warning");
    let message = check["message"].as_str().unwrap();
    assert!(
        message.contains("no .beads/.gitignore ignores them"),
        "warning should name the missing ignore file: {message}"
    );
    assert!(
        message.contains("recovery/"),
        "warning should name the present directory: {message}"
    );
}

#[test]
fn root_gitignore_ignoring_beads_wholesale_stays_quiet() {
    let workspace = workspace();
    std::fs::write(
        workspace.path().join(".beads/.gitignore"),
        PRE_LEAK_GITIGNORE,
    )
    .unwrap();
    std::fs::write(workspace.path().join(".gitignore"), ".beads/\n").unwrap();

    let check = coverage_check(workspace.path());
    assert_eq!(check["status"], "ok");
    assert!(
        check["message"]
            .as_str()
            .unwrap()
            .contains("ignores .beads/ wholesale"),
        "a wholesale root rule leaves nothing for per-directory rules to do: {:?}",
        check["message"]
    );
}

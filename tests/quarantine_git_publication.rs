//! Recovery quarantine must stop every new Git publication.
//!
//! This is intentionally a synthetic, runtime-assembled credential fixture:
//! assertions inspect only the redacted quarantine reason, never the matched
//! bytes. The test keeps an unrelated staged path in the index so a refusal
//! proves that staging is truly a no-op, not merely that the checkpoint path
//! was skipped.

use assert_cmd::Command;
use rusqlite::params;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

fn bead(workspace: &Path) -> Command {
    let mut command = Command::cargo_bin("bead").unwrap();
    command
        .current_dir(workspace)
        .arg("--skip-foreign-workspace");
    command
}

fn git(workspace: &Path, args: &[&str]) -> std::process::Output {
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
    let output = git(workspace, args);
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn head(workspace: &Path) -> String {
    String::from_utf8_lossy(&git(workspace, &["rev-parse", "HEAD"]).stdout)
        .trim()
        .to_string()
}

fn index(workspace: &Path) -> String {
    String::from_utf8_lossy(&git(workspace, &["ls-files", "-s"]).stdout).to_string()
}

fn staged_paths(workspace: &Path) -> Vec<String> {
    String::from_utf8_lossy(&git(workspace, &["diff", "--cached", "--name-only"]).stdout)
        .lines()
        .map(str::to_string)
        .collect()
}

fn database(workspace: &Path) -> PathBuf {
    workspace.join(".beads/beads.db")
}

fn seed_secret_issue(workspace: &Path) -> String {
    // Keep the candidate out of argv, output, and assertion messages.
    let secret = ["AK", "IA", "7Q9W2E4R6T8Y1U3I"].concat();
    let conn = rusqlite::Connection::open(database(workspace)).unwrap();
    conn.execute(
        "INSERT INTO issues (
            id, title, description, notes, priority, issue_type, base_status,
            created_at, updated_at, revision
         ) VALUES ('quarantine-git', 'synthetic fixture', ?1, 'notes', 2,
                   'task', 'open', '2026-10-04T00:00:00Z',
                   '2026-10-04T00:00:00Z', 1)",
        params![secret],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO events (issue_id, kind, actor, time, detail)
         VALUES ('quarantine-git', 'synthetic_fixture', 'test',
                 '2026-10-04T00:00:00Z', '{}')",
        [],
    )
    .unwrap();
    secret
}

fn quarantine(workspace: &Path) {
    let conn = rusqlite::Connection::open(database(workspace)).unwrap();
    conn.execute(
        "INSERT INTO secret_quarantine
            (id, ruleset_version, blocking_count, coverage_incomplete)
         VALUES (1, 4, 1, 0)",
        [],
    )
    .unwrap();
}

#[test]
fn quarantine_withholds_staging_and_commit_until_sanitized_republish() {
    let directory = tempfile::tempdir().unwrap();
    let workspace = directory.path();
    git_ok(workspace, &["init", "-q"]);
    bead(workspace)
        .args(["init", "--prefix", "qgit", "--no-auto-flush"])
        .assert()
        .success();

    let secret = seed_secret_issue(workspace);
    bead(workspace)
        .args(["sync", "flush-only"])
        .assert()
        .success();
    git_ok(workspace, &["add", ".beads/checkpoint"]);
    git_ok(workspace, &["commit", "-q", "-m", "baseline checkpoint"]);
    let baseline_head = head(workspace);

    // This staged path belongs to another worker. A quarantine refusal must
    // preserve it byte-for-byte while also leaving HEAD untouched.
    fs::write(workspace.join("rival.txt"), "unrelated staged work\n").unwrap();
    git_ok(workspace, &["add", "rival.txt"]);
    let index_before = index(workspace);

    quarantine(workspace);
    let staging = bead_rs::service::git_stage::stage_published_checkpoint(
        workspace,
        &workspace.join(".beads/checkpoint"),
        &["current.json".to_string()],
        &[],
    )
    .expect_err("automatic staging must refuse during recovery quarantine");
    assert!(staging.contains("secret_quarantined"));
    assert!(!staging.contains(&secret));
    assert_eq!(index(workspace), index_before);
    assert_eq!(head(workspace), baseline_head);

    let refused = bead(workspace).args(["sync", "commit"]).output().unwrap();
    assert!(!refused.status.success());
    let refused_output = format!(
        "{}{}",
        String::from_utf8_lossy(&refused.stdout),
        String::from_utf8_lossy(&refused.stderr)
    );
    assert!(refused_output.contains("secret_quarantined"));
    assert!(!refused_output.contains(&secret));
    assert_eq!(index(workspace), index_before);
    assert_eq!(head(workspace), baseline_head);

    // --all-blocking performs the audited sanitized publication. Its output
    // is also required to be redacted, and the publisher stages only the
    // newly verified checkpoint fileset after clearing quarantine.
    let redacted = bead(workspace)
        .args([
            "redact",
            "--all-blocking",
            "--actor",
            "publication-test",
            "--reason",
            "remove synthetic fixture",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(
        redacted.status.success(),
        "{}",
        String::from_utf8_lossy(&redacted.stderr)
    );
    let redacted_output = format!(
        "{}{}",
        String::from_utf8_lossy(&redacted.stdout),
        String::from_utf8_lossy(&redacted.stderr)
    );
    assert!(!redacted_output.contains(&secret));
    let receipt: Value = serde_json::from_slice(&redacted.stdout).unwrap();
    assert!(receipt["receipts"].as_array().is_some_and(|receipts| {
        !receipts.is_empty() && receipts[0]["publication_state"] == "published"
    }));

    let conn = rusqlite::Connection::open(database(workspace)).unwrap();
    let quarantine_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM secret_quarantine", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(quarantine_count, 0);

    let committed = bead(workspace).args(["sync", "commit"]).output().unwrap();
    assert!(
        committed.status.success(),
        "{}",
        String::from_utf8_lossy(&committed.stderr)
    );
    assert_ne!(head(workspace), baseline_head);
    assert!(staged_paths(workspace).contains(&"rival.txt".to_string()));
}

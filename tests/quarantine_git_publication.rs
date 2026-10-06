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
use std::collections::BTreeMap;
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

fn checkpoint_contains(workspace: &Path, candidate: &str) -> bool {
    let checkpoint = workspace.join(".beads/checkpoint");
    let mut pending = vec![checkpoint];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = fs::read_dir(directory) else {
            return true;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().ends_with(".lock"))
            {
                continue;
            } else if fs::read(path).is_ok_and(|contents| {
                contents
                    .windows(candidate.len())
                    .any(|w| w == candidate.as_bytes())
            }) {
                return true;
            }
        }
    }
    false
}

fn checkpoint_files(workspace: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn collect(directory: &Path, root: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
        let entries = fs::read_dir(directory).expect("checkpoint directory should be readable");
        for entry in entries {
            let path = entry.expect("checkpoint entry").path();
            if path.is_dir() {
                collect(&path, root, files);
            } else if path
                .file_name()
                .is_some_and(|name| !name.to_string_lossy().ends_with(".lock"))
            {
                files.insert(
                    path.strip_prefix(root).unwrap().to_path_buf(),
                    fs::read(path).expect("checkpoint file should be readable"),
                );
            }
        }
    }

    let checkpoint = workspace.join(".beads/checkpoint");
    let mut files = BTreeMap::new();
    collect(&checkpoint, &checkpoint, &mut files);
    files
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

fn published_secret_source() -> (tempfile::TempDir, String, String) {
    let source = tempfile::tempdir().unwrap();
    bead(source.path())
        .args(["init", "--prefix", "qsrc", "--no-auto-flush"])
        .assert()
        .success();
    let secret = seed_secret_issue(source.path());
    let conn = rusqlite::Connection::open(database(source.path())).unwrap();
    let mut store = bead_rs::store::SqliteStore::from_conn(conn);
    let checkpoint_base = source.path().join(".beads");
    let config = bead_rs::service::load_checkpoint_config(&checkpoint_base).unwrap();
    bead_rs::service::publish_forensic_checkpoint(&mut store, &config, &checkpoint_base).unwrap();
    drop(store);
    let pointer: Value = serde_json::from_slice(
        &fs::read(source.path().join(".beads/checkpoint/current.json")).unwrap(),
    )
    .unwrap();
    (
        source,
        secret,
        pointer["generation_id"].as_str().unwrap().to_string(),
    )
}

fn initialized_git_target(prefix: &str) -> tempfile::TempDir {
    let target = tempfile::tempdir().unwrap();
    git_ok(target.path(), &["init", "-q"]);
    bead(target.path())
        .args(["init", "--prefix", prefix])
        .assert()
        .success();
    git_ok(target.path(), &["add", ".beads/checkpoint"]);
    git_ok(
        target.path(),
        &["commit", "-q", "-m", "baseline checkpoint"],
    );
    target
}

#[test]
fn secret_recovery_keeps_local_state_without_publishing_or_staging() {
    let (source, secret, generation) = published_secret_source();

    for operation in ["restore", "import-only"] {
        let target = initialized_git_target(if operation == "restore" {
            "qrestore"
        } else {
            "qimport"
        });
        let checkpoint_before = checkpoint_files(target.path());
        let index_before = index(target.path());
        let head_before = head(target.path());

        let output = if operation == "restore" {
            bead(target.path())
                .args([
                    "restore",
                    "--source",
                    source.path().join(".beads/checkpoint").to_str().unwrap(),
                    "--generation",
                    &generation,
                    "--actor",
                    "recovery-operator",
                    "--allow-non-empty",
                    "--format",
                    "json",
                ])
                .output()
                .unwrap()
        } else {
            bead(target.path())
                .args([
                    "sync",
                    "import-only",
                    "--input",
                    source.path().join(".beads/checkpoint").to_str().unwrap(),
                    "--restore-into-empty",
                    "--actor",
                    "recovery-operator",
                    "--format",
                    "json",
                ])
                .output()
                .unwrap()
        };

        assert!(
            output.status.success(),
            "{operation} should commit local recovery"
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let report: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["local_recovery_succeeded"], true);
        assert_eq!(report["secret_quarantined"], true);
        assert_eq!(report["checkpoint_publication_withheld"], true);
        assert!(stderr.contains("secret_quarantined"));
        assert!(!stdout.contains(&secret), "{operation} leaked to stdout");
        assert!(!stderr.contains(&secret), "{operation} leaked to stderr");

        let conn = rusqlite::Connection::open(database(target.path())).unwrap();
        let retained: String = conn
            .query_row(
                "SELECT description FROM issues WHERE id='quarantine-git'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            retained == secret,
            "{operation} did not retain local recovery state"
        );
        let held: i64 = conn
            .query_row("SELECT COUNT(*) FROM secret_quarantine", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(held, 1);

        assert_eq!(checkpoint_files(target.path()), checkpoint_before);
        assert_eq!(index(target.path()), index_before);
        assert_eq!(head(target.path()), head_before);
        assert!(!checkpoint_contains(target.path(), &secret));
    }
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

    bead(workspace)
        .args(["sync", "flush-only"])
        .assert()
        .success();
    let secret = seed_secret_issue(workspace);
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

#[test]
fn sync_commit_detects_a_new_finding_before_staging_and_commits_redaction_once() {
    let directory = tempfile::tempdir().unwrap();
    let workspace = directory.path();
    git_ok(workspace, &["init", "-q"]);
    bead(workspace)
        .args(["init", "--prefix", "qnew", "--no-auto-flush"])
        .assert()
        .success();
    bead(workspace)
        .args(["sync", "flush-only"])
        .assert()
        .success();
    git_ok(workspace, &["add", ".beads/checkpoint"]);
    git_ok(workspace, &["commit", "-q", "-m", "baseline checkpoint"]);

    let baseline_head = head(workspace);
    let baseline_index = index(workspace);
    let checkpoint_status = git(
        workspace,
        &[
            "status",
            "--porcelain",
            "--untracked-files=all",
            "--",
            ".beads/checkpoint",
        ],
    );
    assert!(checkpoint_status.status.success());
    let checkpoint_status = checkpoint_status.stdout;
    let candidate = seed_secret_issue(workspace);

    let dry_run = bead(workspace)
        .args(["sync", "commit", "--dry-run"])
        .output()
        .unwrap();
    assert!(!dry_run.status.success());
    let dry_run_output = format!(
        "{}{}",
        String::from_utf8_lossy(&dry_run.stdout),
        String::from_utf8_lossy(&dry_run.stderr)
    );
    assert!(!dry_run_output.contains(&candidate));
    assert_eq!(head(workspace), baseline_head);
    assert_eq!(index(workspace), baseline_index);

    // A quarantine-row failure rolls back the new hold and audit together.
    // This is a local recovery failure only; it cannot write the Git index or
    // checkpoint, and the source record remains available for a retry.
    let conn = rusqlite::Connection::open(database(workspace)).unwrap();
    conn.execute_batch(
        "CREATE TRIGGER fail_quarantine_insert
         BEFORE INSERT ON secret_quarantine
         BEGIN SELECT RAISE(ABORT, 'synthetic quarantine persistence failure'); END;",
    )
    .unwrap();
    let failed_scan = bead(workspace).args(["sync", "commit"]).output().unwrap();
    assert!(!failed_scan.status.success());
    let failed_scan_output = format!(
        "{}{}",
        String::from_utf8_lossy(&failed_scan.stdout),
        String::from_utf8_lossy(&failed_scan.stderr)
    );
    assert!(failed_scan_output.contains("synthetic quarantine persistence failure"));
    assert!(!failed_scan_output.contains(&candidate));
    let quarantine_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM secret_quarantine", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(quarantine_count, 0);
    let retained_description: String = conn
        .query_row(
            "SELECT description FROM issues WHERE id='quarantine-git'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(retained_description == candidate);
    assert_eq!(head(workspace), baseline_head);
    assert_eq!(index(workspace), baseline_index);
    let checkpoint_after_failure = git(
        workspace,
        &[
            "status",
            "--porcelain",
            "--untracked-files=all",
            "--",
            ".beads/checkpoint",
        ],
    );
    assert!(checkpoint_after_failure.status.success());
    assert_eq!(checkpoint_after_failure.stdout, checkpoint_status);
    conn.execute_batch("DROP TRIGGER fail_quarantine_insert")
        .unwrap();
    drop(conn);

    // The newly inserted local finding makes the checkpoint dirty. Commit
    // must discover and durably quarantine it before reaching the staging
    // code, leaving both the shared index and Git-trackable checkpoint as-is.
    let refused = bead(workspace).args(["sync", "commit"]).output().unwrap();
    assert!(!refused.status.success());
    let refused_output = format!(
        "{}{}",
        String::from_utf8_lossy(&refused.stdout),
        String::from_utf8_lossy(&refused.stderr)
    );
    assert!(refused_output.contains("secret_quarantined"));
    assert!(!refused_output.contains(&candidate));
    assert_eq!(head(workspace), baseline_head);
    assert_eq!(index(workspace), baseline_index);
    let checkpoint_after = git(
        workspace,
        &[
            "status",
            "--porcelain",
            "--untracked-files=all",
            "--",
            ".beads/checkpoint",
        ],
    );
    assert!(checkpoint_after.status.success());
    assert_eq!(checkpoint_after.stdout, checkpoint_status);
    assert!(!checkpoint_contains(workspace, &candidate));

    let status = bead(workspace)
        .args(["sync", "status", "--format", "json"])
        .output()
        .unwrap();
    assert!(status.status.success());
    let status_output = format!(
        "{}{}",
        String::from_utf8_lossy(&status.stdout),
        String::from_utf8_lossy(&status.stderr)
    );
    assert!(!status_output.contains(&candidate));
    assert!(status_output.contains("secret_quarantined"));

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
    assert!(!redacted_output.contains(&candidate));
    assert!(!checkpoint_contains(workspace, &candidate));

    let committed = bead(workspace).args(["sync", "commit"]).output().unwrap();
    assert!(
        committed.status.success(),
        "{}",
        String::from_utf8_lossy(&committed.stderr)
    );
    let committed_output = format!(
        "{}{}",
        String::from_utf8_lossy(&committed.stdout),
        String::from_utf8_lossy(&committed.stderr)
    );
    assert!(!committed_output.contains(&candidate));
    let resolved_head = head(workspace);
    assert_ne!(resolved_head, baseline_head);
    let resolved_index = index(workspace);

    // Retrying after the resolved generation is committed is a no-op. This
    // pins the split outcome to one sanitized publication commit exactly.
    let retry = bead(workspace).args(["sync", "commit"]).output().unwrap();
    assert!(retry.status.success());
    let retry_output = format!(
        "{}{}",
        String::from_utf8_lossy(&retry.stdout),
        String::from_utf8_lossy(&retry.stderr)
    );
    assert!(!retry_output.contains(&candidate));
    assert_eq!(head(workspace), resolved_head);
    assert_eq!(index(workspace), resolved_index);
}

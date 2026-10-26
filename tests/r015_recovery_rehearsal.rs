//! Integration tests for R015: Disposable recovery rehearsal.
//!
//! The previous version of this file never once invoked `bead doctor
//! --rehearse` or `run_recovery_rehearsal()`. Every test exercised generic
//! file-hashing/line-counting logic in isolation (unrelated to the actual
//! command), plus two literally-empty placeholder tests. All of that passed
//! while the real command failed unconditionally against any real
//! workspace -- it was built against the pre-forensic `.beads/issues.jsonl`
//! flat-file format and never updated when the checkpoint system moved to
//! `.beads/checkpoint/`. These tests replace that file entirely and
//! actually drive the real CLI command end to end.

use assert_cmd::Command;
use serial_test::serial;
use std::fs;
use std::path::Path;

/// Snapshot a directory as a depth-first, per-directory-sorted list of
/// `(relative path, contents)` pairs. The default checkpoint layout is
/// set-v2 (since 5d4c08a6): a `set/` tree plus a pointer, and no single
/// `forensic.jsonl` file -- so "the rehearsal did not mutate the live
/// checkpoint" has to hold over every published file.
fn snapshot_tree(root: &Path) -> Vec<(String, Vec<u8>)> {
    fn walk(dir: &Path, prefix: &str, out: &mut Vec<(String, Vec<u8>)>) {
        let mut entries: Vec<_> = fs::read_dir(dir)
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            let name = entry.file_name().to_str().unwrap().to_string();
            let relative = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            if entry.file_type().unwrap().is_dir() {
                walk(&path, &relative, out);
            } else {
                out.push((relative, fs::read(&path).unwrap()));
            }
        }
    }
    let mut out = Vec::new();
    walk(root, "", &mut out);
    out
}

/// Every record line published under the checkpoint's `set/` tree -- the
/// set-v2 default's record corpus (`set/shard/`, `set/seg/`, `set/rec/`,
/// and the open `set/tail.jsonl`).
fn set_tree_lines(checkpoint: &Path) -> Vec<String> {
    fn walk(dir: &Path, out: &mut Vec<String>) {
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if entry.file_type().unwrap().is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "jsonl") {
                for line in fs::read_to_string(&path).unwrap().lines() {
                    if !line.trim().is_empty() {
                        out.push(line.to_string());
                    }
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(&checkpoint.join("set"), &mut out);
    out
}

/// Build a workspace with content and a real flushed checkpoint.
fn populated_workspace() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--skip-foreign-workspace"])
        .current_dir(path)
        .env("HOME", path.to_str().unwrap())
        .assert()
        .success();

    for title in ["First bead", "Second bead", "Third bead"] {
        Command::cargo_bin("bead")
            .unwrap()
            .args(["create", "--title", title])
            .current_dir(path)
            .env("HOME", path.to_str().unwrap())
            .assert()
            .success();
    }

    // claim + close generates real events, matching how a live workspace
    // actually accumulates history before anyone rehearses recovery on it.
    // `close` is a claimant-owned mutation, so the fixture holds the epoch the
    // claim issued and presents it as the fencing credential.
    let claim_output = Command::cargo_bin("bead")
        .unwrap()
        .args(["claim", "--assignee", "fixture", "--json"])
        .current_dir(path)
        .env("HOME", path.to_str().unwrap())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let claim: serde_json::Value = serde_json::from_slice(&claim_output).unwrap();
    let epoch = claim["claim_epoch"].as_i64().unwrap().to_string();

    let list_output = Command::cargo_bin("bead")
        .unwrap()
        .args(["list", "--jsonl"])
        .current_dir(path)
        .env("HOME", path.to_str().unwrap())
        .output()
        .unwrap();
    let in_progress_id = String::from_utf8_lossy(&list_output.stdout)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .find(|v| v["status"] == "in_progress")
        .and_then(|v| v["id"].as_str().map(str::to_string))
        .expect("claim should have left one issue in_progress");

    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "close",
            &in_progress_id,
            "--reason",
            "fixture",
            "--fencing-token",
            &epoch,
        ])
        .current_dir(path)
        .env("HOME", path.to_str().unwrap())
        .assert()
        .success();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["sync", "flush-only"])
        .current_dir(path)
        .env("HOME", path.to_str().unwrap())
        .assert()
        .success();

    dir
}

#[test]
#[serial]
fn rehearse_succeeds_against_a_real_checkpoint() {
    let workspace = populated_workspace();

    let assert = Command::cargo_bin("bead")
        .unwrap()
        .args(["doctor", "--rehearse"])
        .current_dir(workspace.path())
        .env("HOME", workspace.path().to_str().unwrap())
        .assert()
        .success();

    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).to_string();
    assert!(
        stderr.contains("EQUIVALENT"),
        "expected the recovered state to be reported equivalent: {stderr}"
    );
    assert!(
        stderr.contains("Recovery rehearsal completed successfully"),
        "{stderr}"
    );
}

#[test]
#[serial]
fn rehearse_reports_correct_issue_and_event_counts() {
    let workspace = populated_workspace();

    let assert = Command::cargo_bin("bead")
        .unwrap()
        .args(["doctor", "--rehearse"])
        .current_dir(workspace.path())
        .env("HOME", workspace.path().to_str().unwrap())
        .assert()
        .success();

    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).to_string();
    assert!(
        stderr.contains("Original: 3 issues"),
        "original checkpoint should report 3 issues: {stderr}"
    );
    assert!(
        stderr.contains("Rehearsal: 3 issues"),
        "recovered+re-exported checkpoint should still report 3 issues: {stderr}"
    );
    // claim + close produced 2 events; the original checkpoint's event count
    // and the recovered-then-re-exported one must match exactly, or the
    // recovery path is silently dropping history.
    assert!(
        !stderr.contains("event_count_mismatch"),
        "recovery must not change the event count: {stderr}"
    );
}

#[test]
#[serial]
fn rehearse_fails_with_a_clear_message_when_no_checkpoint_exists() {
    // `bead init` normally publishes an initial generation; `--no-auto-flush`
    // leaves the workspace with no .beads/checkpoint/ content at all --
    // rehearse must then fail with a message naming the real problem, not a
    // generic "Internal error" (the exact bug this file's predecessor should
    // have, but never did, catch).
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--skip-foreign-workspace", "--no-auto-flush"])
        .current_dir(path)
        .env("HOME", path.to_str().unwrap())
        .assert()
        .success();

    let assert = Command::cargo_bin("bead")
        .unwrap()
        .args(["doctor", "--rehearse"])
        .current_dir(path)
        .env("HOME", path.to_str().unwrap())
        .assert()
        .failure();

    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).to_string();
    assert!(
        stderr.contains("No checkpoint found"),
        "error should name the real problem: {stderr}"
    );
}

#[test]
#[serial]
fn rehearse_does_not_mutate_the_live_workspace() {
    let workspace = populated_workspace();

    // The default layout publishes a `set/` tree plus pointer files (and a
    // `publish.lock`), so the immutability claim is checked over the whole
    // checkpoint directory, not one monolith file.
    let checkpoint_dir = workspace.path().join(".beads/checkpoint");
    let before = snapshot_tree(&checkpoint_dir);

    Command::cargo_bin("bead")
        .unwrap()
        .args(["doctor", "--rehearse"])
        .current_dir(workspace.path())
        .env("HOME", workspace.path().to_str().unwrap())
        .assert()
        .success();

    let after = snapshot_tree(&checkpoint_dir);
    assert_eq!(
        before, after,
        "rehearsal must not touch the live checkpoint tree"
    );

    let assert = Command::cargo_bin("bead")
        .unwrap()
        .arg("list")
        .current_dir(workspace.path())
        .env("HOME", workspace.path().to_str().unwrap())
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();
    for title in ["First bead", "Second bead", "Third bead"] {
        assert!(
            stdout.contains(title),
            "live workspace must be unchanged after rehearsal: missing {title}: {stdout}"
        );
    }
}

#[test]
#[serial]
fn rehearse_survives_a_checkpoint_containing_a_provenance_receipt() {
    // The exact scenario that broke a different way before the
    // SerializedReceipt/$schema fix landed: a checkpoint that itself
    // contains a receipt (because the workspace was restored once already).
    // Rehearsal must import that checkpoint (via the same
    // import_forensic_checkpoint path sync import-only uses) without
    // erroring.
    let source = populated_workspace();
    // The default layout is set-v2: no `forensic.jsonl` exists. `import-only`
    // reads the `current.json` pointer, the same checkpoint entry point the
    // rehearsal's own import uses.
    let source_checkpoint = source.path().join(".beads/checkpoint/current.json");

    let workspace = tempfile::tempdir().unwrap();
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--skip-foreign-workspace"])
        .current_dir(workspace.path())
        .env("HOME", workspace.path().to_str().unwrap())
        .assert()
        .success();
    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "sync",
            "import-only",
            "--input",
            source_checkpoint.to_str().unwrap(),
            "--restore-into-empty",
            "--actor",
            "r015-fixture",
        ])
        .current_dir(workspace.path())
        .env("HOME", workspace.path().to_str().unwrap())
        .assert()
        .success();
    // Flushing now produces a checkpoint that itself contains the receipt
    // the restore above just created. Under the set-v2 default the receipt
    // record lands in a `set/shard/` file, so assert on it there -- without
    // this the rehearsal below could pass without ever facing a receipt.
    Command::cargo_bin("bead")
        .unwrap()
        .args(["sync", "flush-only"])
        .current_dir(workspace.path())
        .env("HOME", workspace.path().to_str().unwrap())
        .assert()
        .success();
    let records = set_tree_lines(&workspace.path().join(".beads/checkpoint"));
    assert!(
        records
            .iter()
            .any(|line| line.contains("\"record_type\":\"provenance_receipt\"")),
        "fixture must actually publish a receipt record to exercise the scenario: {records:?}"
    );

    Command::cargo_bin("bead")
        .unwrap()
        .args(["doctor", "--rehearse"])
        .current_dir(workspace.path())
        .env("HOME", workspace.path().to_str().unwrap())
        .assert()
        .success();
}

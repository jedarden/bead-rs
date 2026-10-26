//! Independent reproduction of the lab split-history claim incident.
use assert_cmd::Command;
use rusqlite::Connection;
use serde_json::Value;
use std::{fs, path::Path};

fn bead(path: &Path) -> Command {
    let mut cmd = Command::cargo_bin("bead").unwrap();
    cmd.current_dir(path)
        .env("HOME", path)
        .arg("--skip-foreign-workspace");
    cmd
}

fn run(path: &Path, args: &[&str]) -> String {
    String::from_utf8(
        bead(path)
            .args(args)
            .assert()
            .success()
            .get_output()
            .stdout
            .clone(),
    )
    .unwrap()
}

fn copy(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            copy(&entry.path(), &to.join(entry.file_name()));
        } else {
            fs::copy(entry.path(), to.join(entry.file_name())).unwrap();
        }
    }
}

fn sequence(path: &Path) -> i64 {
    Connection::open(path.join(".beads/beads.db"))
        .unwrap()
        .query_row("SELECT MAX(sequence) FROM events", [], |row| row.get(0))
        .unwrap()
}

/// Every event record published under the checkpoint's `set/` tree. The
/// default layout since 5d4c08a6 is set-v2: the event log lives in `set/seg/`
/// segments and the open `set/tail.jsonl`, and no `forensic.jsonl` monolith
/// is written.
fn published_events(checkpoint: &Path) -> Vec<Value> {
    fn walk(dir: &Path, out: &mut Vec<Value>) {
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if entry.file_type().unwrap().is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "jsonl") {
                for line in fs::read_to_string(&path).unwrap().lines() {
                    if line.trim().is_empty() {
                        continue;
                    }
                    let record: Value = serde_json::from_str(line).unwrap();
                    if record.get("event").is_some() {
                        out.push(record);
                    }
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(&checkpoint.join("set"), &mut out);
    out
}

/// A hermetic `git` invocation: no global or system config leaks in, and
/// the identity never depends on the host.
fn git(dir: &Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .current_dir(dir)
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
        .expect("git should be runnable in tests");
    assert!(
        out.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
}

/// ADR-017: `ready_to_commit` is true only when the checkpoint is
/// internally consistent AND Git can reach every published file. These
/// fixtures run outside any repository, so give the workspace one and
/// commit the checkpoint right before asserting readiness. Idempotent:
/// an unchanged checkpoint commits nothing new (`--allow-empty`). Only
/// readiness assertions need this; internals-only assertions (dirty,
/// root_verified, coverage) are gate-independent.
fn commit_checkpoint(workspace: &Path) {
    if !workspace.join(".git").exists() {
        git(workspace, &["init", "-q"]);
    }
    git(workspace, &["add", ".beads/checkpoint"]);
    git(
        workspace,
        &["commit", "--allow-empty", "-q", "-m", "checkpoint"],
    );
}

#[test]
fn remote_advancement_blocks_claim_and_suppressed_writes_until_reconciled() {
    let dir = tempfile::tempdir().unwrap();
    let local = dir.path().join("local");
    let remote = dir.path().join("remote");
    fs::create_dir(&local).unwrap();
    run(&local, &["init"]);
    let id = run(&local, &["create", "--title", "shared task"]);
    copy(&local, &remote);
    run(
        &remote,
        &["close", id.trim(), "--reason", "remote completion"],
    );
    run(&remote, &["create", "--title", "new remote task"]);
    fs::remove_dir_all(local.join(".beads/checkpoint")).unwrap();
    copy(
        &remote.join(".beads/checkpoint"),
        &local.join(".beads/checkpoint"),
    );
    let before = sequence(&local);
    for args in [
        vec!["claim", "--assignee", "worker", "--json"],
        vec!["--no-auto-flush", "create", "--title", "unsafe"],
    ] {
        bead(&local).args(args).assert().code(4);
        assert_eq!(sequence(&local), before);
    }
    run(&local, &["sync", "reconcile", "--actor", "operator"]);
    let claimed: Value =
        serde_json::from_str(&run(&local, &["claim", "--assignee", "worker", "--json"])).unwrap();
    assert_ne!(claimed["bead_id"].as_str(), Some(id.trim()));
}

#[test]
fn divergent_histories_refuse_claim_close_and_flush_at_every_sequence_ordering() {
    for local_extra in [0, 1, 4] {
        let dir = tempfile::tempdir().unwrap();
        let local = dir.path().join("local");
        let remote = dir.path().join("remote");
        fs::create_dir(&local).unwrap();
        run(&local, &["init"]);
        let id = run(&local, &["create", "--title", "shared task"]);
        copy(&local, &remote);
        run(
            &remote,
            &["close", id.trim(), "--reason", "remote completion"],
        );
        run(&remote, &["create", "--title", "remote suffix"]);
        run(&local, &["create", "--title", "local suffix"]);
        for _ in 0..local_extra {
            run(&local, &["create", "--title", "local continuation"]);
        }
        fs::remove_dir_all(local.join(".beads/checkpoint")).unwrap();
        copy(
            &remote.join(".beads/checkpoint"),
            &local.join(".beads/checkpoint"),
        );
        let before = sequence(&local);
        let pointer = fs::read(local.join(".beads/checkpoint/current.json")).unwrap();
        for args in [
            vec!["claim", "--assignee", "worker", "--json"],
            vec![
                "--no-auto-flush",
                "close",
                id.trim(),
                "--reason",
                "unsafe close",
            ],
            vec!["sync", "flush-only"],
        ] {
            bead(&local).args(args).assert().code(5);
            assert_eq!(sequence(&local), before);
            assert_eq!(
                fs::read(local.join(".beads/checkpoint/current.json")).unwrap(),
                pointer
            );
        }
    }
}

#[test]
fn fork_preserves_the_published_prefix_and_allows_later_mutations() {
    let dir = tempfile::tempdir().unwrap();
    run(dir.path(), &["init"]);
    run(dir.path(), &["create", "--title", "parent task"]);
    let checkpoint = dir.path().join(".beads/checkpoint");
    let before = published_events(&checkpoint);
    assert!(!before.is_empty());
    run(dir.path(), &["sync", "fork", "--actor", "operator"]);
    run(dir.path(), &["create", "--title", "fork task"]);
    let after = published_events(&checkpoint);
    for event in before {
        assert!(after.contains(&event), "fork rewrote a published event");
    }
    // Reachability is part of readiness (ADR-017); commit the published
    // prefix before asserting it.
    commit_checkpoint(dir.path());
    let status: Value =
        serde_json::from_str(&run(dir.path(), &["sync", "status", "--format", "json"])).unwrap();
    assert_eq!(status["ready_to_commit"], true);
}

#[test]
fn concurrent_claims_remain_unique_and_published() {
    let dir = tempfile::tempdir().unwrap();
    run(dir.path(), &["init"]);
    for _ in 0..6 {
        run(dir.path(), &["create", "--title", "parallel task"]);
    }
    let children: Vec<_> = (0..6)
        .map(|n| {
            std::process::Command::new(env!("CARGO_BIN_EXE_bead"))
                .current_dir(dir.path())
                .env("HOME", dir.path())
                .args(["claim", "--assignee", &format!("worker-{n}"), "--json"])
                .stdout(std::process::Stdio::piped())
                .spawn()
                .unwrap()
        })
        .collect();
    let mut ids = std::collections::HashSet::new();
    let outputs: Vec<_> = children
        .into_iter()
        .map(|child| child.wait_with_output().unwrap())
        .collect();
    for output in outputs {
        assert!(output.status.success());
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(ids.insert(value["bead_id"].as_str().unwrap().to_string()));
    }
    // Reachability is part of readiness (ADR-017); commit the published
    // claims before asserting it.
    commit_checkpoint(dir.path());
    let status: Value =
        serde_json::from_str(&run(dir.path(), &["sync", "status", "--format", "json"])).unwrap();
    assert_eq!(status["ready_to_commit"], true);
}

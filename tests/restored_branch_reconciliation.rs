//! Contract: research/specs/restored-branch-reconciliation-v1.md.

use assert_cmd::Command;
use rusqlite::Connection;
use serde_json::Value;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

fn run(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::cargo_bin("bead")
        .unwrap()
        .current_dir(dir)
        .env("BEAD_ORG_SECRET_SCANNER", "off")
        .args(args)
        .assert()
        .success()
        .get_output()
        .clone()
}

fn create(dir: &Path, title: &str) -> String {
    String::from_utf8(run(dir, &["create", "--title", title]).stdout)
        .unwrap()
        .trim()
        .to_owned()
}

fn pointer(dir: &Path) -> Value {
    serde_json::from_slice(&fs::read(dir.join(".beads/checkpoint/current.json")).unwrap()).unwrap()
}

struct Pair {
    local: TempDir,
    remote: TempDir,
    shared: String,
    local_only: String,
    remote_only: String,
    boundary: String,
}

fn pair(mode: &str) -> Pair {
    let local = tempfile::tempdir().unwrap();
    let remote = tempfile::tempdir().unwrap();
    run(local.path(), &["init", "--prefix", "repair"]);
    let shared = create(local.path(), "shared issue");
    run(
        local.path(),
        &["label", "add", &shared, "--label", "stale-label"],
    );
    run(local.path(), &["sync", "configure", "--mode", mode]);
    let original = pointer(local.path());
    let boundary = (original["snapshot_sequence"].as_i64().unwrap() + 1).to_string();
    run(remote.path(), &["init", "--prefix", "repair"]);
    run(
        remote.path(),
        &[
            "restore",
            "--source",
            local.path().join(".beads/checkpoint").to_str().unwrap(),
            "--generation",
            original["generation_id"].as_str().unwrap(),
            "--actor",
            "restore-operator",
        ],
    );
    run(
        local.path(),
        &["label", "remove", &shared, "--label", "stale-label"],
    );
    run(
        local.path(),
        &["update", &shared, "--notes", "Keep these newer local notes"],
    );
    let local_only = create(local.path(), "local-only work");
    let remote_only = create(remote.path(), "remote-only work");
    run(
        remote.path(),
        &[
            "update",
            &remote_only,
            "--notes",
            "Retain the restored branch notes",
        ],
    );
    run(remote.path(), &["sync", "configure", "--mode", mode]);
    Pair {
        local,
        remote,
        shared,
        local_only,
        remote_only,
        boundary,
    }
}

fn repair(pair: &Pair, dry_run: bool) -> std::process::Output {
    let source = pair.remote.path().join(".beads/checkpoint");
    let p = pointer(pair.remote.path());
    let mut args = vec![
        "sync",
        "import-only",
        "--merge",
        "--input",
        source.to_str().unwrap(),
        "--source-generation",
        p["generation_id"].as_str().unwrap(),
        "--reidentify-restored-branch-at",
        &pair.boundary,
        "--actor",
        "repair-operator",
        "--format",
        "json",
    ];
    if dry_run {
        args.push("--dry-run");
    }
    Command::cargo_bin("bead")
        .unwrap()
        .current_dir(pair.local.path())
        .env("BEAD_ORG_SECRET_SCANNER", "off")
        .args(args)
        .output()
        .unwrap()
}

fn state(dir: &Path) -> Vec<u8> {
    let conn = Connection::open(dir.join(".beads/beads.db")).unwrap();
    let mut stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
        .unwrap();
    let names: Vec<String> = stmt
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    let mut result = String::new();
    for name in names {
        let mut stmt = conn
            .prepare(&format!("SELECT * FROM \"{name}\" ORDER BY rowid"))
            .unwrap();
        let columns = stmt.column_count();
        let rows = stmt
            .query_map([], |row| {
                Ok((0..columns)
                    .map(|i| format!("{:?}", row.get_ref(i).unwrap()))
                    .collect::<Vec<_>>())
            })
            .unwrap();
        for row in rows {
            result.push_str(&format!("{name}:{:?}\n", row.unwrap()));
        }
    }
    result.into_bytes()
}

fn success(output: &std::process::Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn both_checkpoint_modes_preserve_local_state_and_both_histories() {
    for mode in ["monolithic", "sharded"] {
        let pair = pair(mode);
        let before = state(pair.local.path());
        let before_pointer = pointer(pair.local.path());
        let dry = success(&repair(&pair, true));
        assert_eq!(dry["issues_inserted"], 1);
        assert_eq!(dry["events_imported"], 3);
        assert_eq!(state(pair.local.path()), before);
        assert_eq!(pointer(pair.local.path()), before_pointer);
        let report = success(&repair(&pair, false));
        assert_eq!(report["issues_inserted"], 1);
        run(pair.local.path(), &["doctor", "--scope", "store"]);
        let conn = Connection::open(pair.local.path().join(".beads/beads.db")).unwrap();
        let notes: String = conn
            .query_row(
                "SELECT notes FROM issues WHERE id = ?1",
                [&pair.shared],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(notes, "Keep these newer local notes");
        let labels: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM labels WHERE issue_id = ?1",
                [&pair.shared],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(labels, 0);
        for id in [&pair.local_only, &pair.remote_only] {
            let count: i64 = conn
                .query_row("SELECT COUNT(*) FROM issues WHERE id = ?1", [id], |r| {
                    r.get(0)
                })
                .unwrap();
            assert_eq!(count, 1);
        }
        let events: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM events WHERE origin_store_uuid = ?1",
                [report["branch_origin"].as_str().unwrap()],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(events, 3);
        let extension: String = conn.query_row("SELECT extensions_json FROM events WHERE origin_store_uuid = ?1 AND origin_event_sequence = 1", [report["branch_origin"].as_str().unwrap()], |r| r.get(0)).unwrap();
        let extension: Value = serde_json::from_str(&extension).unwrap();
        assert_eq!(
            extension["restored_branch_origin"]["origin_event_sequence"],
            pair.boundary.parse::<i64>().unwrap()
        );
        let restored = tempfile::tempdir().unwrap();
        run(restored.path(), &["init", "--prefix", "repair"]);
        let p = pointer(pair.local.path());
        run(
            restored.path(),
            &[
                "restore",
                "--source",
                pair.local
                    .path()
                    .join(".beads/checkpoint")
                    .to_str()
                    .unwrap(),
                "--generation",
                p["generation_id"].as_str().unwrap(),
                "--actor",
                "verifier",
            ],
        );
        run(
            pair.local.path(),
            &[
                "update",
                &pair.remote_only,
                "--notes",
                "Later local edit survives retry",
            ],
        );
        let before_retry = state(pair.local.path());
        assert_eq!(success(&repair(&pair, false))["already_merged"], true);
        assert_eq!(state(pair.local.path()), before_retry);
    }
}

#[test]
fn unsupported_existing_issue_edits_refuse_without_mutation() {
    let pair = pair("monolithic");
    run(
        pair.remote.path(),
        &["update", &pair.shared, "--notes", "Conflicting remote edit"],
    );
    let before = state(pair.local.path());
    for dry_run in [true, false] {
        let output = repair(&pair, dry_run);
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("edits an existing issue"));
        assert_eq!(state(pair.local.path()), before);
    }
}

#[test]
fn ordinary_merge_stays_strict_and_wrong_boundary_refuses() {
    let mut pair = pair("monolithic");
    let before = state(pair.local.path());
    Command::cargo_bin("bead")
        .unwrap()
        .current_dir(pair.local.path())
        .args([
            "sync",
            "import-only",
            "--merge",
            "--input",
            pair.remote
                .path()
                .join(".beads/checkpoint")
                .to_str()
                .unwrap(),
            "--actor",
            "operator",
        ])
        .assert()
        .failure();
    pair.boundary = (pair.boundary.parse::<i64>().unwrap() + 1).to_string();
    assert!(!repair(&pair, false).status.success());
    assert_eq!(state(pair.local.path()), before);
}

#[test]
fn tampered_source_is_rejected_before_native_changes() {
    let pair = pair("monolithic");
    let p = pointer(pair.remote.path());
    let root = pair
        .remote
        .path()
        .join(".beads/checkpoint")
        .join(p["active_root"]["path"].as_str().unwrap());
    let mut contents = fs::read(&root).unwrap();
    contents.push(b'\n');
    fs::write(root, contents).unwrap();
    let before = state(pair.local.path());
    assert!(!repair(&pair, false).status.success());
    assert_eq!(state(pair.local.path()), before);
}

#[test]
fn concurrent_retries_insert_one_branch_and_one_receipt() {
    let pair = pair("monolithic");
    std::thread::scope(|scope| {
        let first = scope.spawn(|| repair(&pair, false));
        let second = scope.spawn(|| repair(&pair, false));
        let reports = [
            success(&first.join().unwrap()),
            success(&second.join().unwrap()),
        ];
        assert_eq!(
            reports
                .iter()
                .filter(|r| r["already_merged"] == true)
                .count(),
            1
        );
        assert_eq!(
            reports.iter().filter(|r| r["issues_inserted"] == 1).count(),
            1
        );
    });
}

#[test]
fn repair_provenance_does_not_hide_an_unrelated_foreign_origin() {
    let pair = pair("monolithic");
    success(&repair(&pair, false));
    let conn = Connection::open(pair.local.path().join(".beads/beads.db")).unwrap();
    conn.execute(
        "INSERT INTO events (kind, actor, time, detail, origin_store_uuid, origin_event_sequence)
         VALUES ('checkpoint_imported', 'fixture', '2026-10-09T00:00:00Z', '{}', 'unrecorded-origin', 1)",
        [],
    ).unwrap();
    let result = Command::cargo_bin("bead")
        .unwrap()
        .current_dir(pair.local.path())
        .args(["doctor", "--scope", "store"])
        .output()
        .unwrap();
    assert!(!result.status.success());
    let diagnostic = format!(
        "{}{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(diagnostic.contains("UUID divergence"), "{diagnostic}");
}

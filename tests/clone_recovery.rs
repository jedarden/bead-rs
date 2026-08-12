// Regression tests for fresh-clone recovery.
//
// `.beads/config.json` is committed to git while `.beads/beads.db` is
// gitignored, so every fresh clone of a bead-rs workspace arrives with a
// workspace identity and no database. Two defects made that state
// unrecoverable:
//
//   1. `init` treated the presence of `config.json` as proof that the database
//      existed, then queried it and failed with a bare
//      "no such table: workspace". `doctor` failed the same way, so the repair
//      tool could not run.
//   2. `sync flush-only` exported every locally-created event with
//      `origin_store_uuid: ""` and `origin_event_sequence: 0`, giving them all
//      the identity ":0". Importing such a checkpoint aborted on the second
//      event with "duplicate event identity", so the committed checkpoint could
//      not restore the beads it was supposed to protect.
//
// Together these meant a clone could neither open the workspace nor recover its
// contents. These tests lock both fixes.

use assert_cmd::Command;
use serial_test::serial;
use std::path::Path;

/// Build a workspace with content, flush it, and return (tempdir, uuid).
fn populated_workspace() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path();

    Command::cargo_bin("bead")
        .unwrap()
        .arg("init")
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

    // `create` does not write to the events table, so claim and close twice to
    // generate multiple events. More than one event is essential here: a single
    // event cannot expose an identity collision, which is the defect these
    // tests exist to catch.
    for _ in 0..2 {
        Command::cargo_bin("bead")
            .unwrap()
            .args(["claim", "--assignee", "fixture"])
            .current_dir(path)
            .env("HOME", path.to_str().unwrap())
            .assert()
            .success();

        let claimed = in_progress_id(path).expect("claim should leave an issue in progress");

        Command::cargo_bin("bead")
            .unwrap()
            .args(["close", &claimed, "--reason", "fixture"])
            .current_dir(path)
            .env("HOME", path.to_str().unwrap())
            .assert()
            .success();
    }

    dir
}

/// Find the id of the issue currently in progress, if any.
fn in_progress_id(root: &Path) -> Option<String> {
    let output = Command::cargo_bin("bead")
        .unwrap()
        .args(["list", "--json"])
        .current_dir(root)
        .env("HOME", root.to_str().unwrap())
        .output()
        .unwrap();

    // `list --json` emits NDJSON: one object per line, not a JSON array.
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find(|issue| issue["status"] == "in_progress")
        .and_then(|issue| issue["id"].as_str().map(str::to_string))
}

fn read_config_uuid(root: &Path) -> String {
    let raw = std::fs::read_to_string(root.join(".beads/config.json")).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap();
    parsed["uuid"].as_str().unwrap().to_string()
}

/// Simulate `git clone`: keep the tracked files, drop the gitignored database.
fn simulate_fresh_clone(source: &Path, dest: &Path) {
    let src_beads = source.join(".beads");
    let dst_beads = dest.join(".beads");
    std::fs::create_dir_all(&dst_beads).unwrap();

    std::fs::copy(src_beads.join("config.json"), dst_beads.join("config.json")).unwrap();

    // Checkpoint files are not gitignored, so a clone carries them.
    let src_checkpoint = src_beads.join("checkpoint");
    if src_checkpoint.exists() {
        copy_tree(&src_checkpoint, &dst_beads.join("checkpoint"));
    }

    // beads.db, -shm and -wal are gitignored and must NOT be copied.
}

fn copy_tree(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).unwrap();
    for entry in std::fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let target = dst.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

#[test]
#[serial]
fn init_rebuilds_uninitialized_workspace_preserving_identity() {
    let source = populated_workspace();
    let clone = tempfile::tempdir().unwrap();
    simulate_fresh_clone(source.path(), clone.path());

    let expected_uuid = read_config_uuid(clone.path());
    assert!(
        !clone.path().join(".beads/beads.db").exists(),
        "clone must start without a database"
    );

    // init must repair rather than fail.
    Command::cargo_bin("bead")
        .unwrap()
        .arg("init")
        .current_dir(clone.path())
        .env("HOME", clone.path().to_str().unwrap())
        .assert()
        .success();

    assert!(
        clone.path().join(".beads/beads.db").exists(),
        "init must create the database"
    );

    // The committed identity must survive: it is what checkpoint records
    // reference as origin_store_uuid.
    assert_eq!(
        read_config_uuid(clone.path()),
        expected_uuid,
        "init must not mint a new UUID over a committed identity"
    );

    Command::cargo_bin("bead")
        .unwrap()
        .arg("list")
        .current_dir(clone.path())
        .env("HOME", clone.path().to_str().unwrap())
        .assert()
        .success();
}

#[test]
#[serial]
fn commands_report_actionable_error_before_repair() {
    let source = populated_workspace();
    let clone = tempfile::tempdir().unwrap();
    simulate_fresh_clone(source.path(), clone.path());

    // Before repair, a read command must explain the state and the remedy
    // rather than leaking "no such table: workspace".
    let assert = Command::cargo_bin("bead")
        .unwrap()
        .arg("list")
        .current_dir(clone.path())
        .env("HOME", clone.path().to_str().unwrap())
        .assert()
        .failure();

    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).to_string();
    assert!(
        !stderr.contains("no such table"),
        "raw SQLite error leaked to the operator: {stderr}"
    );
    assert!(
        stderr.contains("bead init"),
        "error must name the remedy: {stderr}"
    );
}

#[test]
#[serial]
fn doctor_runs_and_diagnoses_uninitialized_workspace() {
    let source = populated_workspace();
    let clone = tempfile::tempdir().unwrap();
    simulate_fresh_clone(source.path(), clone.path());

    // Doctor is the tool reached for when things are broken, so it must still
    // produce a diagnosis in this state instead of failing to start.
    let assert = Command::cargo_bin("bead")
        .unwrap()
        .arg("doctor")
        .current_dir(clone.path())
        .env("HOME", clone.path().to_str().unwrap())
        .assert()
        .failure();

    let output = assert.get_output();
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(
        combined.contains("workspace_config"),
        "doctor must emit a diagnostic, not just abort: {combined}"
    );
    assert!(
        combined.contains("bead init"),
        "doctor must name the repair: {combined}"
    );
}

#[test]
#[serial]
fn flushed_checkpoint_round_trips_through_import() {
    let source = populated_workspace();
    let path = source.path();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["sync", "flush-only"])
        .current_dir(path)
        .env("HOME", path.to_str().unwrap())
        .assert()
        .success();

    let checkpoint = path.join(".beads/checkpoint/forensic.jsonl");
    assert!(checkpoint.exists(), "flush must write a checkpoint");

    // Every event must carry a distinct identity. Before the fix they were all
    // ("", 0), which collides on the second event.
    let contents = std::fs::read_to_string(&checkpoint).unwrap();
    let mut identities = Vec::new();
    for line in contents.lines().filter(|l| !l.trim().is_empty()) {
        let record: serde_json::Value = serde_json::from_str(line).unwrap();
        if record["record_type"] == "event" {
            let event = &record["event"];
            let uuid = event["origin_store_uuid"].as_str().unwrap_or_default();
            let seq = event["origin_event_sequence"].as_i64().unwrap_or(0);
            assert!(
                !uuid.is_empty(),
                "event exported without an origin store uuid: {line}"
            );
            identities.push(format!("{uuid}:{seq}"));
        }
    }
    assert!(
        identities.len() >= 2,
        "fixture must produce multiple events to exercise identity collisions"
    );
    let mut unique = identities.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(
        unique.len(),
        identities.len(),
        "event identities must be unique, got {identities:?}"
    );

    // And the checkpoint must actually restore into an empty workspace.
    let restore = tempfile::tempdir().unwrap();
    let restore_path = restore.path();
    Command::cargo_bin("bead")
        .unwrap()
        .arg("init")
        .current_dir(restore_path)
        .env("HOME", restore_path.to_str().unwrap())
        .assert()
        .success();

    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "sync",
            "import-only",
            "--input",
            checkpoint.to_str().unwrap(),
            "--restore-into-empty",
            "--actor",
            "regression-test",
        ])
        .current_dir(restore_path)
        .env("HOME", restore_path.to_str().unwrap())
        .assert()
        .success();

    let assert = Command::cargo_bin("bead")
        .unwrap()
        .arg("list")
        .current_dir(restore_path)
        .env("HOME", restore_path.to_str().unwrap())
        .assert()
        .success();

    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();
    for title in ["First bead", "Second bead", "Third bead"] {
        assert!(
            stdout.contains(title),
            "restored workspace missing {title}: {stdout}"
        );
    }
}

/// A checkpoint that itself contains a provenance receipt (because the
/// workspace it was flushed from had already done a restore or merge) must
/// still be importable. `ProvenanceReceipt` (the write side, used by
/// `flush-only`) and `SerializedReceipt` (the read side, used by
/// `import-only`) are separate structs describing the same JSON shape, and
/// they disagreed on the schema field's wire name (`$schema` vs a literal
/// `schema_ref`) -- so any checkpoint containing a receipt could be flushed
/// but never re-imported, failing with "invalid receipt: missing field
/// `schema_ref`". No prior test exercised a receipt through this path at
/// all: every existing checkpoint fixture in this file contains only issues
/// and events. This is the regression test for that gap.
#[test]
#[serial]
fn restore_into_empty_survives_a_checkpoint_containing_a_provenance_receipt() {
    let source = populated_workspace();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["sync", "flush-only"])
        .current_dir(source.path())
        .env("HOME", source.path().to_str().unwrap())
        .assert()
        .success();
    let source_checkpoint = source.path().join(".beads/checkpoint/forensic.jsonl");

    // First restore: an ordinary receipt-free checkpoint into an empty
    // workspace. This is the scenario every other test in this file covers,
    // and it also leaves a provenance_receipt row in `middle`'s own store --
    // exactly the real-world situation this test exists to reproduce (e.g. a
    // workspace that was itself restored once before being flushed again).
    let middle = tempfile::tempdir().unwrap();
    Command::cargo_bin("bead")
        .unwrap()
        .arg("init")
        .current_dir(middle.path())
        .env("HOME", middle.path().to_str().unwrap())
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
            "regression-test",
        ])
        .current_dir(middle.path())
        .env("HOME", middle.path().to_str().unwrap())
        .assert()
        .success();

    // Flushing `middle` now produces a checkpoint that includes the
    // provenance_receipt the restore above just created.
    Command::cargo_bin("bead")
        .unwrap()
        .args(["sync", "flush-only"])
        .current_dir(middle.path())
        .env("HOME", middle.path().to_str().unwrap())
        .assert()
        .success();
    let middle_checkpoint = middle.path().join(".beads/checkpoint/forensic.jsonl");
    let contents = std::fs::read_to_string(&middle_checkpoint).unwrap();
    assert!(
        contents
            .lines()
            .any(|line| line.contains("\"record_type\":\"provenance_receipt\"")),
        "fixture must actually produce a receipt record to exercise the bug: {contents}"
    );

    // Second restore: into a fresh workspace, from a checkpoint that itself
    // contains a receipt. This is the exact step that failed before the fix.
    let restore = tempfile::tempdir().unwrap();
    Command::cargo_bin("bead")
        .unwrap()
        .arg("init")
        .current_dir(restore.path())
        .env("HOME", restore.path().to_str().unwrap())
        .assert()
        .success();
    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "sync",
            "import-only",
            "--input",
            middle_checkpoint.to_str().unwrap(),
            "--restore-into-empty",
            "--actor",
            "regression-test-2",
        ])
        .current_dir(restore.path())
        .env("HOME", restore.path().to_str().unwrap())
        .assert()
        .success();

    let assert = Command::cargo_bin("bead")
        .unwrap()
        .arg("list")
        .current_dir(restore.path())
        .env("HOME", restore.path().to_str().unwrap())
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();
    for title in ["First bead", "Second bead", "Third bead"] {
        assert!(
            stdout.contains(title),
            "restored workspace missing {title}: {stdout}"
        );
    }
}

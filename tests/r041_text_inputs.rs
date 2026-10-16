//! R041 issue text ergonomics: atomic note appends and file/stdin transports.

use assert_cmd::Command;
use bead_rs::service::update_issue_with_append;
use bead_rs::store::open_configured_connection;
use rusqlite::Connection;
use serde_json::Value;
use serial_test::serial;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Barrier};

fn command(root: &Path) -> Command {
    let mut command = Command::cargo_bin("bead").unwrap();
    command
        .current_dir(root)
        .env("BEAD_ORG_SECRET_SCANNER", "off")
        .args(["--skip-foreign-workspace", "--no-auto-flush"]);
    command
}

fn workspace() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    command(root.path()).arg("init").assert().success();
    root
}

fn create(root: &Path, title: &str) -> String {
    let output = command(root)
        .args(["create", "--title"])
        .arg(title)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn show(root: &Path, id: &str) -> Value {
    let output = command(root).args(["show", id, "--json"]).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn close_reason(root: &Path, id: &str) -> String {
    let conn = Connection::open(root.join(".beads/beads.db")).unwrap();
    conn.query_row(
        "SELECT close_reason FROM issues WHERE id = ?1",
        [id],
        |row| row.get(0),
    )
    .unwrap()
}

#[test]
#[serial]
fn append_and_hyphen_leading_values_are_stored_verbatim() {
    let root = workspace();
    let id = create(root.path(), "- title");

    command(root.path())
        .args(["update", &id, "--notes", "--- re-verify"])
        .assert()
        .success();
    command(root.path())
        .args(["update", &id, "--append-notes", "- item"])
        .assert()
        .success();

    let issue = show(root.path(), &id);
    assert_eq!(issue["title"], "- title");
    assert_eq!(issue["notes"], "--- re-verify\n\n- item");

    let closed = create(root.path(), "close me");
    command(root.path())
        .args(["close", &closed, "--reason", "- item"])
        .assert()
        .success();
    assert_eq!(close_reason(root.path(), &closed), "- item");
}

#[test]
#[serial]
fn file_and_stdin_sources_match_inline_values() {
    let root = workspace();
    let description_path = root.path().join("description.txt");
    std::fs::write(&description_path, "description from file\n").unwrap();
    let id = command(root.path())
        .args(["create", "--title", "file input", "--description-file"])
        .arg(&description_path)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let id = String::from_utf8(id).unwrap().trim().to_owned();
    assert_eq!(
        show(root.path(), &id)["description"],
        "description from file\n"
    );

    let notes_path = root.path().join("notes.txt");
    std::fs::write(&notes_path, "notes from file\n").unwrap();
    command(root.path())
        .args(["update", &id, "--notes-file"])
        .arg(&notes_path)
        .assert()
        .success();
    assert_eq!(show(root.path(), &id)["notes"], "notes from file\n");

    let stdin_append = command(root.path())
        .args(["update", &id, "--append-notes", "ignored"])
        .args(["--notes-file", "-"])
        .write_stdin("notes from stdin")
        .output()
        .unwrap();
    assert_eq!(stdin_append.status.code(), Some(2));

    let stdin_notes = command(root.path())
        .args(["update", &id, "--notes-file", "-"])
        .write_stdin("replacement from stdin")
        .output()
        .unwrap();
    assert!(stdin_notes.status.success());
    assert_eq!(show(root.path(), &id)["notes"], "replacement from stdin");

    let reason_path = root.path().join("reason.txt");
    std::fs::write(&reason_path, "reason from file\n").unwrap();
    command(root.path())
        .args(["close", &id, "--reason-file"])
        .arg(&reason_path)
        .assert()
        .success();
    assert_eq!(close_reason(root.path(), &id), "reason from file");
}

#[test]
#[serial]
fn multiple_text_sources_are_usage_errors() {
    let root = workspace();
    let id = create(root.path(), "source cardinality");
    let file = root.path().join("text.txt");
    std::fs::write(&file, "file").unwrap();

    let update = command(root.path())
        .args(["update", &id, "--notes", "inline", "--notes-file"])
        .arg(&file)
        .assert()
        .failure()
        .get_output()
        .clone();
    assert_eq!(update.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&update.stderr)
        .contains("exactly one of --notes, --append-notes, or --notes-file may be supplied"));

    let close = command(root.path())
        .args(["close", &id, "--reason", "inline", "--reason-file"])
        .arg(&file)
        .assert()
        .failure()
        .get_output()
        .clone();
    assert_eq!(close.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&close.stderr)
        .contains("exactly one of --reason or --reason-file is required"));

    let create = command(root.path())
        .args(["create", "--title", "duplicate", "--description", "inline"])
        .args(["--description-file"])
        .arg(&file)
        .assert()
        .failure()
        .get_output()
        .clone();
    assert_eq!(create.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&create.stderr)
        .contains("--description and --description-file are mutually exclusive"));
}

#[test]
#[serial]
fn two_connections_append_without_losing_either_value() {
    let root = workspace();
    let id = create(root.path(), "concurrent append");
    let db_path = root.path().join(".beads/beads.db");
    let barrier = Arc::new(Barrier::new(2));

    let first = append_on_connection(db_path.clone(), id.clone(), "first", barrier.clone());
    let second = append_on_connection(db_path, id.clone(), "second", barrier);
    let first = first.join().unwrap().unwrap();
    let second = second.join().unwrap().unwrap();
    assert_eq!(first, id);
    assert_eq!(second, first);

    let conn = open_configured_connection(&root.path().join(".beads/beads.db")).unwrap();
    let notes: String = conn
        .query_row("SELECT notes FROM issues WHERE id = ?1", [&first], |row| {
            row.get(0)
        })
        .unwrap();
    assert!(
        notes == "first\n\nsecond" || notes == "second\n\nfirst",
        "both appenders must survive: {notes:?}"
    );
}

fn append_on_connection(
    db_path: PathBuf,
    id: String,
    value: &'static str,
    barrier: Arc<Barrier>,
) -> std::thread::JoinHandle<bead_rs::Result<String>> {
    std::thread::spawn(move || {
        let conn = open_configured_connection(&db_path).unwrap();
        barrier.wait();
        update_issue_with_append(&conn, &id, None, None, false, None, Some(value), None, None)
    })
}

#[test]
#[serial]
fn append_honors_revision_and_claim_credentials() {
    let root = workspace();
    let id = create(root.path(), "guards");
    command(root.path())
        .args([
            "update",
            &id,
            "--append-notes",
            "first",
            "--if-revision",
            "1",
        ])
        .assert()
        .success();
    command(root.path())
        .args([
            "update",
            &id,
            "--append-notes",
            "stale",
            "--if-revision",
            "1",
        ])
        .assert()
        .failure()
        .code(4);
    assert_eq!(show(root.path(), &id)["notes"], "first");

    command(root.path())
        .args(["update", &id, "--assignee", "worker"])
        .assert()
        .success();
    let credential = show(root.path(), &id)["claim_epoch"]
        .as_i64()
        .unwrap()
        .to_string();
    command(root.path())
        .args(["update", &id, "--append-notes", "without token"])
        .assert()
        .failure()
        .code(4);
    command(root.path())
        .args([
            "update",
            &id,
            "--append-notes",
            "with token",
            "--fencing-token",
            &credential,
        ])
        .assert()
        .success();
    assert_eq!(show(root.path(), &id)["notes"], "first\n\nwith token");
}

#[test]
#[serial]
fn blocked_secret_in_append_leaves_the_issue_unchanged() {
    let root = tempfile::tempdir().unwrap();
    let init = Command::cargo_bin("bead")
        .unwrap()
        .current_dir(root.path())
        .args(["init", "--skip-foreign-workspace", "--no-auto-flush"])
        .assert()
        .success();
    drop(init);
    let id = {
        let output = Command::cargo_bin("bead")
            .unwrap()
            .current_dir(root.path())
            .args(["create", "--title", "secret test", "--no-auto-flush"])
            .output()
            .unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    };
    command(root.path())
        .args(["update", &id, "--notes", "safe"])
        .assert()
        .success();

    let blocked = ["AK", "IA", "7M4Q9Z2N8C5R3T6V"].concat();
    let result = Command::cargo_bin("bead")
        .unwrap()
        .current_dir(root.path())
        .args(["update", &id, "--append-notes"])
        .arg(&blocked)
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&result.stderr).contains("secret_detected"));
    assert_eq!(show(root.path(), &id)["notes"], "safe");
}

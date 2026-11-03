//! R042.1 conformance for the close-record read projection.

use assert_cmd::Command;
use serde_json::{json, Value};
use tempfile::TempDir;

fn bead(root: &std::path::Path) -> Command {
    let mut command = Command::cargo_bin("bead").unwrap();
    command.current_dir(root);
    command
}

fn workspace() -> TempDir {
    let root = tempfile::tempdir().unwrap();
    bead(root.path())
        .args(["init", "--skip-foreign-workspace", "--prefix", "r042"])
        .assert()
        .success();
    root
}

fn create(root: &std::path::Path, title: &str) -> String {
    let output = bead(root)
        .args(["create", "--title", title])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    String::from_utf8(output).unwrap().trim().to_owned()
}

fn json(root: &std::path::Path, args: &[&str]) -> Value {
    let output = bead(root).args(args).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn assert_close_record(document: &Value, reason: &str) {
    assert!(document["closed_at"].is_string(), "{document}");
    assert_eq!(document["closed_by"], "system");
    assert_eq!(document["close_reason"], reason);
}

#[test]
fn close_record_is_projected_on_show_list_and_why_and_reopen_clears_it() {
    let root = workspace();
    let id = create(root.path(), "close record");

    let never_closed = json(root.path(), &["show", &id, "--json"]);
    assert_eq!(never_closed["closed_at"], Value::Null);
    assert_eq!(never_closed["closed_by"], Value::Null);
    assert_eq!(never_closed["close_reason"], Value::Null);

    bead(root.path())
        .args(["close", &id, "--reason", "first close"])
        .assert()
        .success();

    let shown = json(root.path(), &["show", &id, "--json"]);
    assert_close_record(&shown, "first close");

    let listed = json(root.path(), &["list", "--json"]);
    let listed = listed
        .as_array()
        .unwrap()
        .iter()
        .find(|document| document["id"] == id)
        .unwrap();
    assert_close_record(listed, "first close");

    let selected = json(
        root.path(),
        &[
            "list",
            "--json",
            "--fields",
            "closed_at,closed_by,close_reason",
        ],
    );
    assert_eq!(selected[0].get("close_reason"), Some(&json!("first close")));

    let why = json(root.path(), &["why", "--id", &id, "--json"]);
    assert_close_record(&why, "first close");

    let text = bead(root.path())
        .args(["show", &id])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(text).unwrap();
    assert!(text.contains("Closed: "));
    assert!(text.contains(" by system"));
    assert!(text.contains("Reason: first close"));

    bead(root.path())
        .args(["reopen", &id])
        .assert()
        .success();
    for args in [
        vec!["show", &id, "--json"],
        vec!["why", "--id", &id, "--json"],
    ] {
        let document = json(root.path(), &args);
        assert_eq!(document["closed_at"], Value::Null, "{document}");
        assert_eq!(document["closed_by"], Value::Null, "{document}");
        assert_eq!(document["close_reason"], Value::Null, "{document}");
    }

    bead(root.path())
        .args(["close", &id, "--reason", "second close"])
        .assert()
        .success();
    let second = json(root.path(), &["show", &id, "--json"]);
    assert_close_record(&second, "second close");
}

#[test]
fn redacted_close_reason_is_rendered_as_the_fixed_marker() {
    let root = workspace();
    let id = create(root.path(), "redacted close record");
    bead(root.path())
        .args(["close", &id, "--reason", "secret close reason"])
        .assert()
        .success();

    let connection = rusqlite::Connection::open(root.path().join(".beads/beads.db")).unwrap();
    connection
        .execute(
            "UPDATE issues SET close_reason = ?1 WHERE id = ?2",
            ["[REDACTED:bead-rs]", &id],
        )
        .unwrap();

    let expected = "[REDACTED:bead-rs]";
    assert_eq!(json(root.path(), &["show", &id, "--json"])["close_reason"], expected);
    assert_eq!(
        json(root.path(), &["list", "--json"])[0]["close_reason"],
        expected
    );
    assert_eq!(json(root.path(), &["why", "--id", &id, "--json"])["close_reason"], expected);

    let text = bead(root.path())
        .args(["show", &id])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert!(String::from_utf8(text)
        .unwrap()
        .contains("Reason: [REDACTED:bead-rs]"));
}

#[test]
fn close_record_capability_schema_and_help_are_advertised() {
    let root = workspace();
    for profile in ["native-v1", "needle-v1"] {
        let capabilities = json(root.path(), &["capabilities", "--profile", profile]);
        assert_eq!(capabilities["close_record"], true);
    }

    let guide = json(
        root.path(),
        &[
            "schema",
            "explain",
            "urn:bead-rs:schema:issue:native-v1",
            "--format",
            "json",
        ],
    );
    let members = guide["documents"]
        .as_array()
        .unwrap()
        .iter()
        .find(|document| document["name"] == "cli_issue")
        .unwrap()["members"]
        .as_array()
        .unwrap();
    for name in ["closed_at", "closed_by", "close_reason"] {
        assert!(members.iter().any(|member| member == name), "{name}");
    }

    for args in [
        vec!["list", "--help"],
        vec!["show", "--help"],
        vec!["why", "--help"],
    ] {
        let output = bead(root.path()).args(&args).output().unwrap();
        assert!(output.status.success());
        let help = String::from_utf8(output.stdout).unwrap();
        assert!(help.contains("close"), "{args:?}: {help}");
    }
}

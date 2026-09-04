//! End-to-end coverage for the secret-rejection contract at the CLI boundary.

use assert_cmd::Command;
use rusqlite::Connection;
use serde_json::Value;
use std::path::Path;

fn workspace() -> tempfile::TempDir {
    let workspace = tempfile::Builder::new()
        .prefix("bead-secret-rejection-")
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

fn provider_shaped_value() -> String {
    let mut value = ["AK", "IA"].concat();
    value.push_str("7M4Q9Z2N8C5R3T6V");
    value
}

fn counts(root: &Path) -> (i64, i64) {
    let conn = Connection::open(root.join(".beads/beads.db")).unwrap();
    let issues = conn
        .query_row("SELECT COUNT(*) FROM issues", [], |row| row.get(0))
        .unwrap();
    let events = conn
        .query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))
        .unwrap();
    (issues, events)
}

fn fingerprint(stderr: &str) -> String {
    stderr
        .split_whitespace()
        .map(|word| word.trim_matches(|character: char| !character.is_ascii_hexdigit()))
        .find(|word| {
            word.len() == 64
                && word
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        })
        .expect("redacted diagnostic carries an exact finding fingerprint")
        .to_string()
}

#[test]
fn blocking_finding_is_atomic_redacted_and_exactly_acknowledgeable() {
    let workspace = workspace();
    let value = provider_shaped_value();
    let before = counts(workspace.path());

    let rejected = Command::cargo_bin("bead")
        .unwrap()
        .current_dir(workspace.path())
        .args(["create", "--title", "safe title", "--description"])
        .arg(&value)
        .arg("--no-auto-flush")
        .output()
        .unwrap();
    assert_eq!(rejected.status.code(), Some(2));
    assert_eq!(counts(workspace.path()), before);
    let stderr = String::from_utf8(rejected.stderr).unwrap();
    let stdout = String::from_utf8(rejected.stdout).unwrap();
    assert!(stderr.contains("secret_detected"));
    assert!(!stderr.contains(&value));
    assert!(!stdout.contains(&value));
    let finding_fingerprint = fingerprint(&stderr);

    let admitted = Command::cargo_bin("bead")
        .unwrap()
        .current_dir(workspace.path())
        .args(["create", "--title", "safe title", "--description"])
        .arg(&value)
        .args([
            "--acknowledge-secret",
            &finding_fingerprint,
            "--no-auto-flush",
        ])
        .output()
        .unwrap();
    assert!(admitted.status.success());
    assert!(!String::from_utf8(admitted.stderr).unwrap().contains(&value));

    let conn = Connection::open(workspace.path().join(".beads/beads.db")).unwrap();
    let audit_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM events WHERE kind = 'secret_acknowledged'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(audit_count, 1);
    let (actor, detail): (String, String) = conn
        .query_row(
            "SELECT actor, detail FROM events WHERE kind = 'secret_acknowledged'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(actor, "cli");
    assert!(detail.contains(&finding_fingerprint));
    assert!(detail.contains("description"));
    assert!(!detail.contains(&value));
}

#[test]
fn malformed_policy_fails_closed_without_echoing_its_value() {
    let workspace = workspace();
    let config_path = workspace.path().join(".beads/config.json");
    let mut config: Value = serde_json::from_slice(&std::fs::read(&config_path).unwrap()).unwrap();
    let invalid_mode = "unexpected-policy-value";
    config["secret_scan"] = serde_json::json!({"mode": invalid_mode});
    std::fs::write(&config_path, serde_json::to_vec_pretty(&config).unwrap()).unwrap();
    let before = counts(workspace.path());

    let output = Command::cargo_bin("bead")
        .unwrap()
        .current_dir(workspace.path())
        .args(["create", "--title", "safe title", "--no-auto-flush"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(counts(workspace.path()), before);
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("secret_scan.mode"));
    assert!(!stderr.contains(invalid_mode));
}

#[test]
fn advisory_and_off_workspace_modes_do_not_reject() {
    for mode in ["advisory", "off"] {
        let workspace = workspace();
        let config_path = workspace.path().join(".beads/config.json");
        let mut config: Value =
            serde_json::from_slice(&std::fs::read(&config_path).unwrap()).unwrap();
        config["secret_scan"] = serde_json::json!({"mode": mode});
        std::fs::write(&config_path, serde_json::to_vec_pretty(&config).unwrap()).unwrap();

        Command::cargo_bin("bead")
            .unwrap()
            .current_dir(workspace.path())
            .args(["create", "--title", "safe title", "--description"])
            .arg(provider_shaped_value())
            .arg("--no-auto-flush")
            .assert()
            .success();
        assert_eq!(counts(workspace.path()).0, 1);
    }
}

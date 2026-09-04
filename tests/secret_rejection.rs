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

fn aws_secret_access_key_assignment() -> String {
    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let value: String = (0..40)
        .map(|index| alphabet[(index * 11 + 7) % alphabet.len()] as char)
        .collect();
    format!("BEDROCK_AWS_SECRET_ACCESS_KEY={value}")
}

fn garage_access_key_id_assignment() -> String {
    let value = [["G", "K"].concat(), "7e4a19c2b6d83f501ac942".to_string()].concat();
    format!("SCCACHE_AWS_ACCESS_KEY_ID={value}")
}

fn placeholder_shaped_value() -> String {
    let mut value = ["AK", "IA"].concat();
    value.push_str(&"A".repeat(16));
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
fn aws_secret_access_key_assignment_rejects_atomically_without_disclosure() {
    let workspace = workspace();
    let assignment = aws_secret_access_key_assignment();
    let before = counts(workspace.path());

    let rejected = Command::cargo_bin("bead")
        .unwrap()
        .current_dir(workspace.path())
        .args(["create", "--title", "safe title", "--description"])
        .arg(&assignment)
        .arg("--no-auto-flush")
        .output()
        .unwrap();

    assert_eq!(rejected.status.code(), Some(2));
    assert_eq!(counts(workspace.path()), before);
    let stderr = String::from_utf8(rejected.stderr).unwrap();
    let stdout = String::from_utf8(rejected.stdout).unwrap();
    assert!(stderr.contains("aws-secret-access-key-assignment"));
    assert!(!stderr.contains(&assignment));
    assert!(!stdout.contains(&assignment));
}

#[test]
fn garage_access_key_id_assignment_rejects_atomically_without_disclosure() {
    let workspace = workspace();
    let assignment = garage_access_key_id_assignment();
    let before = counts(workspace.path());

    let rejected = Command::cargo_bin("bead")
        .unwrap()
        .current_dir(workspace.path())
        .args(["create", "--title", "safe title", "--description"])
        .arg(&assignment)
        .arg("--no-auto-flush")
        .output()
        .unwrap();

    assert_eq!(rejected.status.code(), Some(2));
    assert_eq!(counts(workspace.path()), before);
    let stderr = String::from_utf8(rejected.stderr).unwrap();
    let stdout = String::from_utf8(rejected.stdout).unwrap();
    assert!(stderr.contains("garage-access-key-id-assignment"));
    assert!(!stderr.contains(&assignment));
    assert!(!stdout.contains(&assignment));
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

#[test]
fn successful_dry_run_reports_redacted_nonblocking_findings() {
    let workspace = workspace();
    let created = Command::cargo_bin("bead")
        .unwrap()
        .current_dir(workspace.path())
        .args(["create", "--title", "dry-run target", "--no-auto-flush"])
        .output()
        .unwrap();
    assert!(created.status.success());
    let issue_id = String::from_utf8(created.stdout).unwrap();
    let value = placeholder_shaped_value();
    let before = counts(workspace.path());

    let output = Command::cargo_bin("bead")
        .unwrap()
        .current_dir(workspace.path())
        .args(["update", issue_id.trim(), "--notes"])
        .arg(&value)
        .args(["--dry-run", "--no-auto-flush"])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert_eq!(counts(workspace.path()), before);
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("secret_scan dry-run:"));
    assert!(stderr.contains("disposition placeholder"));
    assert!(!stdout.contains(&value));
    assert!(!stderr.contains(&value));
}

#[test]
fn manifest_is_scanned_as_one_request_before_its_transaction() {
    let workspace = workspace();
    let value = provider_shaped_value();
    let manifest_path = workspace.path().join("manifest.json");
    let manifest = serde_json::json!({
        "manifest_version": 1,
        "operations": [
            {
                "op": "create",
                "local_id": "first",
                "title": "safe title",
                "description": value.clone()
            },
            {
                "op": "create",
                "title": "must not partially commit"
            }
        ]
    });
    std::fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    let before = counts(workspace.path());

    let rejected = Command::cargo_bin("bead")
        .unwrap()
        .current_dir(workspace.path())
        .args(["manifest", "commit", "--input"])
        .arg(&manifest_path)
        .arg("--no-auto-flush")
        .output()
        .unwrap();
    assert_eq!(rejected.status.code(), Some(2));
    assert_eq!(counts(workspace.path()), before);
    let stderr = String::from_utf8(rejected.stderr).unwrap();
    assert!(!stderr.contains(&value));
    let finding_fingerprint = fingerprint(&stderr);

    Command::cargo_bin("bead")
        .unwrap()
        .current_dir(workspace.path())
        .args(["manifest", "commit", "--input"])
        .arg(&manifest_path)
        .args([
            "--acknowledge-secret",
            &finding_fingerprint,
            "--no-auto-flush",
        ])
        .assert()
        .success();
    assert_eq!(counts(workspace.path()).0, 2);
    let conn = Connection::open(workspace.path().join(".beads/beads.db")).unwrap();
    let audits: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM events WHERE kind = 'secret_acknowledged'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(audits, 1);
}

#[test]
fn doctor_reports_live_and_both_retained_generations_without_matched_bytes() {
    let workspace = tempfile::Builder::new()
        .prefix("bead-secret-doctor-")
        .tempdir_in("/var/tmp")
        .unwrap();
    Command::cargo_bin("bead")
        .unwrap()
        .current_dir(workspace.path())
        .arg("init")
        .assert()
        .success();
    let value = provider_shaped_value();
    let rejected = Command::cargo_bin("bead")
        .unwrap()
        .current_dir(workspace.path())
        .args(["create", "--title", "safe title", "--description"])
        .arg(&value)
        .output()
        .unwrap();
    let finding_fingerprint = fingerprint(&String::from_utf8(rejected.stderr).unwrap());

    let created = Command::cargo_bin("bead")
        .unwrap()
        .current_dir(workspace.path())
        .args(["create", "--title", "safe title", "--description"])
        .arg(&value)
        .args(["--acknowledge-secret", &finding_fingerprint])
        .output()
        .unwrap();
    assert!(created.status.success());
    let issue_id = String::from_utf8(created.stdout).unwrap();
    let issue_id = issue_id.trim();
    Command::cargo_bin("bead")
        .unwrap()
        .current_dir(workspace.path())
        .args(["label", "add", issue_id, "--label", "safe"])
        .assert()
        .success();

    let output = Command::cargo_bin("bead")
        .unwrap()
        .current_dir(workspace.path())
        .args(["doctor", "--scope", "secrets", "--format", "json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(!String::from_utf8(output.stderr).unwrap().contains(&value));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(!stdout.contains(&value));
    let diagnostics: Value = serde_json::from_str(&stdout).unwrap();
    let check = diagnostics["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|check| check["name"] == "secret_scan")
        .unwrap();
    assert_eq!(check["status"], "warning");
    assert_eq!(
        check["details"]["checkpoint_generations_scanned"],
        serde_json::json!(["current", "previous"])
    );
    let findings = check["details"]["findings"].as_array().unwrap();
    let live_issue_selector = findings
        .iter()
        .filter_map(|finding| finding["selector"].as_str())
        .find(|selector| selector.starts_with("live:issues:"))
        .expect("live issue finding has a stable semantic selector");
    let selector_digest = live_issue_selector.rsplit(':').next().unwrap();
    assert_eq!(selector_digest.len(), 64);
    assert!(selector_digest.bytes().all(|byte| byte.is_ascii_hexdigit()));
    assert!(findings.iter().any(|finding| finding["selector"]
        .as_str()
        .is_some_and(|selector| selector.starts_with("checkpoint:current:"))));
    assert!(findings.iter().any(|finding| finding["selector"]
        .as_str()
        .is_some_and(|selector| selector.starts_with("checkpoint:previous:"))));
}

#[test]
fn recovery_reports_legacy_findings_without_refusing_import() {
    let source = workspace();
    let config_path = source.path().join(".beads/config.json");
    let mut config: Value = serde_json::from_slice(&std::fs::read(&config_path).unwrap()).unwrap();
    config["secret_scan"] = serde_json::json!({"mode": "off"});
    std::fs::write(&config_path, serde_json::to_vec_pretty(&config).unwrap()).unwrap();
    let value = provider_shaped_value();

    Command::cargo_bin("bead")
        .unwrap()
        .current_dir(source.path())
        .args(["create", "--title", "legacy record", "--description"])
        .arg(&value)
        .assert()
        .success();

    let target = workspace();
    let input = source.path().join(".beads/checkpoint/forensic.jsonl");
    let output = Command::cargo_bin("bead")
        .unwrap()
        .current_dir(target.path())
        .args(["sync", "import-only", "--input"])
        .arg(&input)
        .args([
            "--restore-into-empty",
            "--actor",
            "recovery-test",
            "--dry-run",
            "--no-auto-flush",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("secret_scan recovery:"));
    assert!(!stdout.contains(&value));
    assert!(!stderr.contains(&value));
    assert_eq!(counts(target.path()).0, 0);
}

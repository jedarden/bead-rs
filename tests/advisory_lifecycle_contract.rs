//! Value-free summary and semantic lifecycle witnesses (ruleset-v4 5.2).
use assert_cmd::Command;
use bead_rs::scan::{self, Disposition, Finding, ScanReport, Tier};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use std::process::Output;

fn command(root: &Path) -> Command {
    let mut command = Command::cargo_bin("bead").unwrap();
    command
        .current_dir(root)
        .env("BEAD_ORG_SECRET_SCANNER", "off");
    command
}

fn workspace() -> tempfile::TempDir {
    let root = tempfile::Builder::new()
        .prefix("advisory-contract-")
        .tempdir_in("/var/tmp")
        .unwrap();
    // Fence discovery before the first CLI invocation.
    fs::create_dir(root.path().join(".beads")).unwrap();
    fs::write(
        root.path().join(".beads/config.json"),
        serde_json::to_vec(&json!({
            "version":1, "uuid":uuid::Uuid::new_v4().to_string(), "prefix":"notice"
        }))
        .unwrap(),
    )
    .unwrap();
    command(root.path())
        .args(["init", "--no-auto-flush"])
        .assert()
        .success();
    root
}

fn manifest(root: &Path, operations: Value) {
    fs::write(
        root.join("manifest.json"),
        serde_json::to_vec(&json!({"manifest_version":1,"operations":operations})).unwrap(),
    )
    .unwrap();
}

fn commit(root: &Path, verb: &str, suppress_flush: bool) -> Output {
    let mut invocation = command(root);
    invocation.args([
        "manifest",
        verb,
        "--input",
        "manifest.json",
        "--format",
        "json",
    ]);
    if suppress_flush {
        invocation.arg("--no-auto-flush");
    }
    invocation.output().unwrap()
}

fn row_count(root: &Path) -> i64 {
    Connection::open(root.join(".beads/beads.db"))
        .unwrap()
        .query_row("SELECT COUNT(*) FROM issues", [], |row| row.get(0))
        .unwrap()
}

fn notices(output: &Output) -> Vec<String> {
    String::from_utf8(output.stderr.clone())
        .unwrap()
        .split_inclusive('\n')
        .filter(|line| line.starts_with("secret_scan advisory:"))
        .map(str::to_owned)
        .collect()
}

fn assert_summary(output: &Output) -> Value {
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    let summary = &result["secret_scan"];
    let count = summary["advisory_findings"].as_u64().unwrap();
    assert!(count > 0);
    let rules: Vec<_> = summary["rules"]
        .as_array()
        .unwrap()
        .iter()
        .map(|rule| rule.as_str().unwrap())
        .collect();
    assert!(rules.windows(2).all(|pair| pair[0] < pair[1]));
    assert_eq!(summary.as_object().unwrap().len(), 2);
    assert_eq!(notices(output), vec![format!("secret_scan advisory: {count} finding(s), rules {}; inspect bead doctor --scope secrets. Matched bytes are not shown.\n", rules.join(", "))]);
    result
}

#[test]
fn summary_counts_fingerprints_only_in_advisory_tier_and_preserves_result_types() {
    fn finding(rule: &str, tier: Tier, disposition: Disposition, digit: &str) -> Finding {
        Finding {
            rule_id: rule.to_string(),
            tier,
            disposition,
            fingerprint: digit.repeat(64),
            ruleset_version: 4,
            provider: "owned-fixture".to_string(),
            selector: "not-for-output".to_string(),
            field_path: "not-for-output".to_string(),
            start: 0,
            end: 1,
        }
    }
    let advisory = finding(
        "npm-publish-token",
        Tier::Advisory,
        Disposition::ChecksumFailed,
        "1",
    );
    let report = ScanReport {
        findings: vec![
            advisory.clone(),
            advisory,
            finding(
                "github-classic-token",
                Tier::Advisory,
                Disposition::ChecksumFailed,
                "2",
            ),
            finding(
                "aws-access-key-id",
                Tier::Blocking,
                Disposition::Confirmed,
                "3",
            ),
            finding(
                "aws-access-key-id",
                Tier::Blocking,
                Disposition::Placeholder,
                "4",
            ),
        ],
        ..ScanReport::default()
    };
    let guard = scan::arm_advisory_notice(&report);
    let expected =
        json!({"advisory_findings":2,"rules":["github-classic-token","npm-publish-token"]});
    assert_eq!(scan::advisory_summary(), Some(expected.clone()));
    assert_eq!(
        scan::decorate_mutation(&json!({"existing":7})).unwrap(),
        json!({"existing":7,"secret_scan":expected})
    );
    assert_eq!(
        scan::decorate_mutation(&json!([{"existing":7}])).unwrap(),
        json!([{"existing":7}])
    );
    assert_eq!(scan::decorate_mutation(&json!(7)).unwrap(), json!(7));
    drop(guard);
    assert!(scan::advisory_summary().is_none());
}

#[test]
fn notice_and_counted_summary_follow_successful_commit_and_semantic_noop() {
    let root = workspace();
    manifest(
        root.path(),
        json!([{"op":"create","title":"advisory test","description":"aB3".repeat(10),"unique_ref":"tracker:notice"}]),
    );
    let first = commit(root.path(), "commit", true);
    assert!(first.status.success());
    assert_summary(&first);
    assert_eq!(row_count(root.path()), 1);
    let noop = commit(root.path(), "commit", true);
    assert!(noop.status.success());
    let result = assert_summary(&noop);
    assert_eq!(result["semantic_changes"], 0);
    assert_eq!(row_count(root.path()), 1);
}

#[test]
fn dry_run_validation_and_transaction_rollback_have_no_write_time_summary() {
    let root = workspace();
    let candidate = "aB3".repeat(10);
    manifest(
        root.path(),
        json!([{"op":"create","title":"dry run","description":candidate}]),
    );
    let dry_run = commit(root.path(), "dry-run", true);
    assert!(dry_run.status.success());
    let result: Value = serde_json::from_slice(&dry_run.stdout).unwrap();
    assert!(result.get("secret_scan").is_none());
    assert!(notices(&dry_run).is_empty());
    assert_eq!(row_count(root.path()), 0);
    // The second operation fails after the first insert, rolling back both.
    manifest(
        root.path(),
        json!([
            {"op":"create","title":"rolled back","description":candidate},
            {"op":"update","id":"notice-does-not-exist","notes":"stable"}
        ]),
    );
    let rollback = commit(root.path(), "commit", true);
    assert!(!rollback.status.success());
    assert!(notices(&rollback).is_empty());
    assert!(rollback.stdout.is_empty());
    assert_eq!(row_count(root.path()), 0);
    fs::write(root.path().join("manifest.json"), b"{ invalid JSON").unwrap();
    let validation = commit(root.path(), "commit", true);
    assert!(!validation.status.success());
    assert!(notices(&validation).is_empty());
    assert!(validation.stdout.is_empty());
    assert_eq!(row_count(root.path()), 0);
}

#[test]
fn postcommit_publication_failure_keeps_semantic_notice_and_result() {
    let root = workspace();
    let path = root.path().join(".beads/config.json");
    let mut config: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    config["checkpoint"] = json!({"auto_flush":"invalid"});
    fs::write(path, serde_json::to_vec(&config).unwrap()).unwrap();
    manifest(
        root.path(),
        json!([{"op":"create","title":"committed before publication failure","description":"aB3".repeat(10)}]),
    );
    let output = commit(root.path(), "commit", false);
    assert!(!output.status.success());
    assert_summary(&output);
    assert_eq!(row_count(root.path()), 1);
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains("checkpoint publication failed after the mutation committed"));
}

#[cfg(not(feature = "managed-secret-policy"))]
#[test]
fn off_mode_has_no_notice_or_additive_member() {
    let root = workspace();
    let path = root.path().join(".beads/config.json");
    let mut config: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    config["secret_scan"] = json!({"mode":"off"});
    fs::write(path, serde_json::to_vec(&config).unwrap()).unwrap();
    manifest(
        root.path(),
        json!([{"op":"create","title":"off mode","description":"aB3".repeat(10)}]),
    );
    let output = commit(root.path(), "commit", true);
    assert!(output.status.success());
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(result.get("secret_scan").is_none());
    assert!(notices(&output).is_empty());
}

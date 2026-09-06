//! R034 conformance: advisory stale ordinary-claim detection.

use assert_cmd::Command;
use chrono::{Duration, Utc};
use rusqlite::{params, Connection};
use serde_json::Value;
use serial_test::serial;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

fn bead(workspace: &Path, args: &[&str]) -> std::process::Output {
    Command::cargo_bin("bead")
        .unwrap()
        .arg("--skip-foreign-workspace")
        .args(args)
        .current_dir(workspace)
        .output()
        .unwrap()
}

fn run(workspace: &Path, args: &[&str]) -> std::process::Output {
    let output = bead(workspace, args);
    assert!(
        output.status.success(),
        "bead {:?} failed:\nstdout: {}\nstderr: {}",
        args,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    output
}

fn workspace() -> TempDir {
    let temp = tempfile::tempdir().unwrap();
    run(temp.path(), &["init", "--prefix", "r034"]);
    temp
}

fn create(workspace: &Path, title: &str) -> String {
    String::from_utf8(run(workspace, &["create", "--title", title]).stdout)
        .unwrap()
        .trim()
        .to_string()
}

/// Claim the ready frontier and return (issue id, claim epoch). The epoch
/// doubles as the claim's fencing credential, which the claimant-owned
/// mutations later in these suites must present.
fn claim(workspace: &Path, assignee: &str, lease_ttl: Option<&str>) -> (String, String) {
    let mut args = vec!["claim", "--assignee", assignee, "--json"];
    if let Some(ttl) = lease_ttl {
        args.extend(["--lease-ttl", ttl]);
    }
    let output = run(workspace, &args);
    let claim: Value = serde_json::from_slice(&output.stdout).unwrap();
    let id = claim["bead_id"].as_str().unwrap().to_string();
    let epoch = claim["claim_epoch"].as_i64().unwrap().to_string();
    (id, epoch)
}

fn set_threshold(workspace: &Path, seconds: u64) {
    let config_path = workspace.join(".beads/config.json");
    let mut config: Value = serde_json::from_slice(&fs::read(&config_path).unwrap()).unwrap();
    config["doctor"]["stale_in_progress"] = serde_json::json!({
        "version": 1,
        "max_age_seconds": seconds,
    });
    fs::write(&config_path, serde_json::to_vec_pretty(&config).unwrap()).unwrap();
}

fn age_latest_event(workspace: &Path, issue_id: &str, seconds: i64) {
    let db = Connection::open(workspace.join(".beads/beads.db")).unwrap();
    let old_time = (Utc::now() - Duration::seconds(seconds)).to_rfc3339();
    db.execute(
        "UPDATE events
         SET time = ?1
         WHERE sequence = (
             SELECT MAX(sequence) FROM events WHERE issue_id = ?2
         )",
        params![old_time, issue_id],
    )
    .unwrap();
}

fn stale_check(workspace: &Path) -> Value {
    let output = run(workspace, &["doctor", "--scope", "store", "--json"]);
    let doctor: Value = serde_json::from_slice(&output.stdout).unwrap();
    doctor["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|check| check["name"] == "stale_in_progress")
        .cloned()
        .expect("store scope should include stale_in_progress")
}

#[test]
#[serial]
fn reports_stale_ordinary_claim_with_versioned_threshold_and_exact_remedy() {
    let temp = workspace();
    let workspace = temp.path();

    let config: Value =
        serde_json::from_slice(&fs::read(workspace.join(".beads/config.json")).unwrap()).unwrap();
    assert_eq!(config["doctor"]["stale_in_progress"]["version"], 1);
    assert_eq!(
        config["doctor"]["stale_in_progress"]["max_age_seconds"],
        86_400
    );

    set_threshold(workspace, 60);
    create(workspace, "ordinary stale claim");
    let issue_id = claim(workspace, "worker", None).0;
    age_latest_event(workspace, &issue_id, 120);

    let check = stale_check(workspace);
    assert_eq!(check["status"], "warning");
    assert_eq!(check["scope"], "store");
    assert_eq!(check["details"]["config_version"], 1);
    assert_eq!(check["details"]["max_age_seconds"], 60);
    assert_eq!(check["details"]["stale_count"], 1);
    assert!(check["details"]["reason_codes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|code| code == "stale_in_progress"));

    let stale = &check["details"]["stale_issues"][0];
    assert_eq!(stale["id"], issue_id);
    assert!(stale["age_seconds"].as_u64().unwrap() > 60);
    assert_eq!(stale["remedy"], format!("bead release {issue_id}"));
    assert!(check["message"]
        .as_str()
        .unwrap()
        .contains(&format!("bead release {issue_id}")));

    // Even with the repair flag, doctor never releases advisory stale work.
    run(workspace, &["doctor", "--repair"]);
    let shown = run(workspace, &["show", &issue_id, "--json"]);
    let issue: Value = serde_json::from_slice(&shown.stdout).unwrap();
    assert_eq!(issue[0]["status"], "in_progress");
}

#[test]
#[serial]
fn excludes_leased_claims_and_recent_ordinary_claims() {
    let temp = workspace();
    let workspace = temp.path();
    set_threshold(workspace, 60);

    create(workspace, "leased stale claim");
    let leased_id = claim(workspace, "lease-worker", Some("300")).0;
    age_latest_event(workspace, &leased_id, 120);

    create(workspace, "recent ordinary claim");
    let ordinary_id = claim(workspace, "ordinary-worker", None).0;
    age_latest_event(workspace, &ordinary_id, 30);

    let check = stale_check(workspace);
    assert_eq!(check["status"], "ok");
    assert_eq!(check["details"]["stale_count"], 0);
    assert_eq!(check["details"]["stale_issues"], serde_json::json!([]));
    assert_eq!(check["details"]["reason_codes"], serde_json::json!([]));
}

#[test]
#[serial]
fn historical_lease_does_not_hide_a_later_ordinary_claim() {
    let temp = workspace();
    let workspace = temp.path();
    set_threshold(workspace, 60);

    create(workspace, "reclaimed without a lease");
    let (leased_id, leased_epoch) = claim(workspace, "lease-worker", Some("300"));
    run(
        workspace,
        &["release", &leased_id, "--fencing-token", &leased_epoch],
    );
    let (ordinary_id, _ordinary_epoch) = claim(workspace, "ordinary-worker", None);
    assert_eq!(ordinary_id, leased_id);
    age_latest_event(workspace, &ordinary_id, 120);

    let check = stale_check(workspace);
    assert_eq!(check["status"], "warning");
    assert_eq!(check["details"]["stale_count"], 1);
    assert_eq!(check["details"]["stale_issues"][0]["id"], ordinary_id);
}

#[test]
#[serial]
fn rejects_unknown_threshold_configuration_version() {
    let temp = workspace();
    let workspace = temp.path();
    let config_path = workspace.join(".beads/config.json");
    let mut config: Value = serde_json::from_slice(&fs::read(&config_path).unwrap()).unwrap();
    config["doctor"]["stale_in_progress"] = serde_json::json!({
        "version": 2,
        "max_age_seconds": 60,
    });
    fs::write(&config_path, serde_json::to_vec_pretty(&config).unwrap()).unwrap();

    let output = bead(workspace, &["doctor", "--scope", "store", "--json"]);
    assert!(
        !output.status.success(),
        "invalid diagnostic configuration should fail doctor"
    );
    let doctor: Value = serde_json::from_slice(&output.stdout).unwrap();
    let check = doctor["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|check| check["name"] == "stale_in_progress")
        .unwrap();
    assert_eq!(check["status"], "error");
    assert!(check["details"]["error"]
        .as_str()
        .unwrap()
        .contains("Unsupported doctor.stale_in_progress version 2"));
}

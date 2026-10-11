//! R035 conformance: assignment-held readiness diagnosis.
//!
//! This test suite validates that doctor correctly diagnoses open issues
//! held off the ready frontier by assignment regardless of labels, and
//! provides R001 semantic reason codes with machine-readable output.

use assert_cmd::Command;
use serde_json::Value;
use serial_test::serial;
use std::path::Path;
use tempfile::TempDir;

fn bead(dir: &Path) -> Command {
    let mut command = Command::cargo_bin("bead").unwrap();
    command.current_dir(dir);
    command.arg("--skip-foreign-workspace");
    command
}

fn run(dir: &Path, args: &[&str]) -> std::process::Output {
    bead(dir).args(args).assert().success().get_output().clone()
}

fn create_issue(dir: &Path, title: &str) -> String {
    String::from_utf8(run(dir, &["create", "--title", title]).stdout)
        .unwrap()
        .trim()
        .to_string()
}

/// The claim-epoch credential the issue's current claim was issued, read back
/// from the `show --json` projection. Assigning an issue mints an epoch, and
/// clearing that assignment is a claimant-owned mutation that must present it
/// (see tests/claim_epoch.rs).
fn held_credential(dir: &Path, id: &str) -> String {
    let output = run(dir, &["show", id, "--json"]);
    let shown: Value = serde_json::from_slice(&output.stdout).unwrap();
    shown[0]["claim_epoch"].as_i64().unwrap().to_string()
}

#[test]
#[serial]
fn r035_conformance_healthy_to_warning_to_cleared() {
    let temp = TempDir::new().unwrap();
    let workspace = temp.path();

    // Step 1: Initialize workspace - healthy state
    run(workspace, &["init", "--prefix", "r035-healthy"]);

    // Verify ready frontier is initially healthy
    let output = run(workspace, &["doctor", "--json"]);
    let doctor_json: Value = serde_json::from_slice(&output.stdout).unwrap();
    let checks = doctor_json["checks"].as_array().unwrap();

    let frontier_check = checks
        .iter()
        .find(|c| c["name"] == "ready_frontier")
        .expect("ready_frontier check should be present");

    assert_eq!(
        frontier_check["status"], "ok",
        "Initial state should be healthy"
    );
    assert_eq!(
        frontier_check["details"]["held_count"], 0,
        "Should have no held issues initially"
    );

    // Step 2: Create an issue and assign it while keeping it open
    let id = create_issue(workspace, "Issue that will be held");
    run(workspace, &["update", &id, "--assignee", "worker-1"]);

    // Verify doctor now warns about the held issue
    let output = run(workspace, &["doctor", "--json"]);
    let doctor_json: Value = serde_json::from_slice(&output.stdout).unwrap();
    let checks = doctor_json["checks"].as_array().unwrap();

    let frontier_check = checks
        .iter()
        .find(|c| c["name"] == "ready_frontier")
        .expect("ready_frontier check should be present");

    assert_eq!(
        frontier_check["status"], "warning",
        "Held issue should trigger warning"
    );
    assert_eq!(
        frontier_check["details"]["held_count"], 1,
        "Should report one held issue"
    );

    let held_ids = frontier_check["details"]["held_ids"].as_array().unwrap();
    assert_eq!(held_ids.len(), 1, "held_ids should contain exactly one ID");
    assert_eq!(
        held_ids[0].as_str().unwrap(),
        id,
        "held_ids should contain the created issue"
    );

    // Verify R001 reason code is present
    let reason_codes = frontier_check["details"]["reason_codes"]
        .as_array()
        .unwrap();
    assert!(
        reason_codes
            .iter()
            .any(|rc| rc.as_str() == Some("open_issue_held_by_assignee")),
        "Should include open_issue_held_by_assignee reason code"
    );

    // Verify remedy is provided
    let remedy = frontier_check["details"]["remedy"].as_str().unwrap();
    assert_eq!(remedy, "bead update <id> --clear-assignee");
    assert_eq!(
        frontier_check["details"]["held_issues"][0]["remedy"],
        format!("bead update {id} --clear-assignee")
    );

    // Verify doctor has warnings in overall status
    assert_eq!(
        doctor_json["has_warnings"], true,
        "Doctor should report has_warnings=true"
    );

    // Step 3: Clear the assignee - should return to healthy. Assigning minted
    // a claim epoch, so clearing it carries that credential.
    let credential = held_credential(workspace, &id);
    run(
        workspace,
        &[
            "update",
            &id,
            "--clear-assignee",
            "--fencing-token",
            &credential,
        ],
    );

    let output = run(workspace, &["doctor", "--json"]);
    let doctor_json: Value = serde_json::from_slice(&output.stdout).unwrap();
    let checks = doctor_json["checks"].as_array().unwrap();

    let frontier_check = checks
        .iter()
        .find(|c| c["name"] == "ready_frontier")
        .expect("ready_frontier check should be present");

    assert_eq!(
        frontier_check["status"], "ok",
        "Cleared issue should return to healthy state"
    );
    assert_eq!(
        frontier_check["details"]["held_count"], 0,
        "Should have no held issues after clearing"
    );
}

#[test]
#[serial]
fn r035_labels_do_not_suppress_assignment_diagnosis() {
    let temp = TempDir::new().unwrap();
    let workspace = temp.path();
    run(workspace, &["init", "--prefix", "r035-labels"]);

    let id = create_issue(workspace, "Held work with a label");
    run(workspace, &["update", &id, "--assignee", "worker-1"]);
    run(workspace, &["label", "add", &id, "--label", "parked"]);

    let output = run(workspace, &["doctor", "--json"]);
    let doctor_json: Value = serde_json::from_slice(&output.stdout).unwrap();
    let check = doctor_json["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|check| check["name"] == "ready_frontier")
        .expect("assignment-held check must be present");
    assert_eq!(check["status"], "warning");
    assert_eq!(check["details"]["held_ids"], serde_json::json!([id]));
    assert_eq!(
        check["details"]["reason_codes"],
        serde_json::json!(["open_issue_held_by_assignee"])
    );
    assert_eq!(
        check["details"]["held_issues"][0]["remedy"],
        format!("bead update {id} --clear-assignee")
    );
    assert!(doctor_json["has_warnings"].as_bool().unwrap());
}

#[test]
#[serial]
fn r035_doctor_never_clears_assignee_even_under_repair() {
    let temp = TempDir::new().unwrap();
    let workspace = temp.path();

    run(workspace, &["init", "--prefix", "r035-repair"]);

    let id = create_issue(workspace, "Held issue for repair test");
    run(workspace, &["update", &id, "--assignee", "worker-1"]);

    // Capture initial assignee state
    let output = run(workspace, &["show", &id, "--json"]);
    let issue_json: Value = serde_json::from_slice(&output.stdout).unwrap();
    let initial_assignee = issue_json[0]["assignee"].as_str().unwrap();

    // Neither ordinary repair nor forced starvation recovery may clear it.
    run(workspace, &["doctor", "--repair"]);
    run(workspace, &["doctor", "--starvation-recovery", "--force"]);

    // Verify assignee is still present
    let output = run(workspace, &["show", &id, "--json"]);
    let issue_json: Value = serde_json::from_slice(&output.stdout).unwrap();
    let current_assignee = issue_json[0]["assignee"].as_str().unwrap();

    assert_eq!(
        initial_assignee, current_assignee,
        "Doctor repair modes should not clear assignees"
    );

    // Verify the held issue is still diagnosed
    let output = run(workspace, &["doctor", "--json"]);
    let doctor_json: Value = serde_json::from_slice(&output.stdout).unwrap();
    let checks = doctor_json["checks"].as_array().unwrap();

    let frontier_check = checks
        .iter()
        .find(|c| c["name"] == "ready_frontier")
        .expect("ready_frontier check should still warn");

    assert_eq!(
        frontier_check["status"], "warning",
        "Issue should still be diagnosed as held"
    );
}

/// The prose warning names a bounded sample of the offending ids, while the
/// machine-readable `held_ids` projection stays complete: with more assigned
/// open issues than the sample window, the tail must appear in `details` and
/// must stay out of the message, so the sample can never be mistaken for the
/// full set — that is what `held_count` and `held_ids` carry.
#[test]
#[serial]
fn r035_prose_sample_is_bounded_while_details_remain_complete() {
    let temp = TempDir::new().unwrap();
    let workspace = temp.path();

    run(workspace, &["init", "--prefix", "r035-bounded"]);

    // More assigned open issues than the five the prose names.
    let mut held = Vec::new();
    for n in 0..7 {
        let id = create_issue(workspace, &format!("Held by sample bound {n}"));
        run(workspace, &["update", &id, "--assignee", "worker-1"]);
        held.push(id);
    }

    let output = run(workspace, &["doctor", "--json"]);
    let doctor_json: Value = serde_json::from_slice(&output.stdout).unwrap();
    let checks = doctor_json["checks"].as_array().unwrap();

    let frontier_check = checks
        .iter()
        .find(|c| c["name"] == "ready_frontier")
        .expect("ready_frontier check should be present");

    assert_eq!(
        frontier_check["status"], "warning",
        "Assigned open issues should trigger the warning"
    );

    // details carries the complete list: every held id, count to match.
    let held_ids: Vec<String> = frontier_check["details"]["held_ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        held_ids.len(),
        held.len(),
        "details.held_ids must list every held issue"
    );
    for id in &held {
        assert!(held_ids.contains(id), "details.held_ids must contain {id}");
        assert!(
            frontier_check["details"]["held_issues"]
                .as_array()
                .unwrap()
                .iter()
                .any(|entry| {
                    entry["id"] == *id
                        && entry["remedy"] == format!("bead update {id} --clear-assignee")
                }),
            "details.held_issues must give the exact remedy for {id}"
        );
    }
    assert_eq!(
        frontier_check["details"]["held_count"].as_u64().unwrap(),
        held.len() as u64,
        "held_count must match the complete list"
    );

    // The prose names exactly the first five of that list: a bounded sample.
    // Nothing beyond the window may leak into the message, or the sample and
    // the machine-readable list would not be distinguishable.
    let message = frontier_check["message"].as_str().unwrap();
    for id in &held_ids[..5] {
        assert!(
            message.contains(id),
            "prose sample should name {id}: {message}"
        );
    }
    for id in &held_ids[5..] {
        assert!(
            !message.contains(id),
            "prose must stay bounded: {id} leaked past the sample window"
        );
    }
    assert!(
        message.contains("7 open issue(s)"),
        "the count must account for every held issue, not just the sample: {message}"
    );
}

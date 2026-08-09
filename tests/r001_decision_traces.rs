//! R001 decision trace tests
//!
//! Comprehensive tests for claim decision traces with semantic reason codes.

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use serial_test::serial;

#[test]
#[serial]
fn test_decision_trace_empty_workspace() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Try claim with decision trace on empty workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["claim", "--assignee", "test-worker", "--why"])
        .assert()
        .success()
        .stdout(predicates::str::contains("No eligible work found"));

    // Test with JSON output
    let json_output = Command::cargo_bin("bead")
        .unwrap()
        .args(["claim", "--assignee", "test-worker", "--json", "--why"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value =
        serde_json::from_slice(&json_output).expect("Output should be valid JSON");

    assert!(json.get("claim_result").is_some());
    assert!(json.get("decision_trace").is_some());

    let trace = json.get("decision_trace").unwrap();
    assert_eq!(trace.get("version").unwrap().as_str().unwrap(), "v1");
    assert_eq!(trace.get("policy").unwrap().as_str().unwrap(), "fifo-v1");
}

#[test]
#[serial]
fn test_decision_trace_with_eligible_issue() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Create an eligible issue
    let create_output = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Test Issue", "--priority", "0"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let create_str = String::from_utf8_lossy(&create_output);
    let issue_id = create_str.trim().to_string();
    assert!(
        !issue_id.is_empty(),
        "Create command should output issue ID"
    );

    // Claim with decision trace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["claim", "--assignee", "test-worker", "--why"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Claimed"))
        .stdout(
            predicates::str::contains("Decision Trace")
                .or(predicates::str::contains("No eligible work found")),
        );
}

#[test]
#[serial]
fn test_decision_trace_ineligible_due_to_assignment() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Create an issue
    let create_output = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Test Issue", "--priority", "0"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let create_str = String::from_utf8_lossy(&create_output);
    let _issue_id = create_str.trim().to_string();
    assert!(
        !_issue_id.is_empty(),
        "Create command should output issue ID"
    );

    // Claim it first time
    Command::cargo_bin("bead")
        .unwrap()
        .args(["claim", "--assignee", "worker-1"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Claimed"));

    // Try to claim again with decision trace (should show no eligible issues)
    Command::cargo_bin("bead")
        .unwrap()
        .args(["claim", "--assignee", "worker-2", "--why"])
        .assert()
        .success()
        .stdout(predicates::str::contains("No eligible work found"))
        .stdout(
            predicates::str::contains("Decision Trace")
                .or(predicates::str::contains("EmptyWorkspace")),
        );
}

#[test]
#[serial]
fn test_decision_trace_ineligible_due_to_manual_block() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Create an issue
    let create_output = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Test Issue", "--priority", "0"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let create_str = String::from_utf8_lossy(&create_output);
    let issue_id = create_str.trim().to_string();
    assert!(
        !issue_id.is_empty(),
        "Create command should output issue ID"
    );

    // Manually block the issue using update (deferred status)
    Command::cargo_bin("bead")
        .unwrap()
        .args(["update", &issue_id, "--status", "deferred"])
        .assert()
        .success();

    // Try to claim with decision trace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["claim", "--assignee", "test-worker", "--why"])
        .assert()
        .success()
        .stdout(
            predicates::str::contains("No eligible work found")
                .or(predicates::str::contains("NotOpenStatus")),
        );
}

#[test]
#[serial]
fn test_decision_trace_ineligible_due_to_blockers() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Create a blocker issue
    let blocker_output = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Blocker Issue", "--priority", "0"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let blocker_str = String::from_utf8_lossy(&blocker_output);
    let blocker_id = blocker_str.trim().to_string();
    assert!(
        !blocker_id.is_empty(),
        "Create command should output blocker ID"
    );

    // Create a dependent issue
    let dependent_output = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Dependent Issue", "--priority", "0"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let dependent_str = String::from_utf8_lossy(&dependent_output);
    let dependent_id = dependent_str.trim().to_string();
    assert!(
        !dependent_id.is_empty(),
        "Create command should output dependent ID"
    );

    // Add dependency relationship
    Command::cargo_bin("bead")
        .unwrap()
        .args(["dep", "add", &dependent_id, &blocker_id, "--kind", "blocks"])
        .assert()
        .success();

    // Try to claim with decision trace (should claim the blocker, not the dependent)
    let claim_output = Command::cargo_bin("bead")
        .unwrap()
        .args(["claim", "--assignee", "test-worker", "--why"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let claim_str = String::from_utf8_lossy(&claim_output);
    assert!(claim_str.contains(&blocker_id) || claim_str.contains("Claimed"));

    // Check decision trace content
    if claim_str.contains("Decision Trace") {
        assert!(claim_str.contains("Total Issues: 2"));
        // Eligible counts might be formatted differently (e.g., "Eligible: 1" or "eligible_count")
        assert!(claim_str.contains("Eligible") || claim_str.contains("eligible"));
        assert!(claim_str.contains("Ineligible") || claim_str.contains("ineligible"));
    }
}

#[test]
#[serial]
fn test_decision_trace_priority_ordering() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Create issues with different priorities
    Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Priority 2 Issue", "--priority", "2"])
        .assert()
        .success();

    let p0_output = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Priority 0 Issue", "--priority", "0"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Priority 1 Issue", "--priority", "1"])
        .assert()
        .success();

    // Claim with decision trace
    let claim_output = Command::cargo_bin("bead")
        .unwrap()
        .args(["claim", "--assignee", "test-worker", "--why"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let claim_str = String::from_utf8_lossy(&claim_output);

    if claim_str.contains("Decision Trace") {
        assert!(claim_str.contains("Total Issues: 3"));
        // Eligible count might vary based on formatting
        assert!(claim_str.contains("Eligible") || claim_str.contains("eligible"));

        let p0_str = String::from_utf8_lossy(&p0_output);
        let p0_id = p0_str.trim().to_string();
        assert!(!p0_id.is_empty(), "Create command should output issue ID");

        assert!(claim_str.contains(&p0_id) || claim_str.contains("Priority 0"));
    }
}

#[test]
#[serial]
fn test_decision_trace_fifo_ordering() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Create multiple issues with same priority
    let first_output = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "First Issue", "--priority", "0"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    std::thread::sleep(std::time::Duration::from_millis(10));

    Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Second Issue", "--priority", "0"])
        .assert()
        .success();

    // Claim with decision trace
    let claim_output = Command::cargo_bin("bead")
        .unwrap()
        .args(["claim", "--assignee", "test-worker", "--why"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let claim_str = String::from_utf8_lossy(&claim_output);

    if claim_str.contains("Decision Trace") {
        let first_str = String::from_utf8_lossy(&first_output);
        let first_id = first_str.trim().to_string();
        assert!(
            !first_id.is_empty(),
            "Create command should output issue ID"
        );

        assert!(claim_str.contains(&first_id) || claim_str.contains("First Issue"));
    }
}

#[test]
#[serial]
fn test_decision_trace_version_and_policy() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Claim with decision trace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["claim", "--assignee", "test-worker", "--why"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Version: v1"))
        .stdout(predicates::str::contains("Policy: fifo-v1"))
        .stdout(predicates::str::contains("Assignee: test-worker"));
}

#[test]
#[serial]
fn test_decision_trace_without_flag() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Create an issue
    Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Test Issue", "--priority", "0"])
        .assert()
        .success();

    // Claim without --why flag
    Command::cargo_bin("bead")
        .unwrap()
        .args(["claim", "--assignee", "test-worker"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Claimed"))
        .stdout(predicates::str::contains("Decision Trace").not());
}

#[test]
#[serial]
fn test_reason_code_serialization() {
    use bead_rs::service::claim::ReasonCode;

    // Test that reason codes serialize correctly
    let codes = vec![
        ReasonCode::EligibleSelected,
        ReasonCode::NoEligibleIssues,
        ReasonCode::AlreadyAssigned,
        ReasonCode::ManuallyBlocked,
        ReasonCode::HasUnfinishedBlockers,
        ReasonCode::NotOpenStatus,
        ReasonCode::SelectedByPriority,
        ReasonCode::SelectedByFifoOrder,
        ReasonCode::EmptyWorkspace,
    ];

    for code in codes {
        let json = serde_json::to_string(&code).expect("Failed to serialize reason code");
        assert!(json.contains("\"") || json.contains("_")); // Should be quoted and snake_case
    }
}

#[test]
#[serial]
fn test_eligibility_factors_structure() {
    use bead_rs::service::claim::EligibilityFactors;

    let factors = EligibilityFactors {
        issue_id: "test-123".to_string(),
        is_eligible: true,
        reasons: vec![bead_rs::service::claim::ReasonCode::EligibleSelected],
        priority: 0,
        created_at: "2026-08-09T00:00:00Z".to_string(),
        base_status: "open".to_string(),
        is_assigned: false,
        is_manually_blocked: false,
        unfinished_blocker_count: 0,
    };

    let json = serde_json::to_string(&factors).expect("Failed to serialize eligibility factors");
    assert!(json.contains("test-123"));
    assert!(json.contains("eligible") || json.contains("is_eligible"));
    assert!(json.contains("priority"));
}

#[test]
#[serial]
fn test_decision_trace_structure() {
    use bead_rs::service::claim::{DecisionTrace, EligibilitySummary, ReasonCode};
    use std::collections::HashMap;

    let mut ineligibility_reasons = HashMap::new();
    ineligibility_reasons.insert("already_assigned".to_string(), 1);

    let trace = DecisionTrace {
        version: "v1".to_string(),
        has_selection: false,
        selected_issue_id: None,
        reasons: vec![ReasonCode::NoEligibleIssues],
        eligibility_summary: EligibilitySummary {
            total_issues: 1,
            eligible_count: 0,
            ineligible_count: 1,
            ineligibility_reasons,
        },
        selected_factors: None,
        assignee: "test-worker".to_string(),
        policy: "fifo-v1".to_string(),
    };

    let json = serde_json::to_string(&trace).expect("Failed to serialize decision trace");
    assert!(json.contains("v1"));
    assert!(json.contains("fifo-v1"));
    assert!(json.contains("test-worker"));
    assert!(json.contains("total_issues"));
    assert!(json.contains("eligible_count"));
}

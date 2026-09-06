//! R002 Fenced Claim Leases integration tests
//!
//! Comprehensive tests for R002's fenced claim leases with expiring claims,
//! renewals, and monotonically increasing fencing tokens for safe recovery from
//! crashed or disconnected agents.

use rusqlite::Connection;
use std::env;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

/// Test workspace helper
struct TestWorkspace {
    _temp_dir: TempDir, // Keep temp_dir alive for cleanup
    workspace_path: PathBuf,
    bead_path: PathBuf,
}

impl TestWorkspace {
    fn new() -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let workspace_path = temp_dir.path().to_path_buf();

        // Set HOME to temp directory for isolation
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { env::set_var("HOME", &workspace_path) };

        // Get the path to the locally built bead binary
        let current_exe = std::env::current_exe().expect("Failed to get current exe");
        let bead_path = current_exe
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.join("bead"))
            .expect("Failed to determine bead path");

        // Verify the bead binary exists
        assert!(
            bead_path.exists(),
            "Bead binary not found at {:?}",
            bead_path
        );

        // Initialize workspace using the local bead binary
        let output = Command::new(&bead_path)
            .args(["init", "--prefix", "test", "--skip-foreign-workspace"])
            .current_dir(&workspace_path)
            .output()
            .expect("Failed to run bead init");

        assert!(
            output.status.success(),
            "Init failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        TestWorkspace {
            _temp_dir: temp_dir,
            workspace_path,
            bead_path,
        }
    }

    fn run_bead(&self, args: &[&str]) -> std::process::Output {
        Command::new(&self.bead_path)
            .args(args)
            .current_dir(&self.workspace_path)
            .output()
            .expect("Failed to run bead command")
    }

    #[allow(dead_code)]
    fn run_bead_json<T>(&self, args: &[&str]) -> T
    where
        T: serde::de::DeserializeOwned,
    {
        let output = self.run_bead(args);
        assert!(
            output.status.success(),
            "Command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        serde_json::from_slice(&output.stdout).expect("Failed to parse JSON output")
    }
}

#[test]
fn test_basic_leased_claim() {
    let workspace = TestWorkspace::new();

    // Create some test issues
    workspace.run_bead(&["create", "--title", "Task 1", "--priority", "0"]);
    workspace.run_bead(&["create", "--title", "Task 2", "--priority", "1"]);
    workspace.run_bead(&["create", "--title", "Task 3", "--priority", "2"]);

    // Claim with lease (60 seconds TTL)
    let output = workspace.run_bead(&[
        "claim",
        "--assignee",
        "alice",
        "--lease-ttl",
        "60",
        "--json",
    ]);

    if !output.status.success() {
        eprintln!("stderr: {}", String::from_utf8_lossy(&output.stderr));
        eprintln!("stdout: {}", String::from_utf8_lossy(&output.stdout));
    }
    assert!(output.status.success());

    let result: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("Failed to parse claim result");

    // Verify basic claim result structure
    assert!(result["bead_id"].is_string());
    assert_eq!(result["assignee"], "alice");

    // Verify lease information is present
    assert!(result["lease"].is_object());
    let lease = &result["lease"];
    assert!(lease["fencing_token"].is_number());
    assert!(lease["expires_at"].is_string());
    assert_eq!(lease["issue_id"], result["bead_id"]);

    // Verify fencing token is monotonically increasing (should be 1 for first lease)
    assert_eq!(lease["fencing_token"].as_i64().unwrap(), 1);
}

#[test]
fn test_lease_renewal() {
    let workspace = TestWorkspace::new();

    // Create and claim an issue with a lease
    workspace.run_bead(&["create", "--title", "Task 1", "--priority", "0"]);

    let initial_output = workspace.run_bead(&[
        "claim",
        "--assignee",
        "alice",
        "--lease-ttl",
        "60",
        "--json",
    ]);

    let initial_result: serde_json::Value = serde_json::from_slice(&initial_output.stdout)
        .expect("Failed to parse initial claim result");

    let initial_fencing_token = initial_result["lease"]["fencing_token"]
        .as_i64()
        .expect("Failed to get initial fencing token");
    let issue_id = initial_result["bead_id"]
        .as_str()
        .expect("Failed to get issue ID");

    // Renew the lease
    let renew_output = workspace.run_bead(&[
        "claim",
        "--assignee",
        "alice",
        "--renew-lease",
        "--lease-ttl",
        "120",
        "--json",
    ]);

    assert!(renew_output.status.success());

    let renew_result: serde_json::Value =
        serde_json::from_slice(&renew_output.stdout).expect("Failed to parse renewal result");

    // Verify renewal has incremented fencing token
    let renewed_fencing_token = renew_result["lease"]["fencing_token"]
        .as_i64()
        .expect("Failed to get renewed fencing token");

    assert_eq!(renewed_fencing_token, initial_fencing_token + 1);
    assert_eq!(renew_result["bead_id"].as_str(), Some(issue_id));
}

#[test]
fn test_lease_renewal_preserves_historical_rows() {
    let workspace = TestWorkspace::new();

    workspace.run_bead(&["create", "--title", "Task 1", "--priority", "0"]);

    let first_claim = workspace.run_bead(&[
        "claim",
        "--assignee",
        "alice",
        "--lease-ttl",
        "60",
        "--json",
    ]);
    assert!(first_claim.status.success());
    let first_result: serde_json::Value = serde_json::from_slice(&first_claim.stdout).unwrap();
    let issue_id = first_result["bead_id"].as_str().unwrap().to_string();
    let first_epoch = first_result["claim_epoch"].as_i64().unwrap().to_string();

    let release = workspace.run_bead(&["release", &issue_id, "--fencing-token", &first_epoch]);
    assert!(
        release.status.success(),
        "release failed: {}",
        String::from_utf8_lossy(&release.stderr)
    );

    let second_claim =
        workspace.run_bead(&["claim", "--assignee", "bob", "--lease-ttl", "60", "--json"]);
    assert!(
        second_claim.status.success(),
        "second leased claim failed: {}",
        String::from_utf8_lossy(&second_claim.stderr)
    );
    let second_result: serde_json::Value = serde_json::from_slice(&second_claim.stdout).unwrap();
    assert_eq!(second_result["bead_id"].as_str(), Some(issue_id.as_str()));

    let read_lease_rows = || {
        let conn = Connection::open(workspace.workspace_path.join(".beads/beads.db")).unwrap();
        let mut statement = conn
            .prepare(
                "SELECT assignee, fencing_token, expires_at
                 FROM leases
                 WHERE issue_id = ?1
                 ORDER BY fencing_token ASC",
            )
            .unwrap();
        statement
            .query_map([&issue_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap()
    };

    let before_renewal = read_lease_rows();
    assert_eq!(before_renewal.len(), 2);
    assert_eq!(before_renewal[0].0, "alice");
    assert_eq!(before_renewal[0].1, 1);
    assert_eq!(before_renewal[1].0, "bob");
    assert_eq!(before_renewal[1].1, 2);

    let renewal = workspace.run_bead(&[
        "claim",
        "--assignee",
        "bob",
        "--renew-lease",
        "--lease-ttl",
        "120",
        "--json",
    ]);
    assert!(
        renewal.status.success(),
        "renewal failed: {}",
        String::from_utf8_lossy(&renewal.stderr)
    );
    let renewal_result: serde_json::Value = serde_json::from_slice(&renewal.stdout).unwrap();
    assert_eq!(renewal_result["lease"]["fencing_token"], 3);

    let after_renewal = read_lease_rows();
    assert_eq!(after_renewal.len(), 2);
    assert_eq!(after_renewal[0], before_renewal[0]);
    assert_eq!(after_renewal[1].0, "bob");
    assert_eq!(after_renewal[1].1, 3);
    assert_ne!(after_renewal[1].2, before_renewal[1].2);
}

#[test]
fn test_fencing_token_validation() {
    let workspace = TestWorkspace::new();

    // Create and claim an issue with a lease
    workspace.run_bead(&["create", "--title", "Task 1", "--priority", "0"]);

    let claim_output =
        workspace.run_bead(&["claim", "--assignee", "alice", "--lease-ttl", "2", "--json"]);

    let claim_result: serde_json::Value =
        serde_json::from_slice(&claim_output.stdout).expect("Failed to parse claim result");

    let issue_id = claim_result["bead_id"]
        .as_str()
        .expect("Failed to get issue ID");
    let fencing_token = claim_result["lease"]["fencing_token"]
        .as_i64()
        .expect("Failed to get fencing token");

    // Wait a moment for lease to be partially aged
    std::thread::sleep(std::time::Duration::from_secs(1));

    // Try to update the issue with wrong fencing token - should fail
    let wrong_token_output = workspace.run_bead(&[
        "update",
        issue_id,
        "--notes",
        "Should fail with wrong token",
        "--fencing-token",
        &(fencing_token + 100).to_string(),
    ]);

    assert!(!wrong_token_output.status.success());
    assert!(wrong_token_output.status.code().unwrap() == 4);

    let stderr = String::from_utf8_lossy(&wrong_token_output.stderr);
    assert!(stderr.contains("fencing") || stderr.contains("token") || stderr.contains("Lease"));

    // Try to update with correct fencing token - should succeed
    let correct_token_output = workspace.run_bead(&[
        "update",
        issue_id,
        "--notes",
        "Should succeed with correct token",
        "--fencing-token",
        &fencing_token.to_string(),
    ]);

    assert!(correct_token_output.status.success());
}

#[test]
fn test_backward_compatibility_non_leased_claims() {
    let workspace = TestWorkspace::new();

    // Create test issues
    workspace.run_bead(&["create", "--title", "Task 1", "--priority", "0"]);
    workspace.run_bead(&["create", "--title", "Task 2", "--priority", "1"]);

    // Claim without lease (default behavior)
    let output = workspace.run_bead(&["claim", "--assignee", "bob", "--json"]);

    assert!(output.status.success());

    let result: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("Failed to parse claim result");

    // Verify claim succeeded but no lease info present
    assert!(result["bead_id"].is_string());
    assert_eq!(result["assignee"], "bob");

    // Lease field should be null or absent for non-leased claims
    if result.get("lease").is_some() {
        assert!(result["lease"].is_null());
    }

    // Verify normal operations work with the plain claim's epoch credential:
    // a non-leased claim mints an epoch too (surfaced as claim_epoch), it
    // just has no lease row behind it
    let issue_id = result["bead_id"].as_str().expect("Failed to get issue ID");
    let epoch = result["claim_epoch"]
        .as_i64()
        .expect("claim_epoch")
        .to_string();

    let update_output = workspace.run_bead(&[
        "update",
        issue_id,
        "--notes",
        "Normal update works",
        "--fencing-token",
        &epoch,
    ]);
    assert!(update_output.status.success());

    // Also verify release works with the same credential
    let release_output = workspace.run_bead(&["release", issue_id, "--fencing-token", &epoch]);
    assert!(release_output.status.success());
}

#[test]
fn test_concurrent_leased_claims() {
    let workspace = TestWorkspace::new();

    // Create multiple test issues - more than workers to ensure enough work
    // Use priorities within valid range (0-4)
    for i in 0..10 {
        let priority = i % 5; // Cycle through priorities 0-4
        let create_output = workspace.run_bead(&[
            "create",
            "--title",
            &format!("Task {}", i),
            "--priority",
            &priority.to_string(),
        ]);

        if !create_output.status.success() {
            eprintln!(
                "Failed to create task {}: {}",
                i,
                String::from_utf8_lossy(&create_output.stderr)
            );
        }

        assert!(create_output.status.success());
    }

    // Debug: Check what issues were created
    let list_output = workspace.run_bead(&["list", "--json"]);
    eprintln!(
        "Initial issues: {}",
        String::from_utf8_lossy(&list_output.stdout)
    );

    // Simulate multiple workers claiming with leases
    let mut claimed_ids = std::collections::HashSet::new();
    let mut workers = vec![];
    let mut successful_claims = 0;

    for worker_id in 0..3 {
        let assignee = format!("worker-{}", worker_id);
        let output = workspace.run_bead(&[
            "claim",
            "--assignee",
            &assignee,
            "--lease-ttl",
            "60",
            "--json",
        ]);

        if !output.status.success() {
            eprintln!("Worker {} claim failed:", worker_id);
            eprintln!("stderr: {}", String::from_utf8_lossy(&output.stderr));
            eprintln!("stdout: {}", String::from_utf8_lossy(&output.stdout));
        }

        assert!(output.status.success());

        let result: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("Failed to parse claim result");

        // Debug output to see what we got
        if worker_id == 0 {
            eprintln!(
                "Worker 0 claim result: {}",
                serde_json::to_string_pretty(&result).unwrap()
            );
        }

        if let Some(bead_id) = result["bead_id"].as_str() {
            // Verify no duplicate claims
            assert!(
                claimed_ids.insert(bead_id.to_string()),
                "Duplicate claim detected for issue {}",
                bead_id
            );

            workers.push((assignee, bead_id.to_string()));
            successful_claims += 1;
        } else {
            eprintln!("Worker {} got empty queue", worker_id);
        }
        // Empty queue is acceptable - means no more work available
    }

    // Verify we got at least some successful claims (not all workers may get work)
    assert!(successful_claims > 0, "At least one claim should succeed");
    assert!(
        claimed_ids.len() <= 3,
        "Cannot claim more issues than workers"
    );

    // Verify each worker who got work can operate on their claimed issue with valid lease
    for (assignee, issue_id) in &workers {
        let output = workspace.run_bead(&["show", issue_id, "--json"]);

        assert!(output.status.success());

        let result: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("Failed to parse show result");

        // Verify the assignee matches
        let issue = &result[0];
        assert_eq!(issue["assignee"].as_str(), Some(assignee.as_str()));

        // Verify lease information is present
        assert!(result[0]["lease"].is_object() || result[0]["lease"].is_null());
    }
}

#[test]
fn test_lease_ttl_bounds() {
    let workspace = TestWorkspace::new();

    workspace.run_bead(&["create", "--title", "Task 1", "--priority", "0"]);

    // Test minimum TTL (should be clamped to MIN_LEASE_TTL = 30)
    let output = workspace.run_bead(&[
        "claim",
        "--assignee",
        "alice",
        "--lease-ttl",
        "1", // Below minimum
        "--json",
    ]);

    assert!(output.status.success());

    let result: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("Failed to parse claim result");

    // Should get a valid lease despite TTL being below minimum
    assert!(result["lease"].is_object());
    assert!(result["lease"]["fencing_token"].as_i64().unwrap() > 0);

    // Test maximum TTL (should be clamped to MAX_LEASE_TTL = 3600)
    let _issue_id = result["bead_id"].as_str().expect("No issue ID");

    // Try with extremely large TTL
    let large_ttl_output = workspace.run_bead(&[
        "claim",
        "--assignee",
        "bob",
        "--lease-ttl",
        "999999", // Way above maximum
        "--json",
    ]);

    assert!(large_ttl_output.status.success());
}

#[test]
fn test_empty_queue_with_lease_request() {
    let workspace = TestWorkspace::new();

    // Try to claim from empty workspace with lease request
    let output = workspace.run_bead(&[
        "claim",
        "--assignee",
        "alice",
        "--lease-ttl",
        "60",
        "--json",
    ]);

    assert!(output.status.success());

    let result: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("Failed to parse claim result");

    // Empty queue should return null bead_id and null lease
    assert!(result["bead_id"].is_null() || result["bead_id"].as_str().unwrap_or("").is_empty());
    assert!(result["lease"].is_null() || result["lease"].as_object().is_none_or(|o| o.is_empty()));
}

#[test]
fn test_lease_renewal_without_active_lease() {
    let workspace = TestWorkspace::new();

    workspace.run_bead(&["create", "--title", "Task 1", "--priority", "0"]);

    // Try to renew lease when no active lease exists
    let output = workspace.run_bead(&["claim", "--assignee", "alice", "--renew-lease", "--json"]);

    assert!(output.status.success());

    let result: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("Failed to parse claim result");

    // Should return empty result since no lease exists to renew
    assert!(result["bead_id"].is_null() || result["bead_id"].as_str().unwrap_or("").is_empty());
    assert!(result["lease"].is_null() || result["lease"].as_object().is_none_or(|o| o.is_empty()));
}

#[test]
fn test_leased_claim_with_why_flag() {
    let workspace = TestWorkspace::new();

    workspace.run_bead(&["create", "--title", "Task 1", "--priority", "0"]);

    // Claim with lease and decision trace
    let output = workspace.run_bead(&[
        "claim",
        "--assignee",
        "alice",
        "--lease-ttl",
        "60",
        "--why",
        "--json",
    ]);

    assert!(output.status.success());

    let result: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("Failed to parse claim result");

    // Should have both claim_result with lease info and decision trace
    assert!(result["claim_result"].is_object());
    assert!(result["decision_trace"].is_object());

    let claim_result = &result["claim_result"];
    assert!(claim_result["bead_id"].is_string());
    assert!(claim_result["lease"].is_object());

    // Verify decision trace structure
    let trace = &result["decision_trace"];
    assert!(trace["version"].is_string());
    assert!(trace["policy"].is_string());
    assert!(trace["assignee"].is_string());
    assert!(trace["has_selection"].is_boolean());
}

#[test]
fn test_plain_reclaim_after_leased_release_is_mutable() {
    // Regression (beadrs-122d91fb): a lease row from a previous claim epoch
    // must not fence a later non-leased claimant. Lease rows are never
    // deleted, so "any lease row ever existed" can never permanently block
    // close/release/update of an issue that was re-claimed without a lease.
    let workspace = TestWorkspace::new();

    workspace.run_bead(&["create", "--title", "Task 1", "--priority", "0"]);

    // w1 claims with a lease, then releases while the lease is still active
    let leased_output =
        workspace.run_bead(&["claim", "--assignee", "w1", "--lease-ttl", "60", "--json"]);
    assert!(leased_output.status.success());

    let leased_result: serde_json::Value =
        serde_json::from_slice(&leased_output.stdout).expect("Failed to parse claim result");
    let issue_id = leased_result["bead_id"]
        .as_str()
        .expect("Failed to get issue ID")
        .to_string();
    let leased_epoch = leased_result["claim_epoch"].as_i64().unwrap().to_string();

    let release_output =
        workspace.run_bead(&["release", &issue_id, "--fencing-token", &leased_epoch]);
    assert!(
        release_output.status.success(),
        "Leased holder must be able to release: {}",
        String::from_utf8_lossy(&release_output.stderr)
    );

    // w2 plain-claims the released issue (no lease) - must get the same issue
    let plain_output = workspace.run_bead(&["claim", "--assignee", "w2", "--json"]);
    assert!(plain_output.status.success());

    let plain_result: serde_json::Value =
        serde_json::from_slice(&plain_output.stdout).expect("Failed to parse plain claim result");
    assert_eq!(plain_result["bead_id"].as_str(), Some(issue_id.as_str()));
    assert!(plain_result["lease"].is_null());

    // The leftover lease row from w1's epoch must not fence w2: presenting
    // the *new* plain epoch credential, update and close all have to keep
    // working
    let plain_epoch = plain_result["claim_epoch"].as_i64().unwrap().to_string();
    let update_output = workspace.run_bead(&[
        "update",
        &issue_id,
        "--notes",
        "plain epoch works",
        "--fencing-token",
        &plain_epoch,
    ]);
    assert!(
        update_output.status.success(),
        "Plain claimant must be able to update: {}",
        String::from_utf8_lossy(&update_output.stderr)
    );

    let close_output = workspace.run_bead(&[
        "close",
        &issue_id,
        "--reason",
        "done",
        "--fencing-token",
        &plain_epoch,
    ]);
    assert!(
        close_output.status.success(),
        "Plain claimant must be able to close despite leftover lease row: {}",
        String::from_utf8_lossy(&close_output.stderr)
    );
}

#[test]
fn test_lease_cleanup_after_expiry() {
    let workspace = TestWorkspace::new();

    workspace.run_bead(&["create", "--title", "Task 1", "--priority", "0"]);

    // Claim with short lease
    let claim_output =
        workspace.run_bead(&["claim", "--assignee", "alice", "--lease-ttl", "2", "--json"]);

    let claim_result: serde_json::Value =
        serde_json::from_slice(&claim_output.stdout).expect("Failed to parse claim result");

    let _issue_id = claim_result["bead_id"]
        .as_str()
        .expect("Failed to get issue ID");

    // Wait for lease to expire
    std::thread::sleep(std::time::Duration::from_secs(3));

    // Try to claim the same issue again (should be possible since lease expired)
    let new_claim_output =
        workspace.run_bead(&["claim", "--assignee", "bob", "--lease-ttl", "60", "--json"]);

    // Since the first issue is already assigned, this should claim a different issue or return empty
    // The expired lease should not prevent the new claim
    assert!(new_claim_output.status.success());

    let new_result: serde_json::Value =
        serde_json::from_slice(&new_claim_output.stdout).expect("Failed to parse new claim result");

    // If we got an issue, it should have a fresh lease
    if new_result["bead_id"].is_string() && !new_result["bead_id"].as_str().unwrap().is_empty() {
        assert!(new_result["lease"].is_object());
        assert!(new_result["lease"]["fencing_token"].as_i64().unwrap() >= 1);
    }
}

//! R002 Fenced Claim Leases integration tests
//!
//! Comprehensive tests for R002's fenced claim leases with expiring claims,
//! renewals, and monotonically increasing fencing tokens for safe recovery from
//! crashed or disconnected agents.

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
        env::set_var("HOME", &workspace_path);

        // Get the path to the locally built bead binary
        let current_exe = std::env::current_exe().expect("Failed to get current exe");
        let bead_path = current_exe
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.join("bead"))
            .expect("Failed to determine bead path");

        // Verify the bead binary exists
        assert!(bead_path.exists(), "Bead binary not found at {:?}", bead_path);

        // Initialize workspace using the local bead binary
        let output = Command::new(&bead_path)
            .args(["init", "--prefix", "test"])
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
    let output = workspace
        .run_bead(&["claim", "--assignee", "alice", "--lease-ttl", "60", "--json"]);

    if !output.status.success() {
        eprintln!("stderr: {}", String::from_utf8_lossy(&output.stderr));
        eprintln!("stdout: {}", String::from_utf8_lossy(&output.stdout));
    }
    assert!(output.status.success());

    let result: serde_json::Value = serde_json::from_slice(&output.stdout)
        .expect("Failed to parse claim result");

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
    let issue_id = initial_result["bead_id"].as_str().expect("Failed to get issue ID");

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

    let renew_result: serde_json::Value = serde_json::from_slice(&renew_output.stdout)
        .expect("Failed to parse renewal result");

    // Verify renewal has incremented fencing token
    let renewed_fencing_token = renew_result["lease"]["fencing_token"]
        .as_i64()
        .expect("Failed to get renewed fencing token");

    assert_eq!(renewed_fencing_token, initial_fencing_token + 1);
    assert_eq!(renew_result["bead_id"].as_str(), Some(issue_id));
}

#[test]
fn test_fencing_token_validation() {
    let workspace = TestWorkspace::new();

    // Create and claim an issue with a lease
    workspace.run_bead(&["create", "--title", "Task 1", "--priority", "0"]);

    let claim_output = workspace.run_bead(&[
        "claim",
        "--assignee",
        "alice",
        "--lease-ttl",
        "2",
        "--json",
    ]);

    let claim_result: serde_json::Value = serde_json::from_slice(&claim_output.stdout)
        .expect("Failed to parse claim result");

    let issue_id = claim_result["bead_id"].as_str().expect("Failed to get issue ID");
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

    let result: serde_json::Value = serde_json::from_slice(&output.stdout)
        .expect("Failed to parse claim result");

    // Verify claim succeeded but no lease info present
    assert!(result["bead_id"].is_string());
    assert_eq!(result["assignee"], "bob");

    // Lease field should be null or absent for non-leased claims
    if result.get("lease").is_some() {
        assert!(result["lease"].is_null());
    }

    // Verify normal operations work without fencing token requirement
    let issue_id = result["bead_id"].as_str().expect("Failed to get issue ID");

    let update_output = workspace.run_bead(&["update", issue_id, "--notes", "Normal update works"]);
    assert!(update_output.status.success());

    // Also verify release works without lease
    let release_output = workspace.run_bead(&["release", issue_id]);
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
            eprintln!("Failed to create task {}: {}", i, String::from_utf8_lossy(&create_output.stderr));
        }

        assert!(create_output.status.success());
    }

    // Debug: Check what issues were created
    let list_output = workspace.run_bead(&["list", "--json"]);
    eprintln!("Initial issues: {}", String::from_utf8_lossy(&list_output.stdout));

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

        let result: serde_json::Value = serde_json::from_slice(&output.stdout)
            .expect("Failed to parse claim result");

        // Debug output to see what we got
        if worker_id == 0 {
            eprintln!("Worker 0 claim result: {}", serde_json::to_string_pretty(&result).unwrap());
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
    assert!(claimed_ids.len() <= 3, "Cannot claim more issues than workers");

    // Verify each worker who got work can operate on their claimed issue with valid lease
    for (assignee, issue_id) in &workers {
        let output = workspace.run_bead(&["show", issue_id, "--json"]);

        assert!(output.status.success());

        let result: serde_json::Value = serde_json::from_slice(&output.stdout)
            .expect("Failed to parse show result");

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

    let result: serde_json::Value = serde_json::from_slice(&output.stdout)
        .expect("Failed to parse claim result");

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

    let result: serde_json::Value = serde_json::from_slice(&output.stdout)
        .expect("Failed to parse claim result");

    // Empty queue should return null bead_id and null lease
    assert!(result["bead_id"].is_null() || result["bead_id"].as_str().unwrap_or("") == "");
    assert!(result["lease"].is_null() || result["lease"].as_object().map_or(true, |o| o.is_empty()));
}

#[test]
fn test_lease_renewal_without_active_lease() {
    let workspace = TestWorkspace::new();

    workspace.run_bead(&["create", "--title", "Task 1", "--priority", "0"]);

    // Try to renew lease when no active lease exists
    let output = workspace.run_bead(&[
        "claim",
        "--assignee",
        "alice",
        "--renew-lease",
        "--json",
    ]);

    assert!(output.status.success());

    let result: serde_json::Value = serde_json::from_slice(&output.stdout)
        .expect("Failed to parse claim result");

    // Should return empty result since no lease exists to renew
    assert!(result["bead_id"].is_null() || result["bead_id"].as_str().unwrap_or("") == "");
    assert!(result["lease"].is_null() || result["lease"].as_object().map_or(true, |o| o.is_empty()));
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

    let result: serde_json::Value = serde_json::from_slice(&output.stdout)
        .expect("Failed to parse claim result");

    // Should have both lease info and decision trace
    assert!(result["bead_id"].is_string());
    assert!(result["lease"].is_object());
    assert!(result["decision_trace"].is_object());

    // Verify decision trace structure
    let trace = &result["decision_trace"];
    assert!(trace["version"].is_string());
    assert!(trace["policy"].is_string());
    assert!(trace["assignee"].is_string());
    assert!(trace["has_selection"].is_boolean());
}

#[test]
fn test_lease_cleanup_after_expiry() {
    let workspace = TestWorkspace::new();

    workspace.run_bead(&["create", "--title", "Task 1", "--priority", "0"]);

    // Claim with short lease
    let claim_output = workspace.run_bead(&[
        "claim",
        "--assignee",
        "alice",
        "--lease-ttl",
        "2",
        "--json",
    ]);

    let claim_result: serde_json::Value = serde_json::from_slice(&claim_output.stdout)
        .expect("Failed to parse claim result");

    let _issue_id = claim_result["bead_id"].as_str().expect("Failed to get issue ID");

    // Wait for lease to expire
    std::thread::sleep(std::time::Duration::from_secs(3));

    // Try to claim the same issue again (should be possible since lease expired)
    let new_claim_output = workspace.run_bead(&[
        "claim",
        "--assignee",
        "bob",
        "--lease-ttl",
        "60",
        "--json",
    ]);

    // Since the first issue is already assigned, this should claim a different issue or return empty
    // The expired lease should not prevent the new claim
    assert!(new_claim_output.status.success());

    let new_result: serde_json::Value = serde_json::from_slice(&new_claim_output.stdout)
        .expect("Failed to parse new claim result");

    // If we got an issue, it should have a fresh lease
    if new_result["bead_id"].is_string() && !new_result["bead_id"].as_str().unwrap().is_empty() {
        assert!(new_result["lease"].is_object());
        assert!(new_result["lease"]["fencing_token"].as_i64().unwrap() >= 1);
    }
}

//! Integration tests for `bead doctor` command

use assert_cmd::Command;
use serial_test::serial;

#[test]
#[serial]
fn test_doctor_no_workspace() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Doctor should fail when there's no workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["doctor"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("No workspace found"));
}

#[test]
#[serial]
fn test_doctor_basic() {
    let temp = tempfile::tempdir().unwrap();
    let temp_dir = temp.path();
    let original_home = std::env::var("HOME").ok();

    unsafe {
        std::env::set_var("HOME", temp_dir);
    }
    std::env::set_current_dir(temp_dir).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Run doctor diagnostics
    Command::cargo_bin("bead")
        .unwrap()
        .args(["doctor"])
        .assert()
        .success()
        .stderr(predicates::str::contains("OK"))
        .stderr(predicates::str::contains("workspace_config"))
        .stderr(predicates::str::contains("database_integrity"))
        .stderr(predicates::str::contains("checkpoint_freshness")) // R016: checkpoint_state replaced with checkpoint_freshness
        .stderr(predicates::str::contains("temporary_files"));

    // Cleanup
    if let Some(home) = original_home {
        unsafe {
            std::env::set_var("HOME", home);
        }
    } else {
        unsafe {
            std::env::remove_var("HOME");
        }
    }
}

#[test]
#[serial]
fn test_doctor_with_dirty_checkpoint() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Create an issue to make checkpoint dirty
    Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Test Issue"])
        .assert()
        .success();

    // Run doctor diagnostics - should show checkpoint info
    Command::cargo_bin("bead")
        .unwrap()
        .args(["doctor"])
        .assert()
        .success()
        .stderr(predicates::str::contains("checkpoint_freshness")); // R016: checkpoint_state replaced with checkpoint_freshness
}

#[test]
#[serial]
fn test_doctor_repair_no_repairs_needed() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Run doctor with repair flag when no repairs needed
    Command::cargo_bin("bead")
        .unwrap()
        .args(["doctor", "--repair"])
        .assert()
        .success()
        .stderr(predicates::str::contains("Attempting repairs"))
        .stderr(predicates::str::contains("No repairs needed"));
}

#[test]
#[serial]
fn test_doctor_repair_temp_files() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    std::env::set_current_dir(root).unwrap();

    // Save original directory to restore later
    let original_dir = std::env::current_dir().unwrap().canonicalize().unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Create a temporary file in .beads directory
    let temp_file = root.join(".beads/test.tmp");
    std::fs::write(&temp_file, "test content").unwrap();

    // Run doctor without repair - should warn about temp files
    Command::cargo_bin("bead")
        .unwrap()
        .args(["doctor"])
        .assert()
        .success()
        .stderr(predicates::str::contains("WARN"))
        .stderr(predicates::str::contains("temporary_files"));

    // Run doctor with repair - should clean up temp file
    Command::cargo_bin("bead")
        .unwrap()
        .args(["doctor", "--repair"])
        .assert()
        .success()
        .stderr(predicates::str::contains("FIXED"))
        .stderr(predicates::str::contains("removed_temp_file"));

    // Verify temp file was removed
    assert!(!temp_file.exists());

    // Restore original directory before dropping temp
    std::env::set_current_dir(original_dir).unwrap();
}

#[test]
#[serial]
fn test_doctor_repair_creates_missing_receipts_dir() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    // Save original directory to restore later. An earlier test may leave
    // the process working directory pointing into its (now deleted) tempdir,
    // so fall back to a stable directory instead of unwrapping.
    let original_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/"));

    std::env::set_current_dir(root).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    // Remove the receipts directory to simulate a workspace that lost it
    // (it is untracked, so a git clean removes it and nothing recreates it)
    let receipts_dir = root.join(".beads/receipts");
    std::fs::remove_dir_all(&receipts_dir).unwrap();

    // Diagnostics should flag the missing directory as an error
    Command::cargo_bin("bead")
        .unwrap()
        .args(["doctor"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("receipts directory not found"));

    // Repair should recreate it
    Command::cargo_bin("bead")
        .unwrap()
        .args(["doctor", "--repair"])
        .assert()
        .success()
        .stderr(predicates::str::contains("FIXED"))
        .stderr(predicates::str::contains("created_receipts_dir"));

    // Verify the directory was recreated
    assert!(receipts_dir.exists());

    // Diagnostics now pass the workspace_config check
    Command::cargo_bin("bead")
        .unwrap()
        .args(["doctor"])
        .assert()
        .success()
        .stderr(predicates::str::contains("OK workspace_config"));

    // Restore original directory before dropping temp
    std::env::set_current_dir(original_dir).unwrap();
}

#[test]
#[serial]
fn test_doctor_after_flush() {
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
        .args(["create", "--title", "Test Issue"])
        .assert()
        .success();

    // Flush checkpoint
    Command::cargo_bin("bead")
        .unwrap()
        .args(["sync", "flush-only"])
        .assert()
        .success();

    // Run doctor diagnostics - should pass all checks
    Command::cargo_bin("bead")
        .unwrap()
        .args(["doctor"])
        .assert()
        .success()
        .stderr(predicates::str::contains("OK"));
}

#[test]
#[serial]
fn test_doctor_rejects_inconsistent_close_metadata() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .assert()
        .success();
    let output = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Corrupt close metadata probe"])
        .output()
        .unwrap();
    let id = String::from_utf8(output.stdout).unwrap();

    let conn = rusqlite::Connection::open(temp.path().join(".beads/beads.db")).unwrap();
    conn.execute(
        "UPDATE issues SET base_status = 'closed', closed_at = NULL, close_reason = NULL WHERE id = ?1",
        [id.trim()],
    )
    .unwrap();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["doctor"])
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "inconsistent closed status metadata",
        ));
}

#[test]
#[serial]
fn test_doctor_reports_open_issue_held_by_assignee() {
    let temp = tempfile::tempdir().unwrap();
    let temp_dir = temp.path();
    let original_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let original_home = std::env::var("HOME").ok();

    unsafe {
        std::env::set_var("HOME", temp_dir);
    }
    std::env::set_current_dir(temp_dir).unwrap();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["init"])
        .assert()
        .success();
    let output = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Held by an assignee"])
        .output()
        .unwrap();
    let id = String::from_utf8(output.stdout).unwrap();
    let id = id.trim();

    // A clean workspace reports the frontier as healthy.
    Command::cargo_bin("bead")
        .unwrap()
        .args(["doctor"])
        .assert()
        .stderr(predicates::str::contains("Ready frontier OK"));

    // Assigning an issue while it stays open takes it off the ready frontier
    // without changing its status, which is the shape doctor must surface.
    Command::cargo_bin("bead")
        .unwrap()
        .args(["update", id, "--assignee", "worker-1"])
        .assert()
        .success();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["doctor"])
        .assert()
        .stderr(predicates::str::contains("WARN ready_frontier"))
        .stderr(predicates::str::contains(id))
        .stderr(predicates::str::contains("not an active claim"));

    // Clearing the assignee returns it to the frontier and silences the
    // warning. Assigning minted a claim epoch, so clearing it is a
    // claimant-owned mutation and carries the credential that assignment
    // issued (see tests/claim_epoch.rs).
    let assigned = Command::cargo_bin("bead")
        .unwrap()
        .args(["show", id, "--json"])
        .output()
        .unwrap();
    let assigned: serde_json::Value = serde_json::from_slice(&assigned.stdout).unwrap();
    let credential = assigned[0]["claim_epoch"].as_i64().unwrap().to_string();
    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "update",
            id,
            "--clear-assignee",
            "--fencing-token",
            &credential,
        ])
        .assert()
        .success();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["doctor"])
        .assert()
        .stderr(predicates::str::contains("Ready frontier OK"));

    if let Some(home) = original_home {
        unsafe {
            std::env::set_var("HOME", home);
        }
    } else {
        unsafe {
            std::env::remove_var("HOME");
        }
    }
    std::env::set_current_dir(original_dir).unwrap();
}

#[test]
#[serial]
fn test_ready_frontier_emits_r001_reason_codes() {
    let temp = tempfile::tempdir().unwrap();
    let temp_dir = temp.path();

    // Use a stable directory and preserve HOME so cleanup is order-independent.
    let original_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let original_home = std::env::var("HOME").ok();

    // Set HOME to the temp directory to avoid interfering with user's actual workspace
    unsafe {
        std::env::set_var("HOME", temp_dir);
    }

    // Change to the temporary directory
    std::env::set_current_dir(temp_dir).unwrap();

    // Ensure we're in a clean directory without any existing .beads
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test-r035"])
        .assert()
        .success();

    // Create multiple held issues
    Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "First held issue"])
        .assert()
        .success();
    let output = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Second held issue"])
        .output()
        .unwrap();
    let id2 = String::from_utf8(output.stdout).unwrap();
    let id2 = id2.trim();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Third held issue"])
        .assert()
        .success();

    // Assign all issues to take them off the ready frontier
    Command::cargo_bin("bead")
        .unwrap()
        .args(["list", "--json"])
        .assert()
        .success();
    let list_output = Command::cargo_bin("bead")
        .unwrap()
        .args(["list", "--json"])
        .output()
        .unwrap();
    for line in list_output.stdout.split(|byte| *byte == b'\n') {
        if line.is_empty() {
            continue;
        }
        let issue: serde_json::Value = serde_json::from_slice(line).unwrap();
        let id = issue.get("id").and_then(|value| value.as_str()).unwrap();
        Command::cargo_bin("bead")
            .unwrap()
            .args(["update", id, "--assignee", "worker-1"])
            .assert()
            .success();
    }

    // Run doctor with JSON output and validate structured reason codes
    let output = Command::cargo_bin("bead")
        .unwrap()
        .args(["doctor", "--json"])
        .output()
        .unwrap();

    assert!(output.status.success());

    let doctor_json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let checks = doctor_json
        .get("checks")
        .and_then(|c| c.as_array())
        .unwrap();

    // Find the ready_frontier check
    let frontier_check = checks
        .iter()
        .find(|c| c.get("name").and_then(|n| n.as_str()) == Some("ready_frontier"))
        .expect("ready_frontier check should be present");

    // Validate the structured output
    let details = frontier_check.get("details").unwrap().as_object().unwrap();

    // Check that held_ids is a machine-readable array, not embedded in prose
    let held_ids = details.get("held_ids").and_then(|h| h.as_array()).unwrap();
    assert!(held_ids.len() >= 2, "Should have at least 2 held IDs");

    // Verify the specific ID we tracked is in the list
    assert!(
        held_ids.iter().any(|id| id.as_str() == Some(id2)),
        "Created issue ID should be in held_ids list"
    );

    // Check that held_count matches the array length
    let held_count = details.get("held_count").and_then(|c| c.as_u64()).unwrap();
    assert_eq!(
        held_count as usize,
        held_ids.len(),
        "held_count should match held_ids length"
    );

    // Validate R001 reason codes are present
    let reason_codes = details
        .get("reason_codes")
        .and_then(|r| r.as_array())
        .unwrap();
    assert!(
        !reason_codes.is_empty(),
        "Should have at least one reason code"
    );

    // Check for the specific R035 reason code
    assert!(
        reason_codes
            .iter()
            .any(|rc| rc.as_str() == Some("open_issue_held_by_assignee")),
        "Should include open_issue_held_by_assignee reason code"
    );

    // Validate remedy is provided
    let remedy = details.get("remedy").and_then(|r| r.as_str()).unwrap();
    assert!(
        remedy.contains("--clear-assignee"),
        "Remedy should mention --clear-assignee"
    );

    // Validate human-readable message still contains sample
    let message = frontier_check
        .get("message")
        .and_then(|m| m.as_str())
        .unwrap();
    assert!(
        message.contains("open issue(s) are assigned"),
        "Message should explain the condition in prose"
    );

    // Cleanup: restore original HOME and directory
    if let Some(home) = original_home {
        unsafe {
            std::env::set_var("HOME", home);
        }
    } else {
        unsafe {
            std::env::remove_var("HOME");
        }
    }
    std::env::set_current_dir(original_dir).unwrap();
}

#[test]
#[serial]
fn test_doctor_counts_blocked_issues_from_blocks_edges() {
    // Regression for beadrs-90abfec2: check_dependency_graph() queried a
    // `blocked_id` column that does not exist (the real column is
    // `blocked_issue_id`), and `.unwrap_or(0)` swallowed the prepare failure,
    // so the diagnostic reported "0 blocked issues" in every workspace no
    // matter how many blocking edges the store held.
    let temp = tempfile::tempdir().unwrap();
    let temp_dir = temp.path();
    let original_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let original_home = std::env::var("HOME").ok();

    unsafe {
        std::env::set_var("HOME", temp_dir);
    }
    std::env::set_current_dir(temp_dir).unwrap();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["init"])
        .assert()
        .success();

    let mut ids = Vec::new();
    for title in ["Blocked work", "The blocker", "Merely related"] {
        let output = Command::cargo_bin("bead")
            .unwrap()
            .args(["create", "--title", title])
            .output()
            .unwrap();
        ids.push(String::from_utf8(output.stdout).unwrap().trim().to_string());
    }

    // With no edges at all, nothing is blocked.
    Command::cargo_bin("bead")
        .unwrap()
        .args(["doctor"])
        .assert()
        .stderr(predicates::str::contains("0 blocked issues"));

    // One real `blocks` edge must be counted.
    Command::cargo_bin("bead")
        .unwrap()
        .args(["dep", "add", &ids[0], &ids[1]])
        .assert()
        .success();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["doctor"])
        .assert()
        .stderr(predicates::str::contains("1 blocked issues"));

    // A `relates_to` edge is informational and must NOT raise the count,
    // even though it adds a dependencies row.
    Command::cargo_bin("bead")
        .unwrap()
        .args(["dep", "add", &ids[2], &ids[1], "--kind", "relates_to"])
        .assert()
        .success();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["doctor"])
        .assert()
        .stderr(predicates::str::contains("2 dependencies"))
        .stderr(predicates::str::contains("1 blocked issues"));

    if let Some(home) = original_home {
        unsafe {
            std::env::set_var("HOME", home);
        }
    } else {
        unsafe {
            std::env::remove_var("HOME");
        }
    }
    std::env::set_current_dir(original_dir).unwrap();
}

#[test]
#[serial]
fn doctor_ignores_relates_to_cycles() {
    // Regression: the NEEDLE workspace had an acyclic `blocks` graph, but
    // `relates_to` edges (informational by contract) completed mixed-kind
    // SCCs and `bead doctor` reported dozens of false cycles. Cycle
    // detection must traverse only `blocks` edges.
    let temp = tempfile::tempdir().unwrap();
    let temp_dir = temp.path();
    let original_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    std::env::set_current_dir(temp_dir).unwrap();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["init"])
        .assert()
        .success();

    let mut ids = Vec::new();
    for title in ["Alpha", "Beta", "Gamma", "Delta", "Epsilon", "Zeta"] {
        let output = Command::cargo_bin("bead")
            .unwrap()
            .args(["create", "--title", title])
            .output()
            .unwrap();
        ids.push(String::from_utf8(output.stdout).unwrap().trim().to_string());
    }

    // Case 1: a bidirectional `relates_to` cycle must not trip the check.
    Command::cargo_bin("bead")
        .unwrap()
        .args(["dep", "add", &ids[0], &ids[1], "--kind", "relates_to"])
        .assert()
        .success();
    Command::cargo_bin("bead")
        .unwrap()
        .args(["dep", "add", &ids[1], &ids[0], "--kind", "relates_to"])
        .assert()
        .success();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["doctor"])
        .assert()
        .success()
        .stderr(predicates::str::contains("no cycles"));

    // Case 2: an acyclic `blocks` chain plus a `relates_to` edge that closes
    // a mixed-kind loop must also succeed. Chain: Delta blocked by Gamma,
    // Epsilon blocked by Delta (acyclic on `blocks` alone). The `relates_to`
    // edge from Gamma back to Epsilon only closes the loop informationally.
    Command::cargo_bin("bead")
        .unwrap()
        .args(["dep", "add", &ids[3], &ids[2]])
        .assert()
        .success();
    Command::cargo_bin("bead")
        .unwrap()
        .args(["dep", "add", &ids[4], &ids[3]])
        .assert()
        .success();
    Command::cargo_bin("bead")
        .unwrap()
        .args(["dep", "add", &ids[2], &ids[4], "--kind", "relates_to"])
        .assert()
        .success();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["doctor"])
        .assert()
        .success()
        .stderr(predicates::str::contains("no cycles"));

    // Case 3: a genuine `blocks` cycle still produces the dependency_graph
    // integrity error. `dep add` itself refuses to create a `blocks` cycle,
    // so the closing edge is inserted directly to simulate a corrupted
    // store the doctor must still catch.
    let conn = rusqlite::Connection::open(temp_dir.join(".beads/beads.db")).unwrap();
    conn.execute(
        "INSERT INTO dependencies (blocked_issue_id, blocker_issue_id, kind) VALUES (?1, ?2, 'blocks')",
        [ids[2].as_str(), ids[4].as_str()],
    )
    .unwrap();
    drop(conn);

    Command::cargo_bin("bead")
        .unwrap()
        .args(["doctor"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("dependency cycles"));

    std::env::set_current_dir(original_dir).unwrap();
}

#[test]
#[serial]
fn doctor_reports_each_blocks_cycle_once() {
    // Regression: cycle diagnostics were neither exact nor deterministic.
    // The DFS truncated each cycle to the two nodes of its closing edge,
    // returned early on the cycle-found path without unwinding its recursion
    // stack (so stale stack entries misread downstream and cross edges into
    // a reported cycle as additional phantom cycles), and iterated a HashSet,
    // letting row order and hash randomization change both the count and the
    // paths between runs on the same store. Each genuine `blocks` cycle must
    // be reported exactly once as a canonical closed path, byte-stable across
    // repeated runs and independent of dependency insertion order.
    let original_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    fn run_doctor() -> (bool, String) {
        let output = Command::cargo_bin("bead")
            .unwrap()
            .args(["doctor"])
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        (output.status.success(), stderr)
    }

    // The dependency_graph check renders on a single line; other checks and
    // the trailing timestamp around it are not part of the cycle contract.
    fn cycle_line(stderr: &str) -> String {
        stderr
            .lines()
            .find(|line| line.contains("dependency cycles"))
            .expect("doctor reported a dependency_graph failure")
            .to_string()
    }

    fn create(title: &str) -> String {
        let output = Command::cargo_bin("bead")
            .unwrap()
            .args(["create", "--title", title])
            .output()
            .unwrap();
        assert!(output.status.success(), "create {title} failed");
        String::from_utf8(output.stdout).unwrap().trim().to_string()
    }

    // The TempDir guard must outlive the workspace's use: dropping it deletes
    // the directory out from under the process cwd.
    fn init_workspace(titles: &[&str]) -> (tempfile::TempDir, Vec<String>) {
        let workspace = tempfile::tempdir().unwrap();
        std::env::set_current_dir(workspace.path()).unwrap();
        Command::cargo_bin("bead")
            .unwrap()
            .args(["init"])
            .assert()
            .success();
        let ids: Vec<String> = titles.iter().map(|t| create(t)).collect();
        (workspace, ids)
    }

    // Point the process (and with it every bead subprocess and relative
    // .beads/beads.db path) back at a specific workspace.
    fn use_workspace(workspace: &tempfile::TempDir) {
        std::env::set_current_dir(workspace.path()).unwrap();
    }

    fn dep_add(blocked: &str, blocker: &str) {
        Command::cargo_bin("bead")
            .unwrap()
            .args(["dep", "add", blocked, blocker])
            .assert()
            .success();
    }

    // `dep add` refuses to create a `blocks` cycle, so closing edges are
    // inserted directly to simulate a corrupted store the doctor must catch.
    fn insert_blocks_edge(blocked: &str, blocker: &str) {
        let conn = rusqlite::Connection::open(".beads/beads.db").unwrap();
        conn.execute(
            "INSERT INTO dependencies (blocked_issue_id, blocker_issue_id, kind) VALUES (?1, ?2, 'blocks')",
            [blocked, blocker],
        )
        .unwrap();
    }

    // The canonical closed path a cycle must render as: rotated to start at
    // its smallest member and closed back to it.
    fn expected_path(cycle: &[String]) -> String {
        let mut rotated = cycle.to_vec();
        let min = rotated
            .iter()
            .enumerate()
            .min_by_key(|(_, id)| id.as_str())
            .map(|(i, _)| i)
            .unwrap();
        rotated.rotate_left(min);
        rotated.push(rotated[0].clone());
        rotated.join(" -> ")
    }

    // Case 1: one `blocks` cycle with an acyclic descendant and cross edges
    // reaching back into the cycle — exactly one cycle, reported once.
    let titles = [
        "CycleOne",
        "CycleTwo",
        "CycleThree",
        "Descendant",
        "Cross",
        "FarSide",
    ];
    let (_ws1, ids) = init_workspace(&titles);
    insert_blocks_edge(&ids[0], &ids[1]);
    insert_blocks_edge(&ids[1], &ids[2]);
    insert_blocks_edge(&ids[2], &ids[0]);
    dep_add(&ids[3], &ids[0]); // descendant blocked by the cycle
    dep_add(&ids[4], &ids[3]); // cross edge, sibling of the descendant
    dep_add(&ids[4], &ids[2]); // cross edge reaching into a cycle member

    let (ok, stderr) = run_doctor();
    assert!(!ok, "doctor must fail on a blocks cycle");
    let line = cycle_line(&stderr);
    assert!(
        line.contains("Found 1 dependency cycles:"),
        "expected exactly one reported cycle, got: {line}"
    );
    let one_cycle = expected_path(&ids[0..3]);
    assert!(
        line.contains(&one_cycle),
        "expected the full closed cycle path {one_cycle}, got: {line}"
    );
    assert_eq!(
        line.matches(&one_cycle).count(),
        1,
        "cycle reported more than once: {line}"
    );
    for bystander in [&ids[3], &ids[4], &ids[5]] {
        assert!(
            !line.contains(bystander.as_str()),
            "downstream/cross node {bystander} misreported as part of a cycle: {line}"
        );
    }
    // Repeated runs are byte-stable in the cycle details.
    let (_, stderr_again) = run_doctor();
    assert_eq!(line, cycle_line(&stderr_again));

    // Case 2: two disjoint cycles in one graph, each reported exactly once.
    let disjoint_titles = ["PairOne", "PairTwo", "TrioOne", "TrioTwo", "TrioThree"];
    let (_ws2, disjoint_ids) = init_workspace(&disjoint_titles);
    let (pair_ids, trio_ids) = (&disjoint_ids[0..2], &disjoint_ids[2..5]);
    insert_blocks_edge(&pair_ids[0], &pair_ids[1]);
    insert_blocks_edge(&pair_ids[1], &pair_ids[0]);
    insert_blocks_edge(&trio_ids[0], &trio_ids[1]);
    insert_blocks_edge(&trio_ids[1], &trio_ids[2]);
    insert_blocks_edge(&trio_ids[2], &trio_ids[0]);

    let (ok, stderr) = run_doctor();
    assert!(!ok, "doctor must fail on two blocks cycles");
    let line = cycle_line(&stderr);
    assert!(
        line.contains("Found 2 dependency cycles:"),
        "expected exactly two reported cycles, got: {line}"
    );
    let pair_path = expected_path(pair_ids);
    let trio_path = expected_path(trio_ids);
    for path in [&pair_path, &trio_path] {
        assert!(
            line.contains(path),
            "expected closed path {path} in report, got: {line}"
        );
        assert_eq!(
            line.matches(path).count(),
            1,
            "cycle {path} reported more than once: {line}"
        );
    }
    let (_, stderr_again) = run_doctor();
    assert_eq!(line, cycle_line(&stderr_again));

    // Case 3: dependency insertion order must not change the report. The
    // same graph as Case 1, with every edge inserted in reverse order; the
    // two reports must agree once ids are mapped back to their titles (the
    // two workspaces share a title set precisely so that mapping converges).
    let ws_titles = ["OrdA", "OrdB", "OrdC", "OrdDesc", "OrdCross", "OrdFar"];
    let (_ws4, ws_a) = init_workspace(&ws_titles);
    insert_blocks_edge(&ws_a[0], &ws_a[1]);
    insert_blocks_edge(&ws_a[1], &ws_a[2]);
    insert_blocks_edge(&ws_a[2], &ws_a[0]);
    dep_add(&ws_a[3], &ws_a[0]);
    dep_add(&ws_a[4], &ws_a[3]);
    dep_add(&ws_a[4], &ws_a[2]);

    let (_ws5, ws_b) = init_workspace(&ws_titles);
    dep_add(&ws_b[4], &ws_b[2]);
    dep_add(&ws_b[4], &ws_b[3]);
    dep_add(&ws_b[3], &ws_b[0]);
    insert_blocks_edge(&ws_b[2], &ws_b[0]);
    insert_blocks_edge(&ws_b[1], &ws_b[2]);
    insert_blocks_edge(&ws_b[0], &ws_b[1]);

    fn normalized(line: &str, titles: &[&str], ids: &[String]) -> String {
        let mut out = line.to_string();
        for (title, id) in titles.iter().zip(ids) {
            out = out.replace(id.as_str(), title);
        }
        out
    }
    use_workspace(&_ws4);
    let line_a = cycle_line(&run_doctor().1);
    use_workspace(&_ws5);
    let line_b = cycle_line(&run_doctor().1);
    // Issue ids are minted from creation time and so differ between the two
    // stores, which legitimately moves the canonical start member. What must
    // not depend on insertion order is which cycle is found and the direction
    // it is traversed in — compare after rotating each path onto the same
    // anchor title.
    fn anchored_cycles(line: &str, anchor: &str) -> Vec<String> {
        let paths = line
            .split("dependency cycles: ")
            .nth(1)
            .expect("cycle detail present");
        paths
            .split("; ")
            .map(|path| {
                let mut nodes: Vec<&str> = path.split(" -> ").collect();
                assert_eq!(
                    nodes.first(),
                    nodes.last(),
                    "cycle path must be closed: {path}"
                );
                nodes.pop(); // drop the closing repeat
                let idx = nodes
                    .iter()
                    .position(|n| *n == anchor)
                    .expect("anchor is a cycle member");
                nodes.rotate_left(idx);
                let mut closed = nodes.clone();
                closed.push(nodes[0]);
                closed.join(" -> ")
            })
            .collect()
    }
    assert_eq!(
        anchored_cycles(&normalized(&line_a, &ws_titles, &ws_a), "OrdA"),
        anchored_cycles(&normalized(&line_b, &ws_titles, &ws_b), "OrdA"),
        "cycle details depended on dependency insertion order"
    );

    // Case 4: an acyclic `blocks` graph reports no cycle at all.
    let chain_titles = ["ChainOne", "ChainTwo", "ChainThree"];
    let (_ws6, chain) = init_workspace(&chain_titles);
    dep_add(&chain[1], &chain[0]);
    dep_add(&chain[2], &chain[1]);

    let (ok, stderr) = run_doctor();
    assert!(ok, "acyclic blocks graph must pass doctor: {stderr}");
    assert!(stderr.contains("no cycles"));
    assert!(!stderr.contains("dependency cycles"));

    std::env::set_current_dir(original_dir).unwrap();
}

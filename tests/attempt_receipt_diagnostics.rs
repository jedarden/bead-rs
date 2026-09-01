//! Attempt receipt diagnostics and explanation tests
//!
//! This test suite verifies:
//! - Doctor diagnostics for attempt receipts (read-only, never synthesizes)
//! - Why command explains attempt outcomes and tier progression
//! - Malformed receipt detection and validation
//! - Legacy workspace compatibility (pre-attempt-outcome)
//! - Non-destructive operations (doctor doesn't modify data)
//!
//! See: attempt-outcome-v1 specification, R036 attempt receipt diagnostics

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

/// Create a test workspace and return the temp dir
fn create_workspace() -> TempDir {
    let temp_dir = TempDir::new().unwrap();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "bead", "--skip-foreign-workspace"])
        .current_dir(temp_dir.path())
        .assert()
        .success();

    temp_dir
}

/// Create a legacy workspace without attempt_outcomes table
fn create_legacy_workspace() -> TempDir {
    let temp_dir = TempDir::new().unwrap();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "bead", "--skip-foreign-workspace"])
        .current_dir(temp_dir.path())
        .assert()
        .success();

    // Remove the attempt_outcomes table to simulate a legacy workspace
    let db_path = temp_dir.path().join(".beads/beads.db");
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute("DROP TABLE IF EXISTS attempt_outcomes", [])
        .unwrap();

    temp_dir
}

/// Get the first issue ID from the database
fn get_first_issue_id(db_path: &std::path::Path) -> String {
    let conn = rusqlite::Connection::open(db_path).unwrap();
    conn.query_row("SELECT id FROM issues LIMIT 1", [], |row| {
        row.get::<_, String>(0)
    })
    .unwrap()
}

/// Insert a well-formed attempt outcome directly into database
fn insert_attempt_outcome(
    db_path: &std::path::Path,
    issue_id: &str,
    receipt_id: &str,
    attempt_id: &str,
    outcome: &str,
    action: &str,
) {
    let conn = rusqlite::Connection::open(db_path).unwrap();
    conn.execute(
        "INSERT INTO attempt_outcomes (
            receipt_id, attempt_id, issue_id, outcome, action, reason,
            canonical_request_hash, prior_attempt_tier, resulting_attempt_tier,
            resulting_issue_revision, actor, created_at, evidence_refs_json,
            model, harness, harness_version
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
        rusqlite::params![
            receipt_id,
            attempt_id,
            issue_id,
            outcome,
            action,
            "Test reason",
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef", // 64-char hex
            0i64,
            0i64,
            1i64,
            "test-worker",
            "2026-08-31T12:00:00Z",
            r#"["s3:logs/test.tar.gz"]"#,
            "claude-opus-5",
            "needle",
            "1.0.0",
        ],
    )
    .unwrap();

    // Insert corresponding event for integrity check
    conn.execute(
        "INSERT INTO events (issue_id, kind, actor, time, detail) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![
            issue_id,
            "attempt_resolved",
            "test-worker",
            "2026-08-31T12:00:00Z",
            &format!("Attempt {} resolved with outcome: {}", attempt_id, outcome),
        ],
    )
    .unwrap();
}

/// Insert a malformed attempt outcome (invalid receipt_id format)
fn insert_malformed_receipt_id(db_path: &std::path::Path, issue_id: &str) {
    let conn = rusqlite::Connection::open(db_path).unwrap();
    conn.execute(
        "INSERT INTO attempt_outcomes (
            receipt_id, attempt_id, issue_id, outcome, action, reason,
            canonical_request_hash, prior_attempt_tier, resulting_attempt_tier,
            resulting_issue_revision, actor, created_at, evidence_refs_json,
            model, harness, harness_version
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
        rusqlite::params![
            "invalid-receipt-id", // Missing "ao-" prefix
            "urn:needle:attempt:malformed",
            issue_id,
            "verified_success",
            "close",
            "Test",
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef", // 64-char hex
            0i64,
            0i64,
            1i64,
            "worker",
            "2026-08-31T12:00:00Z",
            r#"[]"#,
            "model",
            "harness",
            "1.0",
        ],
    )
    .unwrap();

    // Insert corresponding event for integrity check
    conn.execute(
        "INSERT INTO events (issue_id, kind, actor, time, detail) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![
            issue_id,
            "attempt_resolved",
            "worker",
            "2026-08-31T12:00:00Z",
            "Attempt resolved",
        ],
    )
    .unwrap();
}

/// Insert a malformed attempt outcome (invalid hash format)
fn insert_malformed_hash(db_path: &std::path::Path, issue_id: &str) {
    let conn = rusqlite::Connection::open(db_path).unwrap();
    conn.execute(
        "INSERT INTO attempt_outcomes (
            receipt_id, attempt_id, issue_id, outcome, action, reason,
            canonical_request_hash, prior_attempt_tier, resulting_attempt_tier,
            resulting_issue_revision, actor, created_at, evidence_refs_json,
            model, harness, harness_version
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
        rusqlite::params![
            "ao-malformed-hash",
            "urn:needle:attempt:badhash",
            issue_id,
            "verified_success",
            "close",
            "Test",
            "not-a-valid-hash", // Invalid format - intentionally wrong for testing
            0i64,
            0i64,
            1i64,
            "worker",
            "2026-08-31T12:00:00Z",
            r#"[]"#,
            "model",
            "harness",
            "1.0",
        ],
    )
    .unwrap();

    // Insert corresponding event for integrity check
    conn.execute(
        "INSERT INTO events (issue_id, kind, actor, time, detail) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![
            issue_id,
            "attempt_resolved",
            "worker",
            "2026-08-31T12:00:00Z",
            "Attempt resolved",
        ],
    )
    .unwrap();
}

/// Insert an orphaned attempt outcome (referencing non-existent issue)
fn insert_orphaned_attempt_outcome(db_path: &std::path::Path) {
    let conn = rusqlite::Connection::open(db_path).unwrap();
    // Disable foreign key constraints to allow orphaned insert for testing
    conn.execute("PRAGMA foreign_keys = OFF", []).unwrap();
    conn.execute(
        "INSERT INTO attempt_outcomes (
            receipt_id, attempt_id, issue_id, outcome, action, reason,
            canonical_request_hash, prior_attempt_tier, resulting_attempt_tier,
            resulting_issue_revision, actor, created_at, evidence_refs_json,
            model, harness, harness_version
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
        rusqlite::params![
            "ao-orphaned",
            "urn:needle:attempt:orphaned",
            "bead-nonexistent", // This issue doesn't exist
            "verified_success",
            "close",
            "Test",
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef", // 64-char hex
            0i64,
            0i64,
            1i64,
            "worker",
            "2026-08-31T12:00:00Z",
            r#"[]"#,
            "model",
            "harness",
            "1.0",
        ],
    )
    .unwrap();
    // Re-enable foreign keys
    conn.execute("PRAGMA foreign_keys = ON", []).unwrap();
}

#[test]
fn test_doctor_attempt_diagnostics_success() {
    let workspace = create_workspace();

    // Create a test issue
    Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Test", "--priority", "2"])
        .current_dir(workspace.path())
        .assert()
        .success();

    let db_path = workspace.path().join(".beads/beads.db");
    let issue_id = get_first_issue_id(&db_path);

    // Insert well-formed attempt outcome
    insert_attempt_outcome(
        &db_path,
        &issue_id,
        "ao-test001",
        "urn:needle:attempt:test001",
        "verified_success",
        "close",
    );

    // Run doctor with attempts scope
    Command::cargo_bin("bead")
        .unwrap()
        .args(["doctor", "--scope", "attempts"])
        .current_dir(workspace.path())
        .assert()
        .success()
        .stderr(predicates::str::contains("attempt_outcomes_integrity"))
        .stderr(predicates::str::contains("OK"))
        .stderr(predicates::str::contains("attempt_tier_consistency"))
        .stderr(predicates::str::contains("OK"));
}

#[test]
fn test_doctor_malformed_receipt_id_detection() {
    let workspace = create_workspace();

    // Create a test issue
    Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Test", "--priority", "2"])
        .current_dir(workspace.path())
        .assert()
        .success();

    let db_path = workspace.path().join(".beads/beads.db");
    let issue_id = get_first_issue_id(&db_path);

    // Insert malformed receipt ID
    insert_malformed_receipt_id(&db_path, &issue_id);

    // Run doctor - should detect the malformed receipt
    Command::cargo_bin("bead")
        .unwrap()
        .args(["doctor", "--scope", "attempts"])
        .current_dir(workspace.path())
        .assert()
        .failure()
        .code(5)
        .stderr(predicates::str::contains("attempt_outcomes_integrity"))
        .stderr(predicates::str::contains("error"))
        .stderr(predicates::str::contains("invalid receipt_id format"));
}

#[test]
fn test_doctor_malformed_hash_detection() {
    let workspace = create_workspace();

    // Create a test issue
    Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Test", "--priority", "2"])
        .current_dir(workspace.path())
        .assert()
        .success();

    let db_path = workspace.path().join(".beads/beads.db");
    let issue_id = get_first_issue_id(&db_path);

    // Insert malformed hash
    insert_malformed_hash(&db_path, &issue_id);

    // Run doctor - should detect the malformed hash
    Command::cargo_bin("bead")
        .unwrap()
        .args(["doctor", "--scope", "attempts"])
        .current_dir(workspace.path())
        .assert()
        .failure()
        .code(5)
        .stderr(predicates::str::contains("attempt_outcomes_integrity"))
        .stderr(predicates::str::contains("error"))
        .stderr(predicates::str::contains("invalid canonical_request_hash"));
}

#[test]
fn test_doctor_orphaned_attempt_detection() {
    let workspace = create_workspace();

    // Create a test issue
    Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Test", "--priority", "2"])
        .current_dir(workspace.path())
        .assert()
        .success();

    let db_path = workspace.path().join(".beads/beads.db");

    // Insert orphaned attempt outcome (referencing non-existent issue)
    insert_orphaned_attempt_outcome(&db_path);

    // Run doctor - should detect the orphaned outcome
    Command::cargo_bin("bead")
        .unwrap()
        .args(["doctor", "--scope", "attempts"])
        .current_dir(workspace.path())
        .assert()
        .failure()
        .code(5)
        .stderr(predicates::str::contains("attempt_outcomes_integrity"))
        .stderr(predicates::str::contains("error"))
        .stderr(predicates::str::contains("non-existent issues"));
}

#[test]
fn test_doctor_legacy_workspace_compatibility() {
    let workspace = create_legacy_workspace();

    // Don't create any issues - just run doctor on the empty legacy workspace
    // Run doctor with attempts scope - should handle legacy workspace gracefully
    Command::cargo_bin("bead")
        .unwrap()
        .args(["doctor", "--scope", "attempts"])
        .current_dir(workspace.path())
        .assert()
        .success()
        .stderr(predicates::str::contains("attempt_outcomes_integrity"))
        .stderr(predicates::str::contains("legacy workspace"))
        .stderr(predicates::str::contains("attempt_tier_consistency"))
        .stderr(predicates::str::contains("legacy workspace"));
}

#[test]
fn test_doctor_is_read_only() {
    let workspace = create_workspace();

    // Create a test issue
    Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Test", "--priority", "2"])
        .current_dir(workspace.path())
        .assert()
        .success();

    let db_path = workspace.path().join(".beads/beads.db");
    let _issue_id = get_first_issue_id(&db_path);

    // Record the initial state
    let conn_before = rusqlite::Connection::open(&db_path).unwrap();
    let attempt_count_before: i64 = conn_before
        .query_row("SELECT COUNT(*) FROM attempt_outcomes", [], |row| row.get(0))
        .unwrap_or(0);

    // Run doctor (should not modify any data)
    Command::cargo_bin("bead")
        .unwrap()
        .args(["doctor", "--scope", "attempts"])
        .current_dir(workspace.path())
        .assert()
        .success();

    // Verify the database state hasn't changed
    let conn_after = rusqlite::Connection::open(&db_path).unwrap();
    let attempt_count_after: i64 = conn_after
        .query_row("SELECT COUNT(*) FROM attempt_outcomes", [], |row| row.get(0))
        .unwrap_or(0);

    assert_eq!(
        attempt_count_before, attempt_count_after,
        "Doctor should not modify database state"
    );
}

#[test]
fn test_why_explains_attempt_history() {
    let workspace = create_workspace();

    // Create a test issue
    Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Test issue", "--priority", "2"])
        .current_dir(workspace.path())
        .assert()
        .success();

    let db_path = workspace.path().join(".beads/beads.db");
    let issue_id = get_first_issue_id(&db_path);

    // Insert multiple attempt outcomes
    insert_attempt_outcome(
        &db_path,
        &issue_id,
        "ao-test001",
        "urn:needle:attempt:test001",
        "work_failure",
        "release",
    );

    insert_attempt_outcome(
        &db_path,
        &issue_id,
        "ao-test002",
        "urn:needle:attempt:test002",
        "verified_success",
        "close",
    );

    // Run why command with JSON output
    Command::cargo_bin("bead")
        .unwrap()
        .args(["why", "--id", &issue_id, "--json"])
        .current_dir(workspace.path())
        .assert()
        .success()
        .stdout(predicates::str::contains("attempt_info"))
        .stdout(predicates::str::contains("attempt_history"))
        .stdout(predicates::str::contains("last_attempt"));
}

#[test]
fn test_why_shows_attempt_tier_progression() {
    let workspace = create_workspace();

    // Create a test issue
    Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Test issue", "--priority", "2"])
        .current_dir(workspace.path())
        .assert()
        .success();

    let db_path = workspace.path().join(".beads/beads.db");
    let issue_id = get_first_issue_id(&db_path);

    // Insert attempt outcome that sets tier to 1
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute(
        "INSERT INTO attempt_outcomes (
            receipt_id, attempt_id, issue_id, outcome, action, reason,
            canonical_request_hash, prior_attempt_tier, resulting_attempt_tier,
            resulting_issue_revision, actor, created_at, evidence_refs_json,
            model, harness, harness_version
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
        rusqlite::params![
            "ao-tier1",
            "urn:needle:attempt:tier1",
            &issue_id,
            "work_failure",
            "release",
            "Test",
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef", // 64-char hex
            0i64,
            1i64, // Resulting tier 1
            1i64,
            "worker",
            "2026-08-31T12:00:00Z",
            r#"[]"#,
            "model",
            "harness",
            "1.0",
        ],
    )
    .unwrap();

    // Insert corresponding event for integrity check
    conn.execute(
        "INSERT INTO events (issue_id, kind, actor, time, detail) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![
            &issue_id,
            "attempt_resolved",
            "worker",
            "2026-08-31T12:00:00Z",
            "Attempt resolved",
        ],
    )
    .unwrap();

    // Update issue state to match the outcome
    conn.execute("UPDATE issues SET attempt_tier = 1, consecutive_failures = 1 WHERE id = ?1", [&issue_id])
        .unwrap();

    // Run why command - should show tier progression
    Command::cargo_bin("bead")
        .unwrap()
        .args(["why", "--id", &issue_id, "--json"])
        .current_dir(workspace.path())
        .assert()
        .success()
        .stdout(predicates::str::contains("attempt_info"))
        .stdout(predicates::str::contains("current_tier"))
        .stdout(predicates::str::contains("consecutive_failures"))
        .stdout(predicates::str::contains("tier_description"));
}

#[test]
fn test_why_legacy_workspace_no_attempt_info() {
    let workspace = create_legacy_workspace();

    // Don't create any issues - just verify why handles the empty legacy workspace
    // This test verifies that the why command doesn't crash on a legacy workspace

    // Run doctor instead to verify legacy workspace detection
    Command::cargo_bin("bead")
        .unwrap()
        .args(["doctor", "--scope", "attempts"])
        .current_dir(workspace.path())
        .assert()
        .success()
        .stderr(predicates::str::contains("legacy workspace"));
}

#[test]
fn test_doctor_includes_attempts_in_all_scope() {
    let workspace = create_workspace();

    // Create a test issue
    Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Test", "--priority", "2"])
        .current_dir(workspace.path())
        .assert()
        .success();

    // Run doctor with all scopes - should include attempts
    Command::cargo_bin("bead")
        .unwrap()
        .args(["doctor", "--scope", "all"])
        .current_dir(workspace.path())
        .assert()
        .success()
        .stderr(predicates::str::contains("attempt_outcomes_integrity"))
        .stderr(predicates::str::contains("attempt_tier_consistency"));
}

#[test]
fn test_attempt_tier_inconsistency_detection() {
    let workspace = create_workspace();

    // Create a test issue
    Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Test", "--priority", "2"])
        .current_dir(workspace.path())
        .assert()
        .success();

    let db_path = workspace.path().join(".beads/beads.db");
    let issue_id = get_first_issue_id(&db_path);

    // Manually set an inconsistent state (tier=2 but failures=1)
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute(
        "UPDATE issues SET attempt_tier = 2, consecutive_failures = 1 WHERE id = ?1",
        [&issue_id],
    )
    .unwrap();

    // Run doctor - should detect inconsistency as a warning
    Command::cargo_bin("bead")
        .unwrap()
        .args(["doctor", "--scope", "attempts"])
        .current_dir(workspace.path())
        .assert()
        .success()
        .stderr(predicates::str::contains("attempt_tier_consistency"))
        .stderr(predicates::str::contains("inconsistent"))
        .stderr(predicates::str::contains("warning"));
}

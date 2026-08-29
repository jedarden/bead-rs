//! Tests for fallback starvation recovery feature
//!
//! This module tests the automatic fallback mechanism that detects and recovers
//! from starvation situations where beads should be available but aren't showing
//! up in the ready frontier due to stale assignees.

use assert_cmd::Command;
use bead_rs::service::issues;
use tempfile::TempDir;

fn create_workspace() -> TempDir {
    let temp_dir = TempDir::new().unwrap();

    // Initialize workspace using `bead init`
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test", "--skip-foreign-workspace"])
        .current_dir(temp_dir.path())
        .assert()
        .success();

    temp_dir
}

#[test]
fn test_fallback_activates_on_empty_ready_frontier_with_assigned_open_beads() {
    // Setup: Create a temporary workspace
    let temp_dir = create_workspace();
    let workspace_path = temp_dir.path();
    let db_path = workspace_path.join(".beads/beads.db");
    let conn = rusqlite::Connection::open(&db_path).unwrap();

    // Create test beads:
    // 1. An open bead with an assignee (stale assignment)
    // 2. Another open bead with an assignee (stale assignment)
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string());

    // Insert bead 1 with stale assignee
    conn.execute(
        "INSERT INTO issues (id, title, description, priority, base_status, assignee, issue_type, created_at, updated_at, revision, schema_ref)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        (
            "test-bead-001",
            "Test bead 1",
            "Description",
            1,
            "open",
            "worker-alpha",
            "task",
            &now,
            &now,
            1,
            "urn:bead-rs:schema:issue:native-v1"
        ),
    ).unwrap();

    // Insert bead 2 with stale assignee
    conn.execute(
        "INSERT INTO issues (id, title, description, priority, base_status, assignee, issue_type, created_at, updated_at, revision, schema_ref)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        (
            "test-bead-002",
            "Test bead 2",
            "Description",
            2,
            "open",
            "worker-beta",
            "task",
            &now,
            &now,
            1,
            "urn:bead-rs:schema:issue:native-v1"
        ),
    ).unwrap();

    // Verify initial state - beads have assignees
    let bead1_assignee_before: Option<String> = conn
        .query_row(
            "SELECT assignee FROM issues WHERE id = 'test-bead-001'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        bead1_assignee_before.is_some(),
        "Bead 1 should have an assignee before fallback"
    );

    // Test: Query ready frontier - this should trigger fallback
    let ready_beads = issues::list_issues(&conn, None, None, true, 10).unwrap();
    assert_eq!(
        ready_beads.len(),
        2,
        "Ready frontier should have 2 beads after fallback"
    );

    // Verify fallback was triggered - check that assignees were cleared
    let bead1_assignee: Option<String> = conn
        .query_row(
            "SELECT assignee FROM issues WHERE id = 'test-bead-001'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        bead1_assignee.is_none(),
        "Bead 1 should have assignee cleared after fallback"
    );

    let bead2_assignee: Option<String> = conn
        .query_row(
            "SELECT assignee FROM issues WHERE id = 'test-bead-002'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        bead2_assignee.is_none(),
        "Bead 2 should have assignee cleared after fallback"
    );

    // Verify beads are now in ready frontier
    let ready_beads_after = issues::list_issues(&conn, None, None, true, 10).unwrap();
    assert_eq!(
        ready_beads_after.len(),
        2,
        "Ready frontier should have 2 beads after fallback"
    );

    // Verify beads are now in ready frontier and assignees are cleared
    let bead1_assignee_after: Option<String> = conn
        .query_row(
            "SELECT assignee FROM issues WHERE id = 'test-bead-001'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        bead1_assignee_after.is_none(),
        "Bead 1 should have assignee cleared after fallback"
    );

    let bead2_assignee_after: Option<String> = conn
        .query_row(
            "SELECT assignee FROM issues WHERE id = 'test-bead-002'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        bead2_assignee_after.is_none(),
        "Bead 2 should have assignee cleared after fallback"
    );

    // Verify fallback log was created
    let log_path = workspace_path.join(".beads/diagnostics/pluck-fallback.log");
    assert!(log_path.exists(), "Fallback log should be created");

    let log_content = std::fs::read_to_string(&log_path).unwrap();
    assert!(
        log_content.contains("test-bead-001"),
        "Log should contain bead-001"
    );
    assert!(
        log_content.contains("test-bead-002"),
        "Log should contain bead-002"
    );
}

#[test]
fn test_fallback_does_not_activate_when_ready_frontier_has_beads() {
    // Setup: Create a temporary workspace
    let temp_dir = create_workspace();
    let workspace_path = temp_dir.path();
    let db_path = workspace_path.join(".beads/beads.db");
    let conn = rusqlite::Connection::open(&db_path).unwrap();

    // Create test beads:
    // 1. An open bead WITHOUT assignee (ready)
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string());

    // Insert ready bead (no assignee)
    conn.execute(
        "INSERT INTO issues (id, title, description, priority, base_status, assignee, issue_type, created_at, updated_at, revision, schema_ref)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        (
            "test-bead-ready",
            "Test ready bead",
            "Description",
            1,
            "open",
            None::<&str>,
            "task",
            &now,
            &now,
            1,
            "urn:bead-rs:schema:issue:native-v1"
        ),
    ).unwrap();

    // Test: Query ready frontier (should have 1 bead)
    let ready_beads = issues::list_issues(&conn, None, None, true, 10).unwrap();
    assert_eq!(ready_beads.len(), 1, "Ready frontier should have 1 bead");

    // Verify fallback log was NOT created
    let log_path = workspace_path.join(".beads/diagnostics/pluck-fallback.log");
    assert!(
        !log_path.exists(),
        "Fallback log should not be created when ready frontier is not empty"
    );
}

#[test]
fn test_fallback_does_not_activate_when_no_open_beads_exist() {
    // Setup: Create a temporary workspace
    let temp_dir = create_workspace();
    let workspace_path = temp_dir.path();
    let db_path = workspace_path.join(".beads/beads.db");
    let conn = rusqlite::Connection::open(&db_path).unwrap();

    // Create only closed beads
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string());

    // Insert closed bead
    conn.execute(
        "INSERT INTO issues (id, title, description, priority, base_status, assignee, issue_type, created_at, updated_at, closed_at, close_reason, revision, schema_ref)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        (
            "test-bead-closed",
            "Test closed bead",
            "Description",
            1,
            "closed",
            "worker-alpha",
            "task",
            &now,
            &now,
            &now,
            "completed",
            1,
            "urn:bead-rs:schema:issue:native-v1"
        ),
    ).unwrap();

    // Test: Query ready frontier (should be empty)
    let ready_beads = issues::list_issues(&conn, None, None, true, 10).unwrap();
    assert_eq!(ready_beads.len(), 0, "Ready frontier should be empty");

    // Verify fallback log was NOT created (because no open beads exist)
    let log_path = workspace_path.join(".beads/diagnostics/pluck-fallback.log");
    assert!(
        !log_path.exists(),
        "Fallback log should not be created when no open beads exist"
    );
}

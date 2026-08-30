//! R028: Fork identity for cloned workspaces (bead sync fork)
//!
//! Tests for explicit workspace forking via `bead sync fork`.
//! Forking creates a new workspace UUID with provenance tracking,
//! enabling clones of one repository to become distinct origins.

use assert_cmd::Command;
use bead_rs::service::checkpoint::fork_workspace_identity;
use bead_rs::store::{open_configured_connection, SqliteStore, WorkspaceConfig};
use rusqlite::{params, Connection};
use tempfile::TempDir;

/// Helper to create a test workspace with a clean checkpoint
fn create_forkable_workspace() -> (TempDir, WorkspaceConfig, Connection) {
    create_workspace_with_init_args(&["init", "--prefix", "test-prefix"])
}

/// Like `create_forkable_workspace`, but `bead init --no-auto-flush` leaves
/// the workspace with only the placeholder checkpoint_state row: no
/// generation has been published.
fn create_unpublished_workspace() -> (TempDir, WorkspaceConfig, Connection) {
    create_workspace_with_init_args(&["init", "--prefix", "test-prefix", "--no-auto-flush"])
}

fn create_workspace_with_init_args(args: &[&str]) -> (TempDir, WorkspaceConfig, Connection) {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Initialize in the temporary directory through a child process. The
    // library initializer derives its root from the process-wide cwd, so
    // calling it directly here made every parallel test open and mutate the
    // repository's real workspace instead of its TempDir.
    Command::cargo_bin("bead")
        .unwrap()
        .current_dir(root)
        .args(args)
        .assert()
        .success();

    // Open database connection
    let db_path = root.join(".beads/beads.db");
    let conn = open_configured_connection(&db_path).unwrap();
    let (uuid, prefix) = conn
        .query_row(
            "SELECT uuid, prefix FROM workspace WHERE id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    let config = WorkspaceConfig {
        root: root.to_path_buf(),
        uuid,
        prefix,
    };

    (temp_dir, config, conn)
}

/// Helper to set checkpoint to clean state
fn set_clean_checkpoint(conn: &Connection) {
    // First, remove any existing checkpoint state to start fresh
    conn.execute("DELETE FROM checkpoint_state WHERE id = 1", [])
        .ok();

    let max_sequence: i64 = conn
        .query_row("SELECT COALESCE(MAX(sequence), 0) FROM events", [], |row| {
            row.get(0)
        })
        .unwrap();

    conn.execute(
        "INSERT INTO checkpoint_state (id, covered_event_sequence, current_generation_id, updated_at)
         VALUES (1, ?1, 'gen-001', datetime('now'))",
        params![max_sequence],
    )
    .unwrap();
}

#[test]
fn test_fork_creates_new_uuid_with_provenance() {
    let (_temp_dir, _config, conn) = create_forkable_workspace();

    set_clean_checkpoint(&conn);

    // Get original UUID
    let original_uuid: String = conn
        .query_row("SELECT uuid FROM workspace WHERE id = 1", [], |row| {
            row.get(0)
        })
        .unwrap();

    // Perform fork
    let mut store = SqliteStore::from_conn(conn);
    let result = fork_workspace_identity(&mut store, "test-actor", Some("Test fork")).unwrap();

    // Verify new UUID is different from parent
    assert_ne!(result.new_store_uuid, original_uuid);
    assert_eq!(result.parent_store_uuid, original_uuid);

    // Verify new UUID contains provenance to parent
    assert!(result.new_store_uuid.starts_with(&original_uuid[..8]));
    assert!(result.new_store_uuid.contains("-fork-"));

    println!("Parent UUID: {}", original_uuid);
    println!("New UUID: {}", result.new_store_uuid);
}

#[test]
fn test_fork_creates_fork_receipt() {
    let (_temp_dir, _config, conn) = create_forkable_workspace();

    set_clean_checkpoint(&conn);

    // Perform fork
    let mut store = SqliteStore::from_conn(conn);
    let result = fork_workspace_identity(&mut store, "test-actor", None).unwrap();

    // Verify fork receipt was created
    let receipt_count: i64 = store
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM provenance_receipts WHERE kind = 'fork'",
            [],
            |row| row.get(0),
        )
        .unwrap();

    assert_eq!(
        receipt_count, 1,
        "Expected exactly 1 fork receipt, but found {}",
        receipt_count
    );

    // Verify receipt contents
    let (receipt_id, source_uuid, target_uuid, actor): (String, String, String, String) = store
        .conn()
        .query_row(
            "SELECT receipt_id, source_store_uuid, target_store_uuid, actor
             FROM provenance_receipts WHERE kind = 'fork'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();

    assert_eq!(receipt_id, result.fork_receipt_id);
    assert_eq!(source_uuid, result.parent_store_uuid);
    assert_eq!(target_uuid, result.new_store_uuid);
    assert_eq!(actor, "test-actor");
}

#[test]
fn test_fork_updates_workspace_uuid() {
    let (_temp_dir, _config, conn) = create_forkable_workspace();

    set_clean_checkpoint(&conn);

    // Get original UUID
    let original_uuid: String = conn
        .query_row("SELECT uuid FROM workspace WHERE id = 1", [], |row| {
            row.get(0)
        })
        .unwrap();

    // Perform fork
    let mut store = SqliteStore::from_conn(conn);
    let result = fork_workspace_identity(&mut store, "test-actor", None).unwrap();

    // Verify workspace UUID was updated
    let current_uuid: String = store
        .conn()
        .query_row("SELECT uuid FROM workspace WHERE id = 1", [], |row| {
            row.get(0)
        })
        .unwrap();

    assert_eq!(current_uuid, result.new_store_uuid);
    assert_ne!(current_uuid, original_uuid);
}

#[test]
fn test_fork_creates_summary_event() {
    let (_temp_dir, _config, conn) = create_forkable_workspace();

    set_clean_checkpoint(&conn);

    // Perform fork
    let mut store = SqliteStore::from_conn(conn);
    fork_workspace_identity(&mut store, "test-actor", Some("Test reason")).unwrap();

    // Verify summary event was created
    let event_count: i64 = store
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM events WHERE kind = 'workspace_forked'",
            [],
            |row| row.get(0),
        )
        .unwrap();

    assert_eq!(event_count, 1);

    // Verify event contains fork details
    let (issue_id, kind, actor): (Option<String>, String, String) = store
        .conn()
        .query_row(
            "SELECT issue_id, kind, actor FROM events WHERE kind = 'workspace_forked'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();

    assert!(issue_id.is_none()); // Summary events have no issue_id
    assert_eq!(kind, "workspace_forked");
    assert_eq!(actor, "test-actor");
}

#[test]
fn test_fork_keeps_live_and_wire_sequences_independent() {
    let (_temp_dir, _config, conn) = create_forkable_workspace();

    // Advance SQLite's AUTOINCREMENT high-water mark without leaving a live
    // event. The fork point is therefore 0, while the next live key is 42.
    conn.execute(
        "INSERT INTO events (sequence, issue_id, kind, actor, time, detail)
         VALUES (41, NULL, 'test', 'actor', datetime('now'), '{}')",
        [],
    )
    .unwrap();
    conn.execute("DELETE FROM events", []).unwrap();
    set_clean_checkpoint(&conn);

    let mut store = SqliteStore::from_conn(conn);
    let result = fork_workspace_identity(&mut store, "test-actor", None).unwrap();

    let (live_sequence, wire_sequence): (i64, i64) = store
        .conn()
        .query_row(
            "SELECT sequence, origin_event_sequence
             FROM events WHERE kind = 'workspace_forked'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(live_sequence, 42);
    assert_eq!(result.summary_event_sequence, live_sequence);
    assert_eq!(wire_sequence, 1);
    assert!(result.new_store_uuid.contains("-fork-0-"));
}

#[test]
fn test_fork_marks_checkpoint_dirty() {
    let (_temp_dir, _config, conn) = create_forkable_workspace();

    set_clean_checkpoint(&conn);

    // Get original covered sequence
    let original_covered: i64 = conn
        .query_row(
            "SELECT covered_event_sequence FROM checkpoint_state WHERE id = 1",
            [],
            |row| row.get(0),
        )
        .unwrap();

    // Perform fork
    let mut store = SqliteStore::from_conn(conn);
    let result = fork_workspace_identity(&mut store, "test-actor", None).unwrap();

    // Verify covered sequence was updated (marked dirty)
    let new_covered: i64 = store
        .conn()
        .query_row(
            "SELECT covered_event_sequence FROM checkpoint_state WHERE id = 1",
            [],
            |row| row.get(0),
        )
        .unwrap();

    assert_eq!(new_covered, result.summary_event_sequence);
    assert!(new_covered > original_covered);
}

#[test]
fn test_fork_requires_clean_checkpoint() {
    let (_temp_dir, _config, conn) = create_forkable_workspace();

    set_clean_checkpoint(&conn);

    // Simulate dirty checkpoint by adding an event
    conn.execute(
        "INSERT INTO events (sequence, origin_store_uuid, origin_event_sequence, issue_id, kind, actor, time, detail)
         VALUES (1, 'uuid-123', 1, NULL, 'test', 'actor', datetime('now'), '{}')",
        [],
    )
    .unwrap();

    // Update covered_sequence to be behind (dirty state)
    conn.execute(
        "UPDATE checkpoint_state SET covered_event_sequence = 0 WHERE id = 1",
        [],
    )
    .unwrap();

    // Fork should fail on dirty workspace
    let mut store = SqliteStore::from_conn(conn);
    let result = fork_workspace_identity(&mut store, "test-actor", None);

    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("dirty workspace"));
}

#[test]
fn test_fork_requires_checkpoint() {
    // A workspace initialised with --no-auto-flush has only the placeholder
    // checkpoint_state row; no generation has been published yet.
    let (_temp_dir, _config, conn) = create_unpublished_workspace();

    // Fork should fail when no checkpoint exists
    let mut store = SqliteStore::from_conn(conn);
    let result = fork_workspace_identity(&mut store, "test-actor", None);

    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("no checkpoint"));
}

#[test]
fn test_fork_validates_actor() {
    let (_temp_dir, _config, conn) = create_forkable_workspace();
    let mut store = SqliteStore::from_conn(conn);

    // Test empty actor
    let result = fork_workspace_identity(&mut store, "", None);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("cannot be empty"));

    // Test actor with control characters
    let result = fork_workspace_identity(&mut store, "actor\n", None);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("control characters"));
}

#[test]
fn test_fork_validates_reason() {
    let (_temp_dir, _config, conn) = create_forkable_workspace();
    let mut store = SqliteStore::from_conn(conn);

    // Create overly long reason (> 4096 bytes)
    let long_reason = "x".repeat(4097);
    let result = fork_workspace_identity(&mut store, "test-actor", Some(&long_reason));

    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("cannot exceed 4096"));
}

#[test]
fn test_fork_records_parent_generation() {
    let (_temp_dir, _config, conn) = create_forkable_workspace();

    set_clean_checkpoint(&conn);

    // Set a specific generation ID
    conn.execute(
        "UPDATE checkpoint_state SET current_generation_id = 'test-gen-123' WHERE id = 1",
        [],
    )
    .unwrap();

    // Perform fork
    let mut store = SqliteStore::from_conn(conn);
    let result = fork_workspace_identity(&mut store, "test-actor", None).unwrap();

    // Verify parent generation ID is recorded
    assert_eq!(
        result.parent_generation_id,
        Some("test-gen-123".to_string())
    );
}

#[test]
fn test_multiple_forks_create_distinct_uuids() {
    let (_temp_dir, _config, conn) = create_forkable_workspace();

    set_clean_checkpoint(&conn);

    // Perform first fork
    let mut store = SqliteStore::from_conn(conn);
    let result1 = fork_workspace_identity(&mut store, "test-actor", None).unwrap();
    let uuid1 = result1.new_store_uuid.clone();

    // Update checkpoint state after first fork
    set_clean_checkpoint(store.conn());

    // Perform second fork
    let result2 = fork_workspace_identity(&mut store, "test-actor", None).unwrap();
    let uuid2 = result2.new_store_uuid.clone();

    // Verify UUIDs are distinct
    assert_ne!(uuid1, uuid2);

    // Both should contain the original provenance prefix
    let provenance_prefix = &result1.parent_store_uuid[..8];
    assert!(uuid1.starts_with(provenance_prefix));
    assert!(uuid2.starts_with(provenance_prefix));

    // But should differ in the sequence/suffix part
    let uuid1_suffix = uuid1.trim_start_matches(provenance_prefix);
    let uuid2_suffix = uuid2.trim_start_matches(provenance_prefix);
    assert_ne!(uuid1_suffix, uuid2_suffix);
}

#[test]
fn test_fork_report_contains_all_required_fields() {
    let (_temp_dir, _config, conn) = create_forkable_workspace();

    set_clean_checkpoint(&conn);

    // Perform fork
    let mut store = SqliteStore::from_conn(conn);
    let result = fork_workspace_identity(&mut store, "test-actor", Some("Test reason")).unwrap();

    // Verify all report fields are present
    assert!(!result.parent_store_uuid.is_empty());
    assert!(!result.new_store_uuid.is_empty());
    assert!(!result.fork_receipt_id.is_empty());
    assert!(!result.fork_receipt_sha256.is_empty());
    assert_eq!(result.actor, "test-actor");
    assert!(!result.created_at.is_empty());
    // The counts depend on how much state the fixture staged before the
    // fork; they are covered by test_fork_creates_fork_receipt. Only the
    // summary event the fork itself appends is guaranteed present here.
    assert!(result.summary_event_sequence > 0);
    assert_eq!(result.reason, Some("Test reason".to_string()));
}

#[test]
fn test_fork_never_implicit_or_inferred() {
    // This is a behavioral test: forking must ONLY happen via explicit command
    // There's no programmatic way to test "it doesn't happen automatically",
    // but we can verify the fork function is the ONLY code path that changes UUID

    let (_temp_dir, _config, conn) = create_forkable_workspace();

    // Get original UUID
    let original_uuid: String = conn
        .query_row("SELECT uuid FROM workspace WHERE id = 1", [], |row| {
            row.get(0)
        })
        .unwrap();

    // Perform various operations that should NOT change UUID
    conn.execute(
        "INSERT INTO events (sequence, origin_store_uuid, origin_event_sequence, issue_id, kind, actor, time, detail)
         VALUES (1, 'uuid-123', 1, NULL, 'test', 'actor', datetime('now'), '{}')",
        [],
    )
    .unwrap();

    // Verify UUID is unchanged
    let current_uuid: String = conn
        .query_row("SELECT uuid FROM workspace WHERE id = 1", [], |row| {
            row.get(0)
        })
        .unwrap();

    assert_eq!(current_uuid, original_uuid);
}

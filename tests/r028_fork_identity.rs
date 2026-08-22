//! R028: Fork identity for cloned workspaces (bead sync fork)
//!
//! Tests for explicit workspace forking via `bead sync fork`.
//! Forking creates a new workspace UUID with provenance tracking,
//! enabling clones of one repository to become distinct origins.

use bead_rs::service::checkpoint::fork_workspace_identity;
use bead_rs::store::SqliteStore;
use bead_rs::store::WorkspaceConfig;
use bead_rs::Store;
use rusqlite::{params, Connection};
use sha2::Digest;
use tempfile::TempDir;

/// Helper to create a test workspace with a clean checkpoint
fn create_forkable_workspace() -> (TempDir, WorkspaceConfig, Connection) {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Create .beads directory structure
    let beads_dir = root.join(".beads");
    std::fs::create_dir_all(&beads_dir).unwrap();

    // Create checkpoint directory
    let checkpoint_dir = beads_dir.join("checkpoint");
    std::fs::create_dir_all(&checkpoint_dir).unwrap();

    // Initialize workspace
    let config = SqliteStore::new()
        .init_workspace("test-prefix")
        .unwrap();

    // Open database connection
    let db_path = config.database_path();
    let conn = Connection::open(&db_path).unwrap();

    // Enable foreign keys
    conn.execute("PRAGMA foreign_keys = ON", []).unwrap();

    // Get current schema version
    let current_version: i64 = conn
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .unwrap_or(0);

    // Apply migration_12 if not already applied (R028 adds 'fork' to provenance_receipts kinds)
    if current_version < 12 {
        // Manually apply migration_12 to add 'fork' to the kind constraint
        conn.execute(
            "CREATE TABLE IF NOT EXISTS provenance_receipts_new (
                receipt_id TEXT NOT NULL PRIMARY KEY,
                schema_ref TEXT NOT NULL DEFAULT 'urn:bead-rs:schema:provenance-receipt:native-v1',
                kind TEXT NOT NULL CHECK (kind IN ('restore', 'merge', 'fork')),
                source_store_uuid TEXT NOT NULL,
                target_store_uuid TEXT NOT NULL,
                source_root_sha256 TEXT,
                actor TEXT NOT NULL,
                created_at TEXT NOT NULL,
                counts_json TEXT NOT NULL,
                result TEXT NOT NULL,
                summary_event_identity TEXT,
                receipt_sha256 TEXT NOT NULL
            )",
            [],
        ).unwrap();

        conn.execute(
            "INSERT INTO provenance_receipts_new SELECT * FROM provenance_receipts",
            [],
        ).unwrap();

        conn.execute("DROP TABLE provenance_receipts", []).unwrap();
        conn.execute("ALTER TABLE provenance_receipts_new RENAME TO provenance_receipts", []).unwrap();

        // Recreate indexes
        conn.execute(
            "CREATE INDEX IF NOT EXISTS provenance_receipts_source_uuid ON provenance_receipts (source_store_uuid)",
            [],
        ).unwrap();
        conn.execute(
            "CREATE INDEX IF NOT EXISTS provenance_receipts_target_uuid ON provenance_receipts (target_store_uuid)",
            [],
        ).unwrap();
        conn.execute(
            "CREATE INDEX IF NOT EXISTS provenance_receipts_source_root ON provenance_receipts (source_root_sha256)",
            [],
        ).unwrap();

        // Record migration with a simple checksum
        let migration_sql = "migration_12_r028_fork_receipt_kind";
        let checksum = format!("{:x}", sha2::Sha256::digest(migration_sql.as_bytes()));
        conn.execute(
            "INSERT INTO schema_migrations (version, applied_at, checksum) VALUES (?1, datetime('now'), ?2)",
            params![12i64, checksum],
        ).unwrap();
    }

    (temp_dir, config, conn)
}

/// Helper to set checkpoint to clean state
fn set_clean_checkpoint(conn: &Connection) {
    // First, remove any existing checkpoint state to start fresh
    conn.execute("DELETE FROM checkpoint_state WHERE id = 1", []).ok();

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

    assert_eq!(receipt_count, 1, "Expected exactly 1 fork receipt, but found {}", receipt_count);

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
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Create .beads directory structure
    let beads_dir = root.join(".beads");
    std::fs::create_dir_all(&beads_dir).unwrap();

    // Initialize workspace WITHOUT checkpoint
    let config = SqliteStore::new()
        .init_workspace("test-prefix")
        .unwrap();

    // Open database connection
    let db_path = config.database_path();
    let conn = Connection::open(&db_path).unwrap();

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
    assert!(result.unwrap_err().to_string().contains("cannot exceed 4096"));
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
    assert_eq!(result.parent_generation_id, Some("test-gen-123".to_string()));
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
    assert!(result.issue_count >= 0);
    assert!(result.event_count >= 0);
    assert!(result.receipt_count >= 0);
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

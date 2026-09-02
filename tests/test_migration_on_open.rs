//! Test that with_path applies pending migrations

use bead_rs::store::migrations;
use bead_rs::store::SqliteStore;
use tempfile::TempDir;

#[test]
fn test_with_path_applies_pending_migrations() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // Create a connection and manually set it to an old schema version
    let conn = rusqlite::Connection::open(&db_path).unwrap();

    // Manually create an old schema (version 13 instead of CURRENT_VERSION 14)
    conn.execute(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL,
            checksum TEXT NOT NULL
        )",
        [],
    )
    .unwrap();

    conn.execute(
        "INSERT INTO schema_migrations (version, applied_at, checksum) VALUES (13, '2024-01-01T00:00:00Z', 'dummy')",
        [],
    ).unwrap();

    // Close the connection
    drop(conn);

    // Now open with SqliteStore::with_path - should auto-migrate
    let mut store = SqliteStore::with_path(&db_path).unwrap();

    // Verify we're now at CURRENT_VERSION
    let version = store.schema_version().unwrap();
    assert_eq!(
        version,
        migrations::CURRENT_VERSION,
        "with_path should auto-migrate from version 13 to CURRENT_VERSION"
    );
}

#[test]
fn test_with_path_skip_migration_when_current() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // Create a fresh store at CURRENT_VERSION
    let mut store = SqliteStore::with_path(&db_path).unwrap();

    // First open - should apply migrations to reach CURRENT_VERSION
    let version1 = store.schema_version().unwrap();
    assert_eq!(version1, migrations::CURRENT_VERSION);

    // Re-open - should be a no-op (read-only check)
    let mut store2 = SqliteStore::with_path(&db_path).unwrap();
    let version2 = store2.schema_version().unwrap();
    assert_eq!(
        version2,
        migrations::CURRENT_VERSION,
        "Re-opening a current schema should remain current"
    );
}

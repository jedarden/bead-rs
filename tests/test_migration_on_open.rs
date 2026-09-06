//! Test that with_path applies pending migrations

use bead_rs::store::migrations;
use bead_rs::store::SqliteStore;
use tempfile::TempDir;

#[test]
fn test_with_path_applies_pending_migrations() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // Start from a structurally valid store, then remove exactly the latest
    // additive migration. A version row on an otherwise empty database is
    // not a valid historical schema and can fail for reasons unrelated to
    // opening/migration behavior.
    drop(SqliteStore::with_path(&db_path).unwrap());
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute_batch(
        "ALTER TABLE issues DROP COLUMN claim_epoch;
         DELETE FROM schema_migrations WHERE version = 17;",
    )
    .unwrap();
    drop(conn);

    // Now open with SqliteStore::with_path - should auto-migrate
    let mut store = SqliteStore::with_path(&db_path).unwrap();

    // Verify we're now at CURRENT_VERSION
    let version = store.schema_version().unwrap();
    assert_eq!(
        version,
        migrations::CURRENT_VERSION,
        "with_path should auto-migrate to CURRENT_VERSION"
    );

    let claim_epoch_columns: i64 = store
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('issues') WHERE name = 'claim_epoch'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(claim_epoch_columns, 1);
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

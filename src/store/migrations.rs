//! Database schema migrations for bead-rs
//!
//! This module defines the independent SQLite schema and migration system.
//! Migration 1 implements the core workspace schema as defined in the plan.

use rusqlite::{Connection, Result as SqliteResult};
use sha2::{Digest, Sha256};

/// Current migration version
pub const CURRENT_VERSION: i64 = 1;

/// Apply all pending migrations to the database
pub fn apply_migrations(conn: &Connection) -> SqliteResult<()> {
    // Enable foreign keys
    conn.execute("PRAGMA foreign_keys = ON", [])?;

    // Create schema_migrations table if it doesn't exist
    conn.execute(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL,
            checksum TEXT NOT NULL
        )",
        [],
    )?;

    // Get current version
    let current_version: i64 = conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        [],
        |row| row.get(0),
    ).unwrap_or(0);

    // Apply each migration in order
    for version in (current_version + 1)..=CURRENT_VERSION {
        let migration = get_migration(version);
        let checksum = migration_checksum(&migration.sql);

        // Begin transaction for this migration
        let tx = conn.unchecked_transaction()?;

        // Apply the migration by executing each statement separately
        for statement in migration.sql.split(';') {
            let statement = statement.trim();
            if !statement.is_empty() {
                tx.execute(statement, [])?;
            }
        }

        // Record the migration
        let applied_at = time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default();
        tx.execute(
            "INSERT INTO schema_migrations (version, applied_at, checksum) VALUES (?1, ?2, ?3)",
            [&version.to_string(), &applied_at, &checksum],
        )?;

        tx.commit()?;
    }

    Ok(())
}

/// Get a migration by version number
fn get_migration(version: i64) -> Migration {
    match version {
        1 => migration_1(),
        v => panic!("Unknown migration version: {}", v),
    }
}

/// Migration structure
struct Migration {
    sql: String,
}

/// Migration 1: Core workspace schema
fn migration_1() -> Migration {
    let sql = r#"
-- Workspace metadata
CREATE TABLE IF NOT EXISTS workspace (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    uuid TEXT NOT NULL UNIQUE,
    prefix TEXT NOT NULL,
    layout_version INTEGER NOT NULL,
    created_at TEXT NOT NULL
);

-- Core issue table
CREATE TABLE IF NOT EXISTS issues (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    description TEXT DEFAULT '',
    notes TEXT DEFAULT '',
    priority INTEGER NOT NULL CHECK (priority >= 0 AND priority <= 4),
    issue_type TEXT NOT NULL DEFAULT 'task',
    base_status TEXT NOT NULL CHECK (base_status IN ('open', 'in_progress', 'deferred', 'closed')),
    manual_blocked INTEGER NOT NULL DEFAULT 0 CHECK (manual_blocked IN (0, 1)),
    assignee TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    closed_at TEXT,
    close_reason TEXT,
    source_repo TEXT,
    profile TEXT DEFAULT 'native-v1',
    schema_ref TEXT DEFAULT 'urn:bead-rs:schema:issue:native-v1'
);

-- Issue extensions for profile-specific fields
CREATE TABLE IF NOT EXISTS issue_extensions (
    issue_id TEXT NOT NULL,
    key TEXT NOT NULL,
    value TEXT NOT NULL,
    profile TEXT NOT NULL DEFAULT 'native-v1',
    PRIMARY KEY (issue_id, key),
    FOREIGN KEY (issue_id) REFERENCES issues(id) ON DELETE CASCADE
);

-- Labels
CREATE TABLE IF NOT EXISTS labels (
    issue_id TEXT NOT NULL,
    label TEXT NOT NULL,
    PRIMARY KEY (issue_id, label),
    FOREIGN KEY (issue_id) REFERENCES issues(id) ON DELETE CASCADE
);

-- Dependencies
CREATE TABLE IF NOT EXISTS dependencies (
    blocked_issue_id TEXT NOT NULL,
    blocker_issue_id TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('blocks', 'relates_to')),
    PRIMARY KEY (blocked_issue_id, blocker_issue_id, kind),
    FOREIGN KEY (blocked_issue_id) REFERENCES issues(id) ON DELETE CASCADE,
    FOREIGN KEY (blocker_issue_id) REFERENCES issues(id) ON DELETE CASCADE,
    CHECK (blocked_issue_id != blocker_issue_id)
);

-- Comments
CREATE TABLE IF NOT EXISTS comments (
    id TEXT NOT NULL PRIMARY KEY,
    issue_id TEXT NOT NULL,
    author TEXT NOT NULL,
    body TEXT NOT NULL,
    reply_to_id TEXT,
    resolution_state TEXT CHECK (resolution_state IN ('unresolved', 'resolved')),
    created_at TEXT NOT NULL,
    FOREIGN KEY (issue_id) REFERENCES issues(id) ON DELETE CASCADE
);

-- Structured data
CREATE TABLE IF NOT EXISTS issue_data (
    issue_id TEXT NOT NULL,
    namespace TEXT NOT NULL,
    schema_ref TEXT NOT NULL,
    value TEXT NOT NULL,
    PRIMARY KEY (issue_id, namespace),
    FOREIGN KEY (issue_id) REFERENCES issues(id) ON DELETE CASCADE
);

-- Claim telemetry
CREATE TABLE IF NOT EXISTS claim_telemetry (
    event_sequence INTEGER NOT NULL PRIMARY KEY,
    model TEXT,
    harness TEXT,
    harness_version TEXT,
    FOREIGN KEY (event_sequence) REFERENCES events(sequence) ON DELETE CASCADE
);

-- Audit events
CREATE TABLE IF NOT EXISTS events (
    sequence INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    issue_id TEXT,
    kind TEXT NOT NULL,
    actor TEXT,
    time TEXT NOT NULL,
    detail TEXT NOT NULL,
    FOREIGN KEY (issue_id) REFERENCES issues(id) ON DELETE CASCADE
);

-- Checkpoint state for issue-only JSONL
CREATE TABLE IF NOT EXISTS checkpoint_state (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    last_interchange_hash TEXT NOT NULL DEFAULT '',
    covered_event_sequence INTEGER NOT NULL DEFAULT 0,
    export_time TEXT
);

-- Indexes for performance
CREATE INDEX IF NOT EXISTS issues_readiness ON issues (
    base_status, manual_blocked, assignee, priority, created_at, id
);

CREATE INDEX IF NOT EXISTS dependencies_blocker ON dependencies (blocker_issue_id);
CREATE INDEX IF NOT EXISTS dependencies_blocked ON dependencies (blocked_issue_id);

CREATE INDEX IF NOT EXISTS labels_label ON labels (label);
CREATE INDEX IF NOT EXISTS labels_issue ON labels (issue_id);

CREATE INDEX IF NOT EXISTS comments_issue ON comments (issue_id, created_at);
CREATE INDEX IF NOT EXISTS events_issue ON events (issue_id, time);
"#;

    Migration { sql: sql.to_string() }
}

/// Calculate SHA-256 checksum of a migration
fn migration_checksum(sql: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(sql.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_migration_1_checksum() {
        let migration = migration_1();
        let checksum = migration_checksum(&migration.sql);
        // Ensure checksum is stable
        assert_eq!(checksum.len(), 64);
    }

    #[test]
    fn test_apply_migrations_empty_db() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute("PRAGMA foreign_keys = ON", []).unwrap();
        apply_migrations(&conn).unwrap();

        // Check version
        let version: i64 = conn
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(version, CURRENT_VERSION);

        // Check tables exist
        let tables = [
            "schema_migrations",
            "workspace",
            "issues",
            "issue_extensions",
            "labels",
            "dependencies",
            "comments",
            "issue_data",
            "claim_telemetry",
            "events",
            "checkpoint_state",
        ];

        for table in &tables {
            let exists: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [&table.to_string()],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(exists, 1, "Table {} should exist", table);
        }
    }

    #[test]
    fn test_apply_migrations_idempotent() {
        let mut conn = Connection::open_in_memory().unwrap();
        apply_migrations(&conn).unwrap();
        apply_migrations(&conn).unwrap(); // Should not fail

        let version: i64 = conn
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(version, CURRENT_VERSION);
    }
}

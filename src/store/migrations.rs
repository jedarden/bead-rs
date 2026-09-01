//! Database schema migrations for bead-rs
//!
//! This module defines the independent SQLite schema and migration system.
//! Migration 1 implements the core workspace schema as defined in the plan.

use rusqlite::{Connection, Result as SqliteResult, Transaction, TransactionBehavior};
use sha2::{Digest, Sha256};

/// Current migration version
pub const CURRENT_VERSION: i64 = 14;

/// Whether the store has already reached [`CURRENT_VERSION`].
///
/// Deliberately read-only. [`apply_migrations`] unconditionally runs
/// `CREATE TABLE IF NOT EXISTS schema_migrations`, which takes a write lock,
/// and many workers share one workspace -- doing that on every open would
/// serialise them. A missing `schema_migrations` table reads as "not current"
/// so the caller falls through and creates it.
pub fn schema_is_current(conn: &Connection) -> bool {
    conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        [],
        |row| row.get::<_, i64>(0),
    )
    .map(|version| version >= CURRENT_VERSION)
    .unwrap_or(false)
}

/// Apply pending migrations, skipping the write path when there are none.
///
/// Opening an existing store must advance its schema. Without this a
/// workspace created under an older `CURRENT_VERSION` stays at that version
/// forever while newer code assumes the tables its migrations add, and the
/// mismatch surfaces far from its cause -- as a missing table inside an
/// unrelated operation.
pub fn migrate_if_pending(conn: &Connection) -> SqliteResult<()> {
    if schema_is_current(conn) {
        return Ok(());
    }
    apply_migrations(conn)
}

/// Apply all pending migrations to the database
pub fn apply_migrations(conn: &Connection) -> SqliteResult<()> {
    // Enable foreign keys (this PRAGMA doesn't return rows)
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
    let current_version: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    // Apply each migration in order
    for version in (current_version + 1)..=CURRENT_VERSION {
        let migration = get_migration(version);
        let checksum = migration_checksum(&migration.sql);

        // IMMEDIATE takes the write lock before the re-check below, so two
        // processes cannot both decide this migration is pending. Every
        // connection migrates on open and many workers share a workspace, so
        // that race is ordinary here, not exotic: without the re-check the
        // loser's INSERT violates the primary key and fails whatever command
        // happened to open the store.
        let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;

        let already_applied: i64 = tx.query_row(
            "SELECT COUNT(*) FROM schema_migrations WHERE version = ?1",
            [&version.to_string()],
            |row| row.get(0),
        )?;
        if already_applied > 0 {
            tx.rollback()?;
            continue;
        }

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
        2 => migration_2(),
        3 => migration_3(),
        4 => migration_4(),
        5 => migration_5(),
        6 => migration_6(),
        7 => migration_7(),
        8 => migration_8(),
        9 => migration_9(),
        10 => migration_10(),
        11 => migration_11(),
        12 => migration_12(),
        13 => migration_13(),
        14 => migration_14(),
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

-- Audit events (must be created before claim_telemetry for FK constraint)
CREATE TABLE IF NOT EXISTS events (
    sequence INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    issue_id TEXT,
    kind TEXT NOT NULL,
    actor TEXT,
    time TEXT NOT NULL,
    detail TEXT NOT NULL,
    FOREIGN KEY (issue_id) REFERENCES issues(id) ON DELETE CASCADE
);

-- Claim telemetry (references events table)
CREATE TABLE IF NOT EXISTS claim_telemetry (
    event_sequence INTEGER NOT NULL PRIMARY KEY,
    model TEXT,
    harness TEXT,
    harness_version TEXT,
    FOREIGN KEY (event_sequence) REFERENCES events(sequence) ON DELETE CASCADE
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

    Migration {
        sql: sql.to_string(),
    }
}

/// Migration 2: Forensic checkpoint-set v1 support
///
/// This migration adds support for F017's forensic checkpoint format:
/// - Immutable event origin identity/hash and local ingestion ordering
/// - Provenance receipts table for restore/merge operations
/// - Enhanced checkpoint state with generation/mode/root tracking
/// - Pending tombstones and changed path tracking for Git integration
fn migration_2() -> Migration {
    let sql = r#"
-- Add event origin identity and local ingestion ordering to events table
-- These columns are nullable for backward compatibility with existing events
ALTER TABLE events ADD COLUMN origin_store_uuid TEXT;
ALTER TABLE events ADD COLUMN origin_event_sequence INTEGER;
ALTER TABLE events ADD COLUMN event_sha256 TEXT;
ALTER TABLE events ADD COLUMN local_ingestion_sequence INTEGER;

-- Create indexes for event identity and ordering
CREATE INDEX IF NOT EXISTS events_origin_identity ON events (origin_store_uuid, origin_event_sequence);
CREATE INDEX IF NOT EXISTS events_local_ingestion ON events (local_ingestion_sequence);

-- Provenance receipts table for restore/merge operations
CREATE TABLE IF NOT EXISTS provenance_receipts (
    receipt_id TEXT NOT NULL PRIMARY KEY,
    schema_ref TEXT NOT NULL DEFAULT 'urn:bead-rs:schema:provenance-receipt:native-v1',
    kind TEXT NOT NULL CHECK (kind IN ('restore', 'merge')),
    source_store_uuid TEXT NOT NULL,
    target_store_uuid TEXT NOT NULL,
    source_root_sha256 TEXT NOT NULL,
    actor TEXT NOT NULL,
    created_at TEXT NOT NULL,
    counts_json TEXT NOT NULL,
    result TEXT NOT NULL,
    summary_event_identity TEXT,
    receipt_sha256 TEXT NOT NULL
);

-- Index for provenance receipt uniqueness and queries
CREATE INDEX IF NOT EXISTS provenance_receipts_uniqueness ON provenance_receipts (kind, target_store_uuid, source_root_sha256);

-- Enhanced checkpoint state table for forensic checkpoints
-- We need to recreate this table to add the new columns
CREATE TABLE IF NOT EXISTS checkpoint_state_v2 (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    -- Migration-1 issue-only checkpoint fields
    last_interchange_hash TEXT NOT NULL DEFAULT '',
    covered_event_sequence INTEGER NOT NULL DEFAULT 0,
    export_time TEXT,
    -- F017 forensic checkpoint fields
    current_generation_id TEXT,
    current_mode TEXT CHECK (current_mode IN ('monolithic', 'sharded')),
    current_root_path TEXT,
    current_root_sha256 TEXT,
    previous_generation_id TEXT,
    previous_root_path TEXT,
    pending_tombstones_json TEXT,
    changed_paths_json TEXT,
    store_uuid TEXT,
    max_local_ingestion_sequence INTEGER DEFAULT 0,
    updated_at TEXT NOT NULL
);

-- Migrate existing checkpoint state if it exists
-- Use INSERT OR REPLACE to handle both cases: empty or existing data
INSERT OR REPLACE INTO checkpoint_state_v2 (
    id, last_interchange_hash, covered_event_sequence, export_time,
    store_uuid, updated_at
)
SELECT
    1,
    COALESCE(last_interchange_hash, ''),
    COALESCE(covered_event_sequence, 0),
    export_time,
    (SELECT uuid FROM workspace WHERE id = 1),
    datetime('utc_now')
FROM checkpoint_state
WHERE id = 1;

-- Ensure there's at least one row (for fresh databases or failed migration)
INSERT OR IGNORE INTO checkpoint_state_v2 (
    id, last_interchange_hash, covered_event_sequence, store_uuid, updated_at
) VALUES (
    1, '', 0, '', datetime('utc_now')
);

-- Drop old table and rename new one
DROP TABLE checkpoint_state;
ALTER TABLE checkpoint_state_v2 RENAME TO checkpoint_state;

-- Index for checkpoint state queries
CREATE INDEX IF NOT EXISTS checkpoint_state_generation ON checkpoint_state (current_generation_id);
"#;

    Migration {
        sql: sql.to_string(),
    }
}

/// Migration 3: Logical revision guards (R003)
///
/// This migration adds support for R003's logical revision guards:
/// - Monotonically increasing revision field for each issue
/// - Supports --if-revision precondition on mutations
/// - Prevents silent lost updates across concurrent operations
fn migration_3() -> Migration {
    let sql = r#"
-- Add revision column to issues table
ALTER TABLE issues ADD COLUMN revision INTEGER NOT NULL DEFAULT 1;

-- Create index for revision-based queries
CREATE INDEX IF NOT EXISTS issues_revision ON issues (id, revision);
"#;

    Migration {
        sql: sql.to_string(),
    }
}

/// Migration 4: Fenced claim leases (R002)
///
/// This migration adds support for R002's fenced claim leases:
/// - Leases table for tracking expiring claims with fencing tokens
/// - Monotonically increasing fencing tokens for stale worker detection
/// - Lease expiry validation for safe recovery from crashed agents
/// - Maintains backward compatibility with non-leased claims
fn migration_4() -> Migration {
    let sql = r#"
-- Leases table for fenced claim operations
CREATE TABLE IF NOT EXISTS leases (
    issue_id TEXT NOT NULL PRIMARY KEY,
    assignee TEXT NOT NULL,
    fencing_token INTEGER NOT NULL,
    expires_at TEXT NOT NULL,
    renewed_at TEXT,
    created_at TEXT NOT NULL,
    FOREIGN KEY (issue_id) REFERENCES issues(id) ON DELETE CASCADE
);

-- Index for lease expiry queries (cleanup and validation)
CREATE INDEX IF NOT EXISTS leases_expiry ON leases (expires_at);

-- Index for fencing token validation during mutations
CREATE INDEX IF NOT EXISTS leases_fencing ON leases (issue_id, assignee, fencing_token);

-- Index for assignee lease queries (renewal, listing)
CREATE INDEX IF NOT EXISTS leases_assignee ON leases (assignee, expires_at);
"#;

    Migration {
        sql: sql.to_string(),
    }
}

/// Migration 5: Saved views for R004 safe query language
fn migration_5() -> Migration {
    let sql = r#"
-- Saved views table for storing and reusing queries
CREATE TABLE IF NOT EXISTS saved_views (
    id TEXT NOT NULL PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    description TEXT,
    query_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- Index for view name lookups
CREATE INDEX IF NOT EXISTS saved_views_name ON saved_views (name);

-- Index for temporal queries
CREATE INDEX IF NOT EXISTS saved_views_created ON saved_views (created_at);
"#;

    Migration {
        sql: sql.to_string(),
    }
}

/// Migration 6: External references (R011) namespaced external references
///
/// This migration adds support for R011's namespaced external references:
/// - Generic (namespace, key, value) references for tracker IDs and commit identifiers
/// - Does not replace native bead IDs or resolve anything over the network
/// - Optional namespace-scoped uniqueness supports reliable deduplication
/// - Cross-tool recognition without title heuristics
fn migration_6() -> Migration {
    let sql = r#"
-- External references table for namespaced external identifiers
CREATE TABLE IF NOT EXISTS external_references (
    issue_id TEXT NOT NULL,
    namespace TEXT NOT NULL,
    key TEXT NOT NULL,
    value TEXT NOT NULL,
    PRIMARY KEY (issue_id, namespace, key),
    FOREIGN KEY (issue_id) REFERENCES issues(id) ON DELETE CASCADE
);

-- Index for namespace-scoped queries and deduplication
CREATE INDEX IF NOT EXISTS external_references_namespace ON external_references (namespace, key, value);

-- Index for issue-based lookups
CREATE INDEX IF NOT EXISTS external_references_issue ON external_references (issue_id);

-- Index for cross-tool recognition by value lookup
CREATE INDEX IF NOT EXISTS external_references_value ON external_references (namespace, value);
"#;

    Migration {
        sql: sql.to_string(),
    }
}

/// Migration 7: Conditional dependencies (R017)
///
/// This migration adds support for R017's conditional dependencies:
/// - Condition column in dependencies table for declarative predicates
/// - Supports all/any/not logical composition
/// - Supports comparison/set operators over stored fields, labels, issue type, priority, assignee presence, and schema-bound data
/// - Conditional edges are treated as potentially active during cycle detection
fn migration_7() -> Migration {
    let sql = r#"
-- Add condition column to dependencies table
-- Stored as JSON TEXT for flexibility
ALTER TABLE dependencies ADD COLUMN condition TEXT;

-- Create index for conditional dependency queries
CREATE INDEX IF NOT EXISTS dependencies_condition ON dependencies (condition);
"#;

    Migration {
        sql: sql.to_string(),
    }
}

/// Migration 8: Explicit recurring-bead materialization (R024)
///
/// This migration adds support for R024's recurrence templates:
/// - Immutable recurrence-template versions
/// - Materialization receipts for tracking series occurrences
/// - Series references between templates and created issues
/// - No automatic scheduling; explicit command only
fn migration_8() -> Migration {
    let sql = r#"
-- Recurrence templates table
-- Stores immutable templates that define how recurring issues should be created
CREATE TABLE IF NOT EXISTS recurrence_templates (
    id TEXT NOT NULL PRIMARY KEY,
    title TEXT NOT NULL,
    description TEXT,
    base_title_template TEXT NOT NULL,
    base_description TEXT,
    priority INTEGER NOT NULL DEFAULT 2 CHECK (priority >= 0 AND priority <= 4),
    issue_type TEXT NOT NULL DEFAULT 'task',
    labels_json TEXT,
    created_at TEXT NOT NULL
);

-- Recurrence materializations table
-- Tracks materialization receipts and relationships between templates and occurrences
CREATE TABLE IF NOT EXISTS recurrence_materializations (
    template_id TEXT NOT NULL,
    series_sequence INTEGER NOT NULL,
    occurrence_id TEXT NOT NULL,
    materialized_at TEXT NOT NULL,
    actor TEXT,
    PRIMARY KEY (template_id, series_sequence),
    FOREIGN KEY (template_id) REFERENCES recurrence_templates(id) ON DELETE CASCADE,
    FOREIGN KEY (occurrence_id) REFERENCES issues(id) ON DELETE CASCADE
);

-- Index for looking up materializations by template
CREATE INDEX IF NOT EXISTS recurrence_materializations_template ON recurrence_materializations (template_id);

-- Index for looking up materializations by occurrence
CREATE INDEX IF NOT EXISTS recurrence_materializations_occurrence ON recurrence_materializations (occurrence_id);
"#;

    Migration {
        sql: sql.to_string(),
    }
}

/// Migration 9: Intelligent scheduling metrics (R019)
///
/// This migration adds support for post-0.1 intelligent claim scheduling:
/// - Ready age tracking for aging promotion
/// - Attempt tiers and failure counts
/// - Workspace claim sequence for rotation
/// - Last claim sequence for least-recently-served fairness
/// - Scheduling metrics cache for graph computation
/// - Retry state and quarantine tracking
fn migration_9() -> Migration {
    let sql = r#"
-- Add scheduling state columns to issues table
ALTER TABLE issues ADD COLUMN ready_since TEXT;
ALTER TABLE issues ADD COLUMN last_claim_sequence INTEGER;
ALTER TABLE issues ADD COLUMN attempt_tier INTEGER NOT NULL DEFAULT 0 CHECK (attempt_tier >= 0 AND attempt_tier <= 3);
ALTER TABLE issues ADD COLUMN consecutive_failures INTEGER NOT NULL DEFAULT 0;
ALTER TABLE issues ADD COLUMN retry_after_claim_sequence INTEGER;

-- Create indexes for scheduling queries
CREATE INDEX IF NOT EXISTS issues_ready_frontier ON issues (base_status, manual_blocked, assignee, priority, created_at, id);
CREATE INDEX IF NOT EXISTS issues_attempt_tier ON issues (attempt_tier, last_claim_sequence);

-- Scheduling metrics cache for computed graph metrics
CREATE TABLE IF NOT EXISTS scheduling_metrics (
    issue_id TEXT NOT NULL PRIMARY KEY,
    graph_revision INTEGER NOT NULL,
    downstream_reach INTEGER NOT NULL DEFAULT 0,
    critical_path_reduction INTEGER NOT NULL DEFAULT 0,
    immediate_unlock_count INTEGER NOT NULL DEFAULT 0,
    computed_at TEXT NOT NULL,
    FOREIGN KEY (issue_id) REFERENCES issues(id) ON DELETE CASCADE
);

-- Index for metrics invalidation queries
CREATE INDEX IF NOT EXISTS scheduling_metrics_revision ON scheduling_metrics (graph_revision, computed_at);

-- Workspace claim sequence for rotation tracking
-- This singleton table maintains a monotonically increasing sequence number
CREATE TABLE IF NOT EXISTS workspace_claim_sequence (
    sequence INTEGER NOT NULL DEFAULT 0
);

-- Initialize the workspace claim sequence if not exists
INSERT OR IGNORE INTO workspace_claim_sequence (sequence) VALUES (0);
"#;

    Migration {
        sql: sql.to_string(),
    }
}

/// Migration 10: workspace-local atomic resource locks (R031)
///
/// Declarations are durable issue metadata. The lock table is derived live
/// state: it contains only the keys held by current claims and is never a
/// distributed or cross-workspace lock service.
fn migration_10() -> Migration {
    let sql = r#"
-- Resource keys declared by issues. A declaration is portable issue metadata
-- active ownership below is deliberately native-store scheduling state.
CREATE TABLE IF NOT EXISTS issue_resource_keys (
    issue_id TEXT NOT NULL,
    resource_key TEXT NOT NULL,
    PRIMARY KEY (issue_id, resource_key),
    FOREIGN KEY (issue_id) REFERENCES issues(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS issue_resource_keys_key
    ON issue_resource_keys (resource_key, issue_id);

-- At most one in-progress issue in this workspace may hold a normalized key.
-- A NULL lease_fencing_token denotes an ordinary non-leased claim.
CREATE TABLE IF NOT EXISTS resource_locks (
    resource_key TEXT NOT NULL PRIMARY KEY,
    issue_id TEXT NOT NULL,
    lease_fencing_token INTEGER,
    acquired_at TEXT NOT NULL,
    FOREIGN KEY (issue_id) REFERENCES issues(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS resource_locks_issue
    ON resource_locks (issue_id);
"#;

    Migration {
        sql: sql.to_string(),
    }
}

/// Migration 11: atomic create idempotency bindings (R032)
fn migration_11() -> Migration {
    let sql = r#"
-- A unique create reference is distinct from ordinary R011 references:
-- ordinary references intentionally permit the same namespace/key on several
-- issues, while this table reserves one namespace/key for one issue.
CREATE TABLE IF NOT EXISTS unique_reference_bindings (
    namespace TEXT NOT NULL,
    key TEXT NOT NULL,
    issue_id TEXT NOT NULL,
    PRIMARY KEY (namespace, key),
    FOREIGN KEY (issue_id) REFERENCES issues(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS unique_reference_bindings_issue
    ON unique_reference_bindings (issue_id);
"#;

    Migration {
        sql: sql.to_string(),
    }
}

/// Migration 12: Add 'fork' to provenance_receipts kind constraint (R028)
///
/// Updates the CHECK constraint on provenance_receipts.kind to allow 'fork'
/// receipts, which are created when a workspace identity is explicitly forked.
fn migration_12() -> Migration {
    let sql = r#"
-- Recreate provenance_receipts table with updated CHECK constraint
CREATE TABLE IF NOT EXISTS provenance_receipts_new (
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
);

-- Copy existing data to new table
INSERT INTO provenance_receipts_new
SELECT * FROM provenance_receipts;

-- Drop old table and rename new one
DROP TABLE provenance_receipts;
ALTER TABLE provenance_receipts_new RENAME TO provenance_receipts;

-- Recreate indexes
CREATE INDEX IF NOT EXISTS provenance_receipts_source_uuid
    ON provenance_receipts (source_store_uuid);
CREATE INDEX IF NOT EXISTS provenance_receipts_target_uuid
    ON provenance_receipts (target_store_uuid);
CREATE INDEX IF NOT EXISTS provenance_receipts_source_root
    ON provenance_receipts (source_root_sha256);
"#;

    Migration {
        sql: sql.to_string(),
    }
}

/// Migration 13: retain one append-only lease row per claim epoch (R002)
///
/// Migration 4 originally used issue_id as the lease primary key. Lease
/// history is now intentionally append-only, so a released issue can be
/// leased again without overwriting the prior epoch. Existing rows are
/// preserved while a surrogate row identity removes the one-row-per-issue
/// restriction.
fn migration_13() -> Migration {
    let sql = r#"
CREATE TABLE leases_history_new (
    lease_id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    issue_id TEXT NOT NULL,
    assignee TEXT NOT NULL,
    fencing_token INTEGER NOT NULL,
    expires_at TEXT NOT NULL,
    renewed_at TEXT,
    created_at TEXT NOT NULL,
    FOREIGN KEY (issue_id) REFERENCES issues(id) ON DELETE CASCADE
);

INSERT INTO leases_history_new
    (issue_id, assignee, fencing_token, expires_at, renewed_at, created_at)
SELECT issue_id, assignee, fencing_token, expires_at, renewed_at, created_at
FROM leases;

DROP TABLE leases;
ALTER TABLE leases_history_new RENAME TO leases;

CREATE INDEX leases_expiry ON leases (expires_at);
CREATE INDEX leases_fencing ON leases (issue_id, assignee, fencing_token);
CREATE INDEX leases_assignee ON leases (assignee, expires_at);
"#;

    Migration {
        sql: sql.to_string(),
    }
}

/// Migration 14: Attempt outcomes (attempt-outcome-v1 contract)
///
/// Adds the attempt_outcomes table for recording execution attempt outcomes
/// atomically with lifecycle transitions per ADR-011 and the attempt-outcome-v1
/// specification. This enables idempotent replay, conflict detection, and
/// checkpoint round-trip of attempt receipts.
fn migration_14() -> Migration {
    let sql = r#"
-- Attempt outcomes table for recording execution attempt results
CREATE TABLE IF NOT EXISTS attempt_outcomes (
    receipt_id TEXT NOT NULL PRIMARY KEY,
    attempt_id TEXT NOT NULL UNIQUE,
    issue_id TEXT NOT NULL,
    outcome TEXT NOT NULL CHECK (outcome IN ('verified_success', 'work_failure', 'infrastructure_failure', 'cancelled', 'indeterminate')),
    action TEXT CHECK (action IN ('close', 'release', 'quarantine', 'block', 'none')),
    reason TEXT,
    canonical_request_hash TEXT NOT NULL,
    prior_attempt_tier INTEGER NOT NULL CHECK (prior_attempt_tier >= 0 AND prior_attempt_tier <= 3),
    resulting_attempt_tier INTEGER NOT NULL CHECK (resulting_attempt_tier >= 0 AND resulting_attempt_tier <= 3),
    resulting_issue_revision INTEGER NOT NULL,
    actor TEXT NOT NULL,
    created_at TEXT NOT NULL,
    evidence_refs_json TEXT,
    model TEXT,
    harness TEXT,
    harness_version TEXT,
    FOREIGN KEY (issue_id) REFERENCES issues(id) ON DELETE CASCADE
);

-- Index for looking up outcomes by issue
CREATE INDEX IF NOT EXISTS attempt_outcomes_issue ON attempt_outcomes (issue_id);

-- Index for looking up outcomes by attempt_id
CREATE INDEX IF NOT EXISTS attempt_outcomes_attempt ON attempt_outcomes (attempt_id);

-- Index for chronological queries
CREATE INDEX IF NOT EXISTS attempt_outcomes_created ON attempt_outcomes (created_at);
"#;

    Migration {
        sql: sql.to_string(),
    }
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

    /// A store left behind by an older binary must advance when opened, not
    /// stay behind until someone happens to run `init`. 61 of 63 workspaces on
    /// ex44 were found stuck this way on 2026-09-01.
    /// Losing the race to another process must be a no-op, not a primary-key
    /// violation surfacing inside an unrelated command.
    #[test]
    fn apply_migrations_tolerates_a_concurrent_winner() {
        let conn = Connection::open_in_memory().unwrap();
        apply_migrations(&conn).unwrap();

        // Rewind the bookkeeping only: the table migration 14 adds is still
        // present, exactly as it would be for the process that lost the race.
        conn.execute("DELETE FROM schema_migrations WHERE version >= 14", [])
            .unwrap();
        conn.execute(
            "INSERT INTO schema_migrations (version, applied_at, checksum) VALUES (14, 'now', 'x')",
            [],
        )
        .unwrap();

        apply_migrations(&conn).unwrap();
        assert!(schema_is_current(&conn));
    }

    #[test]
    fn migrate_if_pending_advances_a_legacy_store() {
        let conn = Connection::open_in_memory().unwrap();
        apply_migrations(&conn).unwrap();

        // Rewind to what a pre-14 binary would have left behind.
        conn.execute("DROP TABLE attempt_outcomes", []).unwrap();
        conn.execute("DELETE FROM schema_migrations WHERE version >= 14", [])
            .unwrap();
        assert!(!schema_is_current(&conn));

        migrate_if_pending(&conn).unwrap();

        assert!(schema_is_current(&conn));
        let version: i64 = conn
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(version, CURRENT_VERSION);
        let exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='attempt_outcomes'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(exists, 1);
    }

    /// The common path is a pure read: an already-current store must not be
    /// rewritten on open, or every worker sharing a workspace serialises
    /// behind a write lock.
    #[test]
    fn migrate_if_pending_is_a_noop_when_current() {
        let conn = Connection::open_in_memory().unwrap();
        apply_migrations(&conn).unwrap();
        let before: String = conn
            .query_row(
                "SELECT applied_at FROM schema_migrations WHERE version = ?1",
                [&CURRENT_VERSION.to_string()],
                |row| row.get(0),
            )
            .unwrap();

        migrate_if_pending(&conn).unwrap();

        let after: String = conn
            .query_row(
                "SELECT applied_at FROM schema_migrations WHERE version = ?1",
                [&CURRENT_VERSION.to_string()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(before, after, "an up-to-date store was rewritten on open");
    }

    /// A database with no schema_migrations table at all is still migrated.
    #[test]
    fn migrate_if_pending_bootstraps_an_unversioned_store() {
        let conn = Connection::open_in_memory().unwrap();
        assert!(!schema_is_current(&conn));
        migrate_if_pending(&conn).unwrap();
        assert!(schema_is_current(&conn));
    }

    #[test]
    fn test_apply_migrations_empty_db() {
        let conn = Connection::open_in_memory().unwrap();
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
            "provenance_receipts",
            "leases",
            "external_references",
            "unique_reference_bindings",
            "issue_resource_keys",
            "resource_locks",
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
        let conn = Connection::open_in_memory().unwrap();
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

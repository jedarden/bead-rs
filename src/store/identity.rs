//! Schema-identity marker for `.beads/beads.db`.
//!
//! AGENTS.md's compatibility boundary says bead-rs must never write another
//! implementation's live SQLite database. The field failure mode when that
//! boundary is crossed by accident is silent: a foreign store surfaces only a
//! generic "no such column" error, or -- worse -- `CREATE TABLE IF NOT
//! EXISTS` no-ops against a same-named foreign table while `schema_migrations`
//! still records the migration as applied, so later reads/writes assume
//! columns that were never created. This module gives every connection a
//! cheap, read-only way to tell "ours, possibly old" from "not ours" before
//! any migration, probe, or repair touches the file.

use rusqlite::Connection;
use std::path::Path;

/// Fixed marker identifying the bead-rs on-disk format. This is NOT a
/// per-workspace identity (that is `workspace.uuid`, minted fresh for every
/// `bead init`); it is a single constant stamped into every bead-rs database
/// ever created, so a tampered or foreign row can never coincidentally match
/// it.
pub const FORMAT_UUID: &str = "f6a8c2e0-6b1d-4b7a-9b3a-2e7a7c5e9a4f";

/// Format revision. Bumped only if the marker's own shape changes; unrelated
/// to [`super::migrations::CURRENT_VERSION`].
pub const FORMAT_VERSION: i64 = 1;

/// What a connection's table set looks like before any migration runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemaShape {
    /// No tables at all: a brand-new (possibly zero-byte) database file.
    /// Safe for `apply_migrations` to build the native schema from scratch.
    Empty,
    /// Carries both `schema_migrations` and `workspace`, the two tables
    /// every bead-rs schema has had since migration 1. Possibly an old
    /// schema version; still ours, so migrations must advance it rather
    /// than refuse it.
    Native,
    /// Has tables, but not the native fingerprint pair above. Written by
    /// something other than bead-rs. Must never be migrated, probed, or
    /// repaired.
    Foreign,
}

/// Classify a connection's table set, read-only, before any PRAGMA that
/// could write to the file (notably `journal_mode = WAL`) and before any
/// migration runs.
pub fn classify_schema_shape(conn: &Connection) -> rusqlite::Result<SchemaShape> {
    let table_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table'",
        [],
        |row| row.get(0),
    )?;
    if table_count == 0 {
        return Ok(SchemaShape::Empty);
    }

    let has_table = |name: &str| -> bool {
        conn.query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [name],
            |_| Ok(()),
        )
        .is_ok()
    };

    if has_table("schema_migrations") && has_table("workspace") {
        Ok(SchemaShape::Native)
    } else {
        Ok(SchemaShape::Foreign)
    }
}

/// Structured refusal for a [`SchemaShape::Foreign`] (or unreadable) database.
/// Names the mismatch and points at the fix without claiming to identify the
/// foreign tool -- the clean-room boundary prohibits inspecting or naming
/// another implementation's internals, and the shape check above derives
/// purely from facts about bead-rs's own schema.
pub fn foreign_schema_message(db_path: &Path) -> String {
    format!(
        "Refusing to open {}: it has data but no bead-rs schema-identity marker \
         (the `schema_migrations`/`workspace` tables bead-rs stamps at init). This looks like \
         a database written by a different bead-tracking tool that happens to share this file \
         name. bead-rs will not read, write, migrate, or repair it -- doing so is exactly the \
         silent-corruption failure this guard exists to prevent. Point this workspace at \
         whichever tool created {}, or move/remove the file and run `bead init` here to create \
         a fresh bead-rs database.",
        db_path.display(),
        db_path.display()
    )
}

/// The same refusal, as the `rusqlite::Error` [`super::sqlite::open_configured_connection`]
/// returns so every caller through that chokepoint fails closed identically.
pub(crate) fn foreign_schema_sqlite_error(db_path: &Path) -> rusqlite::Error {
    rusqlite::Error::SqliteFailure(
        rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_MISUSE),
        foreign_schema_message(db_path).into(),
    )
}

/// Verify the format marker on an already-migrated native connection.
///
/// Called on every open, after migrations run (migration 21 is what stamps
/// the marker, so an old-but-native store only carries it after advancing).
/// A missing marker is not an error here: a brand-new workspace reaches this
/// call before its `workspace` row exists yet (the row is inserted by
/// `init_workspace` right after `open_configured_connection` returns), and
/// that row is created with the marker already set. A *present but wrong*
/// marker means the row was written or altered by something that is not
/// this build of bead-rs, which is the same class of risk as a foreign
/// table shape.
pub fn verify_format_identity(conn: &Connection) -> rusqlite::Result<()> {
    let marker: Option<(Option<String>, Option<i64>)> = conn
        .query_row(
            "SELECT format_uuid, format_version FROM workspace WHERE id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .ok();

    if let Some((Some(uuid), Some(version))) = marker {
        if uuid != FORMAT_UUID || version != FORMAT_VERSION {
            return Err(rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_MISUSE),
                format!(
                    "Refusing to operate: workspace schema-identity marker {}/{} does not \
                     match the bead-rs format {}/{}. This database's `workspace` row was not \
                     written by this build of bead-rs.",
                    uuid, version, FORMAT_UUID, FORMAT_VERSION
                )
                .into(),
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_empty_database() {
        let conn = Connection::open_in_memory().unwrap();
        assert_eq!(classify_schema_shape(&conn).unwrap(), SchemaShape::Empty);
    }

    #[test]
    fn classify_native_database() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY);
             CREATE TABLE workspace (id INTEGER PRIMARY KEY);",
        )
        .unwrap();
        assert_eq!(classify_schema_shape(&conn).unwrap(), SchemaShape::Native);
    }

    #[test]
    fn classify_foreign_database() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE issues (id TEXT PRIMARY KEY, status TEXT);
             CREATE TABLE config (key TEXT PRIMARY KEY, value TEXT);",
        )
        .unwrap();
        assert_eq!(classify_schema_shape(&conn).unwrap(), SchemaShape::Foreign);
    }

    #[test]
    fn classify_foreign_database_with_schema_migrations_but_no_workspace() {
        // A near-miss: carries one of the two fingerprint tables but not
        // both. Still foreign -- the pair is the fingerprint, not either
        // table alone.
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY);
             CREATE TABLE other_stuff (id INTEGER PRIMARY KEY);",
        )
        .unwrap();
        assert_eq!(classify_schema_shape(&conn).unwrap(), SchemaShape::Foreign);
    }

    #[test]
    fn verify_format_identity_accepts_matching_marker() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE workspace (id INTEGER PRIMARY KEY, format_uuid TEXT, format_version INTEGER);",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO workspace (id, format_uuid, format_version) VALUES (1, ?1, ?2)",
            rusqlite::params![FORMAT_UUID, FORMAT_VERSION],
        )
        .unwrap();
        verify_format_identity(&conn).unwrap();
    }

    #[test]
    fn verify_format_identity_rejects_mismatched_marker() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE workspace (id INTEGER PRIMARY KEY, format_uuid TEXT, format_version INTEGER);",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO workspace (id, format_uuid, format_version) VALUES (1, 'not-ours', 99)",
            [],
        )
        .unwrap();
        assert!(verify_format_identity(&conn).is_err());
    }

    #[test]
    fn verify_format_identity_tolerates_absent_marker() {
        // No `workspace` row yet (fresh-init window) and no marker columns
        // at all (pre-migration-21 shape) both pass: there is nothing to
        // contradict.
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE workspace (id INTEGER PRIMARY KEY);")
            .unwrap();
        verify_format_identity(&conn).unwrap();
    }
}

//! Durable historical-redaction hold and checked local SQLite cleanup.

use crate::error::{Error, Result};
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use std::path::Path;

/// A committed epoch is unfinished until sanitized publication and local
/// cleanup succeed. This is stored state, not a process-local lock flag.
pub fn pending_redaction(conn: &Connection) -> Result<Option<String>> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = 'redaction_epochs')",
        [],
        |row| row.get(0),
    )?;
    if !exists {
        return Ok(None);
    }
    Ok(conn
        .query_row(
            "SELECT r.receipt_id FROM redaction_epochs e
             JOIN redaction_receipts r ON r.epoch_id = e.epoch_id
             WHERE e.publication_state = 'committed'
             ORDER BY e.opened_at, r.receipt_id LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()?)
}

pub fn ensure_publication_allowed(conn: &Connection) -> Result<()> {
    super::secret_boundary::ensure_not_quarantined(conn)?;
    if let Some(receipt) = pending_redaction(conn)? {
        return Err(Error::conflict(format!(
            "secret_redaction_pending: sanitized publication and local cleanup are unfinished; resume with 'bead redact --resume {receipt}'"
        )));
    }
    Ok(())
}

/// Git helpers have no store parameter. Probe the native database read-only
/// rather than opening an auto-migrating connection merely to stage files.
pub fn ensure_staging_allowed(workspace_root: &Path) -> Result<()> {
    let database = workspace_root.join(".beads/beads.db");
    if database.is_file() {
        let conn = Connection::open_with_flags(database, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        ensure_publication_allowed(&conn)?;
    }
    Ok(())
}

pub fn enable_secure_erasure(conn: &Connection) -> Result<()> {
    conn.execute_batch("PRAGMA secure_delete = ON")?;
    let enabled: i64 = conn.query_row("PRAGMA secure_delete", [], |row| row.get(0))?;
    if enabled != 1 {
        return Err(Error::integrity(
            "SQLite secure deletion could not be enabled",
        ));
    }
    Ok(())
}

/// Must run outside a transaction, with the maintenance/publication locks
/// retained. A busy checkpoint is a retryable failure, never false success.
pub fn purge_local_remnants(conn: &Connection) -> Result<()> {
    enable_secure_erasure(conn)?;
    if !conn.is_autocommit() {
        return Err(Error::integrity(
            "local redaction cleanup requires no open transaction",
        ));
    }
    conn.execute_batch("VACUUM")?;
    let (busy, frames, checkpointed): (i64, i64, i64) =
        conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;
    if busy != 0 || (frames >= 0 && (frames != 0 || checkpointed != 0)) {
        return Err(Error::DatabaseBusy(
            "local redaction cleanup is held by an active SQLite reader or writer; close that connection and resume the redaction receipt".to_string(),
        ));
    }
    Ok(())
}

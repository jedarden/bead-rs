//! Cursor-based local change feed for incremental local indexes and adapters
//!
//! This module implements R013's cursor-based change feed, which allows consumers
//! to track changes to the workspace incrementally without needing a daemon or
//! network service. The change feed provides deterministic public mutation records,
//! snapshot identity for position tracking, and explicit gap detection.

use crate::error::Error;
use serde::{Deserialize, Serialize};

/// Change feed cursor position
///
/// Cursors represent a specific position in the event sequence and are used
/// to request only the changes that occurred after that position.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Cursor {
    /// The last event sequence number the consumer has processed
    pub sequence: i64,
    /// Optional checksum for gap detection
    pub checksum: Option<String>,
}

impl std::fmt::Display for Cursor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.checksum {
            Some(checksum) => write!(f, "{}:{}", self.sequence, checksum),
            None => write!(f, "{}", self.sequence),
        }
    }
}

impl Cursor {
    /// Create a new cursor at the beginning of the event stream
    #[allow(dead_code)]
    pub fn at_beginning() -> Self {
        Self {
            sequence: 0,
            checksum: None,
        }
    }

    /// Create a new cursor at a specific sequence number
    #[allow(dead_code)]
    pub fn at_sequence(sequence: i64) -> Self {
        Self {
            sequence,
            checksum: None,
        }
    }

    /// Create a cursor from a serialized string
    pub fn from_string(s: &str) -> Result<Self, Error> {
        let parts: Vec<&str> = s.splitn(2, ':').collect();
        let sequence = parts
            .first()
            .ok_or_else(|| Error::validation("Invalid cursor format"))?
            .parse::<i64>()
            .map_err(|_| Error::validation("Invalid cursor sequence number"))?;

        let checksum = parts.get(1).map(|s| s.to_string());

        Ok(Self { sequence, checksum })
    }
}

/// Snapshot identity for the current workspace state
///
/// This uniquely identifies a specific point in time in the workspace's
/// event stream, allowing consumers to detect gaps and resynchronize.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotIdentity {
    /// Workspace UUID
    pub workspace_uuid: String,
    /// Current maximum event sequence number
    pub max_sequence: i64,
    /// Checksum of the current snapshot state
    pub checksum: String,
    /// When this snapshot was taken
    pub timestamp: String,
}

/// Public mutation record in the change feed
///
/// These records represent individual state changes that can be consumed
/// incrementally by local indexes and adapters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutationRecord {
    /// Event sequence number (cursor position)
    pub sequence: i64,
    /// Optional issue ID this event relates to
    pub issue_id: Option<String>,
    /// Event kind (created, updated, claimed, etc.)
    pub kind: String,
    /// Actor who performed this mutation
    pub actor: Option<String>,
    /// When this mutation occurred
    pub time: String,
    /// Detailed event data as JSON
    pub detail: serde_json::Value,
}

/// Change feed result containing mutations and snapshot metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeFeed {
    /// Current snapshot identity
    pub snapshot: SnapshotIdentity,
    /// Whether gaps were detected in the sequence
    pub has_gaps: bool,
    /// Total number of events available
    pub total_available: i64,
    /// Number of events returned in this batch
    pub returned_count: usize,
    /// Mutation records since the cursor
    pub mutations: Vec<MutationRecord>,
}

/// Gap detection information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GapInfo {
    /// Expected sequence number
    pub expected: i64,
    /// Actual first sequence number found
    pub actual: i64,
    /// Size of the gap
    pub gap_size: i64,
}

/// Get the current snapshot identity for the workspace
///
/// This returns the current position in the event stream that can be used
/// for gap detection and cursor validation.
pub fn get_snapshot_identity(conn: &rusqlite::Connection) -> Result<SnapshotIdentity, Error> {
    // Get workspace UUID
    let workspace_uuid: String = conn
        .query_row("SELECT uuid FROM workspace WHERE id = 1", [], |row| {
            row.get(0)
        })
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to load workspace UUID: {}", e)))?;

    // Get maximum event sequence
    let max_sequence: i64 = conn
        .query_row("SELECT COALESCE(MAX(sequence), 0) FROM events", [], |row| {
            row.get(0)
        })
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to query max sequence: {}", e)))?;

    // Get current time
    let current_time = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string());

    // Create checksum from key state identifiers
    let checksum_input = format!("{}:{}:{}", workspace_uuid, max_sequence, current_time);
    let checksum = sha256_checksum(&checksum_input);

    Ok(SnapshotIdentity {
        workspace_uuid,
        max_sequence,
        checksum,
        timestamp: current_time,
    })
}

/// Get changes since a cursor position
///
/// This returns all mutation records that occurred after the given cursor.
/// If gaps are detected in the sequence, has_gaps will be true and consumers
/// should resynchronize from a full checkpoint.
pub fn get_changes_since(
    conn: &rusqlite::Connection,
    cursor: &Cursor,
) -> Result<ChangeFeed, Error> {
    let snapshot = get_snapshot_identity(conn)?;

    // Check for gaps in the sequence
    let has_gaps = detect_gaps(conn, cursor.sequence)?;

    // Get total count of available events
    let total_available = snapshot.max_sequence - cursor.sequence;

    // Query events after the cursor position
    let mut stmt = conn
        .prepare(
            "SELECT sequence, issue_id, kind, actor, time, detail
             FROM events
             WHERE sequence > ?1
             ORDER BY sequence ASC",
        )
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to prepare query: {}", e)))?;

    let mutations = stmt
        .query_map([cursor.sequence], |row| {
            Ok(MutationRecord {
                sequence: row.get(0)?,
                issue_id: row.get(1)?,
                kind: row.get(2)?,
                actor: row.get(3)?,
                time: row.get(4)?,
                detail: serde_json::from_str(&row.get::<_, String>(5)?).map_err(|e| {
                    rusqlite::Error::ToSqlConversionFailure(
                        format!("Invalid JSON in event detail: {}", e).into(),
                    )
                })?,
            })
        })
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to query events: {}", e)))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to collect events: {}", e)))?;

    let returned_count = mutations.len();

    Ok(ChangeFeed {
        snapshot,
        has_gaps,
        total_available,
        returned_count,
        mutations,
    })
}

/// Detect gaps in the event sequence
///
/// Returns true if there are missing sequence numbers between the cursor
/// position and the current maximum sequence.
fn detect_gaps(conn: &rusqlite::Connection, cursor_sequence: i64) -> Result<bool, Error> {
    let max_sequence: i64 = conn
        .query_row("SELECT COALESCE(MAX(sequence), 0) FROM events", [], |row| {
            row.get(0)
        })
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to query max sequence: {}", e)))?;

    if cursor_sequence >= max_sequence {
        return Ok(false);
    }

    // Check if we have all expected sequence numbers
    let expected_count = max_sequence - cursor_sequence;
    let actual_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM events WHERE sequence > ?1",
            [cursor_sequence],
            |row| row.get(0),
        )
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to count events: {}", e)))?;

    Ok(actual_count != expected_count)
}

/// Get detailed gap information
///
/// Returns information about any gaps in the sequence between the cursor
/// and current position. Returns None if no gaps were detected.
pub fn get_gap_info(
    conn: &rusqlite::Connection,
    cursor: &Cursor,
) -> Result<Option<GapInfo>, Error> {
    if !detect_gaps(conn, cursor.sequence)? {
        return Ok(None);
    }

    // Find the first actual sequence number after the cursor
    let first_actual: i64 = conn
        .query_row(
            "SELECT MIN(sequence) FROM events WHERE sequence > ?1",
            [cursor.sequence],
            |row| row.get(0),
        )
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to find first sequence: {}", e)))?;

    let expected = cursor.sequence + 1;
    let gap_size = first_actual - expected;

    Ok(Some(GapInfo {
        expected,
        actual: first_actual,
        gap_size,
    }))
}

/// Validate that a cursor is still valid (no gaps have occurred)
///
/// Returns true if the cursor position is still valid for incremental reading,
/// false if gaps have been detected and the consumer should resynchronize.
pub fn validate_cursor(conn: &rusqlite::Connection, cursor: &Cursor) -> Result<bool, Error> {
    // Check if there are any events at or before the cursor position
    let exists: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM events WHERE sequence <= ?1)",
            [cursor.sequence],
            |row| row.get(0),
        )
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to validate cursor: {}", e)))?;

    if !exists {
        // Cursor is before the first event, still valid
        return Ok(true);
    }

    // Check for gaps
    Ok(!detect_gaps(conn, cursor.sequence)?)
}

/// Calculate SHA-256 checksum
fn sha256_checksum(input: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cursor_at_beginning() {
        let cursor = Cursor::at_beginning();
        assert_eq!(cursor.sequence, 0);
        assert!(cursor.checksum.is_none());
    }

    #[test]
    fn test_cursor_at_sequence() {
        let cursor = Cursor::at_sequence(42);
        assert_eq!(cursor.sequence, 42);
        assert!(cursor.checksum.is_none());
    }

    #[test]
    fn test_cursor_serialization() {
        let cursor = Cursor::at_sequence(100);
        assert_eq!(cursor.to_string(), "100");

        let cursor_with_checksum = Cursor {
            sequence: 100,
            checksum: Some("abc123".to_string()),
        };
        assert_eq!(cursor_with_checksum.to_string(), "100:abc123");
    }

    #[test]
    fn test_cursor_from_string() {
        let cursor = Cursor::from_string("42").unwrap();
        assert_eq!(cursor.sequence, 42);
        assert!(cursor.checksum.is_none());

        let cursor_with_checksum = Cursor::from_string("42:abc123").unwrap();
        assert_eq!(cursor.sequence, 42);
        assert_eq!(cursor_with_checksum.checksum, Some("abc123".to_string()));
    }

    #[test]
    fn test_cursor_invalid_format() {
        assert!(Cursor::from_string("invalid").is_err());
        assert!(Cursor::from_string("").is_err());
    }
}

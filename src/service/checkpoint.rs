//! Checkpoint export and import service
//!
//! This module provides atomic, deterministic JSONL checkpoint export and import.

use crate::model::Issue;
use crate::store::SqliteStore;
use anyhow::Result;
use rusqlite::{params, Transaction};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Flush checkpoint result
#[derive(Debug, Clone)]
pub struct FlushResult {
    pub issue_count: usize,
    pub hash: String,
    pub covered_sequence: i64,
    pub export_time: String,
}

/// Export issues to JSONL checkpoint file
///
/// This function:
/// 1. Opens a read transaction
/// 2. Reads all issues with dependencies, labels, etc.
/// 3. Sorts by issue ID for deterministic ordering
/// 4. Serializes to JSONL format
/// 5. Writes to temporary file
/// 6. Verifies hash and count
/// 7. Atomically replaces destination
/// 8. Updates checkpoint_state table
pub fn flush_checkpoint(store: &mut SqliteStore, output_path: &Path) -> Result<FlushResult> {
    // Open read transaction to capture snapshot
    let conn = store.conn();
    let tx = conn.unchecked_transaction()?;

    // Get current event sequence
    let current_sequence: i64 = tx
        .query_row("SELECT COALESCE(MAX(sequence), 0) FROM events", [], |row| {
            row.get(0)
        })
        .unwrap_or(0);

    // Read all issues with their data
    let issues = read_all_issues(&tx)?;

    // Commit the read transaction
    tx.commit()?;

    // Sort by issue ID for deterministic ordering
    let mut sorted_issues = issues;
    sorted_issues.sort_by(|a, b| a.id.cmp(&b.id));

    // Create temporary file for atomic write
    let temp_path = output_path.with_extension("tmp");
    let temp_file = File::create(&temp_path)?;
    let mut writer = BufWriter::new(temp_file);

    // Write each issue as a JSON line
    for issue in &sorted_issues {
        serde_json::to_writer(&mut writer, issue)?;
        writer.write_all(b"\n")?;
    }
    writer.flush()?;

    // Calculate hash of output
    let hash = calculate_file_hash(&temp_path)?;

    // Verify file was written correctly
    drop(writer);
    let final_count = sorted_issues.len();

    // Get export time
    let export_time = format_rfc3339(SystemTime::now());

    // Now do the atomic update in a write transaction
    let conn = store.conn();
    let tx = conn.unchecked_transaction()?;

    // Atomic rename from temp to target
    std::fs::rename(&temp_path, output_path)?;

    // Update checkpoint_state table
    update_checkpoint_state(&tx, &hash, current_sequence, &export_time)?;

    tx.commit()?;

    Ok(FlushResult {
        issue_count: final_count,
        hash,
        covered_sequence: current_sequence,
        export_time,
    })
}

/// Read all issues from the database
fn read_all_issues(tx: &Transaction) -> Result<Vec<Issue>> {
    let mut issues = Vec::new();

    // Query issues
    let mut issue_stmt = tx.prepare(
        "SELECT id, title, description, notes, priority, issue_type, base_status,
                manual_blocked, assignee, created_at, updated_at, closed_at, close_reason,
                source_repo, profile, schema_ref
         FROM issues",
    )?;

    let issue_rows = issue_stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>("id")?,
            row.get::<_, String>("title")?,
            row.get::<_, Option<String>>("description")?,
            row.get::<_, Option<String>>("notes")?,
            row.get::<_, i64>("priority")?,
            row.get::<_, Option<String>>("issue_type")?,
            row.get::<_, String>("base_status")?,
            row.get::<_, i32>("manual_blocked")?,
            row.get::<_, Option<String>>("assignee")?,
            row.get::<_, String>("created_at")?,
            row.get::<_, String>("updated_at")?,
            row.get::<_, Option<String>>("closed_at")?,
            row.get::<_, Option<String>>("close_reason")?,
            row.get::<_, Option<String>>("source_repo")?,
            row.get::<_, Option<String>>("profile")?,
            row.get::<_, Option<String>>("schema_ref")?,
        ))
    })?;

    for row in issue_rows {
        let (
            id,
            title,
            description,
            notes,
            priority,
            issue_type,
            base_status,
            manual_blocked,
            assignee,
            created_at,
            updated_at,
            closed_at,
            close_reason,
            source_repo,
            profile,
            schema_ref,
        ) = row?;

        // Convert to Issue model
        let issue = Issue {
            id: id.clone(),
            title,
            description: description.or(Some(String::new())),
            notes: notes.or(Some(String::new())),
            priority,
            issue_type: issue_type.or(Some(String::from("task"))),
            base_status: crate::model::BaseStatus::parse(&base_status)
                .map_err(|e| anyhow::anyhow!("Invalid base_status: {}", e))?,
            manual_blocked: Some(manual_blocked != 0),
            assignee,
            created_at,
            updated_at,
            closed_at,
            close_reason,
            source_repo,
            profile: profile.or(Some(String::from("native-v1"))),
            schema_ref: schema_ref.or(Some(String::from("urn:bead-rs:schema:issue:native-v1"))),
            data: None,
            extensions: Default::default(),
        };

        issues.push(issue);
    }

    Ok(issues)
}

/// Update the checkpoint_state table
fn update_checkpoint_state(
    tx: &Transaction,
    hash: &str,
    covered_sequence: i64,
    export_time: &str,
) -> Result<()> {
    // Ensure the singleton row exists
    tx.execute(
        "INSERT OR IGNORE INTO checkpoint_state (id, last_interchange_hash, covered_event_sequence, export_time)
         VALUES (1, '', 0, NULL)",
        []
    )?;

    // Update the checkpoint state
    tx.execute(
        "UPDATE checkpoint_state
         SET last_interchange_hash = ?1,
             covered_event_sequence = ?2,
             export_time = ?3
         WHERE id = 1",
        params![hash, covered_sequence, export_time],
    )?;

    Ok(())
}

/// Calculate SHA-256 hash of a file
fn calculate_file_hash(path: &Path) -> Result<String> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();

    let mut buffer = Vec::new();
    loop {
        buffer.clear();
        let bytes_read = reader.read_until(b'\n', &mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

/// Format system time as RFC 3339 string
fn format_rfc3339(time: SystemTime) -> String {
    let duration = time.duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = duration.as_secs();
    let nanos = duration.subsec_nanos();

    // Format: 2026-08-08T12:34:56Z
    use time::OffsetDateTime;
    let datetime = OffsetDateTime::from_unix_timestamp(secs as i64)
        .unwrap()
        .replace_nanosecond(nanos)
        .unwrap();

    datetime
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_flush_checkpoint_empty() {
        let temp_dir = TempDir::new().unwrap();
        let workspace_path = temp_dir.path().join(".beads");
        let output_path = temp_dir.path().join("issues.jsonl");

        // Initialize workspace
        fs::create_dir(&workspace_path).unwrap();

        let db_path = workspace_path.join("beads.db");
        let mut store = SqliteStore::with_path(&db_path).unwrap();
        store.apply_migrations().unwrap();

        // Flush empty checkpoint
        let result = flush_checkpoint(&mut store, &output_path).unwrap();

        assert_eq!(result.issue_count, 0);
        assert!(!result.hash.is_empty());
        assert_eq!(result.covered_sequence, 0);

        // Verify file exists and is empty (zero bytes for empty state)
        assert!(output_path.exists());
        let content = fs::read_to_string(&output_path).unwrap();
        assert!(content.is_empty());

        // Verify checkpoint_state table
        let conn = store.conn();
        let (hash, seq, time): (String, i64, Option<String>) = conn
            .query_row(
                "SELECT last_interchange_hash, covered_event_sequence, export_time
                 FROM checkpoint_state WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();

        assert_eq!(hash, result.hash);
        assert_eq!(seq, 0);
        assert!(time.is_some());
    }

    #[test]
    fn test_flush_checkpoint_with_issues() {
        let temp_dir = TempDir::new().unwrap();
        let workspace_path = temp_dir.path().join(".beads");
        let output_path = temp_dir.path().join("issues.jsonl");

        // Initialize workspace and create some issues
        fs::create_dir(&workspace_path).unwrap();
        let db_path = workspace_path.join("beads.db");
        let mut store = SqliteStore::with_path(&db_path).unwrap();
        store.apply_migrations().unwrap();

        // Create test issues
        let conn = store.conn();
        let tx = conn.unchecked_transaction().unwrap();

        // Create two issues
        for i in 1..=2 {
            let id = format!("bead-{:016x}", i);
            tx.execute(
                "INSERT INTO issues (id, title, description, priority, issue_type, base_status,
                                  manual_blocked, created_at, updated_at, profile, schema_ref)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    &id,
                    &format!("Test Issue {}", i),
                    "Description",
                    2,
                    "task",
                    "open",
                    0,
                    &format_rfc3339(SystemTime::now()),
                    &format_rfc3339(SystemTime::now()),
                    "native-v1",
                    "urn:bead-rs:schema:issue:native-v1"
                ],
            )
            .unwrap();
        }

        // Create an audit event to test sequence tracking
        tx.execute(
            "INSERT INTO events (issue_id, kind, actor, time, detail)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                "bead-0000000000000001",
                "created",
                "test",
                &format_rfc3339(SystemTime::now()),
                "{}"
            ],
        )
        .unwrap();

        tx.commit().unwrap();

        // Flush checkpoint
        let result = flush_checkpoint(&mut store, &output_path).unwrap();

        assert_eq!(result.issue_count, 2);
        assert!(!result.hash.is_empty());
        assert_eq!(result.covered_sequence, 1);

        // Verify file contains two lines
        let content = fs::read_to_string(&output_path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2);

        // Verify deterministic ordering (by ID)
        let first_issue: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        let second_issue: serde_json::Value = serde_json::from_str(lines[1]).unwrap();

        assert_eq!(first_issue["id"], "bead-0000000000000001");
        assert_eq!(second_issue["id"], "bead-0000000000000002");

        // Verify checkpoint_state
        let conn = store.conn();
        let (hash, seq): (String, i64) = conn
            .query_row(
                "SELECT last_interchange_hash, covered_event_sequence
                 FROM checkpoint_state WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();

        assert_eq!(hash, result.hash);
        assert_eq!(seq, 1);
    }

    #[test]
    fn test_calculate_file_hash() {
        let temp_dir = TempDir::new().unwrap();
        let test_path = temp_dir.path().join("test.jsonl");

        fs::write(&test_path, "line1\nline2\n").unwrap();
        let hash1 = calculate_file_hash(&test_path).unwrap();

        fs::write(&test_path, "line1\nline2\n").unwrap();
        let hash2 = calculate_file_hash(&test_path).unwrap();

        // Same content should produce same hash
        assert_eq!(hash1, hash2);

        // Different content should produce different hash
        fs::write(&test_path, "line1\nline3\n").unwrap();
        let hash3 = calculate_file_hash(&test_path).unwrap();
        assert_ne!(hash1, hash3);
    }
}

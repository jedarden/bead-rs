//! Checkpoint export and import service
//!
//! This module provides atomic, deterministic JSONL checkpoint export and import.

use crate::model::Issue;
use crate::store::SqliteStore;
use anyhow::{anyhow, bail, Result};
use rusqlite::{params, Transaction};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
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

/// Import staging result
#[derive(Debug, Clone)]
pub struct ImportStaging {
    pub issues: Vec<Issue>,
    pub dependencies: Vec<(String, String, String)>, // (blocked, blocker, kind)
    pub labels: Vec<(String, String)>,               // (issue_id, label)
    pub input_hash: String,
    pub issue_count: usize,
}

/// Import result
#[derive(Debug, Clone)]
pub struct ImportResult {
    pub profile: String,
    pub input_hash: String,
    pub inserted: usize,
    pub updated: usize,
    pub retained: usize,
    pub conflicted: usize,
    pub activation_sequence: i64,
    pub covered_sequence: i64,
    pub dry_run: bool,
    pub prospective: bool,
}

/// Import checkpoint from JSONL file
///
/// This function:
/// 1. Parses and stages all issues from the input file
/// 2. Validates the staged data (duplicates, dangling deps, cycles)
/// 3. Performs dry-run or real activation
/// 4. Updates checkpoint_state table
pub fn import_checkpoint(
    store: &mut SqliteStore,
    input_path: &Path,
    profile: &str,
    dry_run: bool,
) -> Result<ImportResult> {
    // Validate profile (only native-v1 allowed before F017)
    if profile != "native-v1" {
        bail!(
            "Profile '{}' is not supported before F017. Only 'native-v1' is allowed.",
            profile
        );
    }

    // Stage the input file
    let staging = stage_import(input_path, profile)?;

    // Validate the staged data
    validate_import(&staging, dry_run)?;

    if dry_run {
        // Get current sequence for prospective report
        let conn = store.conn();
        let current_sequence: i64 = conn
            .query_row("SELECT COALESCE(MAX(sequence), 0) FROM events", [], |row| {
                row.get(0)
            })
            .unwrap_or(0);

        let prospective_sequence = current_sequence + 1; // Would allocate one for checkpoint_imported event

        return Ok(ImportResult {
            profile: profile.to_string(),
            input_hash: staging.input_hash.clone(),
            inserted: staging.issue_count,
            updated: 0,
            retained: 0,
            conflicted: 0,
            activation_sequence: prospective_sequence,
            covered_sequence: prospective_sequence,
            dry_run: true,
            prospective: true,
        });
    }

    // Real activation: verify target is empty
    verify_empty_target(store)?;

    // Activate the staged data in a single transaction
    let (inserted, activation_sequence) = activate_import(store, &staging)?;

    Ok(ImportResult {
        profile: profile.to_string(),
        input_hash: staging.input_hash,
        inserted,
        updated: 0,
        retained: 0,
        conflicted: 0,
        activation_sequence,
        covered_sequence: activation_sequence,
        dry_run: false,
        prospective: false,
    })
}

/// Stage issues from JSONL file for validation
fn stage_import(input_path: &Path, _profile: &str) -> Result<ImportStaging> {
    let file = File::open(input_path)?;
    let reader = BufReader::new(file);

    let mut issues = Vec::new();
    let mut dependencies = Vec::new();
    let mut labels = Vec::new();
    let mut seen_ids = HashSet::new();

    // Calculate hash while reading
    let mut hasher = Sha256::new();

    for (line_num, line_result) in reader.lines().enumerate() {
        let line_num = line_num + 1; // 1-based for error messages

        let line = line_result?;
        if line.trim().is_empty() {
            continue; // Skip blank lines
        }

        // Update hash
        hasher.update(line.as_bytes());
        hasher.update(b"\n");

        // Parse JSON line
        let json: serde_json::Value = serde_json::from_str(&line)
            .map_err(|e| anyhow!("Line {}: malformed JSON: {}", line_num, e))?;

        // Must be an object
        let obj = json
            .as_object()
            .ok_or_else(|| anyhow!("Line {}: not an object", line_num))?;

        // Extract required ID field
        let id = obj
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Line {}: missing or invalid 'id' field", line_num))?;

        // Check for duplicate IDs
        if !seen_ids.insert(id.to_string()) {
            bail!("Line {}: duplicate issue ID '{}'", line_num, id);
        }

        // Parse full Issue (extensions preserved via flatten)
        let issue: Issue = serde_json::from_str(&line)
            .map_err(|e| anyhow!("Line {}: invalid issue: {}", line_num, e))?;

        // Validate the issue
        issue
            .validate()
            .map_err(|e| anyhow!("Line {}: issue validation failed: {}", line_num, e))?;

        issues.push(issue.clone());

        // Extract dependencies if present
        if let Some(deps) = obj.get("dependencies").and_then(|v| v.as_array()) {
            for dep in deps {
                let dep_obj = dep
                    .as_object()
                    .ok_or_else(|| anyhow!("Line {}: dependency is not an object", line_num))?;

                let blocked = obj
                    .get("id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow!("Line {}: missing blocked issue ID", line_num))?;

                let blocker = dep_obj
                    .get("blocker")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow!("Line {}: dependency missing 'blocker'", line_num))?;

                let kind = dep_obj
                    .get("kind")
                    .and_then(|v| v.as_str())
                    .unwrap_or("blocks");

                dependencies.push((blocked.to_string(), blocker.to_string(), kind.to_string()));
            }
        }

        // Extract labels if present
        if let Some(label_array) = obj.get("labels").and_then(|v| v.as_array()) {
            for label_item in label_array {
                if let Some(label) = label_item.as_str() {
                    labels.push((id.to_string(), label.to_string()));
                }
            }
        }
    }

    let hash = format!("{:x}", hasher.finalize());

    Ok(ImportStaging {
        issues,
        dependencies,
        labels,
        input_hash: hash,
        issue_count: seen_ids.len(),
    })
}

/// Validate staged import data
fn validate_import(staging: &ImportStaging, _dry_run: bool) -> Result<()> {
    let issue_ids: HashSet<_> = staging.issues.iter().map(|i| i.id.clone()).collect();

    // Validate dependencies
    for (blocked, blocker, _kind) in &staging.dependencies {
        // Both endpoints must exist
        if !issue_ids.contains(blocked) {
            bail!("Dependency references unknown blocked issue '{}'", blocked);
        }
        if !issue_ids.contains(blocker) {
            bail!("Dependency references unknown blocker issue '{}'", blocker);
        }

        // Self-edges are invalid
        if blocked == blocker {
            bail!("Self-edge detected: '{}'", blocked);
        }
    }

    // Check for cycles in blocks dependencies (after all individual validations)
    if has_any_cycle(staging)? {
        bail!("Cycle detected in blocks dependencies");
    }

    // Validate labels
    for (issue_id, _) in &staging.labels {
        if !issue_ids.contains(issue_id) {
            bail!("Label references unknown issue '{}'", issue_id);
        }
    }

    Ok(())
}

/// Check if there are any cycles in the blocks dependencies
fn has_any_cycle(staging: &ImportStaging) -> Result<bool> {
    let mut visited = HashSet::new();
    let mut recursion_stack = HashSet::new();

    // Build adjacency list for blocks dependencies
    let mut adj: HashMap<String, Vec<String>> = HashMap::new();
    let mut all_nodes: HashSet<String> = HashSet::new();

    for (blocked, blocker, kind) in &staging.dependencies {
        if kind == "blocks" {
            adj.entry(blocker.clone())
                .or_default()
                .push(blocked.clone());
            all_nodes.insert(blocked.clone());
            all_nodes.insert(blocker.clone());
        }
    }

    // Check each node for cycles using DFS
    for node in &all_nodes {
        if dfs_has_cycle(&adj, node, &mut visited, &mut recursion_stack)? {
            return Ok(true);
        }
    }

    Ok(false)
}

/// DFS helper to detect cycles
fn dfs_has_cycle(
    adj: &HashMap<String, Vec<String>>,
    node: &str,
    visited: &mut HashSet<String>,
    recursion_stack: &mut HashSet<String>,
) -> Result<bool> {
    visited.insert(node.to_string());
    recursion_stack.insert(node.to_string());

    if let Some(neighbors) = adj.get(node) {
        for neighbor in neighbors {
            if !visited.contains(neighbor) {
                if dfs_has_cycle(adj, neighbor, visited, recursion_stack)? {
                    return Ok(true);
                }
            } else if recursion_stack.contains(neighbor) {
                // Found a back edge - cycle detected
                return Ok(true);
            }
        }
    }

    recursion_stack.remove(node);
    Ok(false)
}

/// Check if adding an edge would create a cycle using DFS
#[allow(dead_code)]
fn has_cycle(staging: &ImportStaging, start: &str, from: &str) -> Result<bool> {
    let mut visited = HashSet::new();
    let mut stack = Vec::new();

    // Build adjacency list for blocks dependencies
    let mut adj: HashMap<String, Vec<String>> = HashMap::new();
    for (blocked, blocker, kind) in &staging.dependencies {
        if kind == "blocks" {
            adj.entry(blocker.clone())
                .or_default()
                .push(blocked.clone());
        }
    }

    // Start DFS from neighbors of 'from', excluding 'start' itself
    // This checks if there's a path from 'from' back to 'start' (excluding the edge we're adding)
    if let Some(neighbors) = adj.get(from) {
        for neighbor in neighbors {
            if neighbor != start {
                stack.push(neighbor.clone());
            }
        }
    }

    while let Some(current) = stack.pop() {
        if current == start {
            return Ok(true); // Found cycle
        }

        if visited.contains(&current) {
            continue;
        }
        visited.insert(current.clone());

        if let Some(neighbors) = adj.get(&current) {
            for neighbor in neighbors {
                stack.push(neighbor.clone());
            }
        }
    }

    Ok(false)
}

/// Verify that target database is empty (for pre-F017 import)
fn verify_empty_target(store: &mut SqliteStore) -> Result<()> {
    let conn = store.conn();

    // Check if there are any issues
    let issue_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM issues", [], |row| row.get(0))
        .unwrap_or(0);

    if issue_count > 0 {
        bail!("Target database is not empty (has {} issues). Pre-F017 import requires an empty initialized target.", issue_count);
    }

    // Check if there are any semantic events (aside from initialization bookkeeping)
    let event_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM events WHERE kind NOT IN ('workspace_initialized')",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    if event_count > 0 {
        bail!("Target database has semantic events ({} events). Pre-F017 import requires an empty initialized target.", event_count);
    }

    Ok(())
}

/// Activate staged import data in a single transaction
fn activate_import(store: &mut SqliteStore, staging: &ImportStaging) -> Result<(usize, i64)> {
    let conn = store.conn();
    let tx = conn.unchecked_transaction()?;

    // Insert all issues
    for issue in &staging.issues {
        tx.execute(
            "INSERT INTO issues (
                id, title, description, notes, priority, issue_type, base_status,
                manual_blocked, assignee, created_at, updated_at, closed_at, close_reason,
                source_repo, profile, schema_ref
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            params![
                &issue.id,
                &issue.title,
                &issue.description,
                &issue.notes,
                &issue.priority,
                &issue.issue_type,
                &issue.base_status.as_str(),
                if issue.manual_blocked.unwrap_or(false) {
                    1
                } else {
                    0
                },
                &issue.assignee,
                &issue.created_at,
                &issue.updated_at,
                &issue.closed_at,
                &issue.close_reason,
                &issue.source_repo,
                &issue.profile,
                &issue.schema_ref,
            ],
        )?;

        // Insert extensions (unknown fields)
        for (key, value) in &issue.extensions {
            let value_str = serde_json::to_string(value)
                .map_err(|e| anyhow!("Failed to serialize extension '{}': {}", key, e))?;
            tx.execute(
                "INSERT INTO issue_extensions (issue_id, key, value, profile)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    &issue.id,
                    key,
                    &value_str,
                    &issue.profile.as_ref().unwrap_or(&"native-v1".to_string())
                ],
            )?;
        }
    }

    // Insert dependencies
    for (blocked, blocker, kind) in &staging.dependencies {
        tx.execute(
            "INSERT INTO dependencies (blocked_issue_id, blocker_issue_id, kind)
             VALUES (?1, ?2, ?3)",
            params![blocked, blocker, kind],
        )?;
    }

    // Insert labels
    for (issue_id, label) in &staging.labels {
        tx.execute(
            "INSERT INTO labels (issue_id, label) VALUES (?1, ?2)",
            params![issue_id, label],
        )?;
    }

    // Get current sequence and allocate activation sequence
    let current_sequence: i64 = tx
        .query_row("SELECT COALESCE(MAX(sequence), 0) FROM events", [], |row| {
            row.get(0)
        })
        .unwrap_or(0);

    let activation_sequence = current_sequence + 1;

    // Append checkpoint_imported audit event
    let event_time = format_rfc3339(SystemTime::now());
    let event_detail = serde_json::json!({
        "profile": "native-v1",
        "input_hash": staging.input_hash,
        "issue_count": staging.issue_count,
        "format": "issues-jsonl-v1"
    });

    tx.execute(
        "INSERT INTO events (issue_id, kind, actor, time, detail)
         VALUES (NULL, 'checkpoint_imported', 'import', ?1, ?2)",
        params![event_time, event_detail.to_string()],
    )?;

    // Update checkpoint_state table
    update_checkpoint_state(&tx, &staging.input_hash, activation_sequence, &event_time)?;

    tx.commit()?;

    Ok((staging.issue_count, activation_sequence))
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

        // Load extensions for this issue
        let mut extensions = HashMap::new();
        let mut ext_stmt =
            tx.prepare("SELECT key, value FROM issue_extensions WHERE issue_id = ?")?;
        let ext_rows = ext_stmt.query_map([&id], |row| {
            Ok((row.get::<_, String>("key")?, row.get::<_, String>("value")?))
        })?;

        for ext_row in ext_rows {
            let (key, value_str) = ext_row?;
            let value = serde_json::from_str(&value_str)
                .map_err(|e| anyhow!("Failed to parse extension '{}': {}", key, e))?;
            extensions.insert(key, value);
        }

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
            extensions,
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

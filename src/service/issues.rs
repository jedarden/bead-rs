//! Issue service implementation
//!
//! This module provides the business logic for creating, listing, and showing issues.

use crate::error::{Error, Result};
use crate::model::{BaseStatus, Issue};
use crate::store::WorkspaceConfig;
use rand::Rng;
use rusqlite::{Connection, OptionalExtension};
use std::collections::HashMap;

/// Create a new issue
#[allow(clippy::too_many_arguments)]
pub fn create_issue(
    conn: &Connection,
    config: &WorkspaceConfig,
    title: String,
    description: Option<String>,
    priority: i64,
    issue_type: Option<String>,
    assignee: Option<String>,
    labels: Vec<String>,
) -> Result<Issue> {
    // Validate inputs
    if title.is_empty() {
        return Err(Error::validation("Title cannot be empty"));
    }

    if title.len() > 4096 {
        return Err(Error::validation("Title cannot exceed 4,096 bytes"));
    }

    if !(0..=4).contains(&priority) {
        return Err(Error::validation("Priority must be between 0 and 4"));
    }

    // Validate assignee if present
    if let Some(ref assignee) = assignee {
        if assignee.is_empty() {
            return Err(Error::validation("assignee cannot be empty"));
        }
    }

    // Validate description if present
    if let Some(ref desc) = description {
        if desc.len() > 4 * 1024 * 1024 {
            return Err(Error::validation("Description cannot exceed 4 MiB"));
        }
    }

    // Validate issue_type if present
    if let Some(ref issue_type) = issue_type {
        if issue_type.is_empty() {
            return Err(Error::validation("issue_type cannot be empty"));
        }
    }

    // Generate unique issue ID
    let id = generate_unique_id(conn, &config.prefix)?;

    // Create timestamps
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string());

    // Begin transaction
    let mut stmt = conn.prepare_cached("INSERT INTO issues (id, title, description, priority, base_status, assignee, issue_type, created_at, updated_at, revision, schema_ref) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)")?;

    let schema_ref = "urn:bead-rs:schema:issue:native-v1";
    let base_status = "open";
    let issue_type_value = issue_type.as_deref().unwrap_or("task");
    let initial_revision = 1i64;

    stmt.execute((
        &id,
        &title,
        &description,
        priority,
        base_status,
        &assignee,
        &issue_type_value,
        &now,
        &now,
        &initial_revision,
        &schema_ref,
    ))?;

    // Add labels if provided
    if !labels.is_empty() {
        let mut label_stmt =
            conn.prepare_cached("INSERT INTO labels (issue_id, label) VALUES (?1, ?2)")?;
        for label in labels {
            label_stmt.execute((&id, &label))?;
        }
    }

    // Fetch the created issue
    let issue = get_issue_by_id(conn, &id)?.ok_or_else(|| {
        Error::Internal(anyhow::anyhow!("Failed to retrieve newly created issue"))
    })?;

    Ok(issue)
}

/// List issues with optional filtering
pub fn list_issues(
    conn: &Connection,
    status_filter: Option<&str>,
    assignee_filter: Option<&str>,
    ready_only: bool,
    limit: i64,
) -> Result<Vec<Issue>> {
    // Build query based on filters
    let mut query = String::from(
        "SELECT id, title, description, priority, base_status, assignee, issue_type,
         created_at, updated_at, closed_at, close_reason, manual_blocked, source_repo,
         profile, schema_ref, notes, revision
         FROM issues WHERE 1=1",
    );

    let mut params: Vec<String> = vec![];

    // Add status filter if specified
    if let Some(status) = status_filter {
        // Validate status value
        BaseStatus::parse(status)?;
        query.push_str(" AND base_status = ?");
        params.push(status.to_string());
    }

    // Add assignee filter if specified
    if let Some(assignee) = assignee_filter {
        if assignee.is_empty() {
            query.push_str(" AND assignee IS NULL");
        } else {
            query.push_str(" AND assignee = ?");
            params.push(assignee.to_string());
        }
    }

    // Add ready frontier filter
    if ready_only {
        // A ready issue must be: open, unassigned, not manually blocked, and has no unfinished blockers
        // A blocker is unfinished unless its base state is 'closed'
        query.push_str(" AND base_status = 'open' AND (manual_blocked IS NULL OR manual_blocked = 0) AND assignee IS NULL AND NOT EXISTS (
            SELECT 1 FROM dependencies WHERE blocked_issue_id = issues.id AND kind = 'blocks'
            AND blocker_issue_id IN (SELECT id FROM issues WHERE base_status != 'closed')
        )");
    }

    // Order by priority (ASC), created_at (ASC), then id (ASC) for FIFO claim order
    query.push_str(" ORDER BY priority ASC, created_at ASC, id ASC");

    // Add limit
    query.push_str(" LIMIT ?");
    params.push(limit.to_string());

    // Execute query
    let mut stmt = conn.prepare_cached(&query)?;

    // Convert String params to &dyn ToSql
    let params_refs: Vec<&dyn rusqlite::ToSql> =
        params.iter().map(|s| s as &dyn rusqlite::ToSql).collect();

    let issue_iter = stmt
        .query_map(params_refs.as_slice(), |row| {
            Ok(Issue {
                id: row.get(0)?,
                title: row.get(1)?,
                description: row.get(2)?,
                priority: row.get(3)?,
                base_status: parse_base_status(row.get(4)?),
                assignee: row.get(5)?,
                issue_type: row.get(6)?,
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
                closed_at: row.get(9)?,
                close_reason: row.get(10)?,
                manual_blocked: row.get(11)?,
                source_repo: row.get(12)?,
                profile: row.get(13)?,
                schema_ref: row.get(14)?,
                notes: row.get(15)?,
                revision: row.get(16)?,
                data: None,                 // Will be loaded separately if needed
                extensions: HashMap::new(), // Will be loaded separately if needed
            })
        })
        .map_err(|e| Error::Internal(anyhow::anyhow!("Query failed: {}", e)))?;

    let mut issues = Vec::new();
    for issue_result in issue_iter {
        issues
            .push(issue_result.map_err(|e| Error::Internal(anyhow::anyhow!("Row error: {}", e)))?);
    }

    Ok(issues)
}

/// Get a single issue by ID
pub fn get_issue_by_id(conn: &Connection, id: &str) -> Result<Option<Issue>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, title, description, priority, base_status, assignee, issue_type,
         created_at, updated_at, closed_at, close_reason, manual_blocked, source_repo,
         profile, schema_ref, notes, revision
         FROM issues WHERE id = ?",
    )?;

    let issue = stmt
        .query_row([id], |row| {
            Ok(Issue {
                id: row.get(0)?,
                title: row.get(1)?,
                description: row.get(2)?,
                priority: row.get(3)?,
                base_status: parse_base_status(row.get(4)?),
                assignee: row.get(5)?,
                issue_type: row.get(6)?,
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
                closed_at: row.get(9)?,
                close_reason: row.get(10)?,
                manual_blocked: row.get(11)?,
                source_repo: row.get(12)?,
                profile: row.get(13)?,
                schema_ref: row.get(14)?,
                notes: row.get(15)?,
                revision: row.get(16)?,
                data: None,
                extensions: HashMap::new(),
            })
        })
        .optional()?;

    Ok(issue)
}

/// Generate a unique issue ID
fn generate_unique_id(conn: &Connection, prefix: &str) -> Result<String> {
    let max_attempts = 5;

    for _attempt in 0..max_attempts {
        // Generate random suffix
        let mut rng = rand::thread_rng();
        let bytes: [u8; 8] = rng.gen();
        let suffix = hex::encode(bytes);

        let id = format!("{}-{}", prefix, suffix);

        // Check if ID already exists
        let exists: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM issues WHERE id = ?",
                [&id],
                |row| row.get(0),
            )
            .unwrap_or(false);

        if !exists {
            return Ok(id);
        }
    }

    Err(Error::Internal(anyhow::anyhow!(
        "Failed to generate unique ID after {} attempts",
        max_attempts
    )))
}

/// Parse base status from database string
fn parse_base_status(s: String) -> BaseStatus {
    match s.to_lowercase().as_str() {
        "open" => BaseStatus::Open,
        "in_progress" => BaseStatus::InProgress,
        "deferred" => BaseStatus::Deferred,
        "closed" => BaseStatus::Closed,
        _ => BaseStatus::Open, // Default to open for now
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_base_status() {
        assert_eq!(parse_base_status("open".to_string()), BaseStatus::Open);
        assert_eq!(
            parse_base_status("in_progress".to_string()),
            BaseStatus::InProgress
        );
        assert_eq!(
            parse_base_status("deferred".to_string()),
            BaseStatus::Deferred
        );
        assert_eq!(parse_base_status("closed".to_string()), BaseStatus::Closed);
    }
}

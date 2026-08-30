//! Issue service implementation
//!
//! This module provides the business logic for creating, listing, and showing issues.

use crate::error::{Error, Result};
use crate::model::{validate_reference_key, validate_reference_namespace, BaseStatus, Issue};
use crate::service::resource_locks::{declare_resource_keys, get_resource_keys};
use crate::store::WorkspaceConfig;
use rand::Rng;
use rusqlite::{Connection, OptionalExtension};
use std::collections::HashMap;

/// R011 key used for the external-reference projection of an R032 binding.
pub const UNIQUE_REF_EXTERNAL_KEY: &str = "unique-ref";

/// Result classification for an idempotent create operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateOutcome {
    /// A new issue and its reference binding were committed.
    Created,
    /// The reference was already bound to an existing issue.
    Existing { closed: bool },
}

/// Issue plus the result classification needed by the CLI output contract.
#[derive(Debug, Clone)]
pub struct CreateIssueResult {
    pub issue: Issue,
    pub outcome: CreateOutcome,
}

/// Create a new issue
#[allow(clippy::too_many_arguments)]
#[allow(dead_code)]
pub fn create_issue(
    conn: &Connection,
    config: &WorkspaceConfig,
    title: String,
    description: Option<String>,
    priority: i64,
    issue_type: Option<String>,
    assignee: Option<String>,
    labels: Vec<String>,
    resource_keys: Vec<String>,
) -> Result<Issue> {
    create_issue_with_unique_ref(
        conn,
        config,
        title,
        description,
        priority,
        issue_type,
        assignee,
        labels,
        resource_keys,
        None,
    )
    .map(|result| result.issue)
}

/// Create an issue and, when requested, bind an R032 unique reference in the
/// same caller-owned transaction as the issue insert.
#[allow(clippy::too_many_arguments)]
pub fn create_issue_with_unique_ref(
    conn: &Connection,
    config: &WorkspaceConfig,
    title: String,
    description: Option<String>,
    priority: i64,
    issue_type: Option<String>,
    assignee: Option<String>,
    labels: Vec<String>,
    resource_keys: Vec<String>,
    unique_ref: Option<&str>,
) -> Result<CreateIssueResult> {
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

    let unique_ref = unique_ref.map(parse_unique_ref).transpose()?;

    // The caller normally holds an IMMEDIATE transaction. This lookup makes
    // the common ref-hit path read-only; the unique binding primary key below
    // remains the authoritative race guard for callers using a deferred
    // transaction.
    if let Some((namespace, key)) = unique_ref.as_ref() {
        let existing_id: Option<String> = conn
            .query_row(
                "SELECT issue_id FROM unique_reference_bindings
                 WHERE namespace = ?1 AND key = ?2",
                [namespace, key],
                |row| row.get(0),
            )
            .optional()?;

        if let Some(existing_id) = existing_id {
            return existing_create_result(conn, &existing_id);
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

    if let Some((namespace, key)) = unique_ref.as_ref() {
        // INSERT OR IGNORE lets a deferred caller lose the binding race
        // without leaking the issue it tentatively inserted. The command
        // path uses an IMMEDIATE transaction, so this is also the exact
        // transaction boundary that serializes concurrent creates.
        let binding_inserted = conn.execute(
            "INSERT OR IGNORE INTO unique_reference_bindings
             (namespace, key, issue_id) VALUES (?1, ?2, ?3)",
            (namespace, key, &id),
        )?;

        if binding_inserted == 0 {
            conn.execute("DELETE FROM issues WHERE id = ?1", [&id])?;
            let existing_id: String = conn.query_row(
                "SELECT issue_id FROM unique_reference_bindings
                 WHERE namespace = ?1 AND key = ?2",
                [namespace, key],
                |row| row.get(0),
            )?;
            return existing_create_result(conn, &existing_id);
        }

        // R011 represents this shorthand as namespace / unique-ref / value.
        // The dedicated binding table above supplies the namespace/key
        // uniqueness that ordinary R011 rows intentionally do not have.
        conn.execute(
            "INSERT INTO external_references (issue_id, namespace, key, value)
             VALUES (?1, ?2, ?3, ?4)",
            (&id, namespace, UNIQUE_REF_EXTERNAL_KEY, key),
        )?;
    }

    // Record the creation as an audit event on the caller's connection, so it
    // commits (or rolls back) in the same transaction as the insert above. The
    // live event sequence is the dirtiness signal (plan 6.2.1 P3), so an
    // unrecorded creation would silently read as no change.
    append_created_event(conn, &id, &title, priority, issue_type_value, None, &now)?;

    // Add labels if provided
    if !labels.is_empty() {
        let mut label_stmt =
            conn.prepare_cached("INSERT INTO labels (issue_id, label) VALUES (?1, ?2)")?;
        for label in labels {
            label_stmt.execute((&id, &label))?;
        }
    }

    declare_resource_keys(conn, &id, &resource_keys)?;

    // Fetch the created issue
    let issue = get_issue_by_id(conn, &id)?.ok_or_else(|| {
        Error::Internal(anyhow::anyhow!("Failed to retrieve newly created issue"))
    })?;

    Ok(CreateIssueResult {
        issue,
        outcome: CreateOutcome::Created,
    })
}

/// Parse and validate the public `NAMESPACE:KEY` spelling for R032.
fn parse_unique_ref(value: &str) -> Result<(String, String)> {
    let (namespace, key) = value
        .split_once(':')
        .ok_or_else(|| Error::validation("unique-ref must use NAMESPACE:KEY form"))?;
    validate_reference_namespace(namespace).map_err(|e| Error::validation(e.to_string()))?;
    validate_reference_key(key).map_err(|e| Error::validation(e.to_string()))?;
    Ok((namespace.to_string(), key.to_string()))
}

fn existing_create_result(conn: &Connection, issue_id: &str) -> Result<CreateIssueResult> {
    let issue = get_issue_by_id(conn, issue_id)?.ok_or_else(|| {
        Error::Integrity(format!(
            "Unique reference binding points to missing issue {}",
            issue_id
        ))
    })?;
    let closed = issue.base_status.is_closed();
    Ok(CreateIssueResult {
        issue,
        outcome: CreateOutcome::Existing { closed },
    })
}

/// Append the audit event for a newly created issue
///
/// Both create paths (public and recurrence-internal) emit this event through
/// the caller's connection, inside the caller's transaction. A create that
/// fails or rolls back therefore leaves no event row behind.
fn append_created_event(
    conn: &Connection,
    id: &str,
    title: &str,
    priority: i64,
    issue_type: &str,
    actor: Option<&str>,
    now: &str,
) -> Result<()> {
    let actor_value = actor.unwrap_or("system");

    let event_detail = serde_json::json!({
        "actor": actor_value,
        "issue_id": id,
        "title": title,
        "priority": priority,
        "issue_type": issue_type,
    });

    let mut event_stmt = conn.prepare_cached(
        "INSERT INTO events (issue_id, kind, actor, time, detail) VALUES (?1, ?2, ?3, ?4, ?5)",
    )?;

    event_stmt.execute((id, "created", actor_value, now, &event_detail.to_string()))?;

    Ok(())
}

/// Detect and recover from starvation by clearing stale assignees
///
/// Returns Some(list of recovered bead IDs) if fallback was triggered, None otherwise.
fn detect_and_recover_starvation(conn: &Connection) -> Result<Option<Vec<String>>> {
    // Check if any open beads exist at all
    let open_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM issues WHERE base_status = 'open'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    if open_count == 0 {
        // No open beads exist - this is not starvation, just an empty workspace
        return Ok(None);
    }

    // Find open beads with assignees (potential starvation candidates)
    let mut stmt = conn.prepare_cached(
        "SELECT id, assignee FROM issues
         WHERE base_status = 'open' AND assignee IS NOT NULL
         AND (manual_blocked IS NULL OR manual_blocked = 0)
         ORDER BY priority ASC, created_at ASC, id ASC",
    )?;

    let fallback_candidates: Vec<(String, Option<String>)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| {
            Error::Internal(anyhow::anyhow!(
                "Failed to fetch fallback candidates: {}",
                e
            ))
        })?;

    if fallback_candidates.is_empty() {
        // No open beads with assignees - not a starvation situation
        return Ok(None);
    }

    // Clear the assignees for all fallback candidates
    let mut recovered_ids = Vec::new();
    for (bead_id, old_assignee) in fallback_candidates {
        let now = time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_else(|_| "unknown".to_string());

        conn.execute(
            "UPDATE issues SET assignee = NULL, updated_at = ?1, revision = revision + 1 WHERE id = ?2",
            [&now, &bead_id],
        )
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to clear assignee for {}: {}", bead_id, e)))?;

        // Record the update as an audit event
        let event_detail = serde_json::json!({
            "actor": "system",
            "field": "assignee",
            "old_value": old_assignee,
            "new_value": null,
            "reason": "fallback_query_starvation_recovery"
        });

        conn.execute(
            "INSERT INTO events (issue_id, kind, actor, time, detail) VALUES (?1, ?2, ?3, ?4, ?5)",
            (
                &bead_id,
                "updated",
                "system",
                &now,
                &event_detail.to_string(),
            ),
        )
        .map_err(|e| {
            Error::Internal(anyhow::anyhow!(
                "Failed to record fallback event for {}: {}",
                bead_id,
                e
            ))
        })?;

        recovered_ids.push(bead_id);
    }

    Ok(Some(recovered_ids))
}

/// Log fallback activation to diagnostics file
#[allow(dead_code)]
fn log_fallback_activation(bead_ids: &[String]) -> Result<()> {
    use std::fs::OpenOptions;
    use std::io::Write;

    let diagnostics_dir = ".beads/diagnostics";
    let log_path = format!("{}/pluck-fallback.log", diagnostics_dir);

    // Create diagnostics directory if it doesn't exist
    std::fs::create_dir_all(diagnostics_dir).map_err(|e| {
        Error::Internal(anyhow::anyhow!(
            "Failed to create diagnostics directory: {}",
            e
        ))
    })?;

    // Append to log file
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to open fallback log: {}", e)))?;

    let timestamp = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string());

    let log_entry = format!(
        "{} | Fallback triggered | Recovered beads: {}\n",
        timestamp,
        bead_ids.join(", ")
    );

    file.write_all(log_entry.as_bytes())
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to write to fallback log: {}", e)))?;

    Ok(())
}

/// Log fallback activation to a specific path
fn log_fallback_activation_to_path(bead_ids: &[String], log_path: &std::path::Path) -> Result<()> {
    use std::fs::OpenOptions;
    use std::io::Write;

    // Create parent directory if it doesn't exist
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            Error::Internal(anyhow::anyhow!(
                "Failed to create diagnostics directory: {}",
                e
            ))
        })?;
    }

    // Append to log file
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to open fallback log: {}", e)))?;

    let timestamp = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string());

    let log_entry = format!(
        "{} | Fallback triggered | Recovered beads: {}\n",
        timestamp,
        bead_ids.join(", ")
    );

    file.write_all(log_entry.as_bytes())
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to write to fallback log: {}", e)))?;

    Ok(())
}

/// Internal function to create an issue with a specific ID (used by recurrence service)
#[allow(clippy::too_many_arguments)]
pub fn create_issue_internal(
    conn: &Connection,
    id: &str,
    title: &str,
    description: Option<&str>,
    priority: i64,
    issue_type: &str,
    labels: &[String],
    actor: Option<&str>,
) -> Result<String> {
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

    // Validate issue_type
    if issue_type.is_empty() {
        return Err(Error::validation("issue_type cannot be empty"));
    }

    // Validate labels
    for label in labels {
        crate::model::validate_label(label)?;
    }

    // Create timestamps
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string());

    // Begin transaction
    let mut stmt = conn.prepare_cached("INSERT INTO issues (id, title, description, priority, base_status, issue_type, created_at, updated_at, revision, schema_ref) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)")?;

    let schema_ref = "urn:bead-rs:schema:issue:native-v1";
    let base_status = "open";
    let initial_revision = 1i64;

    stmt.execute((
        id,
        title,
        description.unwrap_or(""),
        priority,
        base_status,
        issue_type,
        &now,
        &now,
        initial_revision,
        schema_ref,
    ))?;

    // Add labels if provided
    if !labels.is_empty() {
        let mut label_stmt =
            conn.prepare_cached("INSERT INTO labels (issue_id, label) VALUES (?1, ?2)")?;
        for label in labels {
            label_stmt.execute((id, label))?;
        }
    }

    // Create audit event for the issue creation
    append_created_event(conn, id, title, priority, issue_type, actor, &now)?;

    Ok(id.to_string())
}

/// Show exclusion reasons for beads not in the ready frontier
fn show_exclusion_reasons(conn: &Connection, limit: &i64) -> Result<()> {
    let now_string = crate::service::resource_locks::now_string();

    // Get all open beads with their details
    let mut stmt = conn.prepare_cached(
        "SELECT id, title, priority, assignee, manual_blocked, created_at
         FROM issues
         WHERE base_status = 'open'
         ORDER BY priority ASC, created_at ASC, id ASC
         LIMIT ?1")?;

    let beads: Vec<(String, String, i64, Option<String>, Option<bool>, String)> = stmt
        .query_map([limit], |row| {
            Ok((
                row.get(0)?, // id
                row.get(1)?, // title
                row.get(2)?, // priority
                row.get(3)?, // assignee
                row.get(4)?, // manual_blocked
                row.get(5)?, // created_at
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to fetch beads: {}", e)))?;

    if beads.is_empty() {
        eprintln!("No open beads found in workspace");
        return Ok(());
    }

    eprintln!("Analyzing {} open bead(s):", beads.len());

    for (id, title, _priority, assignee, manual_blocked, _created_at) in beads {
        let mut reasons = Vec::new();

        // Check assignee
        if assignee.is_some() {
            reasons.push(format!("has assignee: {}", assignee.as_ref().unwrap()));
        }

        // Check manual block
        if manual_blocked.unwrap_or(false) {
            reasons.push("manually blocked".to_string());
        }

        // Check dependencies
        let has_blockers: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM dependencies
                 WHERE blocked_issue_id = ?1 AND kind = 'blocks'
                 AND blocker_issue_id IN (SELECT id FROM issues WHERE base_status != 'closed')",
                [&id],
                |row| row.get(0),
            )
            .unwrap_or(0);

        if has_blockers > 0 {
            // Get blocker IDs
            let mut blocker_stmt = conn.prepare_cached(
                "SELECT blocker_issue_id FROM dependencies
                 WHERE blocked_issue_id = ?1 AND kind = 'blocks'
                 AND blocker_issue_id IN (SELECT id FROM issues WHERE base_status != 'closed')
                 LIMIT 5")?;

            let blocker_ids: Vec<String> = blocker_stmt
                .query_map([&id], |row| row.get(0))?
                .collect::<std::result::Result<Vec<_>, _>>()
                .unwrap_or_default();

            if blocker_ids.len() < has_blockers as usize {
                reasons.push(format!(
                    "blocked by {}+ unclosed issues (including: {})",
                    has_blockers,
                    blocker_ids.join(", ")
                ));
            } else {
                reasons.push(format!(
                    "blocked by {} unclosed issue(s): {}",
                    has_blockers,
                    blocker_ids.join(", ")
                ));
            }
        }

        // Check resource conflicts
        let has_conflicts: i64 = conn
            .query_row(
                "SELECT COUNT(DISTINCT held_lock.issue_id)
                 FROM issue_resource_keys candidate_key
                 JOIN resource_locks held_lock ON held_lock.resource_key = candidate_key.resource_key
                 WHERE candidate_key.issue_id = ?1
                   AND held_lock.issue_id != ?1
                   AND (held_lock.lease_fencing_token IS NULL OR EXISTS (
                       SELECT 1 FROM leases active_lease
                       WHERE active_lease.issue_id = held_lock.issue_id
                         AND active_lease.fencing_token = held_lock.lease_fencing_token
                         AND active_lease.expires_at > ?2
                   ))",
                [&id, &now_string],
                |row| row.get(0),
            )
            .unwrap_or(0);

        if has_conflicts > 0 {
            reasons.push(format!("resource conflicts with {} other issue(s)", has_conflicts));
        }

        // Print result
        if reasons.is_empty() {
            eprintln!("  ✓ {} [{}] - READY", id, title);
        } else {
            eprintln!("  ✗ {} [{}] - EXCLUDED:", id, title);
            for reason in reasons {
                eprintln!("      - {}", reason);
            }
        }
    }

    Ok(())
}

/// List issues with optional filtering
pub fn list_issues(
    conn: &Connection,
    status_filter: Option<&str>,
    assignee_filter: Option<&str>,
    ready_only: bool,
    blocked_only: bool,
    limit: i64,
    verbose: bool,
) -> Result<Vec<Issue>> {
    // Log total open beads if verbose
    if verbose {
        let total_open: i64 = conn
            .query_row("SELECT COUNT(*) FROM issues WHERE base_status = 'open'", [], |row| {
                row.get(0)
            })
            .unwrap_or(0);

        let total_with_assignee: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM issues WHERE base_status = 'open' AND assignee IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let total_blocked: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM issues WHERE base_status = 'open' AND (manual_blocked = 1)",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let total_with_dependencies: i64 = conn
            .query_row(
                "SELECT COUNT(DISTINCT blocked_issue_id) FROM dependencies WHERE kind = 'blocks'
                 AND blocked_issue_id IN (SELECT id FROM issues WHERE base_status = 'open')",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        eprintln!("=== VERBOSE: Ready Frontier Diagnostics ===");
        eprintln!("Total open beads: {}", total_open);
        eprintln!("Open beads with assignee: {}", total_with_assignee);
        eprintln!("Open beads manually blocked: {}", total_blocked);
        eprintln!("Open beads with dependencies: {}", total_with_dependencies);
    }

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
        // Special case: "blocked" is an alias for manually blocked open issues
        if status == "blocked" {
            // This will be handled by the blocked_only filter below
            // Don't add it to the base_status filter
        } else {
            // Validate status value
            BaseStatus::parse(status)?;
            query.push_str(" AND base_status = ?");
            params.push(status.to_string());
        }
    }

    // Add blocked filter
    if blocked_only {
        query.push_str(" AND base_status = 'open' AND (manual_blocked = 1)");
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
        ) AND NOT EXISTS (
            SELECT 1 FROM issue_resource_keys candidate_key
            JOIN resource_locks held_lock ON held_lock.resource_key = candidate_key.resource_key
            WHERE candidate_key.issue_id = issues.id
              AND held_lock.issue_id != issues.id
              AND (held_lock.lease_fencing_token IS NULL OR EXISTS (
                  SELECT 1 FROM leases active_lease
                  WHERE active_lease.issue_id = held_lock.issue_id
                    AND active_lease.fencing_token = held_lock.lease_fencing_token
                    AND active_lease.expires_at > ?
              ))
        )");
        params.push(crate::service::resource_locks::now_string());
    }

    // Order by priority (ASC), created_at (ASC), then id (ASC) for FIFO claim order
    query.push_str(" ORDER BY priority ASC, created_at ASC, id ASC");

    // Add limit
    query.push_str(" LIMIT ?");
    params.push(limit.to_string());

    // Log filter criteria and SQL query if verbose
    if verbose {
        eprintln!("=== Filter Criteria ===");
        if let Some(status) = status_filter {
            eprintln!("Status filter: {}", status);
        } else {
            eprintln!("Status filter: None");
        }
        if let Some(assignee) = assignee_filter {
            if assignee.is_empty() {
                eprintln!("Assignee filter: NULL (unassigned)");
            } else {
                eprintln!("Assignee filter: {}", assignee);
            }
        } else {
            eprintln!("Assignee filter: None");
        }
        eprintln!("Ready only: {}", ready_only);
        eprintln!("Blocked only: {}", blocked_only);
        eprintln!("Limit: {}", limit);

        eprintln!("=== SQL Query ===");
        eprintln!("{}", query);
        eprintln!("Parameters: {:?}", params);
    }

    // For ready frontier queries, check for starvation before executing
    if ready_only {
        // Build a simple count query for the ready frontier
        let count_query = String::from("SELECT COUNT(*) FROM issues WHERE base_status = 'open' AND (manual_blocked IS NULL OR manual_blocked = 0) AND assignee IS NULL AND NOT EXISTS (
            SELECT 1 FROM dependencies WHERE blocked_issue_id = issues.id AND kind = 'blocks'
            AND blocker_issue_id IN (SELECT id FROM issues WHERE base_status != 'closed')
        ) AND NOT EXISTS (
            SELECT 1 FROM issue_resource_keys candidate_key
            JOIN resource_locks held_lock ON held_lock.resource_key = candidate_key.resource_key
            WHERE candidate_key.issue_id = issues.id
              AND held_lock.issue_id != issues.id
              AND (held_lock.lease_fencing_token IS NULL OR EXISTS (
                  SELECT 1 FROM leases active_lease
                  WHERE active_lease.issue_id = held_lock.issue_id
                    AND active_lease.fencing_token = held_lock.lease_fencing_token
                    AND active_lease.expires_at > ?
              ))
        )");

        let now_string = crate::service::resource_locks::now_string();
        let mut count_stmt = conn.prepare_cached(&count_query)?;
        let count_params_refs: Vec<&dyn rusqlite::ToSql> =
            vec![&now_string as &dyn rusqlite::ToSql];

        let primary_count: i64 = count_stmt
            .query_row(count_params_refs.as_slice(), |row| row.get(0))
            .unwrap_or(0);

        // If primary query returns empty, check for starvation and run fallback
        if primary_count == 0 {
            if let Some(fallback_beads) = detect_and_recover_starvation(conn)? {
                // Get database path for log file resolution
                let db_path = conn
                    .query_row("PRAGMA database_list", [], |row| {
                        let name: String = row.get(1)?;
                        let path: String = row.get(2)?;
                        if name == "main" && !path.is_empty() {
                            Ok(path)
                        } else {
                            Err(rusqlite::Error::InvalidQuery)
                        }
                    })
                    .optional()
                    .unwrap_or_default();

                if let Some(db_path_str) = db_path {
                    let db_path = std::path::Path::new(&db_path_str);
                    if let Some(workspace_dir) = db_path.parent() {
                        let log_path = workspace_dir.join("diagnostics/pluck-fallback.log");
                        log_fallback_activation_to_path(&fallback_beads, &log_path)?;
                    }
                }
            }
        }
    }

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
        let mut issue =
            issue_result.map_err(|e| Error::Internal(anyhow::anyhow!("Row error: {}", e)))?;
        let keys = get_resource_keys(conn, &issue.id)?;
        if !keys.is_empty() {
            issue.extensions.insert(
                crate::service::resource_locks::RESOURCE_KEYS_EXTENSION.to_string(),
                serde_json::json!(keys),
            );
        }
        issues.push(issue);
    }

    // Log result set size if verbose
    if verbose {
        eprintln!("=== Query Results ===");
        eprintln!("Result set size: {}", issues.len());
        if ready_only && issues.is_empty() {
            eprintln!("WARNING: No ready beads found!");
            eprintln!("This may indicate starvation - check diagnostics/pluck-fallback.log");
        }
    }

    // When verbose and ready-only, show exclusion reasons for each bead
    if verbose && ready_only {
        eprintln!("=== Bead Exclusion Analysis ===");
        show_exclusion_reasons(conn, &limit)?;
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

    issue
        .map(|mut issue| {
            let keys = get_resource_keys(conn, &issue.id)?;
            if !keys.is_empty() {
                issue.extensions.insert(
                    crate::service::resource_locks::RESOURCE_KEYS_EXTENSION.to_string(),
                    serde_json::json!(keys),
                );
            }
            Ok(issue)
        })
        .transpose()
}

/// Generate a unique issue ID
fn generate_unique_id(conn: &Connection, prefix: &str) -> Result<String> {
    let max_attempts = 5;

    for _attempt in 0..max_attempts {
        // Generate random suffix. 4 bytes (8 hex chars) keeps ids short and
        // readable; collisions are checked below and retried, so this is
        // not relying on the birthday bound alone.
        let mut rng = rand::thread_rng();
        let bytes: [u8; 4] = rng.r#gen();
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

//! Dry-run service implementation
//!
//! This module provides dry-run functionality for mutation operations.
//! Dry-run operations perform authorization, validation, cycle analysis,
//! and derived-status calculation without committing rows, events, revisions,
//! or checkpoint metadata.

use crate::error::{Error, Result};
use crate::model::{BaseStatus, Issue};
use crate::service::dependencies::would_create_cycle;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// Dry-run result showing before/after semantic delta
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DryRunResult {
    /// Issue ID
    pub id: String,
    /// Current revision
    pub current_revision: i64,
    /// Current workspace event sequence
    pub workspace_sequence: i64,
    /// Before state (current issue state)
    pub before: IssueDryRunState,
    /// After state (projected issue state after mutation)
    pub after: IssueDryRunState,
    /// Whether this would be a semantic change vs idempotent
    pub semantic_change: bool,
    /// Advisory message explaining what would happen
    pub message: String,
}

/// Simplified issue state for dry-run output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueDryRunState {
    pub id: String,
    pub title: String,
    pub base_status: String,
    pub priority: i64,
    pub assignee: Option<String>,
    pub manual_blocked: Option<bool>,
    pub closed_at: Option<String>,
    pub updated_at: String,
}

/// Convert Issue to dry-run state format
fn issue_to_dryrun_state(issue: &Issue) -> IssueDryRunState {
    IssueDryRunState {
        id: issue.id.clone(),
        title: issue.title.clone(),
        base_status: issue.base_status.clone().to_string(),
        priority: issue.priority,
        assignee: issue.assignee.clone(),
        manual_blocked: issue.manual_blocked,
        closed_at: issue.closed_at.clone(),
        updated_at: issue.updated_at.clone(),
    }
}

/// Get current workspace event sequence
fn get_workspace_sequence(conn: &Connection) -> Result<i64> {
    conn.query_row("SELECT COALESCE(MAX(sequence), 0) FROM events", [], |row| {
        row.get(0)
    })
    .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to get workspace sequence: {}", e)))
}

/// Dry-run for update operation
pub fn update_issue_dryrun(
    conn: &Connection,
    id: &str,
    status: Option<&str>,
    assignee: Option<&str>,
    clear_assignee: bool,
    notes: Option<&str>,
) -> Result<DryRunResult> {
    // Get current issue state
    let issue =
        crate::service::issues::get_issue_by_id(conn, id)?.ok_or_else(|| Error::not_found(id))?;

    let before = issue_to_dryrun_state(&issue);
    let current_revision = issue.revision.unwrap_or(1);
    let workspace_sequence = get_workspace_sequence(conn)?;

    // Create projected after state by cloning current and applying changes
    let mut after_issue = issue.clone();
    let mut changes = Vec::new();

    // Apply status change
    if let Some(new_status) = status {
        let new_base_status = BaseStatus::parse(new_status)?;
        if new_base_status != issue.base_status {
            // Validate transition
            crate::model::validate_status_transition(issue.base_status, new_base_status)?;
            after_issue.base_status = new_base_status;
            changes.push(format!("status: {} -> {}", issue.base_status, new_status));
        }
    }

    // Apply assignee changes
    if assignee.is_some() && clear_assignee {
        return Err(Error::validation(
            "Cannot specify both --assignee and --clear-assignee",
        ));
    }

    if let Some(new_assignee) = assignee {
        if new_assignee.is_empty() {
            return Err(Error::validation("assignee cannot be empty"));
        }
        if after_issue.assignee.as_deref() != Some(new_assignee) {
            after_issue.assignee = Some(new_assignee.to_string());
            changes.push(format!(
                "assignee: {} -> {}",
                issue.assignee.as_deref().unwrap_or("none"),
                new_assignee
            ));
        }
    }

    if clear_assignee && after_issue.assignee.is_some() {
        after_issue.assignee = None;
        changes.push(format!(
            "assignee: {} -> none",
            issue.assignee.as_deref().unwrap_or("none")
        ));
    }

    // Apply notes change
    if let Some(new_notes) = notes {
        if new_notes.len() > 4 * 1024 * 1024 {
            return Err(Error::validation("Notes cannot exceed 4 MiB"));
        }
        let notes_changed = match (&issue.notes, Some(new_notes)) {
            (Some(old), Some(new)) => old != new,
            (None, Some(_)) => true,
            (Some(_), None) => true,
            (None, None) => false,
        };
        if notes_changed {
            after_issue.notes = Some(new_notes.to_string());
            changes.push("notes updated".to_string());
        }
    }

    let after = issue_to_dryrun_state(&after_issue);
    let semantic_change = !changes.is_empty();

    let message = if semantic_change {
        format!("Would update: {}", changes.join(", "))
    } else {
        "No changes would be made (idempotent)".to_string()
    };

    Ok(DryRunResult {
        id: id.to_string(),
        current_revision,
        workspace_sequence,
        before,
        after,
        semantic_change,
        message,
    })
}

/// Dry-run for close operation
pub fn close_issue_dryrun(conn: &Connection, id: &str, reason: &str) -> Result<DryRunResult> {
    // Validate reason
    if reason.trim().is_empty() {
        return Err(Error::validation("Close reason cannot be empty"));
    }

    // Get current issue state
    let issue =
        crate::service::issues::get_issue_by_id(conn, id)?.ok_or_else(|| Error::not_found(id))?;

    let before = issue_to_dryrun_state(&issue);
    let current_revision = issue.revision.unwrap_or(1);
    let workspace_sequence = get_workspace_sequence(conn)?;

    // Create projected after state
    let mut after_issue = issue.clone();

    let semantic_change =
        if issue.base_status != BaseStatus::Closed {
            // Can close from open, in_progress, or deferred
            after_issue.base_status = BaseStatus::Closed;
            after_issue.manual_blocked = Some(false);
            after_issue.closed_at = Some(chrono::Utc::now().to_rfc3339());
            true
        } else {
            // Already closed - check if reason matches for idempotency
            match &issue.close_reason {
                Some(existing_reason) if existing_reason.trim() == reason.trim() => false,
                _ => {
                    return Err(Error::conflict(format!(
                    "Issue is already closed with reason: '{}'. Use update to change the reason.",
                    issue.close_reason.unwrap_or_else(|| "(unknown)".to_string())
                )));
                }
            }
        };

    let after = issue_to_dryrun_state(&after_issue);
    let message = if semantic_change {
        format!("Would close issue with reason: '{}'", reason.trim())
    } else {
        "Issue already closed with same reason (idempotent)".to_string()
    };

    Ok(DryRunResult {
        id: id.to_string(),
        current_revision,
        workspace_sequence,
        before,
        after,
        semantic_change,
        message,
    })
}

/// Dry-run for reopen operation
pub fn reopen_issue_dryrun(conn: &Connection, id: &str) -> Result<DryRunResult> {
    // Get current issue state
    let issue =
        crate::service::issues::get_issue_by_id(conn, id)?.ok_or_else(|| Error::not_found(id))?;

    let before = issue_to_dryrun_state(&issue);
    let current_revision = issue.revision.unwrap_or(1);
    let workspace_sequence = get_workspace_sequence(conn)?;

    // Create projected after state
    let mut after_issue = issue.clone();

    let semantic_change = if issue.base_status == BaseStatus::Closed {
        after_issue.base_status = BaseStatus::Open;
        after_issue.closed_at = None;
        after_issue.close_reason = None;
        after_issue.manual_blocked = Some(false);
        true
    } else if issue.base_status == BaseStatus::Open {
        // Already open - idempotent
        false
    } else {
        return Err(Error::conflict(format!(
            "Cannot reopen issue from {} status (only closed issues can be reopened)",
            issue.base_status
        )));
    };

    let after = issue_to_dryrun_state(&after_issue);
    let message = if semantic_change {
        "Would reopen issue to open status".to_string()
    } else {
        "Issue already open (idempotent)".to_string()
    };

    Ok(DryRunResult {
        id: id.to_string(),
        current_revision,
        workspace_sequence,
        before,
        after,
        semantic_change,
        message,
    })
}

/// Dry-run for release operation
pub fn release_issue_dryrun(conn: &Connection, id: &str) -> Result<DryRunResult> {
    // Get current issue state
    let issue =
        crate::service::issues::get_issue_by_id(conn, id)?.ok_or_else(|| Error::not_found(id))?;

    let before = issue_to_dryrun_state(&issue);
    let current_revision = issue.revision.unwrap_or(1);
    let workspace_sequence = get_workspace_sequence(conn)?;

    // Create projected after state
    let mut after_issue = issue.clone();

    let semantic_change = if issue.base_status == BaseStatus::InProgress {
        after_issue.base_status = BaseStatus::Open;
        after_issue.assignee = None;
        true
    } else if issue.base_status == BaseStatus::Open && issue.assignee.is_none() {
        // Already open and unassigned - idempotent
        false
    } else if issue.base_status == BaseStatus::Open {
        return Err(Error::conflict(
            "Cannot release open assigned issue (use 'update --clear-assignee' instead)"
                .to_string(),
        ));
    } else {
        return Err(Error::conflict(format!(
            "Cannot release issue from {} status (only in_progress issues can be released)",
            issue.base_status
        )));
    };

    let after = issue_to_dryrun_state(&after_issue);
    let message = if semantic_change {
        format!(
            "Would release issue from {} to open/unassigned",
            before.base_status
        )
    } else {
        "Issue already open and unassigned (idempotent)".to_string()
    };

    Ok(DryRunResult {
        id: id.to_string(),
        current_revision,
        workspace_sequence,
        before,
        after,
        semantic_change,
        message,
    })
}

/// Dry-run for add dependency operation
pub fn add_dependency_dryrun(
    conn: &Connection,
    blocked: &str,
    blocker: &str,
    kind: &str,
    condition: Option<&str>,
) -> Result<DependencyDryRunResult> {
    // Validate dependency kind
    if kind != "blocks" && kind != "relates_to" {
        return Err(Error::validation(format!(
            "Invalid dependency kind '{}': must be 'blocks' or 'relates_to'",
            kind
        )));
    }

    // Check if both issues exist
    let _blocked_issue = crate::service::issues::get_issue_by_id(conn, blocked)?
        .ok_or_else(|| Error::not_found(format!("blocked issue '{}'", blocked)))?;
    let _blocker_issue = crate::service::issues::get_issue_by_id(conn, blocker)?
        .ok_or_else(|| Error::not_found(format!("blocker issue '{}'", blocker)))?;

    // Check for self-edge
    if blocked == blocker {
        return Err(Error::validation("Cannot create self-edge dependency"));
    }

    // Parse condition if provided
    let condition_expr = if let Some(cond_json) = condition {
        Some(crate::service::conditions::ConditionExpr::from_json(
            cond_json,
        )?)
    } else {
        None
    };

    // Validate condition fields if present
    if let Some(ref cond) = condition_expr {
        cond.validate_fields()?;
    }

    // Check for cycles if this is a blocks dependency
    let current_sequence = get_workspace_sequence(conn)?;
    let would_create_cycle_result = if kind == "blocks" {
        // Simulate the dependency addition and check for cycles
        would_create_cycle(conn, blocked, blocker)?
    } else {
        false
    };

    if would_create_cycle_result {
        return Err(Error::conflict(
            "Adding this dependency would create a cycle",
        ));
    }

    // Check if dependency already exists
    let existing_dep = conn.query_row(
        "SELECT kind FROM dependencies WHERE blocked_issue_id = ?1 AND blocker_issue_id = ?2 AND kind = ?3",
        [blocked, blocker, kind],
        |row| row.get::<_, String>(0),
    );

    let dependency_exists = existing_dep.is_ok();
    let semantic_change = !dependency_exists; // New dependency = semantic change
    let message = if dependency_exists {
        format!(
            "Dependency ({}, {}, {}) already exists (idempotent)",
            blocked, blocker, kind
        )
    } else {
        format!(
            "Would add dependency: {} -> {} ({})",
            blocked, blocker, kind
        )
    };

    Ok(DependencyDryRunResult {
        blocked: blocked.to_string(),
        blocker: blocker.to_string(),
        kind: kind.to_string(),
        condition: condition.map(|c| c.to_string()),
        workspace_sequence: current_sequence,
        semantic_change,
        message,
    })
}

/// Dry-run for remove dependency operation
pub fn remove_dependency_dryrun(
    conn: &Connection,
    blocked: &str,
    blocker: &str,
    kind: Option<&str>,
) -> Result<DependencyDryRunResult> {
    // Check if issues exist
    let _blocked_issue = crate::service::issues::get_issue_by_id(conn, blocked)?
        .ok_or_else(|| Error::not_found(format!("blocked issue '{}'", blocked)))?;
    let _blocker_issue = crate::service::issues::get_issue_by_id(conn, blocker)?
        .ok_or_else(|| Error::not_found(format!("blocker issue '{}'", blocker)))?;

    let current_sequence = get_workspace_sequence(conn)?;

    // Find existing dependencies that would be removed
    let query = if let Some(_k) = kind {
        "SELECT kind FROM dependencies WHERE blocked_issue_id = ?1 AND blocker_issue_id = ?2 AND kind = ?3"
    } else {
        "SELECT kind FROM dependencies WHERE blocked_issue_id = ?1 AND blocker_issue_id = ?2"
    }
    .to_string();

    let mut deps_to_remove = Vec::new();
    let mut stmt = conn.prepare(&query)?;

    // Execute query based on whether kind is specified
    if let Some(k) = kind {
        let rows = stmt.query_map([blocked, blocker, k], |row| row.get::<_, String>(0))?;
        for row in rows {
            deps_to_remove.push(row?);
        }
    } else {
        let rows = stmt.query_map([blocked, blocker], |row| row.get::<_, String>(0))?;
        for row in rows {
            deps_to_remove.push(row?);
        }
    }

    let message = if deps_to_remove.is_empty() {
        if let Some(k) = kind {
            format!(
                "Dependency ({}, {}, {}) does not exist (idempotent)",
                blocked, blocker, k
            )
        } else {
            format!(
                "No dependencies exist between {} and {} (idempotent)",
                blocked, blocker
            )
        }
    } else {
        let kinds = deps_to_remove.join(", ");
        format!(
            "Would remove dependencies: {} -> {} ({})",
            blocked, blocker, kinds
        )
    };

    let semantic_change = !deps_to_remove.is_empty();

    Ok(DependencyDryRunResult {
        blocked: blocked.to_string(),
        blocker: blocker.to_string(),
        kind: kind
            .map(|k| k.to_string())
            .unwrap_or_else(|| "any".to_string()),
        condition: None,
        workspace_sequence: current_sequence,
        semantic_change,
        message,
    })
}

/// Dry-run result for dependency operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyDryRunResult {
    /// Blocked issue ID
    pub blocked: String,
    /// Blocker issue ID
    pub blocker: String,
    /// Dependency kind
    pub kind: String,
    /// Conditional expression (if any)
    pub condition: Option<String>,
    /// Current workspace event sequence
    pub workspace_sequence: i64,
    /// Whether this would be a semantic change vs idempotent
    pub semantic_change: bool,
    /// Advisory message explaining what would happen
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dryrun_result_serialization() {
        let result = DryRunResult {
            id: "test-123".to_string(),
            current_revision: 1,
            workspace_sequence: 100,
            before: IssueDryRunState {
                id: "test-123".to_string(),
                title: "Test Issue".to_string(),
                base_status: "open".to_string(),
                priority: 2,
                assignee: None,
                manual_blocked: Some(false),
                closed_at: None,
                updated_at: "2026-08-09T12:00:00Z".to_string(),
            },
            after: IssueDryRunState {
                id: "test-123".to_string(),
                title: "Test Issue".to_string(),
                base_status: "in_progress".to_string(),
                priority: 2,
                assignee: Some("alice".to_string()),
                manual_blocked: Some(false),
                closed_at: None,
                updated_at: "2026-08-09T12:00:00Z".to_string(),
            },
            semantic_change: true,
            message: "Would update: status: open -> in_progress, assignee: none -> alice"
                .to_string(),
        };

        let json = serde_json::to_string_pretty(&result).unwrap();
        assert!(json.contains("test-123"));
        assert!(json.contains("semantic_change"));
        assert!(json.contains("message"));
    }
}

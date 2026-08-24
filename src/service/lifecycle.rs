//! Lifecycle service implementation
//!
//! This module provides the business logic for update, release, close, and reopen operations.
//! All operations are atomic and include proper audit events and lease validation.

use crate::error::{Error, Result};
use crate::model::{BaseStatus, Issue};
use crate::service::resource_locks::{release_issue_locks, sync_issue_locks};
use crate::service::validate_lease_for_mutation;
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde_json::json;

/// Begin the write transaction every lifecycle mutation runs in.
///
/// The transaction is taken IMMEDIATE -- before the issue is read -- and the
/// revision precondition and lease are validated inside it. A deferred
/// transaction (or validation outside any transaction) leaves a TOCTOU window
/// in the multi-worker shared-workspace model: a concurrent process commits
/// between the read and the write, the guard passes against the stale
/// revision, and the unconditional UPDATE silently clobbers the newer state --
/// the lost update `--if-revision` exists to fail with exit 4. Holding the
/// write lock across read-validate-write pins the snapshot the guard checks
/// to the one the UPDATE lands on.
fn begin_lifecycle_transaction(conn: &Connection) -> Result<Transaction<'_>> {
    Ok(Transaction::new_unchecked(
        conn,
        TransactionBehavior::Immediate,
    )?)
}

/// Fail a mutation whose UPDATE did not touch the row it validated.
///
/// Every lifecycle UPDATE carries `AND revision = ?` for the revision its
/// transaction validated, so an affected-row count of zero means the row
/// moved underneath a snapshot that was somehow taken without holding the
/// write lock. That is the same lost update the revision guard rejects, so
/// it maps to the same conflict (exit 4) rather than to success.
fn ensure_revision_row_affected(changed: usize, expected_revision: i64) -> Result<()> {
    if changed == 0 {
        return Err(Error::conflict(format!(
            "Revision mismatch: expected {}. The issue has been modified since you retrieved it.",
            expected_revision
        )));
    }
    Ok(())
}

/// Update an issue
#[allow(clippy::too_many_arguments)]
pub fn update_issue(
    conn: &Connection,
    id: &str,
    status: Option<&str>,
    assignee: Option<&str>,
    clear_assignee: bool,
    notes: Option<&str>,
    if_revision: Option<i64>,
    fencing_token: Option<i64>,
) -> Result<String> {
    // Process in a write transaction taken before the read, so the revision
    // precondition and lease are validated against the snapshot the UPDATE
    // lands on (see begin_lifecycle_transaction)
    let mut tx = begin_lifecycle_transaction(conn)?;

    let result = update_issue_in_tx(
        &mut tx,
        id,
        status,
        assignee,
        clear_assignee,
        notes,
        if_revision,
        fencing_token,
    )?;

    tx.commit()?;
    Ok(result)
}

/// Run one `bead update` against a caller-owned transaction (R033).
///
/// This is the entire command body between begin and commit, so the
/// manifest composer and the command run the identical validation and
/// apply path against whatever transaction holds the write lock.
#[allow(clippy::too_many_arguments)]
pub fn update_issue_in_tx(
    tx: &mut Transaction,
    id: &str,
    status: Option<&str>,
    assignee: Option<&str>,
    clear_assignee: bool,
    notes: Option<&str>,
    if_revision: Option<i64>,
    fencing_token: Option<i64>,
) -> Result<String> {
    // Validate that assignee and clear_assignee are not both specified
    if assignee.is_some() && clear_assignee {
        return Err(Error::validation(
            "Cannot specify both --assignee and --clear-assignee",
        ));
    }

    // Get current issue state
    let issue = get_issue_for_update(tx, id)?.ok_or_else(|| Error::not_found(id))?;

    // Validate revision precondition if provided
    if let Some(expected_revision) = if_revision {
        let current_revision = issue.revision.unwrap_or(1);
        if current_revision != expected_revision {
            return Err(Error::conflict(format!(
                "Revision mismatch: expected {}, found {}. The issue has been modified since you retrieved it.",
                expected_revision, current_revision
            )));
        }
    }

    // Validate lease if issue has an active lease
    if let Some(current_assignee) = &issue.assignee {
        validate_lease_for_mutation(tx, id, current_assignee, fencing_token)?;
    }

    // Validate assignee value if present
    if let Some(assignee_value) = assignee {
        if assignee_value.is_empty() {
            return Err(Error::validation("assignee cannot be empty"));
        }
    }

    // Validate notes length if present
    if let Some(notes_value) = notes {
        if notes_value.len() > 4 * 1024 * 1024 {
            return Err(Error::validation("Notes cannot exceed 4 MiB"));
        }
    }

    update_issue_impl(tx, &issue, status, assignee, clear_assignee, notes)
}

/// Release an issue
pub fn release_issue(
    conn: &Connection,
    id: &str,
    if_revision: Option<i64>,
    fencing_token: Option<i64>,
) -> Result<String> {
    // Process in a write transaction taken before the read, so the revision
    // precondition and lease are validated against the snapshot the UPDATE
    // lands on (see begin_lifecycle_transaction)
    let mut tx = begin_lifecycle_transaction(conn)?;

    // Get current issue state
    let issue = get_issue_for_update(&tx, id)?.ok_or_else(|| Error::not_found(id))?;

    // Validate revision precondition if provided
    if let Some(expected_revision) = if_revision {
        let current_revision = issue.revision.unwrap_or(1);
        if current_revision != expected_revision {
            return Err(Error::conflict(format!(
                "Revision mismatch: expected {}, found {}. The issue has been modified since you retrieved it.",
                expected_revision, current_revision
            )));
        }
    }

    // Validate lease if issue has an active lease
    if let Some(current_assignee) = &issue.assignee {
        validate_lease_for_mutation(&tx, id, current_assignee, fencing_token)?;
    }

    let result = release_issue_impl(&mut tx, &issue)?;

    tx.commit()?;
    Ok(result)
}

/// Close an issue
pub fn close_issue(
    conn: &Connection,
    id: &str,
    reason: &str,
    if_revision: Option<i64>,
    fencing_token: Option<i64>,
) -> Result<String> {
    // Process in a write transaction taken before the read, so the revision
    // precondition and lease are validated against the snapshot the UPDATE
    // lands on (see begin_lifecycle_transaction)
    let mut tx = begin_lifecycle_transaction(conn)?;

    let result = close_issue_in_tx(&mut tx, id, reason, if_revision, fencing_token)?;

    tx.commit()?;
    Ok(result)
}

/// Run one `bead close` against a caller-owned transaction (R033).
///
/// As with `update_issue_in_tx`, this is the command body between begin and
/// commit, so the manifest composer cannot drift from the command.
pub fn close_issue_in_tx(
    tx: &mut Transaction,
    id: &str,
    reason: &str,
    if_revision: Option<i64>,
    fencing_token: Option<i64>,
) -> Result<String> {
    // Validate reason
    if reason.trim().is_empty() {
        return Err(Error::validation("Close reason cannot be empty"));
    }

    // Get current issue state
    let issue = get_issue_for_update(tx, id)?.ok_or_else(|| Error::not_found(id))?;

    // Validate revision precondition if provided
    if let Some(expected_revision) = if_revision {
        let current_revision = issue.revision.unwrap_or(1);
        if current_revision != expected_revision {
            return Err(Error::conflict(format!(
                "Revision mismatch: expected {}, found {}. The issue has been modified since you retrieved it.",
                expected_revision, current_revision
            )));
        }
    }

    // Validate lease if issue has an active lease
    if let Some(current_assignee) = &issue.assignee {
        validate_lease_for_mutation(tx, id, current_assignee, fencing_token)?;
    }

    close_issue_impl(tx, &issue, reason)
}

/// Reopen an issue
pub fn reopen_issue(
    conn: &Connection,
    id: &str,
    if_revision: Option<i64>,
    fencing_token: Option<i64>,
) -> Result<String> {
    // Process in a write transaction taken before the read, so the revision
    // precondition and lease are validated against the snapshot the UPDATE
    // lands on (see begin_lifecycle_transaction)
    let mut tx = begin_lifecycle_transaction(conn)?;

    // Get current issue state
    let issue = get_issue_for_update(&tx, id)?.ok_or_else(|| Error::not_found(id))?;

    // Validate revision precondition if provided
    if let Some(expected_revision) = if_revision {
        let current_revision = issue.revision.unwrap_or(1);
        if current_revision != expected_revision {
            return Err(Error::conflict(format!(
                "Revision mismatch: expected {}, found {}. The issue has been modified since you retrieved it.",
                expected_revision, current_revision
            )));
        }
    }

    // Validate lease if issue has an active lease
    if let Some(current_assignee) = &issue.assignee {
        validate_lease_for_mutation(&tx, id, current_assignee, fencing_token)?;
    }

    let result = reopen_issue_impl(&mut tx, &issue)?;

    tx.commit()?;
    Ok(result)
}

/// Implementation of update issue within a transaction
fn update_issue_impl(
    tx: &mut Transaction,
    issue: &Issue,
    status: Option<&str>,
    assignee: Option<&str>,
    clear_assignee: bool,
    notes: Option<&str>,
) -> Result<String> {
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string());

    let mut needs_update = false;
    let mut sql_parts = Vec::new();
    let mut params: Vec<String> = vec![];

    // Handle status update
    if let Some(new_status) = status {
        // Special handling for "blocked" status: it is not a BaseStatus
        // variant (BaseStatus::parse would always reject it), it's an
        // overlay flag on top of whatever base_status the issue already
        // has. Must be checked before parsing as a BaseStatus.
        if new_status.eq_ignore_ascii_case("blocked") {
            if issue.base_status == BaseStatus::Closed {
                return Err(Error::conflict(
                    "Cannot set manual blocked flag on closed issue",
                ));
            }
            sql_parts.push("manual_blocked = 1");
            needs_update = true;
        } else {
            let target_status = BaseStatus::parse(new_status)?;

            // Closing has mandatory audit metadata and must go through the
            // dedicated close operation. Allowing generic update to select
            // Closed would bypass closed_at and close_reason invariants.
            if target_status == BaseStatus::Closed {
                return Err(Error::conflict(
                    "Use 'close' command to transition an issue to closed",
                ));
            }

            // Validate transition
            if !issue.base_status.can_transition_to(&target_status) {
                return Err(Error::conflict(format!(
                    "Invalid status transition from {} to {}",
                    issue.base_status, target_status
                )));
            }

            // Check if this is actually trying to reopen a closed issue
            if issue.base_status == BaseStatus::Closed && target_status == BaseStatus::Open {
                return Err(Error::conflict(
                    "Use 'reopen' command to transition from closed to open",
                ));
            }

            sql_parts.push("base_status = ?");
            params.push(new_status.to_string());
            // An explicit non-blocked status transition clears any manual
            // block, mirroring close/reopen's existing manual_blocked = 0
            // reset -- otherwise "blocked" would be settable but never
            // clearable via `update`.
            sql_parts.push("manual_blocked = 0");
            needs_update = true;
        }
    }

    // Handle assignee update
    if let Some(new_assignee) = assignee {
        sql_parts.push("assignee = ?");
        params.push(new_assignee.to_string());
        needs_update = true;
    } else if clear_assignee {
        // clear-assignee only works on open assigned issues
        match issue.base_status {
            BaseStatus::Open => {
                if issue.assignee.is_some() {
                    sql_parts.push("assignee = NULL");
                    needs_update = true;

                    // Append assignment_cleared event
                    append_event(
                        tx,
                        Some(&issue.id),
                        "assignment_cleared",
                        &json!({
                            "prior_assignee": issue.assignee,
                            "resulting_base_status": "open"
                        }),
                        &now,
                    )?;
                }
                // If already unassigned, succeed idempotently without event
            }
            BaseStatus::InProgress => {
                return Err(Error::conflict(
                    "Use 'release' command for in-progress issues",
                ));
            }
            BaseStatus::Deferred => {
                return Err(Error::conflict(
                    "Cannot clear assignee on deferred issue - update status to open first",
                ));
            }
            BaseStatus::Closed => {
                return Err(Error::conflict(
                    "Cannot modify closed issue - use 'reopen' first",
                ));
            }
        }
    }

    // Handle notes update
    if let Some(notes_value) = notes {
        sql_parts.push("notes = ?");
        params.push(notes_value.to_string());
        needs_update = true;
    }

    if !needs_update {
        // No actual changes - succeed idempotently
        return Ok(issue.id.clone());
    }

    // Add updated_at timestamp and increment revision
    sql_parts.push("updated_at = ?");
    params.push(now.clone());
    sql_parts.push("revision = revision + 1");

    // The UPDATE is conditional on the revision the caller's transaction
    // validated, so a row that moved anyway (a snapshot taken without the
    // write lock) affects zero rows and fails as a conflict instead of
    // overwriting the newer state.
    let expected_revision = issue.revision.unwrap_or(1);
    let sql = format!(
        "UPDATE issues SET {} WHERE id = ? AND revision = ?",
        sql_parts.join(", ")
    );

    // Convert String params to &dyn ToSql for execution
    let params_refs: Vec<&dyn rusqlite::ToSql> =
        params.iter().map(|s| s as &dyn rusqlite::ToSql).collect();

    {
        let mut stmt = tx.prepare_cached(&sql)?;
        let mut all_params = params_refs;
        all_params.push(&issue.id as &dyn rusqlite::ToSql);
        all_params.push(&expected_revision as &dyn rusqlite::ToSql);
        let changed = stmt.execute(all_params.as_slice())?;
        ensure_revision_row_affected(changed, expected_revision)?;
    }

    // Resource ownership follows the resulting lifecycle state. If an update
    // enters or leaves in_progress, this reconciliation remains in the same
    // transaction as the issue update and rolls back on a key conflict.
    sync_issue_locks(tx, &issue.id)?;

    // Append general update event if we made semantic changes
    // (assignment_cleared already handled above)
    if clear_assignee && issue.assignee.is_some() {
        // Event already added
    } else if status.is_some() || assignee.is_some() || notes.is_some() {
        append_event(tx, Some(&issue.id), "updated", &json!({}), &now)?;
    }

    Ok(issue.id.clone())
}

/// Implementation of release issue within a transaction
fn release_issue_impl(tx: &mut Transaction, issue: &Issue) -> Result<String> {
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string());

    match issue.base_status {
        BaseStatus::InProgress => {
            // Semantic release: transition to open and clear assignee.
            // Conditional on the validated revision -- see
            // ensure_revision_row_affected.
            let expected_revision = issue.revision.unwrap_or(1);
            {
                let mut stmt = tx.prepare_cached(
                    "UPDATE issues SET base_status = 'open', assignee = NULL, updated_at = ?, revision = revision + 1 WHERE id = ? AND revision = ?"
                )?;
                let changed = stmt.execute((&now, &issue.id, expected_revision))?;
                ensure_revision_row_affected(changed, expected_revision)?;
            }

            release_issue_locks(tx, &issue.id)?;

            // Append release event
            append_event(
                tx,
                Some(&issue.id),
                "released",
                &json!({
                    "prior_assignee": issue.assignee,
                    "resulting_base_status": "open"
                }),
                &now,
            )?;

            Ok(issue.id.clone())
        }
        BaseStatus::Open => {
            if issue.assignee.is_some() {
                // Conflict: open but assigned
                return Err(Error::conflict(
                    "Cannot release assigned open issue - use 'update --clear-assignee' instead",
                ));
            }
            // Idempotent: already open and unassigned
            Ok(issue.id.clone())
        }
        BaseStatus::Deferred | BaseStatus::Closed => Err(Error::conflict(format!(
            "Cannot release issue in {} status - only in-progress issues can be released",
            issue.base_status
        ))),
    }
}

/// Implementation of close issue within a transaction
fn close_issue_impl(tx: &mut Transaction, issue: &Issue, reason: &str) -> Result<String> {
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string());

    match issue.base_status {
        BaseStatus::Closed => {
            // Check if reason matches (idempotent case)
            if let Some(existing_reason) = &issue.close_reason {
                if existing_reason.trim() == reason.trim() {
                    // Idempotent - same reason
                    return Ok(issue.id.clone());
                }
            }
            // Conflict - different reason
            Err(Error::conflict(
                "Issue already closed with different reason",
            ))
        }
        BaseStatus::Open | BaseStatus::InProgress | BaseStatus::Deferred => {
            // Semantic close. Conditional on the validated revision -- see
            // ensure_revision_row_affected.
            let normalized_reason = reason.trim();
            let expected_revision = issue.revision.unwrap_or(1);
            {
                let mut stmt = tx.prepare_cached(
                    "UPDATE issues SET base_status = 'closed', closed_at = ?, close_reason = ?,
                     manual_blocked = 0, updated_at = ?, revision = revision + 1 WHERE id = ? AND revision = ?",
                )?;
                let changed =
                    stmt.execute((&now, &normalized_reason, &now, &issue.id, expected_revision))?;
                ensure_revision_row_affected(changed, expected_revision)?;
            }

            release_issue_locks(tx, &issue.id)?;

            // Append closed event
            append_event(
                tx,
                Some(&issue.id),
                "closed",
                &json!({
                    "prior_base_status": format!("{}", issue.base_status),
                    "reason": normalized_reason
                }),
                &now,
            )?;

            Ok(issue.id.clone())
        }
    }
}

/// Implementation of reopen issue within a transaction
fn reopen_issue_impl(tx: &mut Transaction, issue: &Issue) -> Result<String> {
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string());

    match issue.base_status {
        BaseStatus::Closed => {
            // Semantic reopen. Conditional on the validated revision -- see
            // ensure_revision_row_affected.
            let expected_revision = issue.revision.unwrap_or(1);
            {
                let mut stmt = tx.prepare_cached(
                    "UPDATE issues SET base_status = 'open', closed_at = NULL, close_reason = NULL,
                     manual_blocked = 0, assignee = NULL, updated_at = ?, revision = revision + 1 WHERE id = ? AND revision = ?",
                )?;
                let changed = stmt.execute((&now, &issue.id, expected_revision))?;
                ensure_revision_row_affected(changed, expected_revision)?;
            }

            release_issue_locks(tx, &issue.id)?;

            // Append reopened event
            append_event(
                tx,
                Some(&issue.id),
                "reopened",
                &json!({
                    "prior_base_status": "closed",
                    "resulting_base_status": "open",
                    "prior_assignee": issue.assignee
                }),
                &now,
            )?;

            Ok(issue.id.clone())
        }
        BaseStatus::Open => {
            // Idempotent - already open
            Ok(issue.id.clone())
        }
        BaseStatus::InProgress | BaseStatus::Deferred => Err(Error::conflict(format!(
            "Cannot reopen issue in {} status - only closed issues can be reopened",
            issue.base_status
        ))),
    }
}

/// Get issue for update operations
fn get_issue_for_update(conn: &Connection, id: &str) -> Result<Option<Issue>> {
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
                extensions: Default::default(),
            })
        })
        .optional()?;

    Ok(issue)
}

/// Append an audit event
fn append_event(
    tx: &mut Transaction,
    issue_id: Option<&str>,
    kind: &str,
    detail: &serde_json::Value,
    time: &str,
) -> Result<()> {
    let detail_json = serde_json::to_string(detail)
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to serialize event detail: {}", e)))?;

    let mut stmt = tx.prepare_cached(
        "INSERT INTO events (issue_id, kind, detail, time) VALUES (?1, ?2, ?3, ?4)",
    )?;

    stmt.execute((issue_id, kind, &detail_json, time))?;
    Ok(())
}

/// Parse base status from database string
fn parse_base_status(s: String) -> BaseStatus {
    match s.to_lowercase().as_str() {
        "open" => BaseStatus::Open,
        "in_progress" => BaseStatus::InProgress,
        "deferred" => BaseStatus::Deferred,
        "closed" => BaseStatus::Closed,
        _ => BaseStatus::Open,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_base_status_in_lifecycle() {
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

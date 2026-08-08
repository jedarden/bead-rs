//! Claim service for atomic issue selection and assignment
//!
//! This module implements server-selected claim scheduling using FIFO-v1 policy.
//! Claims execute in a single write transaction to ensure atomicity and prevent
//! duplicate assignments under concurrent access.

use crate::error::Result;
use rusqlite::OptionalExtension;
use rusqlite::Transaction;
use serde_json::json;
use time::OffsetDateTime;

/// Claim result for JSON output
#[derive(Debug, Clone, serde::Serialize)]
pub struct ClaimResult {
    pub bead_id: Option<String>,
    pub assignee: String,
}

/// Claim an issue from the ready frontier using FIFO-v1 policy
///
/// This function implements atomic server-selected claiming:
/// - Finds eligible issues (open, unassigned, no manual block, no unfinished blockers)
/// - Ranks by priority (ASC), created_at (ASC), id (ASC)
/// - Assigns the winner to the actor and transitions to in_progress
/// - Records a claim audit event
/// - Returns empty result if no eligible work exists
///
/// All operations occur in a single write transaction to guarantee atomicity.
pub fn claim_issue(
    tx: &Transaction,
    assignee: &str,
    _model: Option<&str>,
    _harness: Option<&str>,
    _harness_version: Option<&str>,
) -> Result<ClaimResult> {
    // Find the next eligible issue using FIFO-v1 ranking
    let eligible_issue = find_eligible_issue(tx)?;

    let issue_id = match eligible_issue {
        Some(id) => id,
        None => {
            // No eligible work - return empty result successfully
            return Ok(ClaimResult {
                bead_id: None,
                assignee: assignee.to_string(),
            });
        }
    };

    // Transition the issue to in_progress and assign it
    let now = OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string());

    tx.execute(
        "UPDATE issues SET base_status = 'in_progress', assignee = ?1, updated_at = ?2 WHERE id = ?3",
        [assignee, &now, &issue_id],
    )?;

    // Record the claim audit event
    let event_detail = json!({
        "policy": "fifo-v1",
        "resulting_base_status": "in_progress"
    });

    let event_detail_json = serde_json::to_string(&event_detail).map_err(|e| {
        crate::Error::Internal(anyhow::anyhow!("Failed to serialize event detail: {}", e))
    })?;

    let event_time = OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string());

    tx.execute(
        "INSERT INTO events (issue_id, kind, actor, time, detail) VALUES (?1, 'claimed', ?2, ?3, ?4)",
        [&issue_id, assignee, &event_time, &event_detail_json],
    )?;

    Ok(ClaimResult {
        bead_id: Some(issue_id),
        assignee: assignee.to_string(),
    })
}

/// Find the next eligible issue using FIFO-v1 ranking
///
/// Eligibility requires:
/// - Base status is 'open'
/// - No assignee (assignee IS NULL)
/// - Not manually blocked (manual_blocked = 0)
/// - No unfinished 'blocks' dependencies
///
/// Ranking: priority ASC, created_at ASC, id ASC
fn find_eligible_issue(tx: &Transaction) -> Result<Option<String>> {
    // Use a subquery to exclude issues that have unfinished blockers
    let query = r#"
        SELECT i.id
        FROM issues i
        WHERE i.base_status = 'open'
          AND i.assignee IS NULL
          AND i.manual_blocked = 0
          AND NOT EXISTS (
              SELECT 1
              FROM dependencies d
              JOIN issues blocker ON blocker.id = d.blocker_issue_id
              WHERE d.blocked_issue_id = i.id
                AND d.kind = 'blocks'
                AND blocker.base_status != 'closed'
          )
        ORDER BY i.priority ASC, i.created_at ASC, i.id ASC
        LIMIT 1
    "#;

    let issue_id: Option<String> = tx
        .query_row(query, [], |row| row.get(0))
        .optional()
        .map_err(|e| {
            crate::Error::Internal(anyhow::anyhow!("Failed to query eligible issues: {}", e))
        })?;

    Ok(issue_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_claim_result_serialization() {
        let result = ClaimResult {
            bead_id: Some("test-1234567890abcdef".to_string()),
            assignee: "worker-1".to_string(),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("test-1234567890abcdef"));
        assert!(json.contains("worker-1"));

        let empty = ClaimResult {
            bead_id: None,
            assignee: "worker-1".to_string(),
        };
        let json_empty = serde_json::to_string(&empty).unwrap();
        assert!(json_empty.contains("null"));
    }
}

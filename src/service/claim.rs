//! Claim service for atomic issue selection and assignment
//!
//! This module implements server-selected claim scheduling using FIFO-v1 policy.
//! Claims execute in a single write transaction to ensure atomicity and prevent
//! duplicate assignments under concurrent access.

use crate::error::Result;
use rusqlite::OptionalExtension;
use rusqlite::Transaction;
use serde_json::json;
use std::collections::HashMap;
use time::OffsetDateTime;

/// Claim result for JSON output
#[derive(Debug, Clone, serde::Serialize)]
pub struct ClaimResult {
    pub bead_id: Option<String>,
    pub assignee: String,
}

/// Decision trace version for compatibility tracking
const DECISION_TRACE_VERSION: &str = "v1";

/// Semantic reason codes for decision explanations
///
/// These codes explain why an issue was selected or why no issue was available.
/// They provide machine-readable diagnostics without exposing SQL or private
/// store details.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReasonCode {
    /// Issue met all eligibility criteria and was selected
    EligibleSelected,

    /// No issues in the workspace are available for claiming
    NoEligibleIssues,

    /// Issue is already assigned to another worker
    AlreadyAssigned,

    /// Issue is manually blocked from claiming
    ManuallyBlocked,

    /// Issue has unfinished blocker dependencies
    HasUnfinishedBlockers,

    /// Issue is not in open status (e.g., closed, in_progress, deferred)
    NotOpenStatus,

    /// Selected based on priority ranking (highest priority first)
    SelectedByPriority,

    /// Selected based on FIFO ordering within same priority
    SelectedByFifoOrder,

    /// Workspace has no issues at all
    EmptyWorkspace,
}

/// Issue eligibility factors for decision trace
///
/// This structure captures the factors that determined an issue's eligibility
/// for claiming without exposing internal SQL or private store details.
#[derive(Debug, Clone, serde::Serialize)]
pub struct EligibilityFactors {
    pub issue_id: String,
    pub is_eligible: bool,
    pub reasons: Vec<ReasonCode>,
    pub priority: i64,
    pub created_at: String,
    pub base_status: String,
    pub is_assigned: bool,
    pub is_manually_blocked: bool,
    pub unfinished_blocker_count: i64,
}

/// Summary of all issues evaluated for claim decision
#[derive(Debug, Clone, serde::Serialize)]
pub struct EligibilitySummary {
    pub total_issues: i64,
    pub eligible_count: i64,
    pub ineligible_count: i64,
    pub ineligibility_reasons: HashMap<String, i64>, // reason code -> count
}

/// Machine-readable decision trace for claim operations
///
/// This trace explains the claim decision with versioned semantic reason codes,
/// making empty queues and surprising selection behavior diagnosable without
/// revealing SQL or private store details.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DecisionTrace {
    /// Decision trace version for compatibility tracking
    pub version: String,

    /// Whether any issue was selected
    pub has_selection: bool,

    /// Selected issue ID (if any)
    pub selected_issue_id: Option<String>,

    /// Reason codes explaining the selection or lack thereof
    pub reasons: Vec<ReasonCode>,

    /// Summary of eligibility evaluation across all issues
    pub eligibility_summary: EligibilitySummary,

    /// Detailed factors for the selected issue (if any)
    pub selected_factors: Option<EligibilityFactors>,

    /// Assignee who performed or attempted the claim
    pub assignee: String,

    /// Claim policy used for selection
    pub policy: String,
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
        "UPDATE issues SET base_status = 'in_progress', assignee = ?1, updated_at = ?2, revision = revision + 1 WHERE id = ?3",
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

/// Collect eligibility factors for all issues in the workspace
///
/// This function evaluates every issue for eligibility without exposing SQL
/// details, providing the diagnostic information needed for decision traces.
fn collect_eligibility_factors(
    tx: &Transaction,
    _assignee: &str,
) -> Result<Vec<EligibilityFactors>> {
    let query = r#"
        SELECT
            i.id,
            i.base_status,
            i.assignee,
            i.manual_blocked,
            i.priority,
            i.created_at,
            (
                SELECT COUNT(*)
                FROM dependencies d
                JOIN issues blocker ON blocker.id = d.blocker_issue_id
                WHERE d.blocked_issue_id = i.id
                  AND d.kind = 'blocks'
                  AND blocker.base_status != 'closed'
            ) as unfinished_blockers
        FROM issues i
        ORDER BY i.priority ASC, i.created_at ASC, i.id ASC
    "#;

    let mut factors = Vec::new();
    let mut stmt = tx.prepare(query).map_err(|e| {
        crate::Error::Internal(anyhow::anyhow!(
            "Failed to prepare eligibility query: {}",
            e
        ))
    })?;

    let issue_rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>("id")?,
                row.get::<_, String>("base_status")?,
                row.get::<_, Option<String>>("assignee")?,
                row.get::<_, i64>("manual_blocked")?,
                row.get::<_, i64>("priority")?,
                row.get::<_, String>("created_at")?,
                row.get::<_, i64>("unfinished_blockers")?,
            ))
        })
        .map_err(|e| {
            crate::Error::Internal(anyhow::anyhow!(
                "Failed to execute eligibility query: {}",
                e
            ))
        })?;

    for issue in issue_rows {
        let (id, base_status, assignee, manual_blocked, priority, created_at, unfinished_blockers) =
            issue?;

        let is_assigned = assignee.is_some();
        let is_manually_blocked = manual_blocked != 0;
        let is_open = base_status == "open";
        let has_unfinished_blockers = unfinished_blockers > 0;

        let mut reasons = Vec::new();
        let mut is_eligible = true;

        if !is_open {
            reasons.push(ReasonCode::NotOpenStatus);
            is_eligible = false;
        }

        if is_assigned {
            reasons.push(ReasonCode::AlreadyAssigned);
            is_eligible = false;
        }

        if is_manually_blocked {
            reasons.push(ReasonCode::ManuallyBlocked);
            is_eligible = false;
        }

        if has_unfinished_blockers {
            reasons.push(ReasonCode::HasUnfinishedBlockers);
            is_eligible = false;
        }

        if is_eligible {
            reasons.push(ReasonCode::EligibleSelected);
        }

        factors.push(EligibilityFactors {
            issue_id: id,
            is_eligible,
            reasons,
            priority,
            created_at,
            base_status,
            is_assigned,
            is_manually_blocked,
            unfinished_blocker_count: unfinished_blockers,
        });
    }

    Ok(factors)
}

/// Build eligibility summary from collected factors
fn build_eligibility_summary(factors: &[EligibilityFactors]) -> EligibilitySummary {
    let mut ineligibility_reasons: HashMap<String, i64> = HashMap::new();

    for factor in factors {
        if !factor.is_eligible {
            for reason in &factor.reasons {
                if reason != &ReasonCode::EligibleSelected {
                    let reason_str =
                        serde_json::to_string(reason).unwrap_or_else(|_| "unknown".to_string());
                    *ineligibility_reasons.entry(reason_str).or_insert(0) += 1;
                }
            }
        }
    }

    let eligible_count = factors.iter().filter(|f| f.is_eligible).count() as i64;
    let ineligible_count = factors.len() as i64 - eligible_count;

    EligibilitySummary {
        total_issues: factors.len() as i64,
        eligible_count,
        ineligible_count,
        ineligibility_reasons,
    }
}

/// Create a decision trace for claim operations
///
/// This function builds a machine-readable explanation of the claim decision
/// without revealing SQL or private store details.
pub fn create_decision_trace(
    tx: &Transaction,
    selected_issue_id: Option<&str>,
    assignee: &str,
) -> Result<DecisionTrace> {
    let factors = collect_eligibility_factors(tx, assignee)?;
    let eligibility_summary = build_eligibility_summary(&factors);

    let has_selection = selected_issue_id.is_some();
    let selected_factors =
        selected_issue_id.and_then(|id| factors.iter().find(|f| f.issue_id == id).cloned());

    let mut reasons = Vec::new();

    if let Some(selected_id) = selected_issue_id {
        // Find the selected issue and explain why it was chosen
        if let Some(selected) = factors.iter().find(|f| f.issue_id == selected_id) {
            reasons.push(ReasonCode::EligibleSelected);

            // Explain the ranking decision
            let higher_priority_count = factors
                .iter()
                .filter(|f| f.is_eligible && f.priority < selected.priority)
                .count();

            if higher_priority_count == 0 {
                reasons.push(ReasonCode::SelectedByPriority);
            } else {
                reasons.push(ReasonCode::SelectedByFifoOrder);
            }
        }
    } else {
        // No issue was selected - explain why
        if eligibility_summary.total_issues == 0 {
            reasons.push(ReasonCode::EmptyWorkspace);
        } else if eligibility_summary.eligible_count == 0 {
            reasons.push(ReasonCode::NoEligibleIssues);
        }
    }

    Ok(DecisionTrace {
        version: DECISION_TRACE_VERSION.to_string(),
        has_selection,
        selected_issue_id: selected_issue_id.map(|s| s.to_string()),
        reasons,
        eligibility_summary,
        selected_factors,
        assignee: assignee.to_string(),
        policy: "fifo-v1".to_string(),
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

/// Perform claim operation with optional decision trace
///
/// This function wraps the standard claim operation with optional decision trace
/// collection for diagnostic purposes. The decision trace is nonmutating and
/// only reads data to explain the claim decision.
pub fn claim_issue_with_trace(
    tx: &Transaction,
    assignee: &str,
    model: Option<&str>,
    harness: Option<&str>,
    harness_version: Option<&str>,
    include_trace: bool,
) -> Result<(ClaimResult, Option<DecisionTrace>)> {
    // First perform the standard claim operation
    let result = claim_issue(tx, assignee, model, harness, harness_version)?;

    // Collect decision trace if requested
    let trace = if include_trace {
        Some(create_decision_trace(
            tx,
            result.bead_id.as_deref(),
            assignee,
        )?)
    } else {
        None
    };

    Ok((result, trace))
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

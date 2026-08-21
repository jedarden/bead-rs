//! Claim service for atomic issue selection and assignment
//!
//! This module implements server-selected claim scheduling using FIFO-v1 policy.
//! Claims execute in a single write transaction to ensure atomicity and prevent
//! duplicate assignments under concurrent access.

use crate::error::{Error, Result};
use crate::service::leases::{create_lease, renew_lease, DEFAULT_LEASE_TTL};
use crate::service::scheduling::{self, SchedulingPolicy};
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

    /// Assignee already holds an in_progress issue in this workspace,
    /// so the opt-in single-claim guard refused a new claim
    AssigneeHasActiveClaim,

    /// Open issue carries an assignee, excluding it from the ready frontier
    /// despite not being an active claim (a claim sets in_progress)
    OpenIssueHeldByAssignee,

    /// Open issue is intentionally held under its current assignee
    /// and should not be warned about (operator-declared state)
    IntentionallyHeldAssignment,
}

impl ReasonCode {
    /// Machine-readable snake_case identifier for this reason code
    ///
    /// Derived from the serde representation so the string and the enum
    /// variant cannot drift apart.
    pub fn code_string(&self) -> String {
        match serde_json::to_value(self) {
            Ok(serde_json::Value::String(s)) => s,
            _ => format!("{:?}", self),
        }
    }
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
/// When `single_claim` is set, refuses with `assignee_has_active_claim` if the
/// assignee already holds an in_progress issue in this workspace. The guard
/// runs in this same transaction, before selection.
///
/// All operations occur in a single write transaction to guarantee atomicity.
pub fn claim_issue(
    tx: &Transaction,
    assignee: &str,
    _model: Option<&str>,
    _harness: Option<&str>,
    _harness_version: Option<&str>,
    single_claim: bool,
) -> Result<ClaimResult> {
    // Refuse before selecting when the single-claim guard is enabled and the
    // assignee already holds active work
    enforce_single_claim(tx, assignee, single_claim)?;

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

/// Enhanced claim result with optional lease information
#[derive(Debug, Clone, serde::Serialize)]
pub struct EnhancedClaimResult {
    pub bead_id: Option<String>,
    pub assignee: String,
    pub lease: Option<crate::service::LeaseClaimResult>,
}

/// Claim an issue with optional lease support
///
/// This function extends the basic claim logic to support R002's fenced claim leases:
/// - Optionally creates a lease with fencing token and expiry
/// - Supports lease renewal instead of new claim
/// - Validates fencing tokens when provided
/// - Maintains backward compatibility with non-leased claims
///
/// When `single_claim` is set, the new-claim branch refuses with
/// `assignee_has_active_claim` if the assignee already holds an in_progress
/// issue in this workspace. Lease renewal and fencing-token validation are
/// not guarded: both operate on an issue the assignee already holds, so they
/// do not newly assign work.
///
/// # Arguments
/// * `tx` - Database transaction
/// * `assignee` - Who is claiming the issue
/// * `lease_ttl_seconds` - Optional TTL for leased claims
/// * `renew_lease` - If true, renew existing lease instead of claiming new work
/// * `fencing_token` - Optional fencing token for validation
/// * `single_claim` - If true, refuse when the assignee already holds an
///   in_progress issue in this workspace
///
/// # Returns
/// Enhanced claim result including lease information if applicable
pub fn claim_issue_with_lease(
    tx: &Transaction,
    assignee: &str,
    lease_ttl_seconds: Option<u64>,
    renew_lease: bool,
    fencing_token: Option<i64>,
    single_claim: bool,
) -> Result<EnhancedClaimResult> {
    // Handle lease renewal if requested
    if renew_lease {
        return claim_with_renewal(tx, assignee, lease_ttl_seconds);
    }

    // Handle fencing token validation if provided
    if let Some(token) = fencing_token {
        return claim_with_fencing_token(tx, assignee, token, lease_ttl_seconds);
    }

    // Refuse before selecting when the single-claim guard is enabled and the
    // assignee already holds active work
    enforce_single_claim(tx, assignee, single_claim)?;

    // Perform normal claim with optional lease creation
    let eligible_issue = find_eligible_issue(tx)?;

    let issue_id = match eligible_issue {
        Some(id) => id,
        None => {
            // No eligible work - return empty result successfully
            return Ok(EnhancedClaimResult {
                bead_id: None,
                assignee: assignee.to_string(),
                lease: None,
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

    // Create lease if requested
    let lease_info = if let Some(ttl) = lease_ttl_seconds {
        Some(create_lease(tx, &issue_id, assignee, ttl)?)
    } else {
        None
    };

    // Record the claim audit event
    let event_detail = if lease_info.is_some() {
        json!({
            "policy": "fifo-v1",
            "resulting_base_status": "in_progress",
            "lease_ttl_seconds": lease_ttl_seconds,
            "with_lease": true
        })
    } else {
        json!({
            "policy": "fifo-v1",
            "resulting_base_status": "in_progress",
            "with_lease": false
        })
    };

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

    Ok(EnhancedClaimResult {
        bead_id: Some(issue_id),
        assignee: assignee.to_string(),
        lease: lease_info,
    })
}

/// Handle lease renewal instead of new claim
fn claim_with_renewal(
    tx: &Transaction,
    assignee: &str,
    lease_ttl_seconds: Option<u64>,
) -> Result<EnhancedClaimResult> {
    // Find issues currently assigned to this assignee with active leases
    let now = OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string());

    let issue_to_renew: Option<String> = tx
        .query_row(
            "SELECT l.issue_id FROM leases l
             JOIN issues i ON i.id = l.issue_id
             WHERE l.assignee = ?1 AND l.expires_at > ?2
             ORDER BY l.expires_at ASC
             LIMIT 1",
            [assignee, &now],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| {
            crate::Error::Internal(anyhow::anyhow!("Failed to find lease to renew: {}", e))
        })?;

    let issue_id = match issue_to_renew {
        Some(id) => id,
        None => {
            // No active leases found - return empty result
            return Ok(EnhancedClaimResult {
                bead_id: None,
                assignee: assignee.to_string(),
                lease: None,
            });
        }
    };

    // Use default TTL if not specified
    let ttl = lease_ttl_seconds.unwrap_or(DEFAULT_LEASE_TTL);

    // Renew the lease
    let renewed_lease = renew_lease(tx, &issue_id, assignee, ttl)?;

    // Record the renewal audit event
    let event_detail = json!({
        "policy": "fifo-v1",
        "action": "lease_renewed",
        "lease_ttl_seconds": ttl,
        "fencing_token": renewed_lease.fencing_token
    });

    let event_detail_json = serde_json::to_string(&event_detail).map_err(|e| {
        crate::Error::Internal(anyhow::anyhow!("Failed to serialize event detail: {}", e))
    })?;

    let event_time = OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string());

    tx.execute(
        "INSERT INTO events (issue_id, kind, actor, time, detail) VALUES (?1, 'lease_renewed', ?2, ?3, ?4)",
        [&issue_id, assignee, &event_time, &event_detail_json],
    )?;

    Ok(EnhancedClaimResult {
        bead_id: Some(issue_id),
        assignee: assignee.to_string(),
        lease: Some(renewed_lease),
    })
}

/// Handle claim with explicit fencing token
fn claim_with_fencing_token(
    tx: &Transaction,
    assignee: &str,
    expected_token: i64,
    lease_ttl_seconds: Option<u64>,
) -> Result<EnhancedClaimResult> {
    // Find any active leases for this assignee and validate the fencing token
    let now = OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string());

    let (matching_issue, actual_token): (Option<String>, Option<i64>) = tx
        .query_row(
            "SELECT l.issue_id, l.fencing_token FROM leases l
             JOIN issues i ON i.id = l.issue_id
             WHERE l.assignee = ?1 AND l.expires_at > ?2 AND l.fencing_token = ?3
             LIMIT 1",
            [assignee, &now, &expected_token.to_string()],
            |row| Ok((Some(row.get::<_, String>(0)?), Some(row.get::<_, i64>(1)?))),
        )
        .optional()
        .map_err(|e| {
            crate::Error::Internal(anyhow::anyhow!("Failed to validate fencing token: {}", e))
        })?
        .unwrap_or((None, None));

    match (matching_issue, actual_token) {
        (Some(issue_id), Some(token)) if token == expected_token => {
            // Valid fencing token found - perform claim with new lease
            let ttl = lease_ttl_seconds.unwrap_or(DEFAULT_LEASE_TTL);
            let new_lease = create_lease(tx, &issue_id, assignee, ttl)?;

            // Record the fencing token claim audit event
            let event_detail = json!({
                "policy": "fifo-v1",
                "action": "claim_with_fencing_token",
                "previous_fencing_token": expected_token,
                "new_fencing_token": new_lease.fencing_token,
                "lease_ttl_seconds": ttl
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

            Ok(EnhancedClaimResult {
                bead_id: Some(issue_id),
                assignee: assignee.to_string(),
                lease: Some(new_lease),
            })
        }
        _ => {
            // No matching fencing token found - this is an error
            Err(crate::Error::LeaseConflict(format!(
                "No active lease found with fencing token {} for assignee {}",
                expected_token, assignee
            )))
        }
    }
}

/// Collect eligibility factors for all issues in the workspace
///
/// This function evaluates every issue for eligibility without exposing SQL
/// details, providing the diagnostic information needed for decision traces.
/// Read-only: callers inside a write transaction can snapshot the factors
/// before mutating, then assemble the trace with [`build_decision_trace`] so
/// it reflects decision-time state.
pub fn collect_eligibility_factors(
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
/// without revealing SQL or private store details. Factors are collected from
/// current workspace state; callers that mutate before explaining must
/// snapshot with [`collect_eligibility_factors`] first and use
/// [`build_decision_trace`] instead, so the trace reflects decision-time
/// state rather than the mutation's result.
// Public library API; the binary snapshots factors itself (see cmd_claim),
// so this may show as unused when compiling the bin target.
#[allow(dead_code)]
pub fn create_decision_trace(
    tx: &Transaction,
    selected_issue_id: Option<&str>,
    assignee: &str,
) -> Result<DecisionTrace> {
    let factors = collect_eligibility_factors(tx, assignee)?;
    Ok(build_decision_trace(factors, selected_issue_id, assignee))
}

/// Assemble a decision trace from pre-collected eligibility factors
///
/// Split from [`create_decision_trace`] so a caller inside a write
/// transaction can snapshot the factors before claiming and still explain
/// the selection it actually made. Collecting after the claim would describe
/// the just-claimed issue as in_progress/ineligible in its own trace.
pub fn build_decision_trace(
    factors: Vec<EligibilityFactors>,
    selected_issue_id: Option<&str>,
    assignee: &str,
) -> DecisionTrace {
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

    DecisionTrace {
        version: DECISION_TRACE_VERSION.to_string(),
        has_selection,
        selected_issue_id: selected_issue_id.map(|s| s.to_string()),
        reasons,
        eligibility_summary,
        selected_factors,
        assignee: assignee.to_string(),
        policy: "fifo-v1".to_string(),
    }
}

/// Find an in_progress issue currently held by the given assignee
///
/// Returns the blocking issue ID for the single-claim guard when the assignee
/// already holds active work. Ordered deterministically so the reported
/// blocker is stable even if more than one exists (possible in workspaces
/// predating the guard, or via `update --assignee` on an open issue).
fn find_assignee_in_progress_issue(tx: &Transaction, assignee: &str) -> Result<Option<String>> {
    let issue_id: Option<String> = tx
        .query_row(
            "SELECT id FROM issues
             WHERE assignee = ?1 AND base_status = 'in_progress'
             ORDER BY updated_at ASC, id ASC
             LIMIT 1",
            [assignee],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| {
            crate::Error::Internal(anyhow::anyhow!(
                "Failed to query assignee in-progress issues: {}",
                e
            ))
        })?;

    Ok(issue_id)
}

/// Enforce the opt-in single-claim guard before selecting a new issue
///
/// When `single_claim` is set, refuse the claim if the assignee already holds
/// any issue with base status in_progress in this workspace. The refusal
/// carries the `assignee_has_active_claim` reason code and names the blocking
/// issue ID. Must run inside the caller's claim transaction so the check and
/// the subsequent selection/assignment are atomic — no race between check
/// and assign.
fn enforce_single_claim(tx: &Transaction, assignee: &str, single_claim: bool) -> Result<()> {
    if !single_claim {
        return Ok(());
    }

    if let Some(blocking_id) = find_assignee_in_progress_issue(tx, assignee)? {
        return Err(Error::ClaimRefused {
            code: ReasonCode::AssigneeHasActiveClaim.code_string(),
            message: format!(
                "assignee '{}' already holds in_progress issue '{}' in this workspace; \
                 release or close it before claiming another (--single-claim)",
                assignee, blocking_id
            ),
        });
    }

    Ok(())
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
/// collection for diagnostic purposes. The trace is assembled from an
/// eligibility snapshot taken before the claim runs, so it explains the
/// decision as it was made — never from post-claim state, which would
/// describe the just-claimed issue as in_progress/ineligible.
///
/// Part of the library's public claim API. The CLI builds its `--why` trace
/// from `collect_eligibility_factors` + `build_decision_trace` around the
/// claim it already made, so this wrapper may show as unused when compiling
/// the binary.
#[allow(dead_code)]
pub fn claim_issue_with_trace(
    tx: &Transaction,
    assignee: &str,
    model: Option<&str>,
    harness: Option<&str>,
    harness_version: Option<&str>,
    include_trace: bool,
    single_claim: bool,
) -> Result<(ClaimResult, Option<DecisionTrace>)> {
    // Snapshot eligibility before claiming so the trace reflects
    // decision-time state
    let factors = if include_trace {
        Some(collect_eligibility_factors(tx, assignee)?)
    } else {
        None
    };

    // Perform the standard claim operation
    let result = claim_issue(tx, assignee, model, harness, harness_version, single_claim)?;

    let trace = factors.map(|f| build_decision_trace(f, result.bead_id.as_deref(), assignee));

    Ok((result, trace))
}

/// Intelligent claim with policy-based scheduling (R019)
///
/// This function extends claim logic to support R019's intelligent scheduling:
/// - Policy-based ranking (fifo-v1, aging-v1, impact-v1, rotation-v1, balanced-v1)
/// - Ready age promotion and bounded aging buckets
/// - Completion-unlock impact measurement
/// - Least-recently-served rotation with workspace claim sequence
/// - Failure-aware attempt tiers and retry cadence
/// - Comprehensive scheduling metrics and explainability
///
/// # Arguments
/// * `tx` - Database transaction
/// * `assignee` - Who is claiming the issue
/// * `policy` - Scheduling policy to use for selection
/// * `model` - Optional model name for telemetry
/// * `harness` - Optional harness name for telemetry
/// * `harness_version` - Optional harness version for telemetry
/// * `single_claim` - If true, refuse when the assignee already holds an
///   in_progress issue in this workspace
///
/// # Returns
/// Standard claim result enhanced with scheduling information
pub fn claim_issue_with_policy(
    tx: &Transaction,
    assignee: &str,
    policy: &SchedulingPolicy,
    model: Option<&str>,
    harness: Option<&str>,
    harness_version: Option<&str>,
    single_claim: bool,
) -> Result<ClaimResult> {
    match policy {
        SchedulingPolicy::FifoV1 => {
            // Use existing FIFO-v1 logic for backward compatibility
            claim_issue(tx, assignee, model, harness, harness_version, single_claim)
        }
        _ => {
            // Use intelligent scheduling for other policies
            intelligent_claim(
                tx,
                assignee,
                policy,
                model,
                harness,
                harness_version,
                single_claim,
            )
        }
    }
}

/// Intelligent claim with policy-based selection
fn intelligent_claim(
    tx: &Transaction,
    assignee: &str,
    policy: &SchedulingPolicy,
    _model: Option<&str>,
    _harness: Option<&str>,
    _harness_version: Option<&str>,
    single_claim: bool,
) -> Result<ClaimResult> {
    // Refuse before selecting when the single-claim guard is enabled and the
    // assignee already holds active work
    enforce_single_claim(tx, assignee, single_claim)?;

    // Increment workspace claim sequence for rotation tracking
    let workspace_sequence = scheduling::increment_workspace_sequence(tx)?;

    // Find eligible issues using the frontier definition
    let eligible_issues = find_eligible_frontier(tx)?;

    if eligible_issues.is_empty() {
        return Ok(ClaimResult {
            bead_id: None,
            assignee: assignee.to_string(),
        });
    }

    // Rank candidates using intelligent policy
    let ranked_candidates =
        scheduling::rank_candidates(tx, eligible_issues, policy, workspace_sequence)?;

    // Select the top-ranked candidate
    let selected_issue = ranked_candidates.first().cloned();

    let issue_id = match selected_issue {
        Some(id) => id,
        None => {
            return Ok(ClaimResult {
                bead_id: None,
                assignee: assignee.to_string(),
            });
        }
    };

    // Update scheduling state on claim
    scheduling::update_claim_scheduling_state(tx, &issue_id, workspace_sequence)?;

    // Transition the issue to in_progress and assign it
    let now = OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string());

    tx.execute(
        "UPDATE issues SET base_status = 'in_progress', assignee = ?1, updated_at = ?2 WHERE id = ?3",
        [&assignee as &dyn rusqlite::ToSql, &now as &dyn rusqlite::ToSql, &issue_id as &dyn rusqlite::ToSql],
    )
    .map_err(|e| {
        Error::Internal(anyhow::anyhow!("Failed to update issue for claim: {}", e))
    })?;

    // Record the claim audit event with policy information
    let event_detail = json!({
        "policy": policy.as_str(),
        "resulting_base_status": "in_progress",
        "workspace_sequence": workspace_sequence,
        "intelligent_scheduling": true
    });

    let event_detail_json = serde_json::to_string(&event_detail)
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to serialize event detail: {}", e)))?;

    let event_time = OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string());

    tx.execute(
        "INSERT INTO events (issue_id, kind, actor, time, detail) VALUES (?1, 'claimed', ?2, ?3, ?4)",
        [&issue_id as &dyn rusqlite::ToSql, &assignee as &dyn rusqlite::ToSql, &event_time as &dyn rusqlite::ToSql, &event_detail_json as &dyn rusqlite::ToSql],
    )
    .map_err(|e| {
        Error::Internal(anyhow::anyhow!("Failed to insert claim event: {}", e))
    })?;

    Ok(ClaimResult {
        bead_id: Some(issue_id),
        assignee: assignee.to_string(),
    })
}

/// Find all eligible issues from the ready frontier
///
/// The ready frontier consists of issues that are:
/// - Base status 'open'
/// - No assignee (assignee IS NULL)
/// - Not manually blocked (manual_blocked = 0)
/// - No unfinished 'blocks' dependencies
fn find_eligible_frontier(tx: &Transaction) -> Result<Vec<String>> {
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
    "#;

    let mut eligible = Vec::new();
    let mut stmt = tx.prepare(query).map_err(|e| {
        Error::Internal(anyhow::anyhow!(
            "Failed to prepare eligible frontier query: {}",
            e
        ))
    })?;

    let issue_rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| {
            Error::Internal(anyhow::anyhow!(
                "Failed to execute eligible frontier query: {}",
                e
            ))
        })?;

    for issue in issue_rows {
        eligible.push(issue?);
    }

    Ok(eligible)
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

    #[test]
    fn test_assignee_has_active_claim_reason_code() {
        // The guard's machine-readable identifier must stay stable: consumers
        // match on it to distinguish a refused claim from other exit-4 errors
        let code = ReasonCode::AssigneeHasActiveClaim.code_string();
        assert_eq!(code, "assignee_has_active_claim");
        assert_eq!(
            serde_json::to_value(ReasonCode::AssigneeHasActiveClaim).unwrap(),
            serde_json::json!("assignee_has_active_claim")
        );
    }
}

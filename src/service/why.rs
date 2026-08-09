//! Unified why explanation facade (R023)
//!
//! This module implements the comprehensive "why" explanation command
//! that provides a single entry point for understanding issue state, readiness,
//! blockers, claim-ranking factors, and legal next operations.
//!
//! Reuses domain evaluators and reason codes from R001 (decision traces) and
//! R019 (intelligent scheduling) to ensure consistency across all diagnostic interfaces.

use crate::error::{Error, Result};
use crate::service::claim::ReasonCode;
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

/// Comprehensive why explanation for an issue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhyExplanation {
    /// Issue being explained
    pub issue_id: String,

    /// Effective status (what users see)
    pub effective_status: String,

    /// Base status (what's stored)
    pub base_status: String,

    /// Whether the issue is ready for claiming
    pub is_ready: bool,

    /// Current assignment
    pub assignee: Option<String>,

    /// Manual blocking status
    pub manual_blocked: bool,

    /// Priority level
    pub priority: i64,

    /// Issue type
    pub issue_type: String,

    /// Created timestamp
    pub created_at: String,

    /// Updated timestamp
    pub updated_at: String,

    /// Closed timestamp (if applicable)
    pub closed_at: Option<String>,

    /// Active blocker analysis
    pub blockers: BlockerAnalysis,

    /// Claim ranking factors (why this issue ranks where it does)
    pub ranking_factors: RankingFactors,

    /// Legal next operations for this issue
    pub legal_operations: Vec<LegalOperation>,

    /// Additional reason codes for detailed explanation
    pub reasons: Vec<ReasonCode>,
}

/// Analysis of blocking dependencies
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockerAnalysis {
    /// Number of active blockers
    pub active_blocker_count: i64,

    /// List of active blocker issues
    pub active_blockers: Vec<BlockerDetail>,

    /// Whether any conditional blockers are inactive
    pub has_inactive_conditional_blockers: bool,

    /// Total dependency count (active + inactive)
    pub total_dependency_count: i64,
}

/// Detail for a single blocker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockerDetail {
    /// Blocker issue ID
    pub issue_id: String,

    /// Blocker title
    pub title: String,

    /// Blocker status
    pub status: String,

    /// Whether blocker is closed (finished)
    pub is_finished: bool,

    /// Whether this blocker has a conditional dependency
    pub is_conditional: bool,

    /// Condition explanation (if conditional)
    pub condition_explanation: Option<String>,
}

/// Claim ranking factors explaining why this issue ranks where it does
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RankingFactors {
    /// Effective priority after age promotion (if applicable)
    pub effective_priority: i64,

    /// Declared priority
    pub declared_priority: i64,

    /// Age in ready state (seconds)
    pub ready_age_seconds: Option<i64>,

    /// Last claim sequence number (for rotation analysis)
    pub last_claim_sequence: Option<i64>,

    /// Current attempt tier for failure-aware scheduling
    pub attempt_tier: i64,

    /// Consecutive failures count
    pub consecutive_failures: i64,

    /// Graph impact metrics (if available)
    pub graph_impact: Option<GraphImpact>,
}

/// Graph impact metrics for completion-unlock analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphImpact {
    /// Number of issues immediately unlocked by completion
    pub immediate_unlock_count: i64,

    /// Count of unique transitive descendants
    pub downstream_reach: i64,

    /// Critical path reduction score
    pub critical_path_reduction: i64,

    /// Priority distribution of unlocked issues
    pub unlocked_priorities: Vec<i64>,
}

/// Legal operation that can be performed on this issue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegalOperation {
    /// Operation name
    pub operation: String,

    /// Whether this operation is currently valid
    pub is_valid: bool,

    /// Reason why operation might be invalid
    pub invalid_reason: Option<String>,

    /// Command example for this operation
    pub command_example: Option<String>,
}

/// Generate comprehensive why explanation for an issue
pub fn explain_why(conn: &Connection, issue_id: &str) -> Result<WhyExplanation> {
    // Get issue state
    let issue = get_issue_state(conn, issue_id)?;

    // Analyze blockers
    let blockers = analyze_blockers(conn, issue_id)?;

    // Calculate effective status
    let (effective_status, is_ready) = calculate_effective_status(&issue, &blockers);

    // Get ranking factors
    let ranking_factors = get_ranking_factors(conn, issue_id)?;

    // Determine legal operations
    let legal_operations = get_legal_operations(&issue, &blockers, &ranking_factors);

    // Build reason codes
    let reasons = build_reason_codes(&issue, &blockers, is_ready, &ranking_factors);

    Ok(WhyExplanation {
        issue_id: issue_id.to_string(),
        effective_status,
        base_status: issue.base_status,
        is_ready,
        assignee: issue.assignee,
        manual_blocked: issue.manual_blocked != 0,
        priority: issue.priority,
        issue_type: issue.issue_type,
        created_at: issue.created_at,
        updated_at: issue.updated_at,
        closed_at: issue.closed_at,
        blockers,
        ranking_factors,
        legal_operations,
        reasons,
    })
}

/// Issue state snapshot from database
#[derive(Debug, Clone)]
struct IssueState {
    pub id: String,
    pub title: String,
    pub base_status: String,
    pub assignee: Option<String>,
    pub manual_blocked: i64,
    pub priority: i64,
    pub issue_type: String,
    pub created_at: String,
    pub updated_at: String,
    pub closed_at: Option<String>,
    pub ready_since: Option<String>,
    pub last_claim_sequence: Option<i64>,
    pub attempt_tier: i64,
    pub consecutive_failures: i64,
}

/// Get current issue state from database
fn get_issue_state(conn: &Connection, issue_id: &str) -> Result<IssueState> {
    conn.query_row(
        "SELECT id, title, base_status, assignee, manual_blocked, priority, issue_type, created_at, updated_at, closed_at
         FROM issues WHERE id = ?1",
        [issue_id],
        |row| {
            Ok(IssueState {
                id: row.get(0)?,
                title: row.get(1)?,
                base_status: row.get(2)?,
                assignee: row.get(3)?,
                manual_blocked: row.get(4)?,
                priority: row.get(5)?,
                issue_type: row.get(6)?,
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
                closed_at: row.get(9)?,
                // Default values for R019 fields if not present in schema
                ready_since: None,
                last_claim_sequence: None,
                attempt_tier: 0,
                consecutive_failures: 0,
            })
        },
    )
    .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to query issue state: {}", e)))
}

/// Analyze blocking dependencies
fn analyze_blockers(conn: &Connection, issue_id: &str) -> Result<BlockerAnalysis> {
    // Get all blockers (both active and inactive)
    let blockers = get_dependencies(conn, issue_id, Some("blocks"))?;

    let mut active_blockers = Vec::new();
    let mut total_count = 0;
    let mut has_inactive_conditional = false;

    for blocker in blockers {
        total_count += 1;

        // Get blocker state
        let blocker_state = get_issue_state(conn, &blocker.blocker_issue_id)?;

        let is_finished = blocker_state.base_status == "closed";
        let is_active = !is_finished;

        if is_active {
            active_blockers.push(BlockerDetail {
                issue_id: blocker.blocker_issue_id.clone(),
                title: blocker_state.title,
                status: blocker_state.base_status,
                is_finished,
                is_conditional: blocker.condition.is_some(),
                condition_explanation: blocker
                    .condition
                    .as_ref()
                    .map(|c| format!("Condition: {}", c)),
            });
        } else if blocker.condition.is_some() {
            // Inactive conditional blocker
            has_inactive_conditional = true;
        }
    }

    Ok(BlockerAnalysis {
        active_blocker_count: active_blockers.len() as i64,
        active_blockers,
        has_inactive_conditional_blockers: has_inactive_conditional,
        total_dependency_count: total_count,
    })
}

/// Calculate effective status and readiness
fn calculate_effective_status(issue: &IssueState, blockers: &BlockerAnalysis) -> (String, bool) {
    let is_blocked = blockers.active_blocker_count > 0 || issue.manual_blocked != 0;

    let effective_status = if issue.base_status == "closed" {
        "closed".to_string()
    } else if is_blocked {
        "blocked".to_string()
    } else {
        issue.base_status.clone()
    };

    let is_ready = issue.base_status == "open"
        && issue.assignee.is_none()
        && issue.manual_blocked == 0
        && blockers.active_blocker_count == 0;

    (effective_status, is_ready)
}

/// Get ranking factors for claim selection
fn get_ranking_factors(conn: &Connection, issue_id: &str) -> Result<RankingFactors> {
    let issue = get_issue_state(conn, issue_id)?;

    // Calculate ready age
    let ready_age_seconds = if let Some(ready_since) = &issue.ready_since {
        let ready_time = time::OffsetDateTime::parse(
            ready_since,
            &time::format_description::well_known::Rfc3339,
        )
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to parse ready_since: {}", e)))?;

        let now = time::OffsetDateTime::now_utc();
        Some((now - ready_time).whole_seconds().max(0))
    } else {
        None
    };

    // Get graph impact (if available from R019)
    let graph_impact = get_graph_impact(conn, issue_id)?;

    Ok(RankingFactors {
        effective_priority: issue.priority, // R019 age promotion would modify this
        declared_priority: issue.priority,
        ready_age_seconds,
        last_claim_sequence: issue.last_claim_sequence,
        attempt_tier: issue.attempt_tier,
        consecutive_failures: issue.consecutive_failures,
        graph_impact,
    })
}

/// Get graph impact metrics (R019 integration)
fn get_graph_impact(conn: &Connection, issue_id: &str) -> Result<Option<GraphImpact>> {
    // Try to get cached metrics from R019 scheduling_metrics table
    // Return None if table doesn't exist or has no data
    let metrics = conn
        .query_row(
            "SELECT downstream_reach, critical_path_reduction, immediate_unlock_count
             FROM scheduling_metrics WHERE issue_id = ?1",
            [issue_id],
            |row| {
                Ok(GraphImpact {
                    immediate_unlock_count: row.get(0)?,
                    downstream_reach: row.get(1)?,
                    critical_path_reduction: row.get(2)?,
                    unlocked_priorities: vec![], // Would require separate query
                })
            },
        )
        .optional();

    match metrics {
        Ok(opt) => Ok(opt),
        Err(_) => Ok(None), // Return None if table doesn't exist or query fails
    }
}

/// Get legal operations for current state
fn get_legal_operations(
    issue: &IssueState,
    blockers: &BlockerAnalysis,
    _ranking: &RankingFactors,
) -> Vec<LegalOperation> {
    let mut operations = Vec::new();

    // Always legal: show, list (information-only)
    operations.push(LegalOperation {
        operation: "show".to_string(),
        is_valid: true,
        invalid_reason: None,
        command_example: Some(format!("bead show {}", issue.id)),
    });

    operations.push(LegalOperation {
        operation: "list".to_string(),
        is_valid: true,
        invalid_reason: None,
        command_example: Some("bead list --ready".to_string()),
    });

    // Status-dependent operations
    match issue.base_status.as_str() {
        "open" => {
            // Can claim if ready and unassigned
            let can_claim = issue.assignee.is_none()
                && blockers.active_blocker_count == 0
                && issue.manual_blocked == 0;

            if can_claim {
                operations.push(LegalOperation {
                    operation: "claim".to_string(),
                    is_valid: true,
                    invalid_reason: None,
                    command_example: Some(format!("bead claim --assignee <worker> {}", issue.id)),
                });
            } else {
                let invalid_reason = if issue.assignee.is_some() {
                    Some("already assigned".to_string())
                } else if blockers.active_blocker_count > 0 {
                    Some("has active blockers".to_string())
                } else if issue.manual_blocked != 0 {
                    Some("manually blocked".to_string())
                } else {
                    Some("not eligible".to_string())
                };

                operations.push(LegalOperation {
                    operation: "claim".to_string(),
                    is_valid: false,
                    invalid_reason,
                    command_example: Some(format!("bead claim --assignee <worker> {}", issue.id)),
                });
            }

            // Can update, close, release, add dependencies
            operations.push(LegalOperation {
                operation: "update".to_string(),
                is_valid: true,
                invalid_reason: None,
                command_example: Some(format!("bead update {} --status in_progress", issue.id)),
            });

            operations.push(LegalOperation {
                operation: "close".to_string(),
                is_valid: true,
                invalid_reason: None,
                command_example: Some(format!("bead close {} --reason \"Completed\"", issue.id)),
            });

            operations.push(LegalOperation {
                operation: "release".to_string(),
                is_valid: false, // only valid for in_progress
                invalid_reason: Some("not in progress".to_string()),
                command_example: Some(format!("bead release {}", issue.id)),
            });
        }
        "in_progress" => {
            // Can release, update, close
            operations.push(LegalOperation {
                operation: "release".to_string(),
                is_valid: true,
                invalid_reason: None,
                command_example: Some(format!("bead release {}", issue.id)),
            });

            operations.push(LegalOperation {
                operation: "update".to_string(),
                is_valid: true,
                invalid_reason: None,
                command_example: Some(format!("bead update {} --notes \"Progress\"", issue.id)),
            });

            operations.push(LegalOperation {
                operation: "close".to_string(),
                is_valid: true,
                invalid_reason: None,
                command_example: Some(format!("bead close {} --reason \"Completed\"", issue.id)),
            });
        }
        "closed" => {
            // Can reopen
            operations.push(LegalOperation {
                operation: "reopen".to_string(),
                is_valid: true,
                invalid_reason: None,
                command_example: Some(format!("bead reopen {}", issue.id)),
            });

            // Cannot do most lifecycle operations on closed issues
            for op in ["update", "close", "release"] {
                operations.push(LegalOperation {
                    operation: op.to_string(),
                    is_valid: false,
                    invalid_reason: Some("issue is closed".to_string()),
                    command_example: Some(format!("bead {op} {}", issue.id)),
                });
            }
        }
        "deferred" => {
            // Can reopen to open, update, close
            operations.push(LegalOperation {
                operation: "reopen".to_string(),
                is_valid: true,
                invalid_reason: None,
                command_example: Some(format!("bead update {} --status open", issue.id)),
            });

            operations.push(LegalOperation {
                operation: "close".to_string(),
                is_valid: true,
                invalid_reason: None,
                command_example: Some(format!("bead close {} --reason \"Completed\"", issue.id)),
            });
        }
        _ => {}
    }

    operations
}

/// Build reason codes for explanation
fn build_reason_codes(
    issue: &IssueState,
    blockers: &BlockerAnalysis,
    is_ready: bool,
    ranking: &RankingFactors,
) -> Vec<ReasonCode> {
    let mut reasons = Vec::new();

    // Add readiness reason
    if is_ready {
        reasons.push(ReasonCode::EligibleSelected);
    } else {
        if !issue.assignee.is_none() {
            reasons.push(ReasonCode::AlreadyAssigned);
        }
        if blockers.active_blocker_count > 0 {
            reasons.push(ReasonCode::HasUnfinishedBlockers);
        }
        if issue.manual_blocked != 0 {
            reasons.push(ReasonCode::ManuallyBlocked);
        }
        if issue.base_status != "open" {
            reasons.push(ReasonCode::NotOpenStatus);
        }
    }

    // Add failure state reason
    if ranking.consecutive_failures > 0 {
        // Would add a failure reason code if we had one
    }

    reasons
}

/// Dependency record from database
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct DependencyRecord {
    blocker_issue_id: String,
    blocked_issue_id: String,
    kind: String,
    condition: Option<String>,
}

/// Get dependencies for a specific issue from the database
fn get_dependencies(
    conn: &Connection,
    blocked_issue_id: &str,
    kind_filter: Option<&str>,
) -> Result<Vec<DependencyRecord>> {
    let mut dependencies = Vec::new();

    if let Some(kind) = kind_filter {
        let mut stmt = conn.prepare(
            "SELECT blocker_issue_id, blocked_issue_id, kind, condition
             FROM dependencies
             WHERE blocked_issue_id = ?1 AND kind = ?2",
        )?;

        let rows = stmt.query_map([blocked_issue_id, kind], |row| {
            Ok(DependencyRecord {
                blocker_issue_id: row.get(0)?,
                blocked_issue_id: row.get(1)?,
                kind: row.get(2)?,
                condition: row.get(3)?,
            })
        })?;

        for row in rows {
            dependencies.push(row?);
        }
    } else {
        let mut stmt = conn.prepare(
            "SELECT blocker_issue_id, blocked_issue_id, kind, condition
             FROM dependencies
             WHERE blocked_issue_id = ?1",
        )?;

        let rows = stmt.query_map([blocked_issue_id], |row| {
            Ok(DependencyRecord {
                blocker_issue_id: row.get(0)?,
                blocked_issue_id: row.get(1)?,
                kind: row.get(2)?,
                condition: row.get(3)?,
            })
        })?;

        for row in rows {
            dependencies.push(row?);
        }
    }

    Ok(dependencies)
}

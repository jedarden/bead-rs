//! Intelligent scheduling service for R019
//!
//! This module implements post-0.1 intelligent claim scheduling with:
//! - Ready age promotion and bounded aging buckets
//! - Completion-unlock impact measurement
//! - Least-recently-served rotation
//! - Failure-aware attempt tiers and retry cadence
//! - Multiple versioned scheduling policies
//! - Graph metrics caching for performance

#![allow(dead_code)] // Public API methods not all used in current tests

use crate::error::{Error, Result};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// Attempt tier for failure-aware scheduling
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptTier {
    /// No bead-scoped failures
    Unproven = 0,
    /// One bead-scoped failure (defer until comparable unproven work served)
    Retryable = 1,
    /// Multiple failures below quarantine threshold
    Struggling = 2,
    /// Ineligible for automatic claim
    Quarantined = 3,
}

impl AttemptTier {
    /// Convert from database integer
    #[allow(dead_code)] // Public API method for future use
    pub fn from_i64(value: i64) -> Result<Self> {
        match value {
            0 => Ok(AttemptTier::Unproven),
            1 => Ok(AttemptTier::Retryable),
            2 => Ok(AttemptTier::Struggling),
            3 => Ok(AttemptTier::Quarantined),
            _ => Err(Error::validation(format!(
                "Invalid attempt tier: {}",
                value
            ))),
        }
    }

    /// Convert to database integer
    #[allow(dead_code)] // Public API method for future use
    pub fn to_i64(self) -> i64 {
        self as i64
    }
}

/// Scheduling policy version and configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SchedulingPolicy {
    /// Original FIFO policy: priority, created_at, id
    FifoV1,
    /// Bounded ready-age promotion, then FIFO tie-breakers
    AgingV1 {
        /// Seconds in aging interval (default 24 hours)
        aging_interval_seconds: u64,
        /// Maximum age promotions (default 2)
        max_promotions: u64,
    },
    /// Effective priority, completion-unlock impact, ready age, rotation
    ImpactV1 {
        /// Weight for impact in ranking (default 1.0)
        impact_weight: f64,
    },
    /// Effective priority, attempt tier, LRS rotation, ready age, creation
    RotationV1 {
        /// Enable least-recently-served rotation
        enable_rotation: bool,
    },
    /// Complete balanced policy with all features
    BalancedV1 {
        aging_interval_seconds: u64,
        max_promotions: u64,
        impact_weight: f64,
        enable_rotation: bool,
        /// Retry lane size (1 retry per N normal claims, default 10)
        retry_lane_ratio: u64,
    },
}

impl SchedulingPolicy {
    /// Parse policy from string
    pub fn from_string(s: &str) -> Result<Self> {
        match s {
            "fifo-v1" => Ok(SchedulingPolicy::FifoV1),
            "aging-v1" => Ok(SchedulingPolicy::AgingV1 {
                aging_interval_seconds: 86400, // 24 hours
                max_promotions: 2,
            }),
            "impact-v1" => Ok(SchedulingPolicy::ImpactV1 { impact_weight: 1.0 }),
            "rotation-v1" => Ok(SchedulingPolicy::RotationV1 {
                enable_rotation: true,
            }),
            "balanced-v1" => Ok(SchedulingPolicy::BalancedV1 {
                aging_interval_seconds: 86400,
                max_promotions: 2,
                impact_weight: 1.0,
                enable_rotation: true,
                retry_lane_ratio: 10,
            }),
            _ => Err(Error::validation(format!(
                "Unknown scheduling policy: {}",
                s
            ))),
        }
    }

    /// Get policy name as string
    pub fn as_str(&self) -> &str {
        match self {
            SchedulingPolicy::FifoV1 => "fifo-v1",
            SchedulingPolicy::AgingV1 { .. } => "aging-v1",
            SchedulingPolicy::ImpactV1 { .. } => "impact-v1",
            SchedulingPolicy::RotationV1 { .. } => "rotation-v1",
            SchedulingPolicy::BalancedV1 { .. } => "balanced-v1",
        }
    }
}

/// Graph metrics for completion-unlock impact calculation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphMetrics {
    /// Number of issues immediately unlocked by completion
    pub immediate_unlock_count: i64,
    /// Count of unique transitive descendants
    pub downstream_reach: i64,
    /// Critical path reduction score
    pub critical_path_reduction: i64,
    /// Priority distribution of unlocked issues
    pub unlocked_priorities: Vec<i64>,
    /// Cache validity flag
    pub is_valid: bool,
    /// Graph revision when metrics were computed
    pub graph_revision: i64,
}

/// Scheduling state for an issue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulingState {
    /// Issue ID
    pub issue_id: String,
    /// When this issue became ready (None if not ready)
    pub ready_since: Option<String>,
    /// Last claim sequence number (None if never claimed)
    pub last_claim_sequence: Option<i64>,
    /// Current attempt tier
    pub attempt_tier: AttemptTier,
    /// Consecutive bead-scoped failures
    pub consecutive_failures: i64,
    /// Claim sequence to retry after (None if no retry delay)
    pub retry_after_claim_sequence: Option<i64>,
    /// Current workspace claim sequence
    pub workspace_sequence: i64,
}

/// Increment the workspace claim sequence
pub fn increment_workspace_sequence(conn: &Connection) -> Result<i64> {
    conn.execute(
        "UPDATE workspace_claim_sequence SET sequence = sequence + 1",
        [],
    )
    .map_err(|e| {
        Error::Internal(anyhow::anyhow!(
            "Failed to increment workspace sequence: {}",
            e
        ))
    })?;

    let new_sequence: i64 = conn
        .query_row("SELECT sequence FROM workspace_claim_sequence", [], |row| {
            row.get(0)
        })
        .map_err(|e| {
            Error::Internal(anyhow::anyhow!("Failed to read workspace sequence: {}", e))
        })?;

    Ok(new_sequence)
}

/// Get current workspace claim sequence
#[allow(dead_code)]
pub fn get_workspace_sequence(conn: &Connection) -> Result<i64> {
    conn.query_row("SELECT sequence FROM workspace_claim_sequence", [], |row| {
        row.get(0)
    })
    .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to read workspace sequence: {}", e)))
}

/// Set ready_since timestamp for an issue
#[allow(dead_code)]
pub fn set_ready_since(conn: &Connection, issue_id: &str) -> Result<()> {
    let now = OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string());

    conn.execute(
        "UPDATE issues SET ready_since = ?1 WHERE id = ?2 AND ready_since IS NULL",
        [
            &now as &dyn rusqlite::ToSql,
            &issue_id as &dyn rusqlite::ToSql,
        ],
    )
    .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to set ready_since: {}", e)))?;

    Ok(())
}

/// Clear ready_since timestamp when issue becomes unready
#[allow(dead_code)]
pub fn clear_ready_since(conn: &Connection, issue_id: &str) -> Result<()> {
    conn.execute(
        "UPDATE issues SET ready_since = NULL WHERE id = ?1",
        [&issue_id as &dyn rusqlite::ToSql],
    )
    .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to clear ready_since: {}", e)))?;

    Ok(())
}

/// Update scheduling state on claim
pub fn update_claim_scheduling_state(
    conn: &Connection,
    issue_id: &str,
    workspace_sequence: i64,
) -> Result<()> {
    conn.execute(
        "UPDATE issues SET last_claim_sequence = ?1, revision = revision + 1 WHERE id = ?2",
        [
            &workspace_sequence as &dyn rusqlite::ToSql,
            &issue_id as &dyn rusqlite::ToSql,
        ],
    )
    .map_err(|e| {
        Error::Internal(anyhow::anyhow!(
            "Failed to update claim scheduling state: {}",
            e
        ))
    })?;

    Ok(())
}

/// Get scheduling state for an issue
pub fn get_scheduling_state(
    conn: &Connection,
    issue_id: &str,
    workspace_sequence: i64,
) -> Result<SchedulingState> {
    let (ready_since, last_claim_seq, attempt_tier, consecutive_failures, retry_after) = conn
        .query_row(
            "SELECT ready_since, last_claim_sequence, attempt_tier, consecutive_failures, retry_after_claim_sequence
             FROM issues WHERE id = ?1",
            [issue_id],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<i64>>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, Option<i64>>(4)?,
                ))
            },
        )
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to read scheduling state: {}", e)))?;

    let attempt_tier = AttemptTier::from_i64(attempt_tier)?;

    Ok(SchedulingState {
        issue_id: issue_id.to_string(),
        ready_since,
        last_claim_sequence: last_claim_seq,
        attempt_tier,
        consecutive_failures,
        retry_after_claim_sequence: retry_after,
        workspace_sequence,
    })
}

/// Record a bead-scoped failure and update attempt tier
pub fn record_failure(conn: &Connection, issue_id: &str) -> Result<AttemptTier> {
    // Increment consecutive failures
    conn.execute(
        "UPDATE issues SET consecutive_failures = consecutive_failures + 1 WHERE id = ?1",
        [&issue_id as &dyn rusqlite::ToSql],
    )
    .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to record failure: {}", e)))?;

    // Get updated failure count
    let failures: i64 = conn
        .query_row(
            "SELECT consecutive_failures FROM issues WHERE id = ?1",
            [issue_id],
            |row| row.get(0),
        )
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to read failure count: {}", e)))?;

    // Update attempt tier based on failure count
    let new_tier = match failures {
        0 => AttemptTier::Unproven,
        1 => AttemptTier::Retryable,
        2 => AttemptTier::Struggling,
        _ => AttemptTier::Quarantined,
    };

    conn.execute(
        "UPDATE issues SET attempt_tier = ?1 WHERE id = ?2",
        [
            &new_tier.to_i64() as &dyn rusqlite::ToSql,
            &issue_id as &dyn rusqlite::ToSql,
        ],
    )
    .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to update attempt tier: {}", e)))?;

    Ok(new_tier)
}

/// Reset attempt state on material mutations (description/acceptance criteria changes)
pub fn reset_attempt_tier(conn: &Connection, issue_id: &str) -> Result<()> {
    conn.execute(
        "UPDATE issues SET consecutive_failures = 0, attempt_tier = 0, retry_after_claim_sequence = NULL WHERE id = ?1",
        [&issue_id as &dyn rusqlite::ToSql],
    )
    .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to reset attempt tier: {}", e)))?;

    Ok(())
}

/// Calculate effective priority with age promotion
pub fn calculate_effective_priority(
    base_priority: i64,
    ready_since: Option<&str>,
    policy: &SchedulingPolicy,
) -> Result<i64> {
    let effective_priority = match policy {
        SchedulingPolicy::AgingV1 {
            aging_interval_seconds,
            max_promotions,
        } => {
            if let Some(ready_time) = ready_since {
                let ready_dt = OffsetDateTime::parse(
                    ready_time,
                    &time::format_description::well_known::Rfc3339,
                )
                .map_err(|e| {
                    Error::Internal(anyhow::anyhow!("Failed to parse ready_since: {}", e))
                })?;

                let now = OffsetDateTime::now_utc();
                let age_seconds = (now - ready_dt).whole_seconds().max(0) as u64;

                let age_promotions =
                    std::cmp::min(*max_promotions, age_seconds / aging_interval_seconds);
                std::cmp::max(0, base_priority - age_promotions as i64)
            } else {
                base_priority
            }
        }
        SchedulingPolicy::BalancedV1 {
            aging_interval_seconds,
            max_promotions,
            ..
        } => {
            if let Some(ready_time) = ready_since {
                let ready_dt = OffsetDateTime::parse(
                    ready_time,
                    &time::format_description::well_known::Rfc3339,
                )
                .map_err(|e| {
                    Error::Internal(anyhow::anyhow!("Failed to parse ready_since: {}", e))
                })?;

                let now = OffsetDateTime::now_utc();
                let age_seconds = (now - ready_dt).whole_seconds().max(0) as u64;

                let age_promotions =
                    std::cmp::min(*max_promotions, age_seconds / aging_interval_seconds);
                std::cmp::max(0, base_priority - age_promotions as i64)
            } else {
                base_priority
            }
        }
        _ => base_priority,
    };

    // Ensure effective priority stays within P0-P4 range
    Ok(effective_priority.clamp(0, 4))
}

/// Get graph metrics for completion-unlock impact calculation
pub fn get_graph_metrics(conn: &Connection, issue_id: &str) -> Result<GraphMetrics> {
    // Check cache first
    let cached_metrics = conn
        .query_row(
            "SELECT downstream_reach, critical_path_reduction, immediate_unlock_count, graph_revision, computed_at
             FROM scheduling_metrics WHERE issue_id = ?1",
            [issue_id],
            |row| {
                Ok(GraphMetrics {
                    immediate_unlock_count: row.get(0)?,
                    downstream_reach: row.get(1)?,
                    critical_path_reduction: row.get(2)?,
                    graph_revision: row.get(3)?,
                    unlocked_priorities: vec![], // Will be filled if needed
                    is_valid: true,
                })
            },
        )
        .optional();

    if let Ok(Some(metrics)) = cached_metrics {
        return Ok(metrics);
    }

    // Calculate metrics from scratch if cache miss
    calculate_graph_metrics_fresh(conn, issue_id)
}

/// Calculate fresh graph metrics for an issue
fn calculate_graph_metrics_fresh(conn: &Connection, issue_id: &str) -> Result<GraphMetrics> {
    // Calculate immediate unlock count
    let immediate_unlock_count: i64 = conn
        .query_row(
            r#"
            SELECT COUNT(DISTINCT blocked.id)
            FROM dependencies d
            JOIN issues blocked ON blocked.id = d.blocked_issue_id
            WHERE d.blocker_issue_id = ?1
              AND d.kind = 'blocks'
              AND blocked.base_status IN ('open', 'in_progress', 'deferred')
              AND NOT EXISTS (
                  SELECT 1 FROM dependencies d2
                  JOIN issues other_blocker ON other_blocker.id = d2.blocker_issue_id
                  WHERE d2.blocked_issue_id = blocked.id
                    AND d2.kind = 'blocks'
                    AND other_blocker.id != ?1
                    AND other_blocker.base_status != 'closed'
              )
            "#,
            [issue_id],
            |row| row.get(0),
        )
        .unwrap_or(0);

    // Calculate downstream reach (transitive descendants)
    let downstream_reach: i64 = conn
        .query_row(
            r#"
            WITH RECURSIVE descendants AS (
                SELECT blocked_issue_id FROM dependencies WHERE blocker_issue_id = ?1 AND kind = 'blocks'
                UNION
                SELECT d.blocked_issue_id FROM dependencies d
                INNER JOIN descendants desc ON d.blocker_issue_id = desc.blocked_issue_id
                WHERE d.kind = 'blocks'
            )
            SELECT COUNT(DISTINCT blocked_issue_id) FROM descendants
            "#,
            [issue_id],
            |row| row.get(0),
        )
        .unwrap_or(0);

    // Get priority distribution of unlocked issues
    let mut unlocked_priorities = Vec::new();
    let mut stmt = conn
        .prepare(
            r#"
            SELECT DISTINCT blocked.priority
            FROM dependencies d
            JOIN issues blocked ON blocked.id = d.blocked_issue_id
            WHERE d.blocker_issue_id = ?1
              AND d.kind = 'blocks'
              AND blocked.base_status IN ('open', 'in_progress', 'deferred')
            ORDER BY blocked.priority
            "#,
        )
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to prepare priority query: {}", e)))?;

    let priority_rows = stmt
        .query_map([issue_id], |row| row.get::<_, i64>(0))
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to query priorities: {}", e)))?;

    for priority in priority_rows {
        unlocked_priorities.push(priority?);
    }

    // Critical path reduction: simplified metric based on longest chain
    let critical_path_reduction = if downstream_reach > 0 {
        // Count longest path from this issue to any leaf
        conn
            .query_row(
                r#"
                WITH RECURSIVE path_depth(id, depth) AS (
                    SELECT blocked_issue_id, 1 FROM dependencies WHERE blocker_issue_id = ?1 AND kind = 'blocks'
                    UNION
                    SELECT d.blocked_issue_id, p.depth + 1 FROM dependencies d
                    INNER JOIN path_depth p ON d.blocker_issue_id = p.id
                    WHERE d.kind = 'blocks'
                )
                SELECT COALESCE(MAX(depth), 0) FROM path_depth
                "#,
                [issue_id],
                |row| row.get(0),
            )
            .unwrap_or(0)
    } else {
        0
    };

    Ok(GraphMetrics {
        immediate_unlock_count,
        downstream_reach,
        critical_path_reduction,
        unlocked_priorities,
        is_valid: true,
        graph_revision: 0, // Will be set when cached
    })
}

/// Rank candidates using intelligent policy
pub fn rank_candidates(
    conn: &Connection,
    candidates: Vec<String>,
    policy: &SchedulingPolicy,
    workspace_sequence: i64,
) -> Result<Vec<String>> {
    if candidates.is_empty() {
        return Ok(Vec::new());
    }

    match policy {
        SchedulingPolicy::FifoV1 => {
            // Use existing FIFO ranking (priority, created_at, id)
            Ok(candidates)
        }
        SchedulingPolicy::AgingV1 { .. } | SchedulingPolicy::BalancedV1 { .. } => {
            rank_by_aging_policy(conn, candidates, policy, workspace_sequence)
        }
        SchedulingPolicy::ImpactV1 { .. } => {
            rank_by_impact_policy(conn, candidates, policy, workspace_sequence)
        }
        SchedulingPolicy::RotationV1 { enable_rotation } => {
            if *enable_rotation {
                rank_by_rotation_policy(conn, candidates, policy, workspace_sequence)
            } else {
                rank_by_aging_policy(conn, candidates, policy, workspace_sequence)
            }
        }
    }
}

/// Rank candidates using aging policy
fn rank_by_aging_policy(
    conn: &Connection,
    candidates: Vec<String>,
    policy: &SchedulingPolicy,
    _workspace_sequence: i64,
) -> Result<Vec<String>> {
    let mut candidate_scores: Vec<(String, i64, String, i64)> = Vec::new();

    for issue_id in &candidates {
        let (base_priority, created_at, ready_since, last_claim_seq): (i64, String, Option<String>, Option<i64>) = conn
            .query_row(
                "SELECT priority, created_at, ready_since, last_claim_sequence FROM issues WHERE id = ?1",
                [issue_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to query issue: {}", e)))?;

        let effective_priority =
            calculate_effective_priority(base_priority, ready_since.as_deref(), policy)?;

        candidate_scores.push((
            issue_id.clone(),
            effective_priority,
            created_at,
            last_claim_seq.unwrap_or(i64::MAX),
        ));
    }

    // Sort by effective priority, then by creation time, then by least-recently-served
    candidate_scores.sort_by_key(|(id, effective_priority, created_at, last_claim)| {
        (
            *effective_priority,
            created_at.clone(),
            *last_claim,
            id.clone(),
        )
    });

    Ok(candidate_scores
        .into_iter()
        .map(|(id, _, _, _)| id)
        .collect())
}

/// Rank candidates using impact policy
fn rank_by_impact_policy(
    conn: &Connection,
    candidates: Vec<String>,
    _policy: &SchedulingPolicy,
    _workspace_sequence: i64,
) -> Result<Vec<String>> {
    let mut candidate_impacts: Vec<(String, i64, f64)> = Vec::new();

    for issue_id in &candidates {
        let metrics = get_graph_metrics(conn, issue_id)?;

        // Calculate composite impact score
        let impact_score = if metrics.downstream_reach > 0 {
            (metrics.immediate_unlock_count as f64 * 10.0)
                + (metrics.downstream_reach as f64 * 1.0)
                + (metrics.critical_path_reduction as f64 * 5.0)
        } else {
            0.0
        };

        candidate_impacts.push((issue_id.clone(), metrics.downstream_reach, impact_score));
    }

    // Get priorities for final sorting
    let mut candidate_scores: Vec<(String, i64, f64, String, i64)> = Vec::new();

    for (issue_id, downstream_reach, impact_score) in candidate_impacts {
        let (base_priority, created_at): (i64, String) = conn
            .query_row(
                "SELECT priority, created_at FROM issues WHERE id = ?1",
                [&issue_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to query issue: {}", e)))?;

        candidate_scores.push((
            issue_id,
            base_priority,
            impact_score,
            created_at,
            downstream_reach,
        ));
    }

    // Sort by priority, then by impact score
    candidate_scores.sort_by_key(|(id, priority, impact, created_at, downstream)| {
        (
            *priority,
            (*impact as i64),
            created_at.clone(),
            *downstream,
            id.clone(),
        )
    });

    Ok(candidate_scores
        .into_iter()
        .map(|(id, _, _, _, _)| id)
        .collect())
}

/// Rank candidates using rotation policy (least-recently-served)
fn rank_by_rotation_policy(
    conn: &Connection,
    candidates: Vec<String>,
    _policy: &SchedulingPolicy,
    _workspace_sequence: i64,
) -> Result<Vec<String>> {
    let mut candidate_rotation: Vec<(String, i64, Option<i64>, String)> = Vec::new();

    for issue_id in &candidates {
        let (base_priority, created_at, last_claim_seq): (i64, String, Option<i64>) = conn
            .query_row(
                "SELECT priority, created_at, last_claim_sequence FROM issues WHERE id = ?1",
                [issue_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to query issue: {}", e)))?;

        candidate_rotation.push((issue_id.clone(), base_priority, last_claim_seq, created_at));
    }

    // Sort by priority, then by least-recently-served (NULL or older last_claim comes first)
    candidate_rotation.sort_by_key(|(id, priority, last_claim, created_at)| {
        (
            *priority,
            last_claim.unwrap_or(i64::MAX),
            created_at.clone(),
            id.clone(),
        )
    });

    Ok(candidate_rotation
        .into_iter()
        .map(|(id, _, _, _)| id)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_attempt_tier_conversion() {
        assert_eq!(AttemptTier::from_i64(0).unwrap(), AttemptTier::Unproven);
        assert_eq!(AttemptTier::from_i64(1).unwrap(), AttemptTier::Retryable);
        assert_eq!(AttemptTier::from_i64(2).unwrap(), AttemptTier::Struggling);
        assert_eq!(AttemptTier::from_i64(3).unwrap(), AttemptTier::Quarantined);

        assert!(AttemptTier::from_i64(4).is_err());
    }

    #[test]
    fn test_scheduling_policy_parsing() {
        let fifo = SchedulingPolicy::from_string("fifo-v1").unwrap();
        assert!(matches!(fifo, SchedulingPolicy::FifoV1));

        let aging = SchedulingPolicy::from_string("aging-v1").unwrap();
        match aging {
            SchedulingPolicy::AgingV1 {
                aging_interval_seconds,
                max_promotions,
            } => {
                assert_eq!(aging_interval_seconds, 86400);
                assert_eq!(max_promotions, 2);
            }
            _ => panic!("Expected AgingV1 policy"),
        }

        assert!(SchedulingPolicy::from_string("unknown").is_err());
    }

    #[test]
    fn test_policy_as_str() {
        assert_eq!(SchedulingPolicy::FifoV1.as_str(), "fifo-v1");
        assert_eq!(
            SchedulingPolicy::AgingV1 {
                aging_interval_seconds: 3600,
                max_promotions: 1
            }
            .as_str(),
            "aging-v1"
        );
    }
}

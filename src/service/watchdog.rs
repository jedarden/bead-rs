//! Watchdog service for monitoring and auto-releasing stale bead claims
//!
//! This module provides the watchdog functionality that:
//! - Scans for in_progress beads exceeding a time threshold
//! - Uses store-native lease expiry detection (R002 fencing), not process-name search
//! - Automatically releases beads with expired leases
//! - Logs all actions to .beads/watchdog-releases.jsonl
//!
//! The watchdog does NOT use process-name search heuristics. Detection is based
//! exclusively on lease expiry for leased claims, and time-based threshold for
//! non-leased claims (recommendation-only).

use crate::error::{Error, Result};
use crate::model::BaseStatus;
use chrono::{DateTime, Duration, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

/// Watchdog configuration
#[derive(Debug, Clone)]
pub struct WatchdogConfig {
    /// Maximum age of an in_progress bead before it's considered stale
    pub threshold: Duration,
    /// Whether to actually release beads (false = dry-run)
    pub force: bool,
    /// Path to the watchdog releases log
    pub log_path: PathBuf,
}

/// Result of a watchdog scan
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchdogResult {
    /// Total in_progress beads scanned
    pub total_scanned: usize,
    /// Beads considered stale
    pub stale_beads: Vec<StaleBead>,
    /// Beads that were auto-released
    pub released_beads: Vec<ReleasedBead>,
    /// Beads where lease is still valid but no progress (recommendation-only)
    pub lease_valid_but_stale: Vec<StaleBead>,
}

/// A stale bead (old claim)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaleBead {
    /// Bead ID
    pub id: String,
    /// Current assignee
    pub assignee: String,
    /// Time since last update
    pub hours_since_update: f64,
    /// Last update timestamp
    pub updated_at: String,
}

/// A bead that was auto-released
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleasedBead {
    /// Bead ID
    pub id: String,
    /// Previous assignee
    pub assignee: String,
    /// Time since last update
    pub hours_since_update: f64,
    /// Timestamp of release
    pub released_at: String,
    /// Reason for release
    pub reason: String,
}

/// Parse a duration string (e.g., "4h", "2h30m", "8h")
fn parse_duration(s: &str) -> Result<Duration> {
    let s = s.trim().to_lowercase();

    // Parse hours (e.g., "4h", "4")
    if s.ends_with('h') {
        let hours: i64 = s[..s.len() - 1]
            .parse()
            .map_err(|_| Error::validation(format!("Invalid duration: {s}")))?;
        return Ok(Duration::hours(hours));
    }

    // Parse minutes (e.g., "120m")
    if s.ends_with('m') && !s.ends_with("ms") {
        let minutes: i64 = s[..s.len() - 1]
            .parse()
            .map_err(|_| Error::validation(format!("Invalid duration: {s}")))?;
        return Ok(Duration::minutes(minutes));
    }

    // Try as raw number (assume hours)
    let hours: i64 = s.parse().map_err(|_| {
        Error::validation(format!(
            "Invalid duration: {s} (use format like '4h' or '240m')"
        ))
    })?;
    Ok(Duration::hours(hours))
}

/// Check if a claim has an expired lease (R002 store-native fencing)
///
/// This replaces the heuristic process-name search with store-native lease expiry:
/// - For leased claims: lease expiry is authoritative, no process search needed
/// - For non-leased claims: returns false (no fence to enforce)
///
/// The watchdog now only detects stale claims by their lease state, not by
/// searching the process table for matching names.
fn claim_has_expired_lease(conn: &Connection, issue_id: &str, assignee: &str) -> bool {
    use rusqlite::params;

    // Check if this issue has any lease rows at all
    let has_lease_row: i64 = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM leases WHERE issue_id = ?1)",
            params![issue_id],
            |row| row.get(0),
        )
        .unwrap_or(0);

    if has_lease_row == 0 {
        // Non-leased claims have no lease-based fence to enforce
        return false;
    }

    // For leased claims, check if a valid active lease exists for this assignee
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);

    conn.query_row(
        "SELECT EXISTS(
                 SELECT 1
                 FROM leases
                 WHERE issue_id = ?1
                   AND assignee = ?2
                   AND expires_at > ?3
             )",
        params![issue_id, assignee, now],
        |row| row.get::<_, i64>(0),
    )
    .unwrap_or(0)
        == 0
}

/// Log a watchdog action to the releases log
fn log_action(log_path: &PathBuf, action: &ReleasedBead) -> Result<()> {
    let json = serde_json::to_string(action)
        .map_err(|e| Error::integrity(format!("Failed to serialize release: {e}")))?;

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .map_err(|e| Error::integrity(format!("Failed to open watchdog log: {e}")))?;

    writeln!(file, "{}", json)
        .map_err(|e| Error::integrity(format!("Failed to write to watchdog log: {e}")))?;

    Ok(())
}

/// Get all in_progress beads
fn get_in_progress_beads(conn: &Connection) -> Result<Vec<IssueRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, title, assignee, updated_at, base_status
         FROM issues
         WHERE base_status = ?",
    )?;

    let issues = stmt
        .query_map([BaseStatus::InProgress.to_string()], |row| {
            Ok(IssueRow {
                id: row.get(0)?,
                title: row.get(1)?,
                assignee: row.get(2)?,
                updated_at: row.get(3)?,
                base_status: row.get(4)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| Error::integrity(format!("Failed to fetch issues: {e}")))?;

    Ok(issues)
}

/// Internal Issue row structure
#[derive(Debug, Clone)]
struct IssueRow {
    id: String,
    title: String,
    assignee: Option<String>,
    updated_at: String,
    base_status: String,
}

/// Run watchdog scan
pub fn run_watchdog(conn: &Connection, config: WatchdogConfig) -> Result<WatchdogResult> {
    let in_progress = get_in_progress_beads(conn)?;
    let mut result = WatchdogResult {
        total_scanned: in_progress.len(),
        stale_beads: vec![],
        released_beads: vec![],
        lease_valid_but_stale: vec![],
    };

    let now = Utc::now();
    let threshold = config.threshold;

    for issue in in_progress {
        let assignee = match &issue.assignee {
            Some(a) => a.clone(),
            None => continue, // Skip unassigned in_progress beads (shouldn't happen)
        };

        // Parse updated_at timestamp
        let updated_at: DateTime<Utc> = issue
            .updated_at
            .parse()
            .map_err(|e| Error::integrity(format!("Invalid updated_at for {}: {}", issue.id, e)))?;

        let time_since_update = now.signed_duration_since(updated_at);
        let hours_since_update =
            time_since_update.num_milliseconds() as f64 / (1000.0 * 60.0 * 60.0);

        // Check if bead is stale by threshold
        if time_since_update > threshold {
            let stale_bead = StaleBead {
                id: issue.id.clone(),
                assignee: assignee.clone(),
                hours_since_update,
                updated_at: issue.updated_at.clone(),
            };
            result.stale_beads.push(stale_bead.clone());

            // Check if the claim has an expired lease (store-native fencing)
            let lease_expired = claim_has_expired_lease(conn, &issue.id, &assignee);

            if lease_expired || config.force {
                // Lease expired or force mode - release the bead
                if !config.force {
                    // Release only if not in dry-run mode
                    match crate::service::lifecycle::release_issue(conn, &issue.id, None, None) {
                        Ok(_) => {
                            let released = ReleasedBead {
                                id: issue.id.clone(),
                                assignee: assignee.clone(),
                                hours_since_update,
                                released_at: now.to_rfc3339(),
                                reason: if lease_expired {
                                    format!("Lease expired for assignee '{}'", assignee)
                                } else {
                                    "Force release (--force)".to_string()
                                },
                            };

                            log_action(&config.log_path, &released)?;
                            result.released_beads.push(released);
                        }
                        Err(e) => {
                            eprintln!("Failed to release {}: {}", issue.id, e);
                        }
                    }
                } else {
                    // Dry-run mode - would have released
                    let released = ReleasedBead {
                        id: issue.id.clone(),
                        assignee: assignee.clone(),
                        hours_since_update,
                        released_at: now.to_rfc3339(),
                        reason: "[DRY-RUN] Would release".to_string(),
                    };
                    result.released_beads.push(released);
                }
            } else {
                // Lease is still valid but bead is stale (recommendation-only)
                result.lease_valid_but_stale.push(stale_bead);
            }
        }
    }

    Ok(result)
}

/// Create watchdog config from CLI options
pub fn config_from_options(
    threshold_str: &str,
    force: bool,
    workspace_root: &PathBuf,
) -> Result<WatchdogConfig> {
    let threshold = parse_duration(threshold_str)?;
    let log_path = workspace_root
        .join(".beads")
        .join("watchdog-releases.jsonl");

    Ok(WatchdogConfig {
        threshold,
        force,
        log_path,
    })
}

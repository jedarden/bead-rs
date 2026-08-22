//! Lease service for fenced claim operations
//!
//! This module implements R002's fenced claim leases with expiring claims,
//! renewals, and monotonically increasing fencing tokens for safe recovery from
//! crashed or disconnected agents.

use crate::error::Result;
use rusqlite::{OptionalExtension, Transaction};
use time::OffsetDateTime;

/// Lease information for a claimed issue
#[derive(Debug, Clone, serde::Serialize)]
pub struct Lease {
    pub issue_id: String,
    pub assignee: String,
    pub fencing_token: i64,
    pub expires_at: String,
    pub renewed_at: Option<String>,
    pub created_at: String,
}

/// Lease result for claim operations
#[derive(Debug, Clone, serde::Serialize)]
pub struct LeaseClaimResult {
    pub issue_id: String,
    pub assignee: String,
    pub fencing_token: i64,
    pub expires_at: String,
}

/// Default lease TTL in seconds (5 minutes)
pub const DEFAULT_LEASE_TTL: u64 = 300;

/// Maximum lease TTL in seconds (1 hour)
pub const MAX_LEASE_TTL: u64 = 3600;

/// Minimum lease TTL in seconds (30 seconds)
pub const MIN_LEASE_TTL: u64 = 30;

/// Create a new lease for a claimed issue
///
/// This function creates a lease record with a monotonically increasing fencing
/// token. The lease expires after the specified TTL, preventing stale workers
/// from mutating the issue after expiry.
///
/// # Arguments
/// * `tx` - Database transaction
/// * `issue_id` - ID of the claimed issue
/// * `assignee` - Who holds the lease
/// * `ttl_seconds` - Time-to-live in seconds
///
/// # Returns
/// The created lease information including fencing token and expiry time
pub fn create_lease(
    tx: &Transaction,
    issue_id: &str,
    assignee: &str,
    ttl_seconds: u64,
) -> Result<LeaseClaimResult> {
    // Validate TTL range using clamp
    let ttl = ttl_seconds.clamp(MIN_LEASE_TTL, MAX_LEASE_TTL);

    // Calculate expiry time
    let now = OffsetDateTime::now_utc();
    let expires_at = now + time::Duration::seconds(ttl as i64);
    let expires_at_str = expires_at
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string());

    let now_str = now
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string());

    // Generate monotonically increasing fencing token
    // Get the highest fencing token for this issue and increment
    let fencing_token: i64 = tx
        .query_row(
            "SELECT COALESCE(MAX(fencing_token), 0) + 1 FROM leases WHERE issue_id = ?1",
            [issue_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| {
            crate::Error::Internal(anyhow::anyhow!("Failed to generate fencing token: {}", e))
        })?
        .unwrap_or(1); // Start at 1 if no existing lease

    // Insert the lease record
    tx.execute(
        "INSERT INTO leases (issue_id, assignee, fencing_token, expires_at, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        [
            issue_id,
            assignee,
            &fencing_token.to_string(),
            &expires_at_str,
            &now_str,
        ],
    )
    .map_err(|e| crate::Error::Internal(anyhow::anyhow!("Failed to create lease: {}", e)))?;

    Ok(LeaseClaimResult {
        issue_id: issue_id.to_string(),
        assignee: assignee.to_string(),
        fencing_token,
        expires_at: expires_at_str,
    })
}

/// Renew an existing lease for an issue
///
/// This function renews a lease by generating a new fencing token and extending
/// the expiry time. The lease must exist and belong to the specified assignee.
///
/// # Arguments
/// * `tx` - Database transaction
/// * `issue_id` - ID of the issue with existing lease
/// * `assignee` - Who currently holds the lease
/// * `ttl_seconds` - New time-to-live in seconds
///
/// # Returns
/// The updated lease information with new fencing token and expiry time
pub fn renew_lease(
    tx: &Transaction,
    issue_id: &str,
    assignee: &str,
    ttl_seconds: u64,
) -> Result<LeaseClaimResult> {
    // Validate TTL range using clamp
    let ttl = ttl_seconds.clamp(MIN_LEASE_TTL, MAX_LEASE_TTL);

    // Check if a valid lease exists for this assignee
    let existing_lease = get_active_lease(tx, issue_id, assignee)?;
    if existing_lease.is_none() {
        return Err(crate::Error::LeaseExpired(format!(
            "No active lease found for issue {} and assignee {}",
            issue_id, assignee
        )));
    }

    // Calculate new expiry time
    let now = OffsetDateTime::now_utc();
    let expires_at = now + time::Duration::seconds(ttl as i64);
    let expires_at_str = expires_at
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string());

    let now_str = now
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string());

    // Generate new fencing token (monotonically increasing)
    let fencing_token: i64 = tx
        .query_row(
            "SELECT COALESCE(MAX(fencing_token), 0) + 1 FROM leases WHERE issue_id = ?1",
            [issue_id],
            |row| row.get(0),
        )
        .map_err(|e| {
            crate::Error::Internal(anyhow::anyhow!(
                "Failed to generate fencing token for renewal: {}",
                e
            ))
        })?;

    // Update the lease record
    tx.execute(
        "UPDATE leases
         SET assignee = ?1, fencing_token = ?2, expires_at = ?3, renewed_at = ?4
         WHERE issue_id = ?5",
        [
            assignee,
            &fencing_token.to_string(),
            &expires_at_str,
            &now_str,
            issue_id,
        ],
    )
    .map_err(|e| crate::Error::Internal(anyhow::anyhow!("Failed to renew lease: {}", e)))?;

    crate::service::resource_locks::update_issue_lock_lease_token(tx, issue_id, fencing_token)?;

    Ok(LeaseClaimResult {
        issue_id: issue_id.to_string(),
        assignee: assignee.to_string(),
        fencing_token,
        expires_at: expires_at_str,
    })
}

/// Get the active lease for an issue and assignee
///
/// This function retrieves the current active lease if it exists and hasn't expired.
///
/// # Arguments
/// * `tx` - Database transaction
/// * `issue_id` - ID of the issue
/// * `assignee` - Assignee to check for
///
/// # Returns
/// Some(Lease) if a valid active lease exists, None otherwise
pub fn get_active_lease(
    conn: &rusqlite::Connection,
    issue_id: &str,
    assignee: &str,
) -> Result<Option<Lease>> {
    let now = OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string());

    let lease = conn
        .query_row(
            "SELECT issue_id, assignee, fencing_token, expires_at, renewed_at, created_at
             FROM leases
             WHERE issue_id = ?1 AND assignee = ?2 AND expires_at > ?3",
            [issue_id, assignee, &now],
            |row| {
                Ok(Lease {
                    issue_id: row.get(0)?,
                    assignee: row.get(1)?,
                    fencing_token: row.get(2)?,
                    expires_at: row.get(3)?,
                    renewed_at: row.get(4)?,
                    created_at: row.get(5)?,
                })
            },
        )
        .optional()
        .map_err(|e| {
            crate::Error::Internal(anyhow::anyhow!("Failed to query active lease: {}", e))
        })?;

    Ok(lease)
}

/// Validate lease for mutation operations
///
/// This function checks if a valid lease exists and matches the expected fencing
/// token. It's used by update, release, close, and reopen operations to prevent
/// stale workers from mutating expired or reassigned work.
///
/// A lease row only fences the claim epoch that created it. Lease rows are
/// never deleted (they carry the per-issue fencing-token high-water mark), so
/// rows can outlive the claim they fenced. When no active lease matches the
/// issue's current assignee, the historical row is treated as absent unless it
/// belongs to the current assignee AND the current claim epoch is leased
/// (determined from the most recent `claimed` event). Concretely:
///
/// - Issue never leased, or claimed without `--lease-ttl` -> allow.
/// - Lease row from a previous epoch (released/closed lease claim, issue since
///   re-claimed or reassigned) -> allow. Without this, any issue that was ever
///   leased could never again be mutated by a non-leased claimant.
/// - Current assignee's own lease expired during the current leased epoch ->
///   refuse, so a stale worker cannot mutate past expiry.
///
/// # Arguments
/// * `conn` - Database connection
/// * `issue_id` - ID of the issue being mutated
/// * `assignee` - Assignee attempting the mutation
/// * `expected_fencing_token` - Optional expected fencing token for validation
///
/// # Returns
/// Ok(()) if lease is valid, Err otherwise
pub fn validate_lease_for_mutation(
    conn: &rusqlite::Connection,
    issue_id: &str,
    assignee: &str,
    expected_fencing_token: Option<i64>,
) -> Result<()> {
    let lease = get_active_lease(conn, issue_id, assignee)?;

    match lease {
        Some(active_lease) => {
            // Check if fencing token matches (if provided)
            if let Some(expected_token) = expected_fencing_token {
                if active_lease.fencing_token != expected_token {
                    return Err(crate::Error::LeaseConflict(format!(
                        "Fencing token mismatch: expected {}, got {}",
                        expected_token, active_lease.fencing_token
                    )));
                }
            }
            Ok(())
        }
        None => {
            // No active lease for this assignee - this could mean:
            // 1. Issue was never leased (non-leased claim)
            // 2. Lease expired during the current leased claim epoch
            // 3. Lease rows exist only from a previous claim epoch (the issue
            //    was released/closed and later re-claimed or reassigned)

            // Look at who owns the (single) lease row for this issue, if any
            let lease_row_assignee: Option<String> = conn
                .query_row(
                    "SELECT assignee FROM leases WHERE issue_id = ?1",
                    [issue_id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|e| {
                    crate::Error::Internal(anyhow::anyhow!("Failed to check lease history: {}", e))
                })?;

            match lease_row_assignee {
                None => {
                    // No lease ever existed - this is a non-leased claim, allow it
                    Ok(())
                }
                Some(row_assignee) if row_assignee != assignee => {
                    // The row belongs to a previous epoch's claimant; it does
                    // not fence the current assignee, allow
                    Ok(())
                }
                Some(_) => {
                    // The row belongs to the current assignee. It only fences
                    // if the current claim epoch is leased - if the most
                    // recent claim was non-leased, the row predates it
                    if current_claim_epoch_is_leased(conn, issue_id)? {
                        // Current epoch's lease expired or is invalid
                        Err(crate::Error::LeaseExpired(format!(
                            "Lease for issue {} has expired or is invalid for assignee {}",
                            issue_id, assignee
                        )))
                    } else {
                        // Stale row from an earlier epoch of this same
                        // assignee, allow
                        Ok(())
                    }
                }
            }
        }
    }
}

/// Check whether the issue's current claim epoch was made with a lease
///
/// The most recent `claimed` event for the issue records whether the claim
/// created a lease: leased claims carry `"with_lease": true` (or, for the
/// fencing-token claim path, a `claim_with_fencing_token` action). `events`
/// has a monotonically increasing `sequence`, so ordering by it identifies
/// the current epoch without comparing timestamps.
///
/// Returns `true` (treat as leased) when no `claimed` event exists or the
/// detail cannot be parsed - the conservative reading preserves fencing for
/// lease rows whose provenance cannot be established.
fn current_claim_epoch_is_leased(conn: &rusqlite::Connection, issue_id: &str) -> Result<bool> {
    let detail: Option<String> = conn
        .query_row(
            "SELECT detail FROM events
             WHERE issue_id = ?1 AND kind = 'claimed'
             ORDER BY sequence DESC LIMIT 1",
            [issue_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| {
            crate::Error::Internal(anyhow::anyhow!(
                "Failed to inspect claim event history: {}",
                e
            ))
        })?;

    let Some(raw_detail) = detail else {
        return Ok(true);
    };

    let parsed: Option<serde_json::Value> = serde_json::from_str(&raw_detail).ok();
    match parsed {
        Some(detail) => Ok(detail
            .get("with_lease")
            .and_then(serde_json::Value::as_bool)
            == Some(true)
            || detail.get("action").and_then(serde_json::Value::as_str)
                == Some("claim_with_fencing_token")
            || detail
                .get("new_fencing_token")
                .is_some_and(|v| !v.is_null())),
        None => Ok(true),
    }
}

/// Check if an issue has any active lease (regardless of assignee)
///
/// This is used to detect if an issue has been claimed with a lease, which
/// affects reassignment logic.
///
/// # Arguments
/// * `tx` - Database transaction
/// * `issue_id` - ID of the issue to check
///
/// # Returns
/// true if an active lease exists, false otherwise
#[allow(dead_code)]
pub fn has_active_lease(tx: &Transaction, issue_id: &str) -> Result<bool> {
    let now = OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string());

    let count: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM leases WHERE issue_id = ?1 AND expires_at > ?2",
            [issue_id, &now],
            |row| row.get(0),
        )
        .unwrap_or(0);

    Ok(count > 0)
}

/// Clean up expired leases from an issue
///
/// This function removes expired lease records, typically called when an issue
/// is claimed by a new assignee or when leases are renewed.
///
/// # Arguments
/// * `tx` - Database transaction
/// * `issue_id` - ID of the issue to clean up
#[allow(dead_code)]
pub fn cleanup_expired_leases(tx: &Transaction, issue_id: &str) -> Result<()> {
    let now = OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string());

    tx.execute(
        "DELETE FROM leases WHERE issue_id = ?1 AND expires_at <= ?2",
        [issue_id, &now],
    )
    .map_err(|e| {
        crate::Error::Internal(anyhow::anyhow!("Failed to cleanup expired leases: {}", e))
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const _: () = {
        assert!(MIN_LEASE_TTL <= DEFAULT_LEASE_TTL);
        assert!(DEFAULT_LEASE_TTL <= MAX_LEASE_TTL);
    };

    #[test]
    fn test_lease_ttl_bounds() {
        // Test moved to const block above
    }

    #[test]
    fn test_lease_serialization() {
        let lease = Lease {
            issue_id: "test-1".to_string(),
            assignee: "alice".to_string(),
            fencing_token: 1,
            expires_at: "2024-01-01T00:00:00Z".to_string(),
            renewed_at: None,
            created_at: "2023-12-31T23:59:00Z".to_string(),
        };

        let json = serde_json::to_string(&lease);
        assert!(json.is_ok());
    }
}

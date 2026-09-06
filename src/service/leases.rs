//! Lease service for fenced claim operations
//!
//! This module implements R002's fenced claim leases with expiring claims,
//! renewals, and monotonically increasing fencing tokens for safe recovery from
//! crashed or disconnected agents.
//!
//! # Claim epochs
//!
//! The fencing token has evolved into the universal claim-epoch credential:
//! *every* successful claim -- leased or not -- mints `issues.claim_epoch + 1`
//! and returns that number to the claimant. A leased claim also writes a lease
//! row whose `fencing_token` equals the new epoch, so the two sequences are one
//! sequence and pre-epoch lease history stays monotone with it.
//!
//! The epoch is returned to the claimant by `bead claim` and projected by
//! `bead show --json` and the checkpoint, so an automated consumer can retain
//! it without a second lookup. It is also load-bearing: every claimant-owned
//! mutation of an owned issue (update, release, close, reopen, resource-lock
//! change, atomic attempt resolve) must present the exact current epoch, and a
//! missing or stale one conflicts with exit 4 without writing anything. Lease
//! rows stay append-only claim-epoch history: they carry the per-issue
//! high-water mark and the expiry of the timed claims, and a row from an older
//! epoch never fences the epoch that superseded it.

use crate::error::Result;
use rusqlite::{Connection, OptionalExtension, Transaction};
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

/// Mint the next claim-epoch credential for an issue
///
/// Bumps `issues.claim_epoch` inside the caller's claim transaction and
/// returns the new value. Every successful claim -- leased or not -- and every
/// lease renewal calls this, so the epoch is a per-issue counter of ownership
/// tenures: two claims never share an epoch, and a superseded epoch's
/// credential is dead the moment a new one is minted.
///
/// Must run in the same IMMEDIATE transaction as the assignment it
/// credentials.
pub fn mint_claim_epoch(conn: &Connection, issue_id: &str) -> Result<i64> {
    conn.execute(
        "UPDATE issues SET claim_epoch = claim_epoch + 1 WHERE id = ?1",
        [issue_id],
    )
    .map_err(|e| crate::Error::Internal(anyhow::anyhow!("Failed to mint claim epoch: {}", e)))?;

    let epoch: i64 = conn
        .query_row(
            "SELECT claim_epoch FROM issues WHERE id = ?1",
            [issue_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| crate::Error::Internal(anyhow::anyhow!("Failed to read claim epoch: {}", e)))?
        .unwrap_or(0);

    Ok(epoch)
}

/// Read the issue's current claim-epoch high-water mark
///
/// `0` means the issue has never been claimed by a fencing-aware claim: every
/// claim mints an epoch, so an issue claimed and then released keeps its
/// high-water mark and the next claim takes the next number.
pub fn current_claim_epoch(conn: &Connection, issue_id: &str) -> Result<i64> {
    let epoch: i64 = conn
        .query_row(
            "SELECT claim_epoch FROM issues WHERE id = ?1",
            [issue_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| crate::Error::Internal(anyhow::anyhow!("Failed to read claim epoch: {}", e)))?
        .unwrap_or(0);

    Ok(epoch)
}

/// Create a new lease for a claimed issue
///
/// This function creates a lease record whose `fencing_token` is the claim
/// epoch the caller just minted with [`mint_claim_epoch`], so the lease row and
/// the issue's epoch are the same credential. The lease expires after the
/// specified TTL, preventing stale workers from mutating the issue after
/// expiry.
///
/// # Arguments
/// * `tx` - Database transaction
/// * `issue_id` - ID of the claimed issue
/// * `assignee` - Who holds the lease
/// * `ttl_seconds` - Time-to-live in seconds
/// * `fencing_token` - The claim epoch minted for this claim
///
/// # Returns
/// The created lease information including fencing token and expiry time
pub fn create_lease(
    tx: &Transaction,
    issue_id: &str,
    assignee: &str,
    ttl_seconds: u64,
    fencing_token: i64,
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
/// This function renews a lease by rotating it onto the freshly minted claim
/// epoch and extending the expiry time. The lease must exist and belong to the
/// specified assignee. Rotating the epoch is what makes renewal safe: the
/// renewing holder learns the new credential from the result, while any other
/// copy of the previous credential -- including a crashed worker's -- stops
/// working the moment the renewal commits.
///
/// # Arguments
/// * `tx` - Database transaction
/// * `issue_id` - ID of the issue with existing lease
/// * `assignee` - Who currently holds the lease
/// * `ttl_seconds` - New time-to-live in seconds
/// * `fencing_token` - The claim epoch minted for this renewal
///
/// # Returns
/// The updated lease information with new fencing token and expiry time
pub fn renew_lease(
    tx: &Transaction,
    issue_id: &str,
    assignee: &str,
    ttl_seconds: u64,
    fencing_token: i64,
) -> Result<LeaseClaimResult> {
    // Validate TTL range using clamp
    let ttl = ttl_seconds.clamp(MIN_LEASE_TTL, MAX_LEASE_TTL);

    // Check if a valid lease exists for this assignee
    let existing_lease = get_active_lease(tx, issue_id, assignee)?.ok_or_else(|| {
        crate::Error::LeaseExpired(format!(
            "No active lease found for issue {} and assignee {}",
            issue_id, assignee
        ))
    })?;

    // Calculate new expiry time
    let now = OffsetDateTime::now_utc();
    let expires_at = now + time::Duration::seconds(ttl as i64);
    let expires_at_str = expires_at
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string());

    let now_str = now
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string());

    let previous_fencing_token = existing_lease.fencing_token.to_string();

    // Update only the active row that was checked above. A previous claim by
    // another assignee may still have an unexpired historical row; filtering
    // by both assignee and the checked fencing token prevents renewal from
    // rewriting that history (or more than one row).
    let changed = tx
        .execute(
            "UPDATE leases
             SET assignee = ?1, fencing_token = ?2, expires_at = ?3, renewed_at = ?4
             WHERE issue_id = ?5 AND assignee = ?1 AND fencing_token = ?6",
            [
                assignee,
                &fencing_token.to_string(),
                &expires_at_str,
                &now_str,
                issue_id,
                &previous_fencing_token,
            ],
        )
        .map_err(|e| crate::Error::Internal(anyhow::anyhow!("Failed to renew lease: {}", e)))?;

    if changed != 1 {
        return Err(crate::Error::LeaseExpired(format!(
            "Lease for issue {} and assignee {} is no longer active",
            issue_id, assignee
        )));
    }

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
             WHERE issue_id = ?1 AND assignee = ?2 AND expires_at > ?3
             ORDER BY fencing_token DESC
             LIMIT 1",
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

/// Validate the lease-expiry dimension of a mutation on a claimed issue
///
/// This function checks whether the issue's *current claim epoch* is a timed
/// lease and, if it is, whether that lease is still alive and matches the
/// expected fencing token. It is used by update, release, close, and reopen to
/// prevent stale workers from mutating work past lease expiry.
///
/// A lease row only fences the claim epoch that created it. Lease rows are
/// never deleted (they carry the per-issue fencing-token high-water mark), so
/// rows can outlive the claim they fenced. Only a row carrying the issue's
/// *current* `claim_epoch` AND the issue's current assignee decides anything:
///
/// - Current epoch was claimed without `--lease-ttl` -> no lease row at that
///   epoch, nothing to expire, allow.
/// - Lease row from a previous epoch (released/closed lease claim, issue since
///   re-claimed or reassigned) -> allow. Without this, any issue that was ever
///   leased could never again be mutated after reassignment.
/// - Current epoch's own lease expired -> refuse, so a stale worker cannot
///   mutate past expiry.
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
    let epoch = current_claim_epoch(conn, issue_id)?;
    let epoch_lease = get_epoch_lease(conn, issue_id, assignee, epoch)?;

    match epoch_lease {
        Some(lease) if lease_is_expired(&lease) => {
            // Current epoch's lease expired or is invalid
            Err(crate::Error::LeaseExpired(format!(
                "Lease for issue {} has expired or is invalid for assignee {}",
                issue_id, assignee
            )))
        }
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
            // The current epoch was not created as a timed lease (or its row
            // belongs to an earlier holder) - the expiry dimension has nothing
            // to enforce.
            Ok(())
        }
    }
}

/// Validate the claim-epoch credential dimension of a mutation
///
/// Verifies the credential a caller presents against the issue's current
/// claim epoch. This is what stops a process from an older claim from
/// mutating a newer claim after release and reassignment: its credential
/// stopped matching the moment the new claim minted the next epoch.
///
/// - `Some(epoch)` matching the issue's current epoch -> allow.
/// - Any other token -> conflict, exit 4.
/// - No credential at all -> conflict, exit 4. A credential that may be
///   omitted is not a fence: the spaxel duplicate dispatch happened exactly
///   because an older holder could keep mutating by simply not presenting
///   the credential it no longer had. Callers obtain the epoch from
///   `bead claim` or the `claim_epoch` projection on `bead show --json`, and
///   the caller runs this check inside the mutation's own IMMEDIATE
///   transaction, so a refusal leaves no partial write behind.
/// - `claim_epoch == 0` -> a legacy claim that predates fencing (assigned by
///   an older binary or restored from an old checkpoint). Nothing to match,
///   so the mutation is allowed; the next claim, renewal, or assignment mints
///   an epoch and fences from then on.
///
/// Only reaches the match arms for an *owned* issue: the caller
/// ([`crate::service::lifecycle::enforce_claimant_credential`]) returns early
/// when the issue has no assignee, so an unclaimed issue is never asked for a
/// credential.
///
/// The two refusal arms are deliberately the same `LeaseConflict` and not
/// distinguishable by the caller: "you presented nothing" and "you presented a
/// superseded epoch" are one conflict condition, exit 4. A distinct code for
/// the missing-credential case would let a caller probe whether an issue is
/// claimed without ever holding the credential, which is the enumeration this
/// fence exists to prevent.
///
/// # Arguments
/// * `conn` - Database connection
/// * `issue_id` - ID of the issue being mutated
/// * `presented_claim_epoch` - The credential the caller presented, if any
///
/// # Returns
/// Ok(()) if the credential is current, Err otherwise
pub fn validate_claim_epoch_for_mutation(
    conn: &rusqlite::Connection,
    issue_id: &str,
    presented_claim_epoch: Option<i64>,
) -> Result<()> {
    let epoch = current_claim_epoch(conn, issue_id)?;

    if epoch <= 0 {
        // Legacy claim from before fencing existed: nothing to match.
        return Ok(());
    }

    match presented_claim_epoch {
        Some(presented) if presented == epoch => Ok(()),
        Some(presented) => Err(crate::Error::LeaseConflict(format!(
            "Claim-epoch credential mismatch: issue {issue_id} is claimed at claim epoch {epoch}, \
             not {presented}; a stale credential cannot mutate it"
        ))),
        None => Err(crate::Error::LeaseConflict(format!(
            "Claim-epoch credential required: issue {issue_id} is claimed at claim epoch {epoch}; \
             pass --fencing-token {epoch} (the current epoch is projected by \
             `bead show {issue_id} --json`)"
        ))),
    }
}

/// The lease row that fences `epoch` for `assignee`, if the current claim
/// epoch was created as a timed lease
///
/// Filtered by the current assignee as well as the epoch so a row belonging to
/// a previous epoch's claimant never decides anything about the current
/// holder.
fn get_epoch_lease(
    conn: &rusqlite::Connection,
    issue_id: &str,
    assignee: &str,
    epoch: i64,
) -> Result<Option<Lease>> {
    if epoch <= 0 {
        return Ok(None);
    }

    let lease = conn
        .query_row(
            "SELECT issue_id, assignee, fencing_token, expires_at, renewed_at, created_at
             FROM leases
             WHERE issue_id = ?1 AND assignee = ?2 AND fencing_token = ?3
             ORDER BY fencing_token DESC
             LIMIT 1",
            [issue_id, assignee, &epoch.to_string()],
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
            crate::Error::Internal(anyhow::anyhow!("Failed to query epoch lease: {}", e))
        })?;

    Ok(lease)
}

/// A lease row is expired when its expiry has passed
fn lease_is_expired(lease: &Lease) -> bool {
    let now = OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string());

    lease.expires_at <= now
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

    let current_is_active: i64 = tx
        .query_row(
            "SELECT EXISTS(
                 SELECT 1
                 FROM leases current_lease
                 WHERE current_lease.issue_id = ?1
                   AND current_lease.fencing_token = (
                       SELECT MAX(history.fencing_token)
                       FROM leases history
                       WHERE history.issue_id = ?1
                   )
                   AND current_lease.expires_at > ?2
             )",
            [issue_id, &now],
            |row| row.get(0),
        )
        .unwrap_or(0);

    Ok(current_is_active != 0)
}

/// Retain lease history for an issue.
///
/// Lease rows are append-only so fencing-token history remains auditable and
/// the per-issue high-water mark cannot regress. This compatibility helper is
/// intentionally a no-op; expired rows are harmless historical records.
///
/// # Arguments
/// * `tx` - Database transaction
/// * `issue_id` - ID of the issue to clean up
#[allow(dead_code)]
pub fn cleanup_expired_leases(tx: &Transaction, issue_id: &str) -> Result<()> {
    let _ = (tx, issue_id);
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

//! Attempt resolution service for versioned attempt-outcome contract.
//!
//! This module implements the attempt-outcome-v1 specification for recording
//! execution attempt outcomes atomically with lifecycle transitions.
//!
//! # Implementation Boundary
//!
//! This feature was implemented between:
//! - Pre: `attempt-resolution-pre` (53dade0) - ADR documentation only
//! - Complete: `attempt-resolution-complete` (bcda20a) - core implementation
//!
//! See `docs/boundaries/attempt-resolution-feature.md` for the full
//! implementation timeline and verification details.
//!
//! # Architecture
//!
//! The resolve_attempt operation commits one attempt outcome and its requested
//! issue transition in one SQLite transaction, ensuring exactly-once semantics
//! under concurrent access and crash recovery.
//!
//! # Key Operations
//!
//! - `resolve_attempt` - Main entry point for attempt resolution
//! - Canonical hash computation for idempotent replay detection
//! - Tier progression for work_failure outcomes
//! - Lifecycle action validation and application
//! - Audit event generation
//!
//! See: research/specs/attempt-outcome-v1.md

use crate::error::{Error, Result};
use crate::model::attempt::{
    validate_outcome_action_combo, Action, AttemptError, Outcome, ResolveReceipt, ResolveRequest,
};
use crate::service::leases::validate_lease_for_mutation;
use crate::service::lifecycle;
use crate::service::scheduling::AttemptTier;
use rand::Rng;
use rusqlite::Transaction;
use rusqlite::{params, OptionalExtension};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use time::OffsetDateTime;

/// Resolve an attempt with idempotent replay detection
///
/// This operation validates the request, checks for existing attempts,
/// applies tier progression and lifecycle changes atomically, and returns
/// a receipt. Identical replays return the original receipt without mutation.
///
/// # Errors
///
/// - `AttemptError::Usage` - Invalid request parameters (exit 2)
/// - `AttemptError::NotFound` - Issue doesn't exist (exit 3)
/// - `AttemptError::Conflict` - Revision/fencing/semantic mismatch (exit 4)
/// - `AttemptError::Integrity` - Database corruption (exit 5)
/// - `AttemptError::Transient` - Lock contention (exit 6)
pub fn resolve_attempt(
    tx: &mut Transaction,
    workspace_uuid: &str,
    request: ResolveRequest,
) -> std::result::Result<ResolveReceipt, AttemptError> {
    // Validate request structure
    request
        .validate()
        .map_err(|e| AttemptError::Usage(format!("Invalid request: {}", e)))?;

    // Parse outcome and action
    let outcome = Outcome::parse(&request.outcome)
        .map_err(|e| AttemptError::Usage(format!("Invalid outcome: {}", e)))?;
    let action_str = request.action.as_deref().unwrap_or("none");
    let action = Action::parse(action_str)
        .map_err(|e| AttemptError::Usage(format!("Invalid action: {}", e)))?;

    // Validate combination
    validate_outcome_action_combo(&outcome, &action)
        .map_err(|e| AttemptError::Usage(format!("Invalid combination: {}", e)))?;

    // `close` delegates to the standard close path, which requires a reason
    // (mirroring `bead close`). Reject a missing one as usage here, before any
    // mutation, instead of letting the inner validation error surface as an
    // integrity failure.
    if matches!(action, Action::Close)
        && request.reason.as_deref().map_or(true, |r| r.trim().is_empty())
    {
        return Err(AttemptError::Usage(
            "action 'close' requires a non-empty reason".to_string(),
        ));
    }

    // Get current issue state
    let issue_state = get_issue_state(tx, &request.issue_id).map_err(|e| match e {
        Error::Workspace(msg) => AttemptError::NotFound(msg),
        _ => AttemptError::Integrity(format!("Failed to read issue: {}", e)),
    })?;

    // Validate expected revision
    if let Some(expected_revision) = request.if_revision {
        if issue_state.revision != expected_revision {
            return Err(AttemptError::Conflict(format!(
                "Expected revision {} but issue is currently at revision {}",
                expected_revision, issue_state.revision
            )));
        }
    }

    // Validate fencing token if provided
    if let Some(ref fencing_token_str) = request.fencing_token {
        // Parse fencing token as integer
        let fencing_token = fencing_token_str.parse::<i64>().map_err(|_| {
            AttemptError::Usage(format!("Invalid fencing token: {}", fencing_token_str))
        })?;

        validate_lease_for_mutation(tx, &request.issue_id, &request.actor, Some(fencing_token))
            .map_err(|e| match e {
                Error::Conflict(msg) => AttemptError::Conflict(msg),
                _ => AttemptError::Integrity(format!("Failed to validate lease: {}", e)),
            })?;
    } else {
        // Check if there's an active lease without providing a token
        // This will fail if there's an active lease, succeed otherwise
        validate_lease_for_mutation(tx, &request.issue_id, &request.actor, None).map_err(|e| {
            match e {
                Error::Conflict(msg) => {
                    AttemptError::Conflict(format!("Issue has an active lease. {}", msg))
                }
                _ => AttemptError::Integrity(format!("Failed to check lease status: {}", e)),
            }
        })?;
    }

    // Compute canonical request hash
    let canonical_hash = compute_canonical_hash(&request, &issue_state);

    // Check for existing attempt (replay or conflict)
    if let Some(existing) = get_existing_attempt(tx, &request.attempt_id)
        .map_err(|e| AttemptError::Integrity(format!("Failed to check existing attempt: {}", e)))?
    {
        if existing.canonical_request_hash == canonical_hash {
            // Idempotent replay - return existing receipt
            return Ok(ResolveReceipt {
                receipt_id: existing.receipt_id,
                canonical_request_hash: existing.canonical_request_hash,
                issue_id: request.issue_id.clone(),
                attempt_id: request.attempt_id.clone(),
                resulting_issue_revision: existing.resulting_issue_revision,
                resulting_state: derive_resulting_state(&existing.action),
                resulting_attempt_tier: existing.resulting_attempt_tier,
                created_at: existing.created_at,
                is_replay: true,
            });
        } else {
            // Conflicting replay
            return Err(AttemptError::Conflict(format!(
                "Attempt {} already resolved with outcome '{}' (hash {}) but new request has outcome '{}' (hash {})",
                request.attempt_id,
                existing.outcome,
                abbreviate_hash(&existing.canonical_request_hash),
                request.outcome,
                abbreviate_hash(&canonical_hash)
            )));
        }
    }

    // Compute new tier and failures
    let prior_tier = issue_state.attempt_tier;
    let (resulting_tier, consecutive_failures) = if outcome.affects_tier() {
        let new_failures = issue_state.consecutive_failures + 1;
        let new_tier = match new_failures {
            1 => AttemptTier::Retryable as i64,
            2 => AttemptTier::Struggling as i64,
            _ => AttemptTier::Quarantined as i64,
        };
        (new_tier, new_failures)
    } else {
        (prior_tier, issue_state.consecutive_failures)
    };

    // Generate receipt ID
    let receipt_id = generate_receipt_id();

    // Get current timestamp
    let created_at = OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "UTC".to_string());

    // Determine resulting state after action
    let resulting_state = apply_action_validate(tx, &action, &issue_state.status, &request.reason)?;

    // Apply lifecycle action
    apply_action(
        tx,
        &action,
        &request.issue_id,
        &request.reason,
        &request.actor,
    )
    .map_err(|e| AttemptError::Integrity(format!("Failed to apply action: {}", e)))?;

    // Update attempt tier and failures
    update_attempt_tier(tx, &request.issue_id, resulting_tier, consecutive_failures)
        .map_err(|e| AttemptError::Integrity(format!("Failed to update tier: {}", e)))?;

    // Insert attempt outcome record
    insert_attempt_outcome(
        tx,
        &request,
        &receipt_id,
        &canonical_hash,
        &created_at,
        prior_tier,
        resulting_tier,
        issue_state.revision + 1,
    )
    .map_err(|e| AttemptError::Integrity(format!("Failed to insert outcome: {}", e)))?;

    // Append audit event
    append_audit_event(
        tx,
        workspace_uuid,
        &request.issue_id,
        &request.attempt_id,
        &outcome,
        &action,
        &receipt_id,
        prior_tier,
        resulting_tier,
        &resulting_state,
        &request.actor,
        &created_at,
    )
    .map_err(|e| AttemptError::Integrity(format!("Failed to append event: {}", e)))?;

    Ok(ResolveReceipt {
        receipt_id,
        canonical_request_hash: canonical_hash,
        issue_id: request.issue_id,
        attempt_id: request.attempt_id,
        resulting_issue_revision: issue_state.revision + 1,
        resulting_state,
        resulting_attempt_tier: resulting_tier,
        created_at,
        is_replay: false,
    })
}

/// Issue state snapshot for validation
#[derive(Debug, Clone)]
struct IssueState {
    revision: i64,
    status: String,
    attempt_tier: i64,
    consecutive_failures: i64,
}

/// Get current issue state
///
/// Selects `revision` — the logical-revision column migration 3 creates
/// (src/store/migrations.rs) and every mutation bumps — not anything derived
/// from the `updated_at` timestamp. An earlier draft named the column
/// `updated_at_revision`, which no migration creates, so every resolve failed
/// with an exit-5 integrity error (beadrs-6b891bb7) while `capabilities` still
/// advertised the feature; `resolve_attempt_e2e.rs` now drives this path
/// end-to-end so a regression here cannot go unseen again.
fn get_issue_state(tx: &Transaction, issue_id: &str) -> std::result::Result<IssueState, Error> {
    let mut stmt = tx.prepare_cached(
        "SELECT revision, base_status, attempt_tier, consecutive_failures
         FROM issues WHERE id = ?1",
    )?;

    let state = stmt
        .query_row(params![issue_id], |row| {
            Ok(IssueState {
                revision: row.get(0)?,
                status: row.get(1)?,
                attempt_tier: row.get(2)?,
                consecutive_failures: row.get(3)?,
            })
        })
        .optional()?
        .ok_or_else(|| Error::not_found(format!("Issue not found: {}", issue_id)))?;

    Ok(state)
}

/// Compute canonical request hash for idempotency
fn compute_canonical_hash(request: &ResolveRequest, _issue_state: &IssueState) -> String {
    let mut hasher = Sha256::new();

    // Hash components in deterministic order
    hasher.update(request.attempt_id.as_bytes());
    hasher.update([0x00]); // null byte separator

    hasher.update(request.issue_id.as_bytes());
    hasher.update([0x00]);

    hasher.update(request.outcome.as_bytes());
    hasher.update([0x00]);

    let action = request.action.as_deref().unwrap_or("none");
    hasher.update(action.as_bytes());
    hasher.update([0x00]);

    let reason = request.reason.as_deref().unwrap_or("");
    hasher.update(reason.as_bytes());
    hasher.update([0x00]);

    let revision_str = request.if_revision.unwrap_or(0).to_string();
    hasher.update(revision_str.as_bytes());
    hasher.update([0x00]);

    let fencing = request.fencing_token.as_deref().unwrap_or("");
    hasher.update(fencing.as_bytes());
    hasher.update([0x00]);

    // Sort evidence refs for deterministic ordering
    let mut sorted_refs = request.evidence_refs.clone();
    sorted_refs.sort();
    for ref_str in &sorted_refs {
        hasher.update(ref_str.as_bytes());
        hasher.update([0x00]);
    }

    // Add bounded metadata in canonical JSON order
    let mut metadata = BTreeMap::new();
    if let Some(ref model) = request.model {
        metadata.insert("model", model.as_str());
    }
    if let Some(ref harness) = request.harness {
        metadata.insert("harness", harness.as_str());
    }
    if let Some(ref version) = request.harness_version {
        metadata.insert("harness_version", version.as_str());
    }

    if !metadata.is_empty() {
        let metadata_json = serde_json::to_string(&metadata).unwrap_or_default();
        hasher.update(metadata_json.as_bytes());
    }

    format!("{:x}", hasher.finalize())
}

/// Existing attempt for replay detection
#[derive(Debug, Clone)]
struct ExistingAttempt {
    receipt_id: String,
    canonical_request_hash: String,
    outcome: String,
    resulting_issue_revision: i64,
    resulting_attempt_tier: i64,
    created_at: String,
    action: String,
}

/// Derive the resulting state of a resolved attempt from its stored action.
///
/// `attempt_outcomes` does not persist the post-action status, so a replayed
/// receipt reconstructs it from the recorded action, the same derivation the
/// checkpoint publisher applies when emitting outcome records.
fn derive_resulting_state(action: &str) -> String {
    if action == "close" {
        "closed".to_string()
    } else {
        "open".to_string()
    }
}

/// Get existing attempt by attempt_id
fn get_existing_attempt(
    tx: &Transaction,
    attempt_id: &str,
) -> std::result::Result<Option<ExistingAttempt>, Error> {
    let mut stmt = tx.prepare_cached(
        "SELECT receipt_id, canonical_request_hash, outcome, resulting_issue_revision,
                resulting_attempt_tier, created_at, action
         FROM attempt_outcomes WHERE attempt_id = ?1",
    )?;

    let result = stmt
        .query_row(params![attempt_id], |row| {
            Ok(ExistingAttempt {
                receipt_id: row.get(0)?,
                canonical_request_hash: row.get(1)?,
                outcome: row.get(2)?,
                resulting_issue_revision: row.get(3)?,
                resulting_attempt_tier: row.get(4)?,
                created_at: row.get(5)?,
                action: row.get(6)?,
            })
        })
        .optional()?;

    Ok(result)
}

/// Generate a unique receipt ID
fn generate_receipt_id() -> String {
    let mut rng = rand::thread_rng();
    let random_bytes: [u8; 16] = std::array::from_fn(|_| rng.r#gen());
    format!("ao-{}", hex::encode(random_bytes))
}

/// Abbreviate a hash for display
fn abbreviate_hash(hash: &str) -> String {
    if hash.len() <= 12 {
        hash.to_string()
    } else {
        format!("{}...", &hash[..12])
    }
}

/// Validate and get resulting state from action
fn apply_action_validate(
    _tx: &Transaction,
    action: &Action,
    current_status: &str,
    _reason: &Option<String>,
) -> std::result::Result<String, AttemptError> {
    match action {
        Action::None => Ok(current_status.to_string()),
        Action::Close => {
            // Validate that close is allowed
            if current_status == "closed" {
                return Err(AttemptError::Usage("Issue is already closed".to_string()));
            }
            Ok("closed".to_string())
        }
        Action::Release => {
            if current_status != "in_progress" {
                return Err(AttemptError::Conflict(
                    "Can only release issues that are in_progress".to_string(),
                ));
            }
            Ok("open".to_string())
        }
        Action::Quarantine => {
            // Quarantine retains current state but sets tier=3
            Ok(current_status.to_string())
        }
        Action::Block => {
            // Block retains current state but sets manual_blocked=true
            Ok(current_status.to_string())
        }
    }
}

/// Apply lifecycle action
///
/// `tx` is the resolver's own write transaction: the lifecycle bodies applied
/// here are the `_in_tx` variants, which run against a caller-owned
/// transaction instead of opening a nested one.
fn apply_action(
    tx: &mut Transaction,
    action: &Action,
    issue_id: &str,
    reason: &Option<String>,
    _actor: &str,
) -> std::result::Result<(), Error> {
    match action {
        Action::None => {
            // No action
        }
        Action::Close => {
            lifecycle::close_issue_in_tx(tx, issue_id, reason.as_deref().unwrap_or(""), None, None)?;
        }
        Action::Release => {
            lifecycle::release_issue_in_tx(tx, issue_id)?;
        }
        Action::Quarantine => {
            // Quarantine: set tier=3 and optionally retry_after
            // This is handled in update_attempt_tier
        }
        Action::Block => {
            // Block: set manual_blocked=true
            tx.execute(
                "UPDATE issues SET manual_blocked = 1 WHERE id = ?1",
                params![issue_id],
            )?;
        }
    }
    Ok(())
}

/// Update attempt tier and consecutive failures
fn update_attempt_tier(
    tx: &Transaction,
    issue_id: &str,
    new_tier: i64,
    consecutive_failures: i64,
) -> Result<()> {
    tx.execute(
        "UPDATE issues SET attempt_tier = ?1, consecutive_failures = ?2 WHERE id = ?3",
        params![new_tier, consecutive_failures, issue_id],
    )?;
    Ok(())
}

/// Insert attempt outcome record
#[allow(clippy::too_many_arguments)]
fn insert_attempt_outcome(
    tx: &Transaction,
    request: &ResolveRequest,
    receipt_id: &str,
    canonical_hash: &str,
    created_at: &str,
    prior_tier: i64,
    resulting_tier: i64,
    resulting_revision: i64,
) -> Result<()> {
    let evidence_json = serde_json::to_string(&request.evidence_refs).unwrap_or_default();

    let reason_value = request.reason.clone().unwrap_or_default();

    tx.execute(
        "INSERT INTO attempt_outcomes (
            receipt_id, attempt_id, issue_id, outcome, action, reason,
            canonical_request_hash, prior_attempt_tier, resulting_attempt_tier,
            resulting_issue_revision, actor, created_at, evidence_refs_json,
            model, harness, harness_version
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
        params![
            receipt_id,
            request.attempt_id.clone(),
            request.issue_id.clone(),
            request.outcome.clone(),
            request.action.clone().unwrap_or_else(|| "none".to_string()),
            reason_value,
            canonical_hash,
            prior_tier,
            resulting_tier,
            resulting_revision,
            request.actor.clone(),
            created_at,
            evidence_json,
            request.model.clone(),
            request.harness.clone(),
            request.harness_version.clone(),
        ],
    )?;

    Ok(())
}

/// Append audit event for attempt resolution
#[allow(clippy::too_many_arguments)]
fn append_audit_event(
    tx: &Transaction,
    workspace_uuid: &str,
    issue_id: &str,
    attempt_id: &str,
    outcome: &Outcome,
    action: &Action,
    receipt_id: &str,
    prior_tier: i64,
    resulting_tier: i64,
    resulting_state: &str,
    actor: &str,
    created_at: &str,
) -> Result<()> {
    let event_data = json!({
        "attempt_id": attempt_id,
        "outcome": outcome.as_str(),
        "action": action.as_str(),
        "receipt_id": receipt_id,
        "prior_attempt_tier": prior_tier,
        "resulting_attempt_tier": resulting_tier,
        "resulting_state": resulting_state,
    });

    tx.execute(
        "INSERT INTO events (
            origin_store_uuid, issue_id, kind, actor, detail, time
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            workspace_uuid,
            issue_id,
            "attempt_resolved",
            actor,
            event_data.to_string(),
            created_at,
        ],
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_canonical_hash() {
        let request = ResolveRequest {
            attempt_id: "urn:needle:attempt:abc123".to_string(),
            issue_id: "bead-0123456789abcdef".to_string(),
            outcome: "verified_success".to_string(),
            action: Some("close".to_string()),
            reason: Some("All tests passed".to_string()),
            if_revision: Some(42),
            fencing_token: None,
            evidence_refs: vec!["s3:logs/abc.tar.gz".to_string()],
            actor: "needle-worker-alpha".to_string(),
            model: Some("claude-opus-5".to_string()),
            harness: Some("needle".to_string()),
            harness_version: Some("1.2.3".to_string()),
        };

        let issue_state = IssueState {
            revision: 42,
            status: "in_progress".to_string(),
            attempt_tier: 0,
            consecutive_failures: 0,
        };

        let hash1 = compute_canonical_hash(&request, &issue_state);
        let hash2 = compute_canonical_hash(&request, &issue_state);

        // Deterministic
        assert_eq!(hash1, hash2);

        // SHA-256 format (64 hex chars)
        assert_eq!(hash1.len(), 64);
        assert!(hash1.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_abbreviate_hash() {
        let long_hash = "a1b2c3d4e5f6789012345678901234567890123456789012345678901234";
        assert_eq!(abbreviate_hash(long_hash), "a1b2c3d4e5f6...");

        let short_hash = "a1b2c3";
        assert_eq!(abbreviate_hash(short_hash), "a1b2c3");
    }

    #[test]
    fn test_generate_receipt_id() {
        let id1 = generate_receipt_id();
        let id2 = generate_receipt_id();

        // Unique
        assert_ne!(id1, id2);

        // Format: ao- followed by hex
        assert!(id1.starts_with("ao-"));
        assert!(id1.len() > 3);
    }
}

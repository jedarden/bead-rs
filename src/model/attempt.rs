//! Attempt outcome model for versioned attempt resolution contract.
//!
//! This module implements the attempt-outcome-v1 specification for recording
//! execution attempt outcomes atomically with lifecycle transitions.
//!
//! See: research/specs/attempt-outcome-v1.md

use serde::{Deserialize, Serialize};
use std::fmt;

/// Schema references for attempt outcome contract
pub const SCHEMA_ATTEMPT_OUTCOME: &str = "urn:bead-rs:schema:attempt-outcome:native-v1";
pub const SCHEMA_RESOLVE_RECEIPT: &str = "urn:bead-rs:schema:resolve-receipt:native-v1";
pub const SCHEMA_RESOLVE_REQUEST: &str = "urn:bead-rs:schema:resolve-request:native-v1";

/// Attempt outcome classification (v1 vocabulary)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// Caller asserts work completed successfully
    VerifiedSuccess,
    /// Bead-scoped failure: invalid assumptions, repeatable test failure
    WorkFailure,
    /// Worker crash, provider outage, rate limit, network loss
    InfrastructureFailure,
    /// Explicit cancellation or interruption by operator
    Cancelled,
    /// Unable to determine outcome; requires manual review
    Indeterminate,
}

impl Outcome {
    /// Parse from string, returning error for unknown outcomes
    pub fn parse(s: &str) -> Result<Self, AttemptError> {
        match s {
            "verified_success" => Ok(Outcome::VerifiedSuccess),
            "work_failure" => Ok(Outcome::WorkFailure),
            "infrastructure_failure" => Ok(Outcome::InfrastructureFailure),
            "cancelled" => Ok(Outcome::Cancelled),
            "indeterminate" => Ok(Outcome::Indeterminate),
            _ => Err(AttemptError::Usage(format!("Unknown outcome: {}", s))),
        }
    }

    /// Return string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            Outcome::VerifiedSuccess => "verified_success",
            Outcome::WorkFailure => "work_failure",
            Outcome::InfrastructureFailure => "infrastructure_failure",
            Outcome::Cancelled => "cancelled",
            Outcome::Indeterminate => "indeterminate",
        }
    }

    /// Check if this outcome affects attempt tier
    pub fn affects_tier(&self) -> bool {
        matches!(self, Outcome::WorkFailure)
    }
}

impl fmt::Display for Outcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Lifecycle action to apply atomically with outcome
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// Set closed_at, store close_reason
    Close,
    /// Clear assignee, retain state
    Release,
    /// Set attempt_tier=3, set retry_after
    Quarantine,
    /// Set manual_blocked=true
    Block,
    /// No lifecycle transition
    None,
}

impl Action {
    /// Parse from string, returning error for unknown actions
    pub fn parse(s: &str) -> Result<Self, AttemptError> {
        match s {
            "close" => Ok(Action::Close),
            "release" => Ok(Action::Release),
            "quarantine" => Ok(Action::Quarantine),
            "block" => Ok(Action::Block),
            "none" => Ok(Action::None),
            _ => Err(AttemptError::Usage(format!("Unknown action: {}", s))),
        }
    }

    /// Return string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            Action::Close => "close",
            Action::Release => "release",
            Action::Quarantine => "quarantine",
            Action::Block => "block",
            Action::None => "none",
        }
    }
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Validate outcome-action combination per spec section 5
pub fn validate_outcome_action_combo(
    outcome: &Outcome,
    action: &Action,
) -> Result<(), AttemptError> {
    match (outcome, action) {
        // Valid combinations from spec
        (Outcome::VerifiedSuccess, Action::Close) => Ok(()),
        (Outcome::VerifiedSuccess, Action::None) => Ok(()),
        (Outcome::VerifiedSuccess, Action::Release) => Ok(()),
        (Outcome::WorkFailure, Action::Close) => Ok(()),
        (Outcome::WorkFailure, Action::Quarantine) => Ok(()),
        (Outcome::WorkFailure, Action::Release) => Ok(()),
        (Outcome::WorkFailure, Action::None) => Ok(()),
        (Outcome::InfrastructureFailure, Action::None) => Ok(()),
        (Outcome::InfrastructureFailure, Action::Release) => Ok(()),
        (Outcome::Cancelled, Action::Close) => Ok(()),
        (Outcome::Cancelled, Action::Release) => Ok(()),
        (Outcome::Cancelled, Action::None) => Ok(()),
        (Outcome::Indeterminate, Action::Block) => Ok(()),
        (Outcome::Indeterminate, Action::Release) => Ok(()),
        (Outcome::Indeterminate, Action::None) => Ok(()),

        // Invalid combinations
        _ => Err(AttemptError::Usage(format!(
            "Invalid outcome-action combination: {} + {}",
            outcome.as_str(),
            action.as_str()
        ))),
    }
}

/// Resolve attempt request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolveRequest {
    /// Unique attempt identifier (required)
    pub attempt_id: String,

    /// Issue ID to resolve (required)
    pub issue_id: String,

    /// Outcome classification (required)
    pub outcome: String,

    /// Lifecycle action (optional, default "none")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,

    /// Human-readable reason (optional, default "")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,

    /// Expected revision for optimistic concurrency (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub if_revision: Option<i64>,

    /// Fencing token for lease validation (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fencing_token: Option<String>,

    /// Evidence references (optional)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub evidence_refs: Vec<String>,

    /// Actor identity (required)
    pub actor: String,

    /// Model identifier for telemetry (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    /// Harness name for telemetry (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub harness: Option<String>,

    /// Harness version for telemetry (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub harness_version: Option<String>,
}

impl ResolveRequest {
    /// Validate the request
    pub fn validate(&self) -> Result<(), AttemptError> {
        // Validate attempt_id
        if self.attempt_id.is_empty() {
            return Err(AttemptError::Usage(
                "attempt_id cannot be empty".to_string(),
            ));
        }
        if self.attempt_id.len() > 255 {
            return Err(AttemptError::Usage(
                "attempt_id cannot exceed 255 bytes".to_string(),
            ));
        }

        // Validate actor
        if self.actor.is_empty() {
            return Err(AttemptError::Usage("actor cannot be empty".to_string()));
        }
        if self.actor.len() > 255 {
            return Err(AttemptError::Usage(
                "actor cannot exceed 255 bytes".to_string(),
            ));
        }

        // Parse outcome
        let outcome = Outcome::parse(&self.outcome)?;

        // Parse action (default to "none")
        let action_str = self.action.as_deref().unwrap_or("none");
        let action = Action::parse(action_str)?;

        // Validate combination
        validate_outcome_action_combo(&outcome, &action)?;

        // Validate evidence refs format
        for ref_str in &self.evidence_refs {
            validate_evidence_ref(ref_str)?;
        }

        Ok(())
    }
}

/// Resolve attempt receipt returned from successful resolution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolveReceipt {
    /// Unique receipt ID
    pub receipt_id: String,

    /// Canonical request hash
    pub canonical_request_hash: String,

    /// Issue ID
    pub issue_id: String,

    /// Attempt ID
    pub attempt_id: String,

    /// Resulting issue revision
    pub resulting_issue_revision: i64,

    /// Resulting issue state
    pub resulting_state: String,

    /// Resulting attempt tier
    pub resulting_attempt_tier: i64,

    /// Creation timestamp (RFC 3339)
    pub created_at: String,

    /// Whether this was an idempotent replay
    pub is_replay: bool,
}

/// Attempt outcome record for checkpoints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttemptOutcomeRecord {
    /// Schema reference
    #[serde(rename = "$schema")]
    pub schema_ref: String,

    /// Attempt ID
    pub attempt_id: String,

    /// Issue ID
    pub issue_id: String,

    /// Outcome classification
    pub outcome: String,

    /// Lifecycle action
    pub action: String,

    /// Human-readable reason
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,

    /// Canonical request hash
    pub canonical_request_hash: String,

    /// Resulting issue revision
    pub resulting_issue_revision: i64,

    /// Resulting state
    pub resulting_state: String,

    /// Resulting attempt tier
    pub resulting_attempt_tier: i64,

    /// Receipt ID
    pub receipt_id: String,

    /// Actor identity
    pub actor: String,

    /// Creation timestamp (RFC 3339)
    pub created_at: String,

    /// Evidence references
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub evidence_refs: Vec<String>,

    /// Model identifier for telemetry
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    /// Harness name for telemetry
    #[serde(skip_serializing_if = "Option::is_none")]
    pub harness: Option<String>,

    /// Harness version for telemetry
    #[serde(skip_serializing_if = "Option::is_none")]
    pub harness_version: Option<String>,
}

/// Validate evidence reference format
fn validate_evidence_ref(ref_str: &str) -> Result<(), AttemptError> {
    if ref_str.is_empty() {
        return Err(AttemptError::Usage(
            "evidence_ref cannot be empty".to_string(),
        ));
    }

    // Check format: NAMESPACE:VALUE
    let parts: Vec<&str> = ref_str.splitn(2, ':').collect();
    if parts.len() != 2 {
        return Err(AttemptError::Usage(format!(
            "evidence_ref must be NAMESPACE:VALUE format, got: {}",
            ref_str
        )));
    }

    let namespace = parts[0];
    let value = parts[1];

    // Validate namespace: [a-z][a-z0-9-]*, 1-32 chars
    if namespace.is_empty() || namespace.len() > 32 {
        return Err(AttemptError::Usage(format!(
            "evidence_ref namespace must be 1-32 chars, got: {}",
            namespace
        )));
    }

    if !namespace
        .chars()
        .next()
        .map(|c| c.is_ascii_lowercase())
        .unwrap_or(false)
    {
        return Err(AttemptError::Usage(format!(
            "evidence_ref namespace must start with lowercase letter, got: {}",
            namespace
        )));
    }

    if !namespace
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(AttemptError::Usage(format!(
            "evidence_ref namespace must be [a-z][a-z0-9-]*, got: {}",
            namespace
        )));
    }

    // Validate value: 1-255 bytes, no control characters
    if value.is_empty() || value.len() > 255 {
        return Err(AttemptError::Usage(
            "evidence_ref value must be 1-255 bytes".to_string(),
        ));
    }

    if value.chars().any(|c| c.is_control()) {
        return Err(AttemptError::Usage(
            "evidence_ref value cannot contain control characters".to_string(),
        ));
    }

    Ok(())
}

/// Attempt resolution errors
#[derive(Debug, Clone, thiserror::Error)]
pub enum AttemptError {
    /// Usage or validation error (exit 2)
    #[error("Usage error: {0}")]
    Usage(String),

    /// Issue not found (exit 3)
    #[error("Issue not found: {0}")]
    NotFound(String),

    /// Conflict (revision/fencing/semantic mismatch) (exit 4)
    #[error("Conflict: {0}")]
    Conflict(String),

    /// Integrity failure (exit 5)
    #[error("Integrity error: {0}")]
    Integrity(String),

    /// Transient error (exit 6)
    #[error("Transient error: {0}")]
    Transient(String),
}

impl AttemptError {
    /// Map to exit code
    pub fn exit_code(&self) -> i32 {
        match self {
            AttemptError::Usage(_) => 2,
            AttemptError::NotFound(_) => 3,
            AttemptError::Conflict(_) => 4,
            AttemptError::Integrity(_) => 5,
            AttemptError::Transient(_) => 6,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_outcome_parse() {
        assert_eq!(
            Outcome::parse("verified_success").unwrap(),
            Outcome::VerifiedSuccess
        );
        assert_eq!(
            Outcome::parse("work_failure").unwrap(),
            Outcome::WorkFailure
        );
        assert!(Outcome::parse("unknown").is_err());
    }

    #[test]
    fn test_action_parse() {
        assert_eq!(Action::parse("close").unwrap(), Action::Close);
        assert_eq!(Action::parse("none").unwrap(), Action::None);
        assert!(Action::parse("unknown").is_err());
    }

    #[test]
    fn test_valid_combinations() {
        assert!(validate_outcome_action_combo(&Outcome::VerifiedSuccess, &Action::Close).is_ok());
        assert!(validate_outcome_action_combo(&Outcome::WorkFailure, &Action::Quarantine).is_ok());
        assert!(
            validate_outcome_action_combo(&Outcome::InfrastructureFailure, &Action::Release)
                .is_ok()
        );
    }

    #[test]
    fn test_invalid_combination() {
        assert!(
            validate_outcome_action_combo(&Outcome::VerifiedSuccess, &Action::Quarantine).is_err()
        );
    }

    #[test]
    fn test_evidence_ref_validation() {
        assert!(validate_evidence_ref("s3:build-logs/a1b2c3d4.tar.gz").is_ok());
        assert!(validate_evidence_ref("coverage:report-xyz.html").is_ok());
        assert!(validate_evidence_ref("invalid-format").is_err());
        assert!(validate_evidence_ref(":value").is_err());
        assert!(validate_evidence_ref("namespace:").is_err());
    }

    #[test]
    fn test_request_validation() {
        let req = ResolveRequest {
            attempt_id: "urn:needle:attempt:abc123".to_string(),
            issue_id: "bead-0123456789abcdef".to_string(),
            outcome: "verified_success".to_string(),
            action: Some("close".to_string()),
            reason: Some("All tests passing".to_string()),
            if_revision: None,
            fencing_token: None,
            evidence_refs: vec!["s3:logs/abc.tar.gz".to_string()],
            actor: "needle-worker-alpha".to_string(),
            model: Some("claude-opus-5".to_string()),
            harness: Some("needle".to_string()),
            harness_version: Some("1.2.3".to_string()),
        };
        assert!(req.validate().is_ok());
    }

    #[test]
    fn test_request_validation_empty_attempt_id() {
        let req = ResolveRequest {
            attempt_id: "".to_string(),
            issue_id: "bead-0123456789abcdef".to_string(),
            outcome: "verified_success".to_string(),
            action: None,
            reason: None,
            if_revision: None,
            fencing_token: None,
            evidence_refs: vec![],
            actor: "needle-worker-alpha".to_string(),
            model: None,
            harness: None,
            harness_version: None,
        };
        assert!(req.validate().is_err());
    }
}

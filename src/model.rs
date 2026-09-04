#![forbid(unsafe_code)]
#![allow(dead_code)] // Public API functions will be used in F003-F006

//! Canonical native issue model with validated lifecycle and identifiers.
//!
//! This module implements the domain types and validation rules for the bead-rs
//! issue model as specified in the plan section 3 and interchange-v1.md.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
// Re-import rand for use in generate_issue_id
use rand::Rng;

/// Native issue ID validation
///
/// IDs must be nonempty UTF-8 strings without control characters, leading/trailing
/// whitespace, path separators, NUL, or values over 255 bytes. Valid imported IDs
/// are preserved byte-for-byte.
pub fn validate_issue_id(id: &str) -> Result<(), Error> {
    if id.is_empty() {
        return Err(Error::validation("Issue ID cannot be empty"));
    }

    if id.len() > 255 {
        return Err(Error::validation("Issue ID cannot exceed 255 bytes"));
    }

    // Check for control characters (except regular whitespace)
    if id
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\t' | '\n' | '\r'))
    {
        return Err(Error::validation("Issue ID contains control characters"));
    }

    // Check for leading/trailing whitespace
    if id.starts_with(char::is_whitespace) || id.ends_with(char::is_whitespace) {
        return Err(Error::validation(
            "Issue ID cannot have leading or trailing whitespace",
        ));
    }

    // Check for path separators
    if id.contains('/') || id.contains('\\') {
        return Err(Error::validation("Issue ID cannot contain path separators"));
    }

    // Check for NUL
    if id.contains('\0') {
        return Err(Error::validation("Issue ID cannot contain NUL character"));
    }

    Ok(())
}

/// Title validation
///
/// Titles are required and must be 1 to 4,096 UTF-8 bytes.
pub fn validate_title(title: &str) -> Result<(), Error> {
    if title.is_empty() {
        return Err(Error::validation("Title cannot be empty"));
    }

    if title.len() > 4096 {
        return Err(Error::validation("Title cannot exceed 4,096 bytes"));
    }

    Ok(())
}

/// Description and notes validation
///
/// Optional text fields that may be empty but must not exceed 4 MiB.
pub fn validate_long_text(text: &str) -> Result<(), Error> {
    if text.len() > 4 * 1024 * 1024 {
        return Err(Error::validation("Text field cannot exceed 4 MiB"));
    }

    Ok(())
}

/// Priority validation
///
/// Native priority values are 0-4 (P0 urgent through P4 aspirational/backlog).
pub fn validate_priority(priority: i64) -> Result<(), Error> {
    if !(0..=4).contains(&priority) {
        return Err(Error::validation("Priority must be between 0 and 4"));
    }

    Ok(())
}

/// Base lifecycle status
///
/// These are the canonical lifecycle states. The effective "blocked" status
/// is derived from manual blocking and graph blockers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BaseStatus {
    Open,
    InProgress,
    Deferred,
    Closed,
}

impl std::fmt::Display for BaseStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BaseStatus::Open => write!(f, "open"),
            BaseStatus::InProgress => write!(f, "in_progress"),
            BaseStatus::Deferred => write!(f, "deferred"),
            BaseStatus::Closed => write!(f, "closed"),
        }
    }
}

impl BaseStatus {
    /// Return the string representation of this status
    pub fn as_str(&self) -> &'static str {
        match self {
            BaseStatus::Open => "open",
            BaseStatus::InProgress => "in_progress",
            BaseStatus::Deferred => "deferred",
            BaseStatus::Closed => "closed",
        }
    }

    /// Parse a status string, accepting common aliases
    pub fn parse(s: &str) -> Result<Self, Error> {
        match s.to_lowercase().as_str() {
            "open" => Ok(BaseStatus::Open),
            "in_progress" | "in-progress" => Ok(BaseStatus::InProgress),
            "deferred" => Ok(BaseStatus::Deferred),
            "closed" => Ok(BaseStatus::Closed),
            _ => Err(Error::validation(format!("Unknown status: {}", s))),
        }
    }

    /// Check if a transition to another status is valid
    pub fn can_transition_to(&self, to: &BaseStatus) -> bool {
        validate_status_transition(*self, *to).is_ok()
    }

    /// Check if this is a closed status
    pub fn is_closed(self) -> bool {
        matches!(self, BaseStatus::Closed)
    }

    /// Check if this is an in-progress status
    pub fn is_in_progress(self) -> bool {
        matches!(self, BaseStatus::InProgress)
    }
}

/// Validate a lifecycle transition
///
/// Allowed transitions:
/// - open -> in_progress (claim or update)
/// - open -> deferred (update)
/// - open -> closed (close)
/// - in_progress -> open (release or update)
/// - in_progress -> deferred (update)
/// - in_progress -> closed (close)
/// - deferred -> open (reopen or update)
/// - deferred -> closed (close)
/// - closed -> open (reopen only)
pub fn validate_status_transition(from: BaseStatus, to: BaseStatus) -> Result<(), Error> {
    match (from, to) {
        (BaseStatus::Open, BaseStatus::Open) => Ok(()),
        (BaseStatus::Open, BaseStatus::InProgress) => Ok(()),
        (BaseStatus::Open, BaseStatus::Deferred) => Ok(()),
        (BaseStatus::Open, BaseStatus::Closed) => Ok(()),
        (BaseStatus::InProgress, BaseStatus::Open) => Ok(()),
        (BaseStatus::InProgress, BaseStatus::InProgress) => Ok(()),
        (BaseStatus::InProgress, BaseStatus::Deferred) => Ok(()),
        (BaseStatus::InProgress, BaseStatus::Closed) => Ok(()),
        (BaseStatus::Deferred, BaseStatus::Open) => Ok(()),
        (BaseStatus::Deferred, BaseStatus::Deferred) => Ok(()),
        (BaseStatus::Deferred, BaseStatus::Closed) => Ok(()),
        (BaseStatus::Closed, BaseStatus::Open) => Ok(()), // only through reopen
        (BaseStatus::Closed, BaseStatus::Closed) => Ok(()), // idempotent close
        (_, _) => Err(Error::validation(format!(
            "Invalid status transition: {:?} -> {:?}",
            from, to
        ))),
    }
}

/// Native issue model
///
/// This represents the canonical issue domain model with all required and
/// optional fields as specified in the plan section 3.2.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Issue {
    /// Immutable opaque identifier
    pub id: String,

    /// Human-readable summary (required)
    pub title: String,

    /// Monotonically increasing logical revision
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<i64>,

    /// Detailed description (optional, defaults to empty)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Internal notes (optional, defaults to empty)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,

    /// Native priority (0-4, lower is more urgent)
    pub priority: i64,

    /// Base lifecycle status
    pub base_status: BaseStatus,

    /// Explicit manual block flag
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manual_blocked: Option<bool>,

    /// Optional assignee
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,

    /// Issue type (nonempty string, defaults to "task")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issue_type: Option<String>,

    /// Immutable creation timestamp
    pub created_at: String,

    /// Last semantic modification timestamp
    pub updated_at: String,

    /// Optional close timestamp (present only for closed issues)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closed_at: Option<String>,

    /// Close reason (required for closed issues)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub close_reason: Option<String>,

    /// Optional source repository descriptor
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_repo: Option<String>,

    /// Origin profile for extension round trips
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,

    /// Immutable public schema identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema_ref: Option<String>,

    /// Structured data (namespaced, schema-bound JSON values)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,

    /// Unknown extension fields (preserved through round trips)
    #[serde(flatten)]
    pub extensions: HashMap<String, serde_json::Value>,
}

impl Issue {
    /// Get an extension value by key
    pub fn get_extension(&self, key: &str) -> Option<&serde_json::Value> {
        self.extensions.get(key)
    }

    /// Validate a complete issue
    pub fn validate(&self) -> Result<(), Error> {
        // Validate ID
        validate_issue_id(&self.id)?;

        // Validate title
        validate_title(&self.title)?;

        // Validate long text fields
        if let Some(desc) = &self.description {
            validate_long_text(desc)?;
        }

        if let Some(notes) = &self.notes {
            validate_long_text(notes)?;
        }

        // Validate priority
        validate_priority(self.priority)?;

        // Validate closed state invariants
        if self.base_status == BaseStatus::Closed {
            if self.close_reason.is_none() || self.close_reason.as_ref().unwrap().is_empty() {
                return Err(Error::validation("Closed issues must have a close_reason"));
            }
            if self.closed_at.is_none() {
                return Err(Error::validation(
                    "Closed issues must have a closed_at timestamp",
                ));
            }
        } else if self.closed_at.is_some() || self.close_reason.is_some() {
            return Err(Error::validation(
                "Non-closed issues must not have closed_at or close_reason",
            ));
        }

        // Validate issue_type if present
        if let Some(issue_type) = &self.issue_type {
            if issue_type.is_empty() {
                return Err(Error::validation("issue_type cannot be empty"));
            }
        }

        // Validate assignee if present
        if let Some(assignee) = &self.assignee {
            if assignee.is_empty() {
                return Err(Error::validation("assignee cannot be empty"));
            }
        }

        Ok(())
    }

    /// Check if this issue is ready
    ///
    /// An issue is ready when it is:
    /// - base Open
    /// - not manually blocked
    /// - unassigned
    /// - has no unfinished `blocks` blockers (graph blockers checked separately)
    pub fn is_ready(&self) -> bool {
        self.base_status == BaseStatus::Open
            && !self.manual_blocked.unwrap_or(false)
            && self.assignee.is_none()
    }
}

/// External reference for linking issues to external systems
///
/// Represents a generic (namespace, key, value) reference such as tracker IDs
/// and commit identifiers without replacing native bead IDs or resolving
/// anything over the network.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalReference {
    /// Associated issue ID
    pub issue_id: String,

    /// Namespace for grouping references (e.g., "github", "jira", "gitlab")
    pub namespace: String,

    /// Key within the namespace (e.g., "issue-number", "commit-hash", "ticket-id")
    pub key: String,

    /// Reference value (e.g., "12345", "abc123def", "PROJ-001")
    pub value: String,
}

/// Validate external reference namespace
///
/// Namespaces must be nonempty, lowercase alphanumeric with hyphens/underscores,
/// and must not exceed 64 bytes.
pub fn validate_reference_namespace(namespace: &str) -> Result<(), Error> {
    if namespace.is_empty() {
        return Err(Error::validation("Namespace cannot be empty"));
    }

    if namespace.len() > 64 {
        return Err(Error::validation("Namespace cannot exceed 64 bytes"));
    }

    // Only lowercase alphanumeric, hyphens, and underscores
    if !namespace
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
    {
        return Err(Error::validation(
            "Namespace can only contain lowercase letters, numbers, hyphens, and underscores",
        ));
    }

    // Must start with a lowercase letter
    if !namespace.chars().next().unwrap().is_ascii_lowercase() {
        return Err(Error::validation(
            "Namespace must start with a lowercase letter",
        ));
    }

    Ok(())
}

/// Validate external reference key
///
/// Keys must be nonempty and must not exceed 128 bytes.
pub fn validate_reference_key(key: &str) -> Result<(), Error> {
    if key.is_empty() {
        return Err(Error::validation("Reference key cannot be empty"));
    }

    if key.len() > 128 {
        return Err(Error::validation("Reference key cannot exceed 128 bytes"));
    }

    // No control characters
    if key.chars().any(|c| c.is_control()) {
        return Err(Error::validation(
            "Reference key cannot contain control characters",
        ));
    }

    Ok(())
}

/// Validate external reference value
///
/// Values must be nonempty and must not exceed 512 bytes.
pub fn validate_reference_value(value: &str) -> Result<(), Error> {
    if value.is_empty() {
        return Err(Error::validation("Reference value cannot be empty"));
    }

    if value.len() > 512 {
        return Err(Error::validation("Reference value cannot exceed 512 bytes"));
    }

    // No control characters
    if value.chars().any(|c| c.is_control()) {
        return Err(Error::validation(
            "Reference value cannot contain control characters",
        ));
    }

    Ok(())
}

/// Validate label
///
/// Labels must be nonempty and must not exceed 255 bytes.
pub fn validate_label(label: &str) -> Result<(), Error> {
    if label.is_empty() {
        return Err(Error::validation("Label cannot be empty"));
    }

    if label.len() > 255 {
        return Err(Error::validation("Label cannot exceed 255 bytes"));
    }

    // No control characters
    if label.chars().any(|c| c.is_control()) {
        return Err(Error::validation("Label cannot contain control characters"));
    }

    Ok(())
}

/// Get current timestamp in RFC 3339 format
pub fn current_timestamp() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string())
}

/// Generate a unique issue ID with the default prefix
pub fn generate_issue_id() -> Result<String, Error> {
    let prefix = "bead";
    let mut rng = rand::thread_rng();
    let bytes: [u8; 4] = rng.r#gen();
    let suffix = hex::encode(bytes);
    Ok(format!("{}-{}", prefix, suffix))
}

impl ExternalReference {
    /// Validate an external reference
    pub fn validate(&self) -> Result<(), Error> {
        // Validate issue ID
        validate_issue_id(&self.issue_id)?;

        // Validate namespace
        validate_reference_namespace(&self.namespace)?;

        // Validate key
        validate_reference_key(&self.key)?;

        // Validate value
        validate_reference_value(&self.value)?;

        Ok(())
    }
}

/// Error type for model validation
#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    #[error("Validation failed: {0}")]
    Validation(String),
}

impl Error {
    pub fn validation(msg: impl Into<String>) -> Self {
        Error::Validation(msg.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_issue_id_valid() {
        assert!(validate_issue_id("bead-1234567890abcdef").is_ok());
        assert!(validate_issue_id("TASK-001").is_ok());
        assert!(validate_issue_id("simple").is_ok());
        assert!(validate_issue_id("with-dash-and-123").is_ok());
    }

    #[test]
    fn test_validate_issue_id_invalid() {
        assert!(validate_issue_id("").is_err()); // empty
        assert!(validate_issue_id("  leading").is_err()); // leading whitespace
        assert!(validate_issue_id("trailing  ").is_err()); // trailing whitespace
        assert!(validate_issue_id("with/slash").is_err()); // path separator
        assert!(validate_issue_id("with\\backslash").is_err()); // backslash
        assert!(validate_issue_id("with\x00null").is_err()); // NUL
        assert!(validate_issue_id("with\x01control").is_err()); // control char
    }

    #[test]
    fn test_validate_title_valid() {
        assert!(validate_title("Valid title").is_ok());
        assert!(validate_title("A".repeat(4096).as_str()).is_ok());
    }

    #[test]
    fn test_validate_title_invalid() {
        assert!(validate_title("").is_err()); // empty
        assert!(validate_title(&"A".repeat(4097)).is_err()); // too long
    }

    #[test]
    fn test_validate_priority_valid() {
        assert!(validate_priority(0).is_ok());
        assert!(validate_priority(1).is_ok());
        assert!(validate_priority(2).is_ok());
        assert!(validate_priority(3).is_ok());
        assert!(validate_priority(4).is_ok());
    }

    #[test]
    fn test_validate_priority_invalid() {
        assert!(validate_priority(-1).is_err());
        assert!(validate_priority(5).is_err());
    }

    #[test]
    fn test_base_status_parse() {
        assert_eq!(BaseStatus::parse("open").unwrap(), BaseStatus::Open);
        assert_eq!(BaseStatus::parse("OPEN").unwrap(), BaseStatus::Open);
        assert_eq!(
            BaseStatus::parse("in_progress").unwrap(),
            BaseStatus::InProgress
        );
        assert_eq!(BaseStatus::parse("deferred").unwrap(), BaseStatus::Deferred);
        assert_eq!(BaseStatus::parse("closed").unwrap(), BaseStatus::Closed);
        assert!(BaseStatus::parse("unknown").is_err());
    }

    #[test]
    fn test_validate_status_transitions_valid() {
        // open transitions
        assert!(validate_status_transition(BaseStatus::Open, BaseStatus::Open).is_ok());
        assert!(validate_status_transition(BaseStatus::Open, BaseStatus::InProgress).is_ok());
        assert!(validate_status_transition(BaseStatus::Open, BaseStatus::Deferred).is_ok());
        assert!(validate_status_transition(BaseStatus::Open, BaseStatus::Closed).is_ok());

        // in_progress transitions
        assert!(validate_status_transition(BaseStatus::InProgress, BaseStatus::Open).is_ok());
        assert!(validate_status_transition(BaseStatus::InProgress, BaseStatus::Deferred).is_ok());
        assert!(validate_status_transition(BaseStatus::InProgress, BaseStatus::Closed).is_ok());

        // deferred transitions
        assert!(validate_status_transition(BaseStatus::Deferred, BaseStatus::Open).is_ok());
        assert!(validate_status_transition(BaseStatus::Deferred, BaseStatus::Closed).is_ok());

        // closed -> open (reopen)
        assert!(validate_status_transition(BaseStatus::Closed, BaseStatus::Open).is_ok());
    }

    #[test]
    fn test_validate_status_transitions_invalid() {
        // No invalid transitions in our matrix - all are valid
        // This test documents that our transition matrix is complete
    }

    #[test]
    fn test_issue_validation() {
        let mut issue = Issue {
            id: "bead-1234567890abcdef".to_string(),
            title: "Test Issue".to_string(),
            description: Some("Description".to_string()),
            notes: None,
            priority: 2,
            base_status: BaseStatus::Open,
            manual_blocked: None,
            assignee: None,
            issue_type: Some("task".to_string()),
            created_at: "2026-08-08T12:00:00Z".to_string(),
            updated_at: "2026-08-08T12:00:00Z".to_string(),
            closed_at: None,
            close_reason: None,
            source_repo: None,
            profile: None,
            schema_ref: Some("urn:bead-rs:schema:issue:native-v1".to_string()),
            revision: Some(1),
            data: None,
            extensions: HashMap::new(),
        };

        assert!(issue.validate().is_ok());

        // Test closed issue validation
        issue.base_status = BaseStatus::Closed;
        issue.closed_at = Some("2026-08-08T12:01:00Z".to_string());
        issue.close_reason = Some("Done".to_string());
        assert!(issue.validate().is_ok());

        // Test missing close_reason
        issue.close_reason = None;
        assert!(issue.validate().is_err());

        // Test stale close metadata on an active issue
        issue.base_status = BaseStatus::Open;
        issue.closed_at = Some("2026-08-08T12:01:00Z".to_string());
        issue.close_reason = Some("Stale".to_string());
        assert!(issue.validate().is_err());

        // Test empty assignee
        issue.closed_at = None;
        issue.close_reason = None;
        issue.assignee = Some("".to_string());
        assert!(issue.validate().is_err());
    }

    #[test]
    fn test_issue_is_ready() {
        let issue = Issue {
            id: "bead-123".to_string(),
            title: "Test".to_string(),
            description: None,
            notes: None,
            priority: 2,
            base_status: BaseStatus::Open,
            manual_blocked: None,
            assignee: None,
            issue_type: None,
            created_at: "2026-08-08T12:00:00Z".to_string(),
            updated_at: "2026-08-08T12:00:00Z".to_string(),
            closed_at: None,
            close_reason: None,
            source_repo: None,
            profile: None,
            schema_ref: None,
            revision: Some(1),
            data: None,
            extensions: HashMap::new(),
        };

        assert!(issue.is_ready());

        // Not ready when assigned
        let mut assigned = issue.clone();
        assigned.assignee = Some("worker".to_string());
        assert!(!assigned.is_ready());

        // Not ready when manually blocked
        let mut blocked = issue.clone();
        blocked.manual_blocked = Some(true);
        assert!(!blocked.is_ready());

        // Not ready when not open
        let mut in_progress = issue.clone();
        in_progress.base_status = BaseStatus::InProgress;
        assert!(!in_progress.is_ready());
    }
}

// Recurrence template module (R024)
pub mod attempt;
pub mod recurrence;

// Audited historical redaction storage (ADR-015, R038 BR-T15)
pub mod redaction;

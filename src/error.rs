//! Error types for bead-rs
//!
//! This module defines the error taxonomy used throughout the application.
//! Exit codes are mapped at the CLI boundary.

use std::path::PathBuf;
use thiserror::Error;

/// Structured validation failures that have an operation-specific exit code.
#[derive(Error, Debug, PartialEq, Eq)]
pub enum ValidationError {
    /// The dependency kind is not supported by the native dependency graph.
    #[error("Invalid dependency kind: {kind}")]
    InvalidKind { kind: String },
}

/// Main error type for bead-rs operations
#[derive(Error, Debug)]
pub enum Error {
    /// CLI usage or validation error (exit 2)
    #[error("CLI usage error: {0}")]
    CliUsage(String),

    /// Workspace or not-found error (exit 3)
    #[error("Workspace error: {0}")]
    Workspace(String),

    /// Conflict, invalid transition, or dependency cycle (exit 4)
    #[error("Conflict: {0}")]
    #[allow(dead_code)]
    Conflict(String),

    /// Structured validation failure (exit 4)
    #[error("Validation error: {0}")]
    Validation(#[from] ValidationError),

    /// Lease expiry or conflict (exit 4)
    #[error("Lease error: {0}")]
    LeaseExpired(String),

    /// Lease fencing token conflict (exit 4)
    #[error("Lease conflict: {0}")]
    LeaseConflict(String),

    /// Claim refused by an opt-in claim-time guard, e.g. --single-claim (exit 4)
    ///
    /// `code` carries the machine-readable reason code (snake_case, sourced
    /// from the claim ReasonCode taxonomy); `message` names the blocking
    /// state, including the blocking issue ID.
    #[error("{code}: {message}")]
    ClaimRefused { code: String, message: String },

    /// Integrity, import, or migration failure (exit 5)
    #[error("Integrity error: {0}")]
    #[allow(dead_code)]
    Integrity(String),

    /// Transient database busy or I/O failure (exit 6)
    #[error("Database busy or I/O error: {0}")]
    #[allow(dead_code)]
    DatabaseBusy(String),

    /// Automatic checkpoint publication failed after the mutation committed
    /// (exit 1) -- the split outcome of plan 6.2.1 item 5.
    ///
    /// Constructed only by the post-commit publication chokepoint, strictly
    /// after the command's own transaction committed and its success output
    /// printed. Carrying the failure in a dedicated variant is what makes
    /// the outcome defined rather than incidental: exit 1 with this message
    /// always means the mutation is still committed and visible and only the
    /// durable checkpoint is behind, never that the mutation was rolled
    /// back. The underlying publication error rides along as `source`.
    #[error(
        "checkpoint publication failed after the mutation committed: {source}. \
         The mutation is still committed and visible - exit 1 here never means \
         it was rolled back. The durable checkpoint did not advance; run \
         'bead sync flush-only' to publish it"
    )]
    PostCommitPublicationFailed {
        #[source]
        source: anyhow::Error,
    },

    /// Historical redaction committed but its mandatory sanitized checkpoint
    /// publication did not complete (exit 1). The receipt identity is safe to
    /// print and is the only input accepted by the resume path.
    #[error(
        "historical redaction receipt {receipt_id} committed, but sanitized checkpoint publication failed: {source}. The redaction remains committed; resume with 'bead redact --resume {receipt_id}'"
    )]
    RedactionPublicationFailed {
        receipt_id: String,
        #[source]
        source: anyhow::Error,
    },

    /// Typed historical-redaction validation/not-found/conflict failure.
    #[error("{0}")]
    Redaction(#[from] crate::model::redaction::RedactionError),

    /// Uncategorized internal failure (exit 1)
    #[error("Internal error: {0}")]
    Internal(#[from] anyhow::Error),

    /// SQLite-specific errors
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    /// JSON serialization/deserialization errors
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Model validation errors
    #[error("Model validation error: {0}")]
    Model(#[from] crate::model::Error),

    /// I/O errors
    #[error("I/O error: {path}: {msg}")]
    Io {
        path: PathBuf,
        #[source]
        msg: std::io::Error,
    },
}

impl Error {
    /// Map the error to its appropriate exit code
    pub fn exit_code(&self) -> i32 {
        match self {
            Error::CliUsage(_) | Error::Model(_) => 2,
            Error::Workspace(_) => 3,
            Error::Conflict(_)
            | Error::Validation(_)
            | Error::LeaseExpired(_)
            | Error::LeaseConflict(_)
            | Error::ClaimRefused { .. } => 4,
            Error::Integrity(_) => 5,
            Error::DatabaseBusy(_) => 6,
            // Defined, not inherited from the catch-all: plan 6.2.1 item 5
            // pins this split outcome to exit 1 regardless of what the
            // underlying publication error would have mapped to on its own.
            Error::PostCommitPublicationFailed { .. }
            | Error::RedactionPublicationFailed { .. } => 1,
            Error::Redaction(error) => error.exit_code(),
            _ => 1,
        }
    }

    /// Create a CLI usage error
    pub fn cli_usage(msg: impl Into<String>) -> Self {
        Error::CliUsage(msg.into())
    }

    /// Create a workspace error
    pub fn workspace(msg: impl Into<String>) -> Self {
        Error::Workspace(msg.into())
    }

    /// Create a not-found error (uses workspace error type)
    pub fn not_found(msg: impl Into<String>) -> Self {
        Error::Workspace(msg.into())
    }

    /// Create a validation error
    pub fn validation(msg: impl Into<String>) -> Self {
        Error::CliUsage(msg.into())
    }

    /// Create a conflict error
    #[allow(dead_code)]
    pub fn conflict(msg: impl Into<String>) -> Self {
        Error::Conflict(msg.into())
    }

    /// Create an integrity error
    #[allow(dead_code)]
    pub fn integrity(msg: impl Into<String>) -> Self {
        Error::Integrity(msg.into())
    }

    /// Create a database busy error
    #[allow(dead_code)]
    pub fn database_busy(msg: impl Into<String>) -> Self {
        Error::DatabaseBusy(msg.into())
    }
}

/// Result type alias for bead-rs operations
pub type Result<T> = std::result::Result<T, Error>;

//! Error types for bead-rs
//!
//! This module defines the error taxonomy used throughout the application.
//! Exit codes are mapped at the CLI boundary.

use std::path::PathBuf;
use thiserror::Error;

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

    /// Lease expiry or conflict (exit 4)
    #[error("Lease error: {0}")]
    LeaseExpired(String),

    /// Lease fencing token conflict (exit 4)
    #[error("Lease conflict: {0}")]
    LeaseConflict(String),

    /// Integrity, import, or migration failure (exit 5)
    #[error("Integrity error: {0}")]
    #[allow(dead_code)]
    Integrity(String),

    /// Transient database busy or I/O failure (exit 6)
    #[error("Database busy or I/O error: {0}")]
    #[allow(dead_code)]
    DatabaseBusy(String),

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
            Error::Conflict(_) | Error::LeaseExpired(_) | Error::LeaseConflict(_) => 4,
            Error::Integrity(_) => 5,
            Error::DatabaseBusy(_) => 6,
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

//! Capabilities service for machine-readable feature discovery
//!
//! This module provides the `bead capabilities` command that returns
//! a JSON document describing what this implementation supports.

use crate::error::Result;
use crate::service::checkpoint::AUTO_FLUSH_COMPILED_DEFAULT;
use serde::{Deserialize, Serialize};

/// Capabilities document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capabilities {
    /// Contract identifier (e.g., "needle-v1", "native-v1")
    pub contract: String,
    /// Implementation name
    pub implementation: String,
    /// Version string
    pub version: String,
    /// Store layout version
    pub store_layout: i32,
    /// Whether atomic claim is supported
    pub atomic_claim: bool,
    /// Priority range and behavior
    pub priorities: Priorities,
    /// Valid status values
    pub statuses: Vec<String>,
    /// Supported checkpoint modes
    pub checkpoint_modes: Vec<String>,
    /// Supported checkpoint formats
    pub checkpoint_formats: Vec<String>,
    /// Whether logical revision guards are supported
    #[serde(rename = "logical_revision")]
    pub logical_revision: bool,
    /// Schema reference for this capabilities document
    #[serde(rename = "schema_ref")]
    pub schema_ref: String,
    /// Supported schemas
    pub schemas: Vec<SchemaEntry>,
    /// Available commands
    pub commands: Vec<String>,
    /// Whether this binary publishes a checkpoint generation after every
    /// successful semantic mutation (plan 6.2.1, R026). Reports the
    /// compiled default, never workspace state: `checkpoint.auto_flush`
    /// and `--no-auto-flush` suppress publication without changing this
    /// advertisement, and `sync --status` remains the only authority on
    /// whether a given workspace is clean. Present and `true` since the
    /// R026 activation flipped the compiled default on (plan section 11).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_flush: Option<bool>,
}

/// Priority capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Priorities {
    /// Minimum priority value
    pub min: i32,
    /// Maximum priority value
    pub max: i32,
    /// Default priority value
    pub default: i32,
    /// Whether P4 is claimable under fifo-v1
    #[serde(rename = "p4_claimable_by_fifo")]
    pub p4_claimable_by_fifo: bool,
}

/// Schema catalog entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaEntry {
    /// Schema reference (URN)
    #[serde(rename = "schema_ref")]
    pub schema_ref: String,
    /// Document kind
    #[serde(rename = "document_kind")]
    pub document_kind: String,
    /// Whether validation is supported
    pub validate: bool,
    /// Whether this schema can be read (deserialized/parsed)
    pub readable: bool,
    /// Whether this schema can be written (serialized/emitted)
    pub writable: bool,
    /// Optional description of lossy support or read-only limitations
    pub lossy: Option<String>,
    /// Operations that consume this document
    pub consume: Vec<String>,
    /// Operations that emit this document
    pub emit: Vec<String>,
}

/// Generate capabilities for the native profile
pub fn generate_capabilities(profile: &str) -> Result<Capabilities> {
    // Validate profile
    if profile != "native-v1" && profile != "needle-v1" {
        return Err(crate::Error::validation(format!(
            "Unsupported profile: {}. Supported profiles: native-v1, needle-v1",
            profile
        )));
    }

    let contract = if profile == "needle-v1" {
        "needle-v1".to_string()
    } else {
        "native-v1".to_string()
    };

    Ok(Capabilities {
        contract,
        implementation: "bead-rs".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        store_layout: 1,
        atomic_claim: true,
        priorities: Priorities {
            min: 0,
            max: 4,
            default: 2,
            p4_claimable_by_fifo: true,
        },
        logical_revision: true,
        // "blocked" is advertised alongside the BaseStatus values: it is a
        // settable status overlay (`update --status blocked` sets
        // manual_blocked) and a filterable/reportable effective status, so
        // consumers need it in the list (plan capabilities example,
        // needle-v1 contract).
        statuses: vec![
            "blocked".to_string(),
            "closed".to_string(),
            "deferred".to_string(),
            "in_progress".to_string(),
            "open".to_string(),
        ],
        checkpoint_modes: vec!["monolithic".to_string(), "sharded".to_string()],
        checkpoint_formats: vec![
            "issues-jsonl-v1".to_string(),
            "checkpoint-set-v1".to_string(),
        ],
        schema_ref: "urn:bead-rs:schema:capabilities:native-v1".to_string(),
        schemas: crate::service::schema::schema_catalog()?,
        // All public root commands in alphabetical order
        commands: vec![
            "capabilities".to_string(),
            "changes".to_string(),
            "claim".to_string(),
            "close".to_string(),
            "compare".to_string(),
            "create".to_string(),
            "data".to_string(),
            "dep".to_string(),
            "doctor".to_string(),
            "init".to_string(),
            "label".to_string(),
            "list".to_string(),
            "policy".to_string(),
            "query".to_string(),
            "recurrence".to_string(),
            "ref".to_string(),
            "release".to_string(),
            "reopen".to_string(),
            "restore".to_string(),
            "show".to_string(),
            "sync".to_string(),
            "update".to_string(),
            "why".to_string(),
        ],
        // The additive R026 handshake (plan section 11): `auto_flush`
        // reports the compiled default, `true` since the activation
        // flipped it on. The workspace key and the per-invocation
        // flag change behavior, never the advertisement.
        auto_flush: AUTO_FLUSH_COMPILED_DEFAULT.then_some(AUTO_FLUSH_COMPILED_DEFAULT),
    })
}

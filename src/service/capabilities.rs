//! Capabilities service for machine-readable feature discovery
//!
//! This module provides the `bead capabilities` command that returns
//! a JSON document describing what this implementation supports.

use crate::error::Result;
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
        version: "0.1.0".to_string(),
        store_layout: 1,
        atomic_claim: true,
        priorities: Priorities {
            min: 0,
            max: 4,
            default: 2,
            p4_claimable_by_fifo: true,
        },
        logical_revision: true,
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
        schemas: vec![
            SchemaEntry {
                schema_ref: "urn:bead-rs:schema:event:native-v1".to_string(),
                document_kind: "audit_event".to_string(),
                validate: true,
                readable: true,
                writable: true,
                lossy: None,
                consume: vec![],
                emit: vec!["checkpoint-set-v1".to_string()],
            },
            SchemaEntry {
                schema_ref: "urn:bead-rs:schema:issue:native-v1".to_string(),
                document_kind: "issue".to_string(),
                validate: true,
                readable: true,
                writable: true,
                lossy: None,
                consume: vec!["sync.import-only".to_string()],
                emit: vec![
                    "sync.flush-only".to_string(),
                    "checkpoint-set-v1".to_string(),
                ],
            },
            SchemaEntry {
                schema_ref: "urn:bead-rs:schema:migration-receipt:native-v1".to_string(),
                document_kind: "migration_receipt".to_string(),
                validate: true,
                readable: true,
                writable: true,
                lossy: None,
                consume: vec![],
                emit: vec!["migrate".to_string(), "checkpoint-set-v1".to_string()],
            },
            SchemaEntry {
                schema_ref: "urn:bead-rs:schema:provenance-receipt:native-v1".to_string(),
                document_kind: "provenance_receipt".to_string(),
                validate: true,
                readable: true,
                writable: true,
                lossy: None,
                consume: vec!["checkpoint-set-v1".to_string()],
                emit: vec!["checkpoint-set-v1".to_string()],
            },
            SchemaEntry {
                schema_ref: "urn:bead-rs:schema:checkpoint-pointer:native-v1".to_string(),
                document_kind: "checkpoint_pointer".to_string(),
                validate: true,
                readable: true,
                writable: true,
                lossy: None,
                consume: vec!["checkpoint-set-v1".to_string()],
                emit: vec!["checkpoint-set-v1".to_string()],
            },
            SchemaEntry {
                schema_ref: "urn:bead-rs:schema:checkpoint-manifest:native-v1".to_string(),
                document_kind: "checkpoint_manifest".to_string(),
                validate: true,
                readable: true,
                writable: true,
                lossy: None,
                consume: vec!["checkpoint-set-v1".to_string()],
                emit: vec!["checkpoint-set-v1".to_string()],
            },
        ],
        // All public root commands in alphabetical order
        commands: vec![
            "capabilities".to_string(),
            "claim".to_string(),
            "close".to_string(),
            "create".to_string(),
            "dep".to_string(),
            "doctor".to_string(),
            "init".to_string(),
            "label".to_string(),
            "list".to_string(),
            "migrate".to_string(),
            "release".to_string(),
            "reopen".to_string(),
            "schema".to_string(),
            "show".to_string(),
            "sync".to_string(),
            "update".to_string(),
        ],
    })
}

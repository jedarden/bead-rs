//! Capabilities service for machine-readable feature discovery
//!
//! This module provides the `bead capabilities` command that returns
//! a JSON document describing what this implementation supports.

use crate::error::Result;
use crate::scan::{Mode, CONTRACT_IDENTITY, RULESET_VERSION};
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
    /// Attempt outcome resolution capabilities (ADR-012)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempt_outcome: Option<AttemptOutcome>,
    /// Secret rejection and diagnostic capabilities (ADR-014).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret_scan: Option<SecretScanCapabilities>,
    /// Audited historical-redaction capability handshake (ADR-015).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub historical_redaction: Option<HistoricalRedactionCapabilities>,
}

/// Offline secret-scanning capability handshake.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretScanCapabilities {
    pub contract_identity: String,
    pub ruleset_version: u32,
    pub effective_mode: String,
    pub blocking: bool,
    pub advisory: bool,
    pub exact_fingerprint_acknowledgment: bool,
}

/// Exceptional maintenance capability for already-stored sensitive bytes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoricalRedactionCapabilities {
    pub contract: String,
    pub doctor_findings: bool,
    pub atomic_redact: bool,
    pub anti_resurrection: bool,
    pub sanitized_generation_set: bool,
    pub resumable_publication: bool,
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

/// Attempt outcome resolution capabilities (ADR-012)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttemptOutcome {
    /// Whether attempt outcome resolution is supported
    pub supported: bool,
    /// Supported outcome classifications
    pub outcomes: Vec<String>,
    /// Supported lifecycle actions
    pub actions: Vec<String>,
    /// Whether idempotent replay detection is supported
    pub replay_detection: bool,
    /// Whether revision guards are supported
    pub revision_guard: bool,
    /// Whether fencing tokens are supported
    pub fencing_token: bool,
    /// Whether evidence references are supported
    pub evidence_refs: bool,
    /// Schema reference for resolve receipt
    #[serde(rename = "resolve_receipt_schema")]
    pub resolve_receipt_schema: String,
    /// Schema reference for resolve request
    #[serde(rename = "resolve_request_schema")]
    pub resolve_request_schema: String,
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
    generate_capabilities_with_secret_mode(profile, Mode::Enforce)
}

/// Generate capabilities using the effective policy of a discovered
/// workspace. Callers without a workspace use [`generate_capabilities`],
/// which advertises the compiled `enforce` default.
pub fn generate_capabilities_with_secret_mode(
    profile: &str,
    secret_mode: Mode,
) -> Result<Capabilities> {
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
        statuses: vec![
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
            "analyze-exclusion".to_string(),
            "capabilities".to_string(),
            "changes".to_string(),
            "claim".to_string(),
            "close".to_string(),
            "compare".to_string(),
            "create".to_string(),
            "data".to_string(),
            "dep".to_string(),
            "doctor".to_string(),
            "help".to_string(),
            "init".to_string(),
            "label".to_string(),
            "list".to_string(),
            "manifest".to_string(),
            "policy".to_string(),
            "query".to_string(),
            "redact".to_string(),
            "recurrence".to_string(),
            "ref".to_string(),
            "release".to_string(),
            "reopen".to_string(),
            "resolve".to_string(),
            "restore".to_string(),
            "resource".to_string(),
            "schema".to_string(),
            "show".to_string(),
            "sync".to_string(),
            "update".to_string(),
            "watchdog".to_string(),
            "why".to_string(),
        ],
        // The additive R026 handshake (plan section 11): `auto_flush`
        // reports the compiled default, `true` since the activation
        // flipped it on. The workspace key and the per-invocation
        // flag change behavior, never the advertisement.
        auto_flush: AUTO_FLUSH_COMPILED_DEFAULT.then_some(AUTO_FLUSH_COMPILED_DEFAULT),
        // ADR-012: advertise attempt outcome resolution capabilities
        attempt_outcome: Some(AttemptOutcome {
            supported: true,
            outcomes: vec![
                "verified_success".to_string(),
                "work_failure".to_string(),
                "infrastructure_failure".to_string(),
                "cancelled".to_string(),
                "indeterminate".to_string(),
            ],
            actions: vec![
                "close".to_string(),
                "release".to_string(),
                "quarantine".to_string(),
                "block".to_string(),
                "none".to_string(),
            ],
            replay_detection: true,
            revision_guard: true,
            fencing_token: true,
            evidence_refs: true,
            resolve_receipt_schema: "urn:bead-rs:schema:resolve-receipt:native-v1".to_string(),
            resolve_request_schema: "urn:bead-rs:schema:resolve-request:native-v1".to_string(),
        }),
        secret_scan: Some(SecretScanCapabilities {
            contract_identity: CONTRACT_IDENTITY.to_string(),
            ruleset_version: RULESET_VERSION,
            effective_mode: secret_mode.as_str().to_string(),
            blocking: true,
            advisory: true,
            exact_fingerprint_acknowledgment: true,
        }),
        historical_redaction: Some(HistoricalRedactionCapabilities {
            contract: "urn:bead-rs:spec:historical-redaction:v1".to_string(),
            doctor_findings: true,
            atomic_redact: true,
            anti_resurrection: true,
            sanitized_generation_set: true,
            resumable_publication: true,
        }),
    })
}

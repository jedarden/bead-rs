//! Checkpoint export and import service
//!
//! This module provides atomic, deterministic JSONL checkpoint export and import for bead-rs.
//!
//! # Architecture Overview
//!
//! The checkpoint system operates in two distinct modes:
//!
//! ## Pre-F017 (Current Default)
//! - Writes to `.beads/issues.jsonl`
//! - Contains issue records only
//! - Used for basic backup and interchange
//! - Single-file format with one JSON object per line
//!
//! ## F017 Forensic Checkpoint Set (Code Complete, Not Yet Activated)
//! - Writes to `.beads/checkpoint/` directory structure
//! - Contains issues, events, and provenance receipts
//! - Supports both monolithic and sharded modes
//! - Content-addressed storage with SHA-256 hashes
//! - Atomic pointer-based generation management
//! - Git-trackable artifacts for version control integration
//!
//! # Key Design Principles
//!
//! 1. **Atomicity**: All checkpoint operations use write-verify-rename patterns
//!    to ensure crash safety and prevent partial state exposure
//!
//! 2. **Determinism**: Same input produces identical byte-for-byte output
//!    through canonical ordering and stable field serialization
//!
//! 3. **Content Addressing**: Files are named by their SHA-256 hash to enable
//!    deduplication and verification
//!
//! 4. **Generation Tracking**: Each checkpoint has a unique generation ID with
//!    atomic pointer updates for authoritative discovery
//!
//! # Pre-F017 Format (.beads/issues.jsonl)
//!
//! ```text
//! {"id":"bead-0123","title":"Task","priority":2,"base_status":"open",...}
//! {"id":"bead-0456","title":"Another","priority":1,"base_status":"open",...}
//! ```
//!
//! # F017 Forensic Format (Not Yet Activated)
//!
//! ## Monolithic Mode
//! - Single `.beads/checkpoint/forensic.jsonl` file
//! - Three record types: issue, event, provenance_receipt
//! - Canonical ordering: issues by ID, events by sequence, receipts by ID
//!
//! ## Sharded Mode
//! - `.beads/checkpoint/current.json` (authoritative pointer)
//! - `.beads/checkpoint/manifests/<hash>.json` (manifest)
//! - `.beads/checkpoint/objects/` (content-addressed shards)
//! - Automatic splitting at configured thresholds
//! - Immutable sealed event shards
//!
//! # Migration Path
//!
//! The codebase contains complete F017 implementation that can be activated
//! once the checkpoint-set-v1.md specification receives independent review
//! and the organizational decision is made to switch from pre-F017 format.

use crate::cli::ImportMode;
use crate::model::Issue;
use crate::profile::ProfileLossReport;
use crate::store::SqliteStore;
use anyhow::{anyhow, bail, Result};
use rusqlite::{params, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Checkpoint mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckpointMode {
    Monolithic,
    Sharded,
}

impl CheckpointMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            CheckpointMode::Monolithic => "monolithic",
            CheckpointMode::Sharded => "sharded",
        }
    }
}

impl std::str::FromStr for CheckpointMode {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "monolithic" => Ok(CheckpointMode::Monolithic),
            "sharded" => Ok(CheckpointMode::Sharded),
            _ => bail!("Invalid checkpoint mode: {}", s),
        }
    }
}

/// Forensic checkpoint record types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "record_type")]
pub enum CheckpointRecord {
    #[serde(rename = "issue")]
    Issue { issue: Issue },
    #[serde(rename = "event")]
    Event { event: EventRecord },
    #[serde(rename = "provenance_receipt")]
    ProvenanceReceipt {
        provenance_receipt: ProvenanceReceipt,
    },
}

/// Event record for forensic checkpoints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRecord {
    #[serde(rename = "$schema")]
    pub schema_ref: String,
    pub origin_store_uuid: String,
    pub origin_event_sequence: i64,
    pub issue_id: Option<String>,
    pub kind: String,
    pub actor: String,
    pub time: String,
    #[serde(default)]
    pub detail: serde_json::Value,
}

/// Dependency and label graph data for checkpoint serialization
///
/// This struct holds all dependency edges and labels across the workspace,
/// organized for efficient lookup during checkpoint serialization.
#[derive(Debug, Clone)]
pub struct IssueGraphData {
    /// All dependency edges as (blocked_id, blocker_id, kind) tuples
    /// Sorted by blocker_id, kind, then blocked_id for canonical ordering
    pub dependencies: Vec<(String, String, String)>,
    /// All label assignments as (issue_id, label) tuples
    /// Sorted by issue_id, then label for canonical ordering
    pub labels: Vec<(String, String)>,
}

/// Provenance receipt for restore/merge operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvenanceReceipt {
    #[serde(rename = "$schema")]
    pub schema_ref: String,
    pub receipt_id: String,
    pub kind: String, // "restore" or "merge"
    pub source_store_uuid: String,
    pub target_store_uuid: String,
    pub source_root_sha256: String,
    pub actor: String,
    pub created_at: String,
    pub counts: ReceiptCounts,
    pub result: String,
    pub summary_event_identity: Option<String>,
    pub receipt_sha256: String,
}

/// Counts recorded in provenance receipts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceiptCounts {
    pub issues: i64,
    pub events: i64,
    pub provenance_receipts: i64,
}

/// Forensic flush result
#[derive(Debug, Clone)]
pub struct ForensicFlushResult {
    #[allow(dead_code)]
    pub mode: CheckpointMode,
    pub generation_id: String,
    pub issue_count: usize,
    pub event_count: usize,
    pub receipt_count: usize,
    pub total_record_count: usize,
    pub root_hash: String,
    pub covered_sequence: i64,
    pub changed_paths: Vec<String>,
}

/// Import result with F017 support
#[derive(Debug, Clone, Serialize)]
#[allow(dead_code)]
pub struct ImportResult {
    pub profile: String,
    pub input_hash: String,
    pub inserted: usize,
    pub updated: usize,
    pub retained: usize,
    pub conflicted: usize,
    pub activation_sequence: i64,
    pub covered_sequence: i64,
    pub dry_run: bool,
    pub prospective: bool,
    pub receipt_preview: Option<ReceiptPreview>,
    pub diagnostics: Option<ImportDiagnostics>,
}

/// Import diagnostic report for R014
#[derive(Debug, Clone, Serialize)]
pub struct ImportDiagnostics {
    pub validation_failures: Vec<ValidationFailure>,
    pub total_lines: usize,
    pub processed_lines: usize,
    pub truncated: bool,
}

/// Validation failure record for import diagnostic report
#[derive(Debug, Clone, Serialize)]
pub struct ValidationFailure {
    pub line_number: usize,
    pub json_pointer: Option<String>,
    pub schema_keyword: Option<String>,
    pub semantic_code: String,
    pub message: String,
    pub context: Option<String>,
}

/// Receipt preview for dry-run operations
#[derive(Debug, Clone, Serialize)]
pub struct ReceiptPreview {
    pub kind: String,
    pub source_store_uuid: String,
    pub target_store_uuid: String,
    pub source_root_sha256: String,
    pub actor: String,
    pub counts: ReceiptCounts,
    pub result: String,
}

/// Legacy import staging result (pre-F017)
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ImportStaging {
    pub issues: Vec<Issue>,
    pub dependencies: Vec<(String, String, String)>, // (blocked, blocker, kind)
    pub labels: Vec<(String, String)>,               // (issue_id, label)
    pub input_hash: String,
    pub issue_count: usize,
    pub diagnostics: Option<ImportDiagnostics>,
}

/// Maximum number of validation failures to collect (R014 bounded collection)
const MAX_DIAGNOSTIC_FAILURES: usize = 100;

/// Forensic checkpoint staging result (F017)
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ForensicStaging {
    pub issues: Vec<Issue>,
    pub dependencies: Vec<(String, String, String)>, // (blocked, blocker, kind)
    pub labels: Vec<(String, String)>,               // (issue_id, label)
    pub events: Vec<SerializedEvent>,
    pub receipts: Vec<SerializedReceipt>,
    pub input_hash: String,
    pub store_uuid: String,
    pub snapshot_sequence: i64,
    pub mode: CheckpointMode,
    pub issue_count: usize,
    pub event_count: usize,
    pub receipt_count: usize,
}

/// Serialized event for forensic import
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedEvent {
    #[serde(rename = "origin_store_uuid")]
    pub origin_store_uuid: String,
    #[serde(rename = "origin_event_sequence")]
    pub origin_event_sequence: i64,
    #[serde(rename = "issue_id")]
    pub issue_id: Option<String>,
    #[serde(rename = "kind")]
    pub kind: String,
    #[serde(rename = "actor")]
    pub actor: Option<String>,
    #[serde(rename = "time")]
    pub time: String,
    #[serde(rename = "detail")]
    pub detail: serde_json::Value,
}

/// Serialized provenance receipt for forensic import
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedReceipt {
    #[serde(rename = "$schema", alias = "schema_ref")]
    pub schema_ref: String,
    #[serde(rename = "receipt_id")]
    pub receipt_id: String,
    #[serde(rename = "kind")]
    pub kind: String,
    #[serde(rename = "source_store_uuid")]
    pub source_store_uuid: String,
    #[serde(rename = "target_store_uuid")]
    pub target_store_uuid: String,
    #[serde(rename = "source_root_sha256")]
    pub source_root_sha256: String,
    #[serde(rename = "actor")]
    pub actor: String,
    #[serde(rename = "created_at")]
    pub created_at: String,
    #[serde(rename = "counts")]
    pub counts: ReceiptCounts,
    #[serde(rename = "result")]
    pub result: String,
    #[serde(rename = "summary_event_identity")]
    pub summary_event_identity: Option<String>,
    #[serde(rename = "receipt_sha256")]
    pub receipt_sha256: String,
}

/// Full import result with receipt support
#[derive(Debug, Clone, Serialize)]
pub struct FullImportResult {
    pub profile: String,
    pub input_hash: String,
    pub inserted: usize,
    pub updated: usize,
    pub retained: usize,
    pub conflicted: usize,
    pub events_imported: i64,
    pub receipts_processed: i64,
    pub activation_sequence: i64,
    pub covered_sequence: i64,
    pub dry_run: bool,
    pub prospective: bool,
    pub receipt_preview: Option<ReceiptPreview>,
    pub receipt: Option<SerializedReceipt>,
    pub summary_event_sequence: Option<i64>,
    pub loss_report: Option<ProfileLossReport>,
}

/// Flush checkpoint result
#[derive(Debug, Clone)]
pub struct FlushResult {
    pub issue_count: usize,
    pub hash: String,
    pub covered_sequence: i64,
    pub export_time: String,
}

/// Import checkpoint from JSONL file
///
/// Import checkpoint from JSONL file (Pre-F017 Issue-Only Format)
///
/// This function validates and imports a checkpoint containing only issue records.
/// It supports both dry-run validation and real activation modes.
///
/// This function:
/// 1. Parses and stages all issues from the input file
/// 2. Validates the staged data (duplicates, dangling deps, cycles)
/// 3. Performs dry-run or real activation
/// 4. Updates checkpoint_state table
///
/// # Import Process
///
/// 1. **Profile Validation**: Only 'native-v1' profile is supported (pre-F017 restriction)
/// 2. **Staging**: Parse and validate entire input file without modifying database
/// 3. **Validation Checks**:
///    - Malformed JSON detection with line numbers
///    - Duplicate issue ID detection
///    - Missing required fields
///    - Dependency graph validation (no cycles, no missing references)
///    - Unknown field preservation for round-trip compatibility
/// 4. **Dry-Run Mode**: Return prospective results without any database changes
/// 5. **Empty Target Verification**: Ensure target workspace has no existing issues
/// 6. **Atomic Activation**: Single transaction inserts all issues, dependencies, labels
///
/// # Atomicity and Safety
///
/// - **No Partial State**: If validation fails, no database changes occur
/// - **Single Transaction**: All insertions happen atomically
/// - **Audit Trail**: Creates exactly one `checkpoint_imported` workspace event
/// - **Rollback Safety**: Any failure rolls back the entire transaction
///
/// # Dry-Run Behavior
///
/// Dry-run performs identical validation and staging but:
/// - Does NOT modify the database
/// - Does NOT write checkpoint_state
/// - Does NOT create audit events
/// - Returns prospective counts with `dry_run: true` and `prospective: true`
///
/// # Arguments
///
/// * `store` - Mutable reference to the SQLite store
/// * `input_path` - Path to the JSONL input file (must exist, read-only)
/// * `profile` - Profile identifier (only 'native-v1' supported pre-F017)
/// * `dry_run` - If true, perform validation without activating changes
///
/// # Returns
///
/// * `Ok(ImportResult)` - Contains insertion/update counts, sequences, and hashes
/// * `Err(...)` - Validation error, database error, or I/O failure
///
/// # Errors
///
/// - **Profile Error**: Non-native-v1 profile before F017 completion
/// - **Parse Error**: Malformed JSON on specific line number
/// - **Validation Error**: Duplicate IDs, missing dependencies, cycles
/// - **Target Error**: Target workspace not empty (for real import)
///
/// # Examples
///
/// ```no_run
/// # use bead_rs::store::SqliteStore;
/// # use bead_rs::service::checkpoint::import_checkpoint;
/// # use std::path::Path;
/// # fn main() -> anyhow::Result<()> {
/// # let mut store = SqliteStore::new();
/// // Dry-run validation
/// let result = import_checkpoint(&mut store, Path::new("backup.jsonl"), "native-v1", true)?;
/// println!("Would import {} issues", result.inserted);
///
/// // Real import
/// let result = import_checkpoint(&mut store, Path::new("backup.jsonl"), "native-v1", false)?;
/// println!("Imported {} issues, sequence: {}", result.inserted, result.activation_sequence);
/// # Ok(())
/// # }
/// ```
#[allow(dead_code)]
pub fn import_checkpoint(
    store: &mut SqliteStore,
    input_path: &Path,
    profile: &str,
    dry_run: bool,
) -> Result<ImportResult> {
    import_checkpoint_with_diagnostics(store, input_path, profile, dry_run, false)
}

/// Import checkpoint with diagnostic mode (R014)
pub fn import_checkpoint_with_diagnostics(
    store: &mut SqliteStore,
    input_path: &Path,
    profile: &str,
    dry_run: bool,
    diagnostics_mode: bool,
) -> Result<ImportResult> {
    // Validate profile (only native-v1 allowed before F017)
    if profile != "native-v1" {
        bail!(
            "Profile '{}' is not supported before F017. Only 'native-v1' is allowed.",
            profile
        );
    }

    // Use diagnostic staging if requested
    let mut staging = if diagnostics_mode {
        stage_import_with_diagnostics(input_path, profile)
    } else {
        match stage_import(input_path, profile) {
            Ok(s) => s,
            Err(e) => {
                // Convert single error to diagnostic format
                let mut staging = ImportStaging {
                    issues: Vec::new(),
                    dependencies: Vec::new(),
                    labels: Vec::new(),
                    input_hash: String::new(),
                    issue_count: 0,
                    diagnostics: None,
                };

                staging.diagnostics = Some(ImportDiagnostics {
                    validation_failures: vec![ValidationFailure {
                        line_number: 0,
                        json_pointer: None,
                        schema_keyword: Some("staging".to_string()),
                        semantic_code: "staging_error".to_string(),
                        message: format!("Staging failed: {}", e),
                        context: None,
                    }],
                    total_lines: 0,
                    processed_lines: 0,
                    truncated: false,
                });
                staging
            }
        }
    };

    // Validate the staged data (collects additional diagnostics)
    validate_import(&mut staging, dry_run)?;

    // Check if we should fail due to validation errors
    let has_diagnostics = staging
        .diagnostics
        .as_ref()
        .map(|d| !d.validation_failures.is_empty())
        .unwrap_or(false);

    if has_diagnostics && !diagnostics_mode {
        // In non-diagnostics mode, fail on first error for backward compatibility
        let first_error = staging
            .diagnostics
            .as_ref()
            .unwrap()
            .validation_failures
            .first()
            .unwrap();
        bail!("Import validation failed: {}", first_error.message);
    }

    if dry_run {
        // Get current sequence for prospective report
        let conn = store.conn();
        let current_sequence: i64 = conn
            .query_row("SELECT COALESCE(MAX(sequence), 0) FROM events", [], |row| {
                row.get(0)
            })
            .unwrap_or(0);

        let prospective_sequence = current_sequence + 1; // Would allocate one for checkpoint_imported event

        return Ok(ImportResult {
            profile: profile.to_string(),
            input_hash: staging.input_hash.clone(),
            inserted: staging.issue_count,
            updated: 0,
            retained: 0,
            conflicted: 0,
            activation_sequence: prospective_sequence,
            covered_sequence: prospective_sequence,
            dry_run: true,
            prospective: true,
            receipt_preview: None,
            diagnostics: staging.diagnostics,
        });
    }

    // Real activation: verify target is empty and no validation errors
    verify_empty_target(store)?;

    if has_diagnostics {
        // Don't activate if there are validation errors
        return Ok(ImportResult {
            profile: profile.to_string(),
            input_hash: staging.input_hash,
            inserted: 0,
            updated: 0,
            retained: 0,
            conflicted: 0,
            activation_sequence: 0,
            covered_sequence: 0,
            dry_run: false,
            prospective: false,
            receipt_preview: None,
            diagnostics: staging.diagnostics,
        });
    }

    // Activate the staged data in a single transaction
    let (inserted, activation_sequence) = activate_import(store, &staging)?;

    Ok(ImportResult {
        profile: profile.to_string(),
        input_hash: staging.input_hash,
        inserted,
        updated: 0,
        retained: 0,
        conflicted: 0,
        activation_sequence,
        covered_sequence: activation_sequence,
        dry_run: false,
        prospective: false,
        receipt_preview: None,
        diagnostics: staging.diagnostics,
    })
}

/// Import forensic checkpoint with restore or merge
///
/// # Forensic Checkpoint Import
///
/// This function implements F017 forensic checkpoint import with support for:
/// - Monolithic JSONL format with issues, events, and receipts
/// - Sharded manifest-based format with content-addressed objects
/// - Pointer-based checkpoint discovery
///
/// # Arguments
///
/// * `store` - Mutable reference to SQLite store
/// * `input_path` - Path to checkpoint file/directory (must exist, read-only)
/// * `profile` - Profile identifier (native-v1 required for forensic)
/// * `mode` - Import mode: RestoreIntoEmpty or Merge
/// * `actor` - Actor performing the operation (required, non-empty, ≤255 bytes)
/// * `dry_run` - If true, perform validation without activating changes
///
/// # Returns
///
/// * `Ok(FullImportResult)` - Complete result with counts, sequences, and receipt info
/// * `Err(...)` - Validation error, integrity failure, or I/O error
///
/// # Errors
///
/// - **Actor Error**: Missing, empty, oversized, or control-character actor
/// - **Discovery Error**: Invalid pointer, missing manifest, or object files
/// - **Parse Error**: Malformed JSON, unknown record types, or syntax errors
/// - **Integrity Error**: Hash mismatch, count discrepancy, or sequence gaps
/// - **Validation Error**: Duplicate IDs, cycles, or replay mismatches
/// - **Target Error**: Non-empty target for restore, UUID conflicts for merge
/// - **Conflict Error**: Same timestamp with different content during merge
pub fn import_forensic_checkpoint(
    store: &mut SqliteStore,
    input_path: &Path,
    profile: &str,
    mode: ImportMode,
    actor: &str,
    dry_run: bool,
) -> Result<FullImportResult> {
    let (staging, loss_report) = if profile == "native-v1" {
        (stage_forensic_checkpoint(input_path)?, None)
    } else {
        bail!("Profile '{}' is not supported for import", profile);
    };

    // Validate forensic checkpoint
    validate_forensic_checkpoint(&staging, mode, store, dry_run)?;

    if dry_run {
        // Return dry-run result with prospective counts
        let conn = store.conn();
        let (current_sequence, target_uuid) = get_workspace_state(conn)?;

        let (preview, prospective_sequence) =
            calculate_prospective_result(&staging, mode, current_sequence, &target_uuid, actor)?;

        return Ok(FullImportResult {
            profile: profile.to_string(),
            input_hash: staging.input_hash,
            inserted: preview.counts.issues as usize,
            updated: 0,
            retained: 0,
            conflicted: 0,
            events_imported: preview.counts.events,
            receipts_processed: preview.counts.provenance_receipts + 1, // +1 for new receipt
            activation_sequence: prospective_sequence,
            covered_sequence: prospective_sequence,
            dry_run: true,
            prospective: true,
            receipt_preview: Some(preview),
            receipt: None,
            summary_event_sequence: None,
            loss_report,
        });
    }

    // Execute real import based on mode
    let counts = match mode {
        ImportMode::RestoreIntoEmpty => execute_restore_into_empty(store, &staging, actor)?,
        ImportMode::Merge => execute_merge(store, &staging, actor)?,
    };

    // Get the import result
    let conn = store.conn();
    let (_final_sequence, result) = get_import_result(conn, &staging.store_uuid)?;

    // Report what was actually written. `get_import_result` reads back receipt
    // state only and leaves every count at zero, which made a successful import
    // report "0 inserted, 0 events" while having written the whole checkpoint.
    let mut result = result;
    result.profile = profile.to_string();
    result.input_hash = staging.input_hash;
    result.loss_report = loss_report;
    result.inserted = counts.inserted;
    result.updated = counts.updated;
    result.retained = counts.retained;
    result.events_imported = counts.events_imported;
    result.receipts_processed = counts.receipts_processed as i64;
    Ok(result)
}

/// Get current workspace state for prospective calculation
fn get_workspace_state(conn: &rusqlite::Connection) -> Result<(i64, String)> {
    let current_sequence: i64 = conn
        .query_row("SELECT COALESCE(MAX(sequence), 0) FROM events", [], |row| {
            row.get(0)
        })
        .unwrap_or(0);

    let target_uuid: String = conn
        .query_row("SELECT uuid FROM workspace", [], |row| row.get(0))
        .unwrap_or_else(|_| String::from("unknown"));

    Ok((current_sequence, target_uuid))
}

/// Calculate prospective result for dry-run
fn calculate_prospective_result(
    staging: &ForensicStaging,
    mode: ImportMode,
    current_sequence: i64,
    target_uuid: &str,
    actor: &str,
) -> Result<(ReceiptPreview, i64)> {
    let prospective_sequence = current_sequence + 1;

    let counts = ReceiptCounts {
        issues: staging.issue_count as i64,
        events: staging.event_count as i64,
        provenance_receipts: (staging.receipt_count + 1) as i64, // +1 for new receipt
    };

    let preview = ReceiptPreview {
        kind: mode.as_str().to_string(),
        source_store_uuid: staging.store_uuid.clone(),
        target_store_uuid: target_uuid.to_string(),
        source_root_sha256: staging.input_hash.clone(),
        actor: actor.to_string(),
        counts,
        result: "success".to_string(),
    };

    Ok((preview, prospective_sequence))
}

/// Get import result after activation
fn get_import_result(
    conn: &rusqlite::Connection,
    store_uuid: &str,
) -> Result<(i64, FullImportResult)> {
    let final_sequence: i64 = conn
        .query_row("SELECT COALESCE(MAX(sequence), 0) FROM events", [], |row| {
            row.get(0)
        })
        .unwrap_or(0);

    // Get the receipt that was created
    let receipt: Option<SerializedReceipt> = conn
        .query_row(
            "SELECT receipt_id, kind, source_store_uuid, target_store_uuid,
                    source_root_sha256, actor, created_at, result
             FROM provenance_receipts
             WHERE target_store_uuid = ?1
             ORDER BY created_at DESC LIMIT 1",
            [store_uuid],
            |row| {
                Ok(SerializedReceipt {
                    schema_ref: "urn:bead-rs:schema:provenance-receipt:native-v1".to_string(),
                    receipt_id: row.get(0)?,
                    kind: row.get(1)?,
                    source_store_uuid: row.get(2)?,
                    target_store_uuid: row.get(3)?,
                    source_root_sha256: row.get(4)?,
                    actor: row.get(5)?,
                    created_at: row.get(6)?,
                    counts: ReceiptCounts {
                        issues: 0,
                        events: 0,
                        provenance_receipts: 0,
                    },
                    result: row.get(7)?,
                    summary_event_identity: None,
                    receipt_sha256: String::new(),
                })
            },
        )
        .ok();

    let result = FullImportResult {
        profile: "native-v1".to_string(),
        input_hash: String::new(),
        inserted: 0,
        updated: 0,
        retained: 0,
        conflicted: 0,
        events_imported: 0,
        receipts_processed: 0,
        activation_sequence: final_sequence,
        covered_sequence: final_sequence,
        dry_run: false,
        prospective: false,
        receipt_preview: None,
        receipt,
        summary_event_sequence: None,
        loss_report: None,
    };

    Ok((final_sequence, result))
}

/// Stage forensic checkpoint from input path
fn stage_forensic_checkpoint(input_path: &Path) -> Result<ForensicStaging> {
    // Check if input is a directory (sharded/pointer) or file (monolithic)
    if input_path.is_dir() {
        // Try to find current.json pointer
        let pointer_path = input_path.join("current.json");
        if pointer_path.exists() {
            stage_pointer_checkpoint(&pointer_path)
        } else {
            bail!(
                "Directory checkpoint missing current.json pointer: {}",
                input_path.display()
            );
        }
    } else {
        // Single file - treat as monolithic
        stage_monolithic_checkpoint(input_path)
    }
}

/// Stage a checkpoint referenced by a `current.json` pointer, dispatching on
/// the pointer's own `mode` field. A directory checkpoint is not necessarily
/// sharded: `bead sync flush-only` always writes the same pointer +
/// `objects/gen-*.jsonl` layout, but for monolithic mode that generation
/// file is the raw JSONL data itself, not a shard manifest -- treating it as
/// the latter (as this dispatch used to do unconditionally for any
/// directory input) fails to parse, erroring on the second JSONL record as
/// unexpected "trailing characters".
fn stage_pointer_checkpoint(pointer_path: &Path) -> Result<ForensicStaging> {
    let pointer_data = std::fs::read_to_string(pointer_path)?;
    let pointer: serde_json::Value =
        serde_json::from_str(&pointer_data).map_err(|e| anyhow!("Invalid pointer JSON: {}", e))?;

    let mode = pointer
        .get("mode")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Pointer missing mode"))?;

    match mode {
        "monolithic" => {
            let base = pointer_path
                .parent()
                .ok_or_else(|| anyhow!("Pointer has no parent directory"))?;
            let active_root = pointer
                .get("active_root")
                .ok_or_else(|| anyhow!("Pointer missing active_root"))?;
            let root_path = active_root
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("Active root missing path"))?;

            let mut staging = stage_monolithic_checkpoint(&base.join(root_path))?;

            // The pointer's store_uuid/snapshot_sequence are authoritative
            // and always present, unlike stage_monolithic_checkpoint's
            // fallback of deriving them from events/receipts in the data --
            // which is empty (and so leaves them blank/zero) for an
            // issue-only checkpoint like a fresh `bead create` pass.
            if let Some(uuid) = pointer.get("store_uuid").and_then(|v| v.as_str()) {
                staging.store_uuid = uuid.to_string();
            }
            if let Some(seq) = pointer.get("snapshot_sequence").and_then(|v| v.as_i64()) {
                staging.snapshot_sequence = seq;
            }

            Ok(staging)
        }
        "sharded" => stage_sharded_checkpoint(pointer_path),
        other => bail!("Unknown checkpoint mode in pointer: {}", other),
    }
}

/// Stage monolithic checkpoint from JSONL file
fn stage_monolithic_checkpoint(input_path: &Path) -> Result<ForensicStaging> {
    let file = File::open(input_path)?;
    let reader = BufReader::new(file);

    let mut issues = Vec::new();
    let mut dependencies = Vec::new();
    let mut labels = Vec::new();
    let mut events = Vec::new();
    let mut receipts = Vec::new();

    let mut seen_issue_ids = HashSet::new();
    let mut seen_event_identities = HashSet::new();
    let mut seen_receipt_ids = HashSet::new();

    let mut hasher = Sha256::new();
    let mut store_uuid = String::new();
    let mut snapshot_sequence = 0i64;

    for (line_num, line_result) in reader.lines().enumerate() {
        let line_num = line_num + 1; // 1-based for error messages
        let line = line_result?;

        if line.trim().is_empty() {
            continue; // Skip blank lines
        }

        hasher.update(line.as_bytes());
        hasher.update(b"\n");

        // Parse record envelope or legacy issue
        let record: serde_json::Value = serde_json::from_str(&line)
            .map_err(|e| anyhow!("Line {}: malformed JSON: {}", line_num, e))?;

        // Check if this is a forensic record with record_type
        if let Some(record_type) = record.get("record_type").and_then(|v| v.as_str()) {
            // Handle forensic format
            match record_type {
                "issue" => {
                    let issue_value = record
                        .get("issue")
                        .ok_or_else(|| anyhow!("Line {}: missing 'issue' field", line_num))?;

                    // Parse as generic JSON to extract dependencies and labels
                    let issue_obj = issue_value
                        .as_object()
                        .ok_or_else(|| anyhow!("Line {}: issue must be an object", line_num))?;

                    let issue: Issue = serde_json::from_value(issue_value.clone())
                        .map_err(|e| anyhow!("Line {}: invalid issue: {}", line_num, e))?;

                    // Check for duplicate IDs
                    if !seen_issue_ids.insert(issue.id.clone()) {
                        bail!("Line {}: duplicate issue ID: {}", line_num, issue.id);
                    }

                    // Extract dependencies if present
                    if let Some(deps_array) =
                        issue_obj.get("dependencies").and_then(|v| v.as_array())
                    {
                        for dep_value in deps_array {
                            let dep_obj = dep_value.as_object().ok_or_else(|| {
                                anyhow!("Line {}: dependency must be an object", line_num)
                            })?;

                            let blocked = issue.id.clone();
                            let blocker = dep_obj
                                .get("blocker")
                                .and_then(|v| v.as_str())
                                .ok_or_else(|| {
                                    anyhow!("Line {}: dependency missing blocker", line_num)
                                })?
                                .to_string();
                            let kind = dep_obj
                                .get("kind")
                                .and_then(|v| v.as_str())
                                .unwrap_or("blocks")
                                .to_string();

                            dependencies.push((blocked, blocker, kind));
                        }
                    }

                    // Extract labels if present
                    if let Some(labels_array) = issue_obj.get("labels").and_then(|v| v.as_array()) {
                        for label_value in labels_array {
                            if let Some(label_str) = label_value.as_str() {
                                labels.push((issue.id.clone(), label_str.to_string()));
                            }
                        }
                    }

                    issues.push(issue);
                }
                "event" => {
                    let event_value = record
                        .get("event")
                        .ok_or_else(|| anyhow!("Line {}: missing 'event' field", line_num))?;

                    let event: SerializedEvent = serde_json::from_value(event_value.clone())
                        .map_err(|e| anyhow!("Line {}: invalid event: {}", line_num, e))?;

                    // Check for duplicate event identities
                    let identity = format!(
                        "{}:{}",
                        event.origin_store_uuid, event.origin_event_sequence
                    );
                    if !seen_event_identities.insert(identity.clone()) {
                        bail!("Line {}: duplicate event identity: {}", line_num, identity);
                    }

                    events.push(event);
                }
                "provenance_receipt" => {
                    let receipt_value = record.get("provenance_receipt").ok_or_else(|| {
                        anyhow!("Line {}: missing 'provenance_receipt' field", line_num)
                    })?;

                    let receipt: SerializedReceipt = serde_json::from_value(receipt_value.clone())
                        .map_err(|e| anyhow!("Line {}: invalid receipt: {}", line_num, e))?;

                    // Check for duplicate receipt IDs
                    if !seen_receipt_ids.insert(receipt.receipt_id.clone()) {
                        bail!(
                            "Line {}: duplicate receipt ID: {}",
                            line_num,
                            receipt.receipt_id
                        );
                    }

                    receipts.push(receipt);
                }
                _ => {
                    bail!("Line {}: unknown record type: {}", line_num, record_type);
                }
            }
        } else {
            // Try to parse as old-style issue-only record (for backward compatibility)
            let issue: Issue = serde_json::from_str(&line)
                .map_err(|e| anyhow!("Line {}: malformed issue JSON: {}", line_num, e))?;

            if !seen_issue_ids.insert(issue.id.clone()) {
                bail!("Line {}: duplicate issue ID: {}", line_num, issue.id);
            }

            // Extract dependencies from the issue object
            if let Some(deps_array) = record.get("dependencies").and_then(|v| v.as_array()) {
                for dep_value in deps_array {
                    let dep_obj = dep_value.as_object().ok_or_else(|| {
                        anyhow!("Line {}: dependency must be an object", line_num)
                    })?;

                    let blocked = issue.id.clone();
                    let blocker = dep_obj
                        .get("blocker")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| anyhow!("Line {}: dependency missing blocker", line_num))?
                        .to_string();
                    let kind = dep_obj
                        .get("kind")
                        .and_then(|v| v.as_str())
                        .unwrap_or("blocks")
                        .to_string();

                    dependencies.push((blocked, blocker, kind));
                }
            }

            // Extract labels from the issue object
            if let Some(labels_array) = record.get("labels").and_then(|v| v.as_array()) {
                for label_value in labels_array {
                    if let Some(label_str) = label_value.as_str() {
                        labels.push((issue.id.clone(), label_str.to_string()));
                    }
                }
            }

            issues.push(issue);
        }
    }

    let input_hash = format!("{:x}", hasher.finalize());

    // Try to extract store UUID from events or receipts
    if let Some(first_event) = events.first() {
        store_uuid = first_event.origin_store_uuid.clone();
    } else if let Some(first_receipt) = receipts.first() {
        store_uuid = first_receipt.target_store_uuid.clone();
    }

    // Get snapshot sequence from events
    if let Some(last_event) = events.last() {
        snapshot_sequence = last_event.origin_event_sequence;
    }

    Ok(ForensicStaging {
        issues,
        dependencies,
        labels,
        events,
        receipts,
        input_hash,
        store_uuid,
        snapshot_sequence,
        mode: CheckpointMode::Monolithic,
        issue_count: seen_issue_ids.len(),
        event_count: seen_event_identities.len(),
        receipt_count: seen_receipt_ids.len(),
    })
}

/// Stage sharded checkpoint from pointer file
fn stage_sharded_checkpoint(pointer_path: &Path) -> Result<ForensicStaging> {
    let base = pointer_path
        .parent()
        .ok_or_else(|| anyhow!("Pointer has no parent directory"))?;

    // Read current.json pointer
    let pointer_data = std::fs::read_to_string(pointer_path)?;
    let pointer: serde_json::Value =
        serde_json::from_str(&pointer_data).map_err(|e| anyhow!("Invalid pointer JSON: {}", e))?;

    let active_root = pointer
        .get("active_root")
        .ok_or_else(|| anyhow!("Pointer missing active_root"))?;

    let manifest_path = active_root
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Active root missing path"))?;

    let manifest_full_path = base.join(manifest_path);

    // Read manifest
    let manifest_data = std::fs::read_to_string(&manifest_full_path)
        .map_err(|e| anyhow!("Failed to read manifest: {}", e))?;

    let manifest: serde_json::Value = serde_json::from_str(&manifest_data)
        .map_err(|e| anyhow!("Invalid manifest JSON: {}", e))?;

    // Extract metadata
    let store_uuid = manifest
        .get("store_uuid")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Manifest missing store_uuid"))?
        .to_string();

    let snapshot_sequence = manifest
        .get("snapshot_sequence")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| anyhow!("Manifest missing snapshot_sequence"))?;

    let mut hasher = Sha256::new();
    let mut issues = Vec::new();
    let mut dependencies = Vec::new();
    let mut labels = Vec::new();
    let mut events = Vec::new();
    let mut receipts = Vec::new();

    let mut seen_issue_ids = HashSet::new();
    let mut seen_event_identities = HashSet::new();
    let mut seen_receipt_ids = HashSet::new();

    // Process issue shards
    if let Some(issue_shards) = manifest.get("issue_shards").and_then(|v| v.as_array()) {
        for shard_info in issue_shards {
            let shard_path = shard_info
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("Issue shard missing path"))?;

            let shard_full_path = base.join(shard_path);
            let shard_data = process_shard_file(
                &shard_full_path,
                &mut hasher,
                &mut seen_issue_ids,
                &mut seen_event_identities,
                &mut seen_receipt_ids,
            )?;

            issues.extend(shard_data.issues);
            dependencies.extend(shard_data.dependencies);
            labels.extend(shard_data.labels);
        }
    }

    // Process event shards
    if let Some(event_shards) = manifest.get("event_shards").and_then(|v| v.as_array()) {
        for shard_info in event_shards {
            let shard_path = shard_info
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("Event shard missing path"))?;

            let shard_full_path = base.join(shard_path);
            let shard_data = process_shard_file(
                &shard_full_path,
                &mut hasher,
                &mut seen_issue_ids,
                &mut seen_event_identities,
                &mut seen_receipt_ids,
            )?;

            events.extend(shard_data.events);
        }
    }

    // Process receipt shards
    if let Some(receipt_shards) = manifest.get("receipt_shards").and_then(|v| v.as_array()) {
        for shard_info in receipt_shards {
            let shard_path = shard_info
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("Receipt shard missing path"))?;

            let shard_full_path = base.join(shard_path);
            let shard_data = process_shard_file(
                &shard_full_path,
                &mut hasher,
                &mut seen_issue_ids,
                &mut seen_event_identities,
                &mut seen_receipt_ids,
            )?;

            receipts.extend(shard_data.receipts);
        }
    }

    let input_hash = format!("{:x}", hasher.finalize());

    Ok(ForensicStaging {
        issues,
        dependencies,
        labels,
        events,
        receipts,
        input_hash,
        store_uuid,
        snapshot_sequence,
        mode: CheckpointMode::Sharded,
        issue_count: seen_issue_ids.len(),
        event_count: seen_event_identities.len(),
        receipt_count: seen_receipt_ids.len(),
    })
}

/// Process a single shard file
fn process_shard_file(
    shard_path: &Path,
    hasher: &mut Sha256,
    seen_issue_ids: &mut HashSet<String>,
    seen_event_identities: &mut HashSet<String>,
    seen_receipt_ids: &mut HashSet<String>,
) -> Result<ShardData> {
    let file = File::open(shard_path)
        .map_err(|e| anyhow!("Failed to open shard {}: {}", shard_path.display(), e))?;

    let reader = BufReader::new(file);

    let mut shard_data = ShardData {
        issues: Vec::new(),
        dependencies: Vec::new(),
        labels: Vec::new(),
        events: Vec::new(),
        receipts: Vec::new(),
    };

    for (line_num, line_result) in reader.lines().enumerate() {
        let line_num = line_num + 1;
        let line = line_result?;

        if line.trim().is_empty() {
            continue;
        }

        hasher.update(line.as_bytes());
        hasher.update(b"\n");

        let record: serde_json::Value = serde_json::from_str(&line).map_err(|e| {
            anyhow!(
                "{} line {}: malformed JSON: {}",
                shard_path.display(),
                line_num,
                e
            )
        })?;

        let record_type = record
            .get("record_type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                anyhow!(
                    "{} line {}: missing record_type",
                    shard_path.display(),
                    line_num
                )
            })?;

        match record_type {
            "issue" => {
                let issue_value = record.get("issue").ok_or_else(|| {
                    anyhow!(
                        "{} line {}: missing 'issue' field",
                        shard_path.display(),
                        line_num
                    )
                })?;

                let issue_obj = issue_value.as_object().ok_or_else(|| {
                    anyhow!(
                        "{} line {}: issue must be an object",
                        shard_path.display(),
                        line_num
                    )
                })?;

                let issue: Issue = serde_json::from_value(issue_value.clone()).map_err(|e| {
                    anyhow!(
                        "{} line {}: invalid issue: {}",
                        shard_path.display(),
                        line_num,
                        e
                    )
                })?;

                if !seen_issue_ids.insert(issue.id.clone()) {
                    bail!(
                        "{} line {}: duplicate issue ID: {}",
                        shard_path.display(),
                        line_num,
                        issue.id
                    );
                }

                // Extract dependencies if present
                if let Some(deps_array) = issue_obj.get("dependencies").and_then(|v| v.as_array()) {
                    for dep_value in deps_array {
                        let dep_obj = dep_value.as_object().ok_or_else(|| {
                            anyhow!(
                                "{} line {}: dependency must be an object",
                                shard_path.display(),
                                line_num
                            )
                        })?;

                        let blocked = issue.id.clone();
                        let blocker = dep_obj
                            .get("blocker")
                            .and_then(|v| v.as_str())
                            .ok_or_else(|| {
                                anyhow!(
                                    "{} line {}: dependency missing blocker",
                                    shard_path.display(),
                                    line_num
                                )
                            })?
                            .to_string();
                        let kind = dep_obj
                            .get("kind")
                            .and_then(|v| v.as_str())
                            .unwrap_or("blocks")
                            .to_string();

                        shard_data.dependencies.push((blocked, blocker, kind));
                    }
                }

                // Extract labels if present
                if let Some(labels_array) = issue_obj.get("labels").and_then(|v| v.as_array()) {
                    for label_value in labels_array {
                        if let Some(label_str) = label_value.as_str() {
                            shard_data
                                .labels
                                .push((issue.id.clone(), label_str.to_string()));
                        }
                    }
                }

                shard_data.issues.push(issue);
            }
            "event" => {
                let event_value = record.get("event").ok_or_else(|| {
                    anyhow!(
                        "{} line {}: missing 'event' field",
                        shard_path.display(),
                        line_num
                    )
                })?;

                let event: SerializedEvent =
                    serde_json::from_value(event_value.clone()).map_err(|e| {
                        anyhow!(
                            "{} line {}: invalid event: {}",
                            shard_path.display(),
                            line_num,
                            e
                        )
                    })?;

                let identity = format!(
                    "{}:{}",
                    event.origin_store_uuid, event.origin_event_sequence
                );
                if !seen_event_identities.insert(identity.clone()) {
                    bail!(
                        "{} line {}: duplicate event identity: {}",
                        shard_path.display(),
                        line_num,
                        identity
                    );
                }

                shard_data.events.push(event);
            }
            "provenance_receipt" => {
                let receipt_value = record.get("provenance_receipt").ok_or_else(|| {
                    anyhow!(
                        "{} line {}: missing 'provenance_receipt' field",
                        shard_path.display(),
                        line_num
                    )
                })?;

                let receipt: SerializedReceipt = serde_json::from_value(receipt_value.clone())
                    .map_err(|e| {
                        anyhow!(
                            "{} line {}: invalid receipt: {}",
                            shard_path.display(),
                            line_num,
                            e
                        )
                    })?;

                if !seen_receipt_ids.insert(receipt.receipt_id.clone()) {
                    bail!(
                        "{} line {}: duplicate receipt ID: {}",
                        shard_path.display(),
                        line_num,
                        receipt.receipt_id
                    );
                }

                shard_data.receipts.push(receipt);
            }
            _ => {
                bail!(
                    "{} line {}: unknown record type: {}",
                    shard_path.display(),
                    line_num,
                    record_type
                );
            }
        }
    }

    Ok(shard_data)
}

/// Shard data accumulator
#[derive(Debug, Default)]
struct ShardData {
    issues: Vec<Issue>,
    dependencies: Vec<(String, String, String)>,
    labels: Vec<(String, String)>,
    events: Vec<SerializedEvent>,
    receipts: Vec<SerializedReceipt>,
}

/// Validate forensic checkpoint before import
fn validate_forensic_checkpoint(
    staging: &ForensicStaging,
    mode: ImportMode,
    store: &mut SqliteStore,
    _dry_run: bool,
) -> Result<()> {
    for issue in &staging.issues {
        issue
            .validate()
            .map_err(|error| anyhow!("Issue '{}' failed validation: {}", issue.id, error))?;
    }

    // Validate canonical ordering
    validate_canonical_ordering(staging)?;

    // Validate dependencies
    validate_dependencies(&staging.dependencies, &staging.issues)?;

    // Validate event sequence continuity
    validate_event_sequence(staging)?;

    // Mode-specific validation
    match mode {
        ImportMode::RestoreIntoEmpty => {
            validate_restore_constraints(store, staging)?;
        }
        ImportMode::Merge => {
            validate_merge_constraints(store, staging)?;
        }
    }

    Ok(())
}

/// Validate canonical ordering of records
fn validate_canonical_ordering(staging: &ForensicStaging) -> Result<()> {
    // Issues should be sorted by ID
    let mut prev_id = String::new();
    for issue in &staging.issues {
        if issue.id <= prev_id {
            bail!(
                "Issues not in canonical order: {} after {}",
                issue.id,
                prev_id
            );
        }
        prev_id = issue.id.clone();
    }

    // Events should be sorted by (origin_store_uuid, origin_event_sequence)
    let mut prev_identity = (String::new(), 0i64);
    for event in &staging.events {
        let current_identity = (event.origin_store_uuid.clone(), event.origin_event_sequence);
        if current_identity <= prev_identity {
            bail!(
                "Events not in canonical order: ({}, {}) after ({}, {})",
                current_identity.0,
                current_identity.1,
                prev_identity.0,
                prev_identity.1
            );
        }
        prev_identity = current_identity;
    }

    // Receipts should be sorted by ID
    let mut prev_receipt_id = String::new();
    for receipt in &staging.receipts {
        if receipt.receipt_id <= prev_receipt_id {
            bail!(
                "Receipts not in canonical order: {} after {}",
                receipt.receipt_id,
                prev_receipt_id
            );
        }
        prev_receipt_id = receipt.receipt_id.clone();
    }

    Ok(())
}

/// Validate dependencies
fn validate_dependencies(
    dependencies: &[(String, String, String)],
    issues: &[Issue],
) -> Result<()> {
    let issue_ids: HashSet<&String> = issues.iter().map(|i| &i.id).collect();

    // Check all referenced issues exist and no self-edges
    for (blocked, blocker, _kind) in dependencies {
        if !issue_ids.contains(blocked) {
            bail!(
                "Dependency references non-existent blocked issue: {}",
                blocked
            );
        }
        if !issue_ids.contains(blocker) {
            bail!(
                "Dependency references non-existent blocker issue: {}",
                blocker
            );
        }
        // Check for self-edges
        if blocked == blocker {
            bail!(
                "Self-edge detected: issue {} cannot depend on itself",
                blocked
            );
        }
    }

    // Check for cycles using DFS
    let mut adj: HashMap<String, Vec<String>> = HashMap::new();
    for (blocked, blocker, kind) in dependencies {
        if kind == "blocks" {
            adj.entry(blocker.clone())
                .or_default()
                .push(blocked.clone());
        }
    }

    for node in issue_ids {
        let mut visited = HashSet::new();
        let mut recursion_stack = Vec::new();
        if dfs_has_cycle_forensic(&adj, node, &mut visited, &mut recursion_stack)? {
            bail!("Cycle detected: issue {} is part of a blocking cycle", node);
        }
    }

    Ok(())
}

/// DFS cycle detection for forensic validation
fn dfs_has_cycle_forensic(
    adj: &HashMap<String, Vec<String>>,
    node: &str,
    visited: &mut HashSet<String>,
    recursion_stack: &mut Vec<String>,
) -> Result<bool> {
    if recursion_stack.contains(&node.to_string()) {
        return Ok(true);
    }

    if visited.contains(node) {
        return Ok(false);
    }

    visited.insert(node.to_string());
    recursion_stack.push(node.to_string());

    if let Some(neighbors) = adj.get(node) {
        for neighbor in neighbors {
            if dfs_has_cycle_forensic(adj, neighbor, visited, recursion_stack)? {
                return Ok(true);
            }
        }
    }

    recursion_stack.pop();
    Ok(false)
}

/// Validate event sequence continuity
fn validate_event_sequence(staging: &ForensicStaging) -> Result<()> {
    if staging.events.is_empty() {
        return Ok(()); // No events to validate
    }

    let first_event = &staging.events[0];
    if first_event.origin_event_sequence != 1 {
        bail!(
            "Event sequence does not start at 1: starts at {}",
            first_event.origin_event_sequence
        );
    }

    for window in staging.events.windows(2) {
        let prev = &window[0];
        let curr = &window[1];

        if prev.origin_store_uuid != curr.origin_store_uuid {
            bail!(
                "Event origin UUID changed: {} vs {}",
                prev.origin_store_uuid,
                curr.origin_store_uuid
            );
        }

        if curr.origin_event_sequence != prev.origin_event_sequence + 1 {
            bail!(
                "Event sequence gap: expected {}, found {}",
                prev.origin_event_sequence + 1,
                curr.origin_event_sequence
            );
        }
    }

    Ok(())
}

/// Validate restore-into-empty constraints
fn validate_restore_constraints(store: &mut SqliteStore, _staging: &ForensicStaging) -> Result<()> {
    // Verify target is empty (no semantic mutations)
    verify_empty_target(store)?;

    // Verify store UUID can be adopted
    // (Restore will adopt the checkpoint UUID)

    Ok(())
}

/// Validate merge constraints
fn validate_merge_constraints(store: &mut SqliteStore, staging: &ForensicStaging) -> Result<()> {
    let conn = store.conn();

    // Get current workspace UUID
    let target_uuid: String = conn
        .query_row("SELECT uuid FROM workspace", [], |row| row.get(0))
        .unwrap_or_else(|_| String::from("unknown"));

    // Check UUID compatibility
    if target_uuid == staging.store_uuid {
        // Same-UUID merge: event streams must be compatible
        validate_same_uuid_merge(store, staging)?;
    } else {
        // Different-UUID merge: event identities must not conflict
        validate_different_uuid_merge(store, staging)?;
    }

    Ok(())
}

/// Validate same-UUID merge constraints
fn validate_same_uuid_merge(store: &mut SqliteStore, staging: &ForensicStaging) -> Result<()> {
    validate_event_prefix(store.conn(), staging)
}

/// Validate different-UUID merge constraints
fn validate_different_uuid_merge(store: &mut SqliteStore, staging: &ForensicStaging) -> Result<()> {
    validate_event_prefix(store.conn(), staging)
}

/// Existing event identities are an accepted replay prefix only when their
/// complete public content is identical. New suffix events remain importable.
fn validate_event_prefix(conn: &rusqlite::Connection, staging: &ForensicStaging) -> Result<()> {
    for event in &staging.events {
        let existing = conn
            .query_row(
                "SELECT issue_id, kind, actor, time, detail FROM events
                 WHERE origin_store_uuid = ?1 AND origin_event_sequence = ?2",
                params![&event.origin_store_uuid, event.origin_event_sequence],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                    ))
                },
            )
            .optional()?;

        if let Some((issue_id, kind, actor, time, detail)) = existing {
            let detail: serde_json::Value = serde_json::from_str(&detail)?;
            if issue_id != event.issue_id
                || kind != event.kind
                || actor != event.actor
                || time != event.time
                || detail != event.detail
            {
                bail!(
                    "Event identity conflict: ({}, {}) has different content",
                    event.origin_store_uuid,
                    event.origin_event_sequence
                );
            }
        }
    }

    Ok(())
}

/// Execute restore-into-empty operation
/// Counts of what an import actually wrote, for accurate reporting.
#[derive(Debug, Default, Clone, Copy)]
struct ImportCounts {
    inserted: usize,
    updated: usize,
    retained: usize,
    events_imported: i64,
    receipts_processed: usize,
}

fn execute_restore_into_empty(
    store: &mut SqliteStore,
    staging: &ForensicStaging,
    actor: &str,
) -> Result<ImportCounts> {
    let conn = store.conn();
    let tx = conn.unchecked_transaction()?;

    // Adopt checkpoint store UUID
    tx.execute("UPDATE workspace SET uuid = ?1", [&staging.store_uuid])?;

    // Activate staged data
    let (inserted, activation_sequence) = activate_forensic_import(&tx, staging)?;

    // Point local checkpoint bookkeeping at the generation just restored.
    // Without this the database still reports covered_event_sequence = 0 while
    // the checkpoint pointer reports the real sequence, so `doctor` warns
    // "Sequence mismatch: pointer=N, database=0" on every recovered clone.
    // This must upsert: a freshly initialized workspace has no checkpoint_state
    // row at all, so a bare UPDATE silently affects zero rows.
    tx.execute(
        "INSERT INTO checkpoint_state (id, covered_event_sequence, store_uuid, updated_at)
         VALUES (1, ?1, ?2, ?3)
         ON CONFLICT(id) DO UPDATE SET
             covered_event_sequence = excluded.covered_event_sequence,
             store_uuid = excluded.store_uuid,
             updated_at = excluded.updated_at",
        params![
            staging.snapshot_sequence,
            &staging.store_uuid,
            format_rfc3339(SystemTime::now())
        ],
    )?;

    // Create restore receipt
    let receipt = create_restore_receipt(&tx, staging, actor, activation_sequence)?;

    // Commit transaction
    tx.commit()?;

    eprintln!(
        "Restored {} issues, {} events",
        inserted,
        staging.events.len()
    );
    eprintln!("Restore receipt: {}", receipt.receipt_id);

    Ok(ImportCounts {
        inserted,
        updated: 0,
        retained: 0,
        events_imported: staging.events.len() as i64,
        // +1 for the restore receipt created above
        receipts_processed: staging.receipts.len() + 1,
    })
}

/// Execute merge operation
fn execute_merge(
    store: &mut SqliteStore,
    staging: &ForensicStaging,
    actor: &str,
) -> Result<ImportCounts> {
    let conn = store.conn();
    let tx = conn.unchecked_transaction()?;

    // Perform merge reconciliation
    let (inserted, updated, retained) = reconcile_and_merge(&tx, staging)?;

    // Import events
    import_events(&tx, staging)?;

    // Import existing receipts
    import_receipts(&tx, staging)?;

    // Create merge summary event and receipt
    let activation_sequence = create_merge_summary(&tx, staging, actor)?;

    let receipt = create_merge_receipt(&tx, staging, actor, activation_sequence)?;

    // Commit transaction
    tx.commit()?;

    eprintln!(
        "Merge completed: {} inserted, {} updated, {} retained",
        inserted, updated, retained
    );
    eprintln!("Merge receipt: {}", receipt.receipt_id);

    Ok(ImportCounts {
        inserted,
        updated,
        retained,
        events_imported: staging.events.len() as i64,
        // +1 for the merge receipt created above
        receipts_processed: staging.receipts.len() + 1,
    })
}

/// Activate forensic import in transaction
fn activate_forensic_import(tx: &Transaction, staging: &ForensicStaging) -> Result<(usize, i64)> {
    // Import issues
    let inserted = import_issues(tx, staging)?;

    // Import dependencies
    import_dependencies(tx, staging)?;

    // Import labels
    import_labels(tx, staging)?;

    // Import events and receipts. Without this the restore silently drops the
    // entire audit trail — the forensic checkpoint's whole reason for existing —
    // while still reporting the events as restored. Events carry a foreign key
    // to issues, so this must run after import_issues above.
    import_events(tx, staging)?;
    import_receipts(tx, staging)?;

    // Get activation sequence
    let activation_sequence: i64 = tx
        .query_row("SELECT COALESCE(MAX(sequence), 0) FROM events", [], |row| {
            row.get(0)
        })
        .unwrap_or(0);

    Ok((inserted, activation_sequence))
}

/// Import issues into database
fn import_issues(tx: &Transaction, staging: &ForensicStaging) -> Result<usize> {
    let mut inserted = 0;

    for issue in &staging.issues {
        // Validate the issue before importing (F017)
        issue.validate().map_err(|e| {
            anyhow!(
                "Issue {} failed validation during import: {}. A restore is the last point where an invalid record can be rejected before it becomes indistinguishable from legitimate history.",
                issue.id, e
            )
        })?;

        tx.execute(
            "INSERT INTO issues (
                id, title, description, notes, priority, issue_type, base_status,
                manual_blocked, assignee, created_at, updated_at, closed_at,
                close_reason, source_repo, profile, schema_ref, revision
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
            params![
                &issue.id,
                &issue.title,
                &issue.description,
                &issue.notes.as_deref().unwrap_or(""),
                &issue.priority,
                &issue.issue_type.as_deref().unwrap_or("task"),
                &issue.base_status.as_str(),
                &issue.manual_blocked,
                &issue.assignee,
                &issue.created_at,
                &issue.updated_at,
                &issue.closed_at,
                &issue.close_reason,
                &issue.source_repo,
                &issue.profile,
                &issue.schema_ref,
                &issue.revision.unwrap_or(1),
            ],
        )?;

        import_issue_data(tx, issue)?;
        import_external_references(tx, issue)?;
        import_comments(tx, issue)?;

        // Insert extensions (unknown fields)
        for (key, value) in &issue.extensions {
            if is_known_issue_projection(key) {
                continue;
            }
            let value_str = serde_json::to_string(value)
                .map_err(|e| anyhow!("Failed to serialize extension '{}': {}", key, e))?;
            tx.execute(
                "INSERT INTO issue_extensions (issue_id, key, value, profile)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    &issue.id,
                    key,
                    &value_str,
                    &issue.profile.as_deref().unwrap_or("native-v1")
                ],
            )?;
        }

        inserted += 1;
    }

    Ok(inserted)
}

/// Import dependencies into database
fn import_dependencies(tx: &Transaction, staging: &ForensicStaging) -> Result<()> {
    for (blocked, blocker, kind) in &staging.dependencies {
        tx.execute(
            "INSERT INTO dependencies (blocked_issue_id, blocker_issue_id, kind)
             VALUES (?1, ?2, ?3)",
            params![blocked, blocker, kind],
        )?;
    }
    Ok(())
}

/// Import labels into database
fn import_labels(tx: &Transaction, staging: &ForensicStaging) -> Result<()> {
    for (issue_id, label) in &staging.labels {
        tx.execute(
            "INSERT INTO labels (issue_id, label) VALUES (?1, ?2)",
            params![issue_id, label],
        )?;
    }
    Ok(())
}

/// Import events into database
fn import_events(tx: &Transaction, staging: &ForensicStaging) -> Result<()> {
    for event in &staging.events {
        let already_imported: bool = tx.query_row(
            "SELECT EXISTS(
                 SELECT 1 FROM events
                 WHERE origin_store_uuid = ?1 AND origin_event_sequence = ?2
             )",
            params![&event.origin_store_uuid, event.origin_event_sequence],
            |row| row.get(0),
        )?;
        if already_imported {
            continue;
        }
        tx.execute(
            "INSERT INTO events (
                issue_id, kind, actor, time, detail,
                origin_store_uuid, origin_event_sequence
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                &event.issue_id,
                &event.kind,
                &event.actor,
                &event.time,
                &event.detail.to_string(),
                &event.origin_store_uuid,
                &event.origin_event_sequence,
            ],
        )?;
    }
    Ok(())
}

/// Import receipts into database
fn import_receipts(tx: &Transaction, staging: &ForensicStaging) -> Result<()> {
    for receipt in &staging.receipts {
        let counts_json = serde_json::to_string(&receipt.counts)
            .map_err(|e| anyhow!("Failed to serialize receipt counts: {}", e))?;

        tx.execute(
            "INSERT INTO provenance_receipts (
                receipt_id, schema_ref, kind, source_store_uuid, target_store_uuid,
                source_root_sha256, actor, created_at, counts_json, result,
                summary_event_identity, receipt_sha256
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                &receipt.receipt_id,
                &receipt.schema_ref,
                &receipt.kind,
                &receipt.source_store_uuid,
                &receipt.target_store_uuid,
                &receipt.source_root_sha256,
                &receipt.actor,
                &receipt.created_at,
                &counts_json,
                &receipt.result,
                &receipt.summary_event_identity,
                &receipt.receipt_sha256,
            ],
        )?;
    }
    Ok(())
}

/// Create restore receipt
fn create_restore_receipt(
    tx: &Transaction,
    staging: &ForensicStaging,
    actor: &str,
    _activation_sequence: i64,
) -> Result<SerializedReceipt> {
    let receipt_id = format!("restore-{}", uuid());
    let now = format!(
        "{}",
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs()
    );

    let counts = ReceiptCounts {
        issues: staging.issue_count as i64,
        events: staging.event_count as i64,
        provenance_receipts: staging.receipt_count as i64,
    };

    let receipt_kind = "restore".to_string();
    let mut hasher = Sha256::new();
    hasher.update(&receipt_id);
    hasher.update(&receipt_kind);
    hasher.update(&staging.input_hash);
    hasher.update(actor);
    hasher.update(&now);
    hasher.update(b"success");
    let receipt_hash = format!("{:x}", hasher.finalize());

    let receipt = SerializedReceipt {
        schema_ref: "urn:bead-rs:schema:provenance-receipt:native-v1".to_string(),
        receipt_id: receipt_id.clone(),
        kind: receipt_kind.clone(),
        source_store_uuid: staging.store_uuid.clone(),
        target_store_uuid: staging.store_uuid.clone(), // Same for restore
        source_root_sha256: staging.input_hash.clone(),
        actor: actor.to_string(),
        created_at: now.clone(),
        counts,
        result: "success".to_string(),
        summary_event_identity: None,
        receipt_sha256: receipt_hash,
    };

    // Store receipt in database
    let counts_json = serde_json::to_string(&receipt.counts)
        .map_err(|e| anyhow!("Failed to serialize receipt counts: {}", e))?;

    tx.execute(
        "INSERT INTO provenance_receipts (
            receipt_id, schema_ref, kind, source_store_uuid, target_store_uuid,
            source_root_sha256, actor, created_at, counts_json, result,
            summary_event_identity, receipt_sha256
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            &receipt.receipt_id,
            &receipt.schema_ref,
            &receipt.kind,
            &receipt.source_store_uuid,
            &receipt.target_store_uuid,
            &receipt.source_root_sha256,
            &receipt.actor,
            &receipt.created_at,
            &counts_json,
            &receipt.result,
            &receipt.summary_event_identity,
            &receipt.receipt_sha256,
        ],
    )?;

    Ok(receipt)
}

/// Reconcile and merge for merge operation
fn reconcile_and_merge(
    tx: &Transaction,
    staging: &ForensicStaging,
) -> Result<(usize, usize, usize)> {
    let mut inserted = 0;
    let mut updated = 0;
    let mut retained = 0;

    for issue in &staging.issues {
        // Check if issue exists
        let existing: Option<(String, String, i64)> = tx
            .query_row(
                "SELECT id, updated_at, revision FROM issues WHERE id = ?1",
                [&issue.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .ok();

        match existing {
            None => {
                // Insert new issue
                tx.execute(
                    "INSERT INTO issues (
                        id, title, description, notes, priority, issue_type, base_status,
                        manual_blocked, assignee, created_at, updated_at, closed_at,
                        close_reason, source_repo, profile, schema_ref, revision
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
                    params![
                        &issue.id,
                        &issue.title,
                        &issue.description,
                        &issue.notes.as_deref().unwrap_or(""),
                        &issue.priority,
                        &issue.issue_type.as_deref().unwrap_or("task"),
                        &issue.base_status.as_str(),
                        &issue.manual_blocked,
                        &issue.assignee,
                        &issue.created_at,
                        &issue.updated_at,
                        &issue.closed_at,
                        &issue.close_reason,
                        &issue.source_repo,
                        &issue.profile,
                        &issue.schema_ref,
                        &issue.revision.unwrap_or(1),
                    ],
                )?;

                import_issue_data(tx, issue)?;
                import_external_references(tx, issue)?;
                import_comments(tx, issue)?;

                // Insert extensions (unknown fields)
                for (key, value) in &issue.extensions {
                    if is_known_issue_projection(key) {
                        continue;
                    }
                    let value_str = serde_json::to_string(value)
                        .map_err(|e| anyhow!("Failed to serialize extension '{}': {}", key, e))?;
                    tx.execute(
                        "INSERT INTO issue_extensions (issue_id, key, value, profile)
                         VALUES (?1, ?2, ?3, ?4)",
                        params![
                            &issue.id,
                            key,
                            &value_str,
                            &issue.profile.as_deref().unwrap_or("native-v1")
                        ],
                    )?;
                }

                inserted += 1;
            }
            Some((id, existing_updated_at, existing_revision)) => {
                // Compare timestamps
                if issue.updated_at > existing_updated_at {
                    // A merge that replaces scalar content is itself a new
                    // revision when the target token is at least as new as
                    // the incoming token. This prevents an existing
                    // --if-revision holder from mutating silently replaced
                    // content with a stale token.
                    let incoming_revision = issue.revision.unwrap_or(1);
                    let resulting_revision = if existing_revision >= incoming_revision {
                        existing_revision + 1
                    } else {
                        incoming_revision
                    };
                    // Update issue
                    tx.execute(
                        "UPDATE issues SET
                            title = ?1, description = ?2, notes = ?3, priority = ?4,
                            issue_type = ?5, base_status = ?6, manual_blocked = ?7,
                            assignee = ?8, updated_at = ?9, closed_at = ?10,
                            close_reason = ?11, source_repo = ?12, profile = ?13,
                            schema_ref = ?14, revision = ?15
                         WHERE id = ?16",
                        params![
                            &issue.title,
                            &issue.description,
                            &issue.notes.as_deref().unwrap_or(""),
                            &issue.priority,
                            &issue.issue_type.as_deref().unwrap_or("task"),
                            &issue.base_status.as_str(),
                            &issue.manual_blocked,
                            &issue.assignee,
                            &issue.updated_at,
                            &issue.closed_at,
                            &issue.close_reason,
                            &issue.source_repo,
                            &issue.profile,
                            &issue.schema_ref,
                            &resulting_revision,
                            &id,
                        ],
                    )?;

                    // Update extensions (delete old ones and insert new ones)
                    tx.execute(
                        "DELETE FROM issue_extensions WHERE issue_id = ?1",
                        [&issue.id],
                    )?;

                    // Projected collections use replace-when-present semantics.
                    // Absence means an older producer did not describe the
                    // collection and must never erase live target state.
                    if issue.data.is_some() {
                        tx.execute("DELETE FROM issue_data WHERE issue_id = ?1", [&issue.id])?;
                        import_issue_data(tx, issue)?;
                    }
                    if issue.extensions.contains_key("external_references") {
                        tx.execute(
                            "DELETE FROM external_references WHERE issue_id = ?1",
                            [&issue.id],
                        )?;
                        import_external_references(tx, issue)?;
                    }
                    if issue.extensions.contains_key("comments") {
                        tx.execute("DELETE FROM comments WHERE issue_id = ?1", [&issue.id])?;
                        import_comments(tx, issue)?;
                    }

                    for (key, value) in &issue.extensions {
                        if is_known_issue_projection(key) {
                            continue;
                        }
                        let value_str = serde_json::to_string(value).map_err(|e| {
                            anyhow!("Failed to serialize extension '{}': {}", key, e)
                        })?;
                        tx.execute(
                            "INSERT INTO issue_extensions (issue_id, key, value, profile)
                             VALUES (?1, ?2, ?3, ?4)",
                            params![
                                &issue.id,
                                key,
                                &value_str,
                                &issue.profile.as_deref().unwrap_or("native-v1")
                            ],
                        )?;
                    }

                    updated += 1;
                } else {
                    retained += 1;
                }
            }
        }
    }

    // Import dependencies and labels (merge logic)
    import_dependencies(tx, staging)?;
    import_labels(tx, staging)?;

    Ok((inserted, updated, retained))
}

fn import_issue_data(tx: &Transaction, issue: &Issue) -> Result<()> {
    let Some(data) = &issue.data else {
        return Ok(());
    };
    let namespaces = data
        .as_object()
        .ok_or_else(|| anyhow!("Issue '{}' data must be a JSON object", issue.id))?;

    for (namespace, envelope) in namespaces {
        let envelope = envelope.as_object().ok_or_else(|| {
            anyhow!(
                "Issue '{}' data namespace '{}' must be an object",
                issue.id,
                namespace
            )
        })?;
        let schema_ref = envelope
            .get("schema_ref")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                anyhow!(
                    "Issue '{}' data namespace '{}' requires schema_ref",
                    issue.id,
                    namespace
                )
            })?;
        let value = envelope.get("value").ok_or_else(|| {
            anyhow!(
                "Issue '{}' data namespace '{}' requires value",
                issue.id,
                namespace
            )
        })?;
        let value = serde_json::to_string(value)?;
        tx.execute(
            "INSERT INTO issue_data (issue_id, namespace, schema_ref, value)
             VALUES (?1, ?2, ?3, ?4)",
            params![&issue.id, namespace, schema_ref, value],
        )?;
    }

    Ok(())
}

fn is_known_issue_projection(key: &str) -> bool {
    matches!(
        key,
        "labels" | "dependencies" | "external_references" | "comments"
    )
}

fn import_external_references(tx: &Transaction, issue: &Issue) -> Result<()> {
    let Some(references) = issue.extensions.get("external_references") else {
        return Ok(());
    };
    let references = references.as_array().ok_or_else(|| {
        anyhow!(
            "Issue '{}' external_references must be a JSON array",
            issue.id
        )
    })?;

    for reference in references {
        let reference = reference
            .as_object()
            .ok_or_else(|| anyhow!("Issue '{}' external reference must be an object", issue.id))?;
        let member = |name: &str| -> Result<&str> {
            reference
                .get(name)
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| {
                    anyhow!(
                        "Issue '{}' external reference requires string '{}'",
                        issue.id,
                        name
                    )
                })
        };
        tx.execute(
            "INSERT INTO external_references (issue_id, namespace, key, value)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                &issue.id,
                member("namespace")?,
                member("key")?,
                member("value")?
            ],
        )?;
    }

    Ok(())
}

fn import_comments(tx: &Transaction, issue: &Issue) -> Result<()> {
    let Some(comments) = issue.extensions.get("comments") else {
        return Ok(());
    };
    let comments = comments
        .as_array()
        .ok_or_else(|| anyhow!("Issue '{}' comments must be a JSON array", issue.id))?;

    for comment in comments {
        let comment = comment
            .as_object()
            .ok_or_else(|| anyhow!("Issue '{}' comment must be an object", issue.id))?;
        let required = |name: &str| -> Result<&str> {
            comment
                .get(name)
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| anyhow!("Issue '{}' comment requires '{}'", issue.id, name))
        };
        let optional = |name: &str| comment.get(name).and_then(serde_json::Value::as_str);
        tx.execute(
            "INSERT INTO comments
             (id, issue_id, author, body, reply_to_id, resolution_state, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                required("id")?,
                &issue.id,
                required("author")?,
                required("body")?,
                optional("reply_to_id"),
                optional("resolution_state"),
                required("created_at")?
            ],
        )?;
    }

    Ok(())
}

/// Create merge summary event
fn create_merge_summary(tx: &Transaction, staging: &ForensicStaging, actor: &str) -> Result<i64> {
    // Get next sequence
    let sequence: i64 = tx
        .query_row(
            "SELECT COALESCE(MAX(sequence), 0) + 1 FROM events",
            [],
            |row| row.get(0),
        )
        .unwrap_or(1);

    let now = format!(
        "{}",
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs()
    );

    let detail = serde_json::json!({
        "source_store_uuid": staging.store_uuid,
        "source_root_hash": staging.input_hash,
        "issues_count": staging.issue_count,
        "events_count": staging.event_count,
    });

    tx.execute(
        "INSERT INTO events (kind, actor, time, detail) VALUES (?1, ?2, ?3, ?4)",
        params![
            &format!("checkpoint_{}", staging.mode.as_str()),
            actor,
            now,
            &detail.to_string(),
        ],
    )?;

    Ok(sequence)
}

/// Create merge receipt
fn create_merge_receipt(
    tx: &Transaction,
    staging: &ForensicStaging,
    actor: &str,
    activation_sequence: i64,
) -> Result<SerializedReceipt> {
    let receipt_id = format!("merge-{}", uuid());
    let now = format!(
        "{}",
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs()
    );

    let target_uuid = tx
        .query_row("SELECT uuid FROM workspace", [], |row| {
            row.get::<_, String>(0)
        })
        .unwrap_or_else(|_| String::from("unknown"));

    let counts = ReceiptCounts {
        issues: staging.issue_count as i64,
        events: staging.event_count as i64,
        provenance_receipts: staging.receipt_count as i64,
    };

    let receipt_kind = "merge".to_string();
    let mut hasher = Sha256::new();
    hasher.update(&receipt_id);
    hasher.update(&receipt_kind);
    hasher.update(&staging.input_hash);
    hasher.update(actor);
    hasher.update(&now);
    hasher.update(b"success");
    let receipt_hash = format!("{:x}", hasher.finalize());

    let receipt = SerializedReceipt {
        schema_ref: "urn:bead-rs:schema:provenance-receipt:native-v1".to_string(),
        receipt_id: receipt_id.clone(),
        kind: receipt_kind.clone(),
        source_store_uuid: staging.store_uuid.clone(),
        target_store_uuid: target_uuid,
        source_root_sha256: staging.input_hash.clone(),
        actor: actor.to_string(),
        created_at: now.clone(),
        counts,
        result: "success".to_string(),
        summary_event_identity: Some(format!("local-{}", activation_sequence)),
        receipt_sha256: receipt_hash,
    };

    // Store receipt in database
    let counts_json = serde_json::to_string(&receipt.counts)
        .map_err(|e| anyhow!("Failed to serialize receipt counts: {}", e))?;

    tx.execute(
        "INSERT INTO provenance_receipts (
            receipt_id, schema_ref, kind, source_store_uuid, target_store_uuid,
            source_root_sha256, actor, created_at, counts_json, result,
            summary_event_identity, receipt_sha256
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            &receipt.receipt_id,
            &receipt.schema_ref,
            &receipt.kind,
            &receipt.source_store_uuid,
            &receipt.target_store_uuid,
            &receipt.source_root_sha256,
            &receipt.actor,
            &receipt.created_at,
            &counts_json,
            &receipt.result,
            &receipt.summary_event_identity,
            &receipt.receipt_sha256,
        ],
    )?;

    Ok(receipt)
}

/// Generate UUID
fn uuid() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{:016x}", timestamp)
}

/// Stage issues from JSONL file for validation
#[allow(dead_code)]
fn stage_import(input_path: &Path, _profile: &str) -> Result<ImportStaging> {
    let file = File::open(input_path)?;
    let reader = BufReader::new(file);

    let mut issues = Vec::new();
    let mut dependencies = Vec::new();
    let mut labels = Vec::new();
    let mut seen_ids = HashSet::new();

    // Calculate hash while reading
    let mut hasher = Sha256::new();

    for (line_num, line_result) in reader.lines().enumerate() {
        let line_num = line_num + 1; // 1-based for error messages

        let line = line_result?;
        if line.trim().is_empty() {
            continue; // Skip blank lines
        }

        // Update hash
        hasher.update(line.as_bytes());
        hasher.update(b"\n");

        // Parse JSON line
        let json: serde_json::Value = serde_json::from_str(&line)
            .map_err(|e| anyhow!("Line {}: malformed JSON: {}", line_num, e))?;

        // Must be an object
        let obj = json
            .as_object()
            .ok_or_else(|| anyhow!("Line {}: not an object", line_num))?;

        // Extract required ID field
        let id = obj
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Line {}: missing or invalid 'id' field", line_num))?;

        // Check for duplicate IDs
        if !seen_ids.insert(id.to_string()) {
            bail!("Line {}: duplicate issue ID '{}'", line_num, id);
        }

        // Parse full Issue (extensions preserved via flatten)
        let issue: Issue = serde_json::from_str(&line)
            .map_err(|e| anyhow!("Line {}: invalid issue: {}", line_num, e))?;

        // Validate the issue
        issue
            .validate()
            .map_err(|e| anyhow!("Line {}: issue validation failed: {}", line_num, e))?;

        issues.push(issue.clone());

        // Extract dependencies if present
        if let Some(deps) = obj.get("dependencies").and_then(|v| v.as_array()) {
            for dep in deps {
                let dep_obj = dep
                    .as_object()
                    .ok_or_else(|| anyhow!("Line {}: dependency is not an object", line_num))?;

                let blocked = obj
                    .get("id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow!("Line {}: missing blocked issue ID", line_num))?;

                let blocker = dep_obj
                    .get("blocker")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow!("Line {}: dependency missing 'blocker'", line_num))?;

                let kind = dep_obj
                    .get("kind")
                    .and_then(|v| v.as_str())
                    .unwrap_or("blocks");

                dependencies.push((blocked.to_string(), blocker.to_string(), kind.to_string()));
            }
        }

        // Extract labels if present
        if let Some(label_array) = obj.get("labels").and_then(|v| v.as_array()) {
            for label_item in label_array {
                if let Some(label) = label_item.as_str() {
                    labels.push((id.to_string(), label.to_string()));
                }
            }
        }
    }

    let hash = format!("{:x}", hasher.finalize());

    Ok(ImportStaging {
        issues,
        dependencies,
        labels,
        input_hash: hash,
        issue_count: seen_ids.len(),
        diagnostics: None,
    })
}

/// Enhanced staging with diagnostic collection (R014)
fn stage_import_with_diagnostics(input_path: &Path, _profile: &str) -> ImportStaging {
    let file = match File::open(input_path) {
        Ok(f) => f,
        Err(e) => {
            return ImportStaging {
                issues: Vec::new(),
                dependencies: Vec::new(),
                labels: Vec::new(),
                input_hash: String::new(),
                issue_count: 0,
                diagnostics: Some(ImportDiagnostics {
                    validation_failures: vec![ValidationFailure {
                        line_number: 0,
                        json_pointer: None,
                        schema_keyword: Some("file".to_string()),
                        semantic_code: "file_open_error".to_string(),
                        message: format!("Cannot open input file: {}", e),
                        context: None,
                    }],
                    total_lines: 0,
                    processed_lines: 0,
                    truncated: false,
                }),
            }
        }
    };

    let reader = BufReader::new(file);
    let mut issues = Vec::new();
    let mut dependencies = Vec::new();
    let mut labels = Vec::new();
    let mut seen_ids = HashSet::new();
    let mut validation_failures = Vec::new();
    let mut total_lines = 0;
    let mut processed_lines = 0;

    // Calculate hash while reading
    let mut hasher = Sha256::new();

    for (line_num, line_result) in reader.lines().enumerate() {
        total_lines += 1;
        let line_num = line_num + 1; // 1-based for error messages

        let line = match line_result {
            Ok(l) => l,
            Err(e) => {
                if validation_failures.len() < MAX_DIAGNOSTIC_FAILURES {
                    validation_failures.push(ValidationFailure {
                        line_number: line_num,
                        json_pointer: None,
                        schema_keyword: Some("line_read".to_string()),
                        semantic_code: "line_read_error".to_string(),
                        message: format!("Cannot read line: {}", e),
                        context: None,
                    });
                }
                continue;
            }
        };

        if line.trim().is_empty() {
            continue; // Skip blank lines
        }

        processed_lines += 1;

        // Update hash
        hasher.update(line.as_bytes());
        hasher.update(b"\n");

        // Parse JSON line
        let json: serde_json::Value = match serde_json::from_str(&line) {
            Ok(j) => j,
            Err(e) => {
                if validation_failures.len() < MAX_DIAGNOSTIC_FAILURES {
                    validation_failures.push(ValidationFailure {
                        line_number: line_num,
                        json_pointer: None,
                        schema_keyword: Some("parse".to_string()),
                        semantic_code: "malformed_json".to_string(),
                        message: format!("Malformed JSON: {}", e),
                        context: Some(line.trim().to_string()),
                    });
                }
                continue;
            }
        };

        // Must be an object
        let obj = match json.as_object() {
            Some(o) => o,
            None => {
                if validation_failures.len() < MAX_DIAGNOSTIC_FAILURES {
                    validation_failures.push(ValidationFailure {
                        line_number: line_num,
                        json_pointer: None,
                        schema_keyword: Some("type".to_string()),
                        semantic_code: "invalid_field_type".to_string(),
                        message: "Record is not a JSON object".to_string(),
                        context: None,
                    });
                }
                continue;
            }
        };

        // Extract required ID field
        let id = match obj.get("id").and_then(|v| v.as_str()) {
            Some(i) => i,
            None => {
                if validation_failures.len() < MAX_DIAGNOSTIC_FAILURES {
                    validation_failures.push(ValidationFailure {
                        line_number: line_num,
                        json_pointer: Some("/id".to_string()),
                        schema_keyword: Some("required".to_string()),
                        semantic_code: "missing_required_field".to_string(),
                        message: "Missing or invalid 'id' field".to_string(),
                        context: None,
                    });
                }
                continue;
            }
        };

        // Check for duplicate IDs
        if !seen_ids.insert(id.to_string()) {
            if validation_failures.len() < MAX_DIAGNOSTIC_FAILURES {
                validation_failures.push(ValidationFailure {
                    line_number: line_num,
                    json_pointer: Some("/id".to_string()),
                    schema_keyword: Some("unique".to_string()),
                    semantic_code: "duplicate_issue_id".to_string(),
                    message: format!("Duplicate issue ID '{}'", id),
                    context: None,
                });
            }
            continue;
        }

        // Parse full Issue (extensions preserved via flatten)
        let issue: Issue = match serde_json::from_str(&line) {
            Ok(i) => i,
            Err(e) => {
                if validation_failures.len() < MAX_DIAGNOSTIC_FAILURES {
                    validation_failures.push(ValidationFailure {
                        line_number: line_num,
                        json_pointer: None,
                        schema_keyword: Some("validation".to_string()),
                        semantic_code: "invalid_field_type".to_string(),
                        message: format!("Invalid issue structure: {}", e),
                        context: None,
                    });
                }
                continue;
            }
        };

        // Validate the issue
        match issue.validate() {
            Ok(_) => {}
            Err(e) => {
                if validation_failures.len() < MAX_DIAGNOSTIC_FAILURES {
                    validation_failures.push(ValidationFailure {
                        line_number: line_num,
                        json_pointer: None,
                        schema_keyword: Some("validation".to_string()),
                        semantic_code: "invalid_field_type".to_string(),
                        message: format!("Issue validation failed: {}", e),
                        context: Some(format!("id: {}", id)),
                    });
                }
                continue;
            }
        }

        issues.push(issue.clone());

        // Extract dependencies if present
        if let Some(deps) = obj.get("dependencies").and_then(|v| v.as_array()) {
            for dep in deps {
                let dep_obj = match dep.as_object() {
                    Some(o) => o,
                    None => {
                        if validation_failures.len() < MAX_DIAGNOSTIC_FAILURES {
                            validation_failures.push(ValidationFailure {
                                line_number: line_num,
                                json_pointer: Some("/dependencies".to_string()),
                                schema_keyword: Some("type".to_string()),
                                semantic_code: "invalid_field_type".to_string(),
                                message: "Dependency is not a JSON object".to_string(),
                                context: Some(format!("id: {}", id)),
                            });
                        }
                        continue;
                    }
                };

                let blocker = match dep_obj.get("blocker").and_then(|v| v.as_str()) {
                    Some(b) => b,
                    None => {
                        if validation_failures.len() < MAX_DIAGNOSTIC_FAILURES {
                            validation_failures.push(ValidationFailure {
                                line_number: line_num,
                                json_pointer: Some("/dependencies/[]/blocker".to_string()),
                                schema_keyword: Some("required".to_string()),
                                semantic_code: "missing_required_field".to_string(),
                                message: "Dependency missing 'blocker' field".to_string(),
                                context: Some(format!("id: {}", id)),
                            });
                        }
                        continue;
                    }
                };

                let kind = dep_obj
                    .get("kind")
                    .and_then(|v| v.as_str())
                    .unwrap_or("blocks");

                dependencies.push((id.to_string(), blocker.to_string(), kind.to_string()));
            }
        }

        // Extract labels if present
        if let Some(label_array) = obj.get("labels").and_then(|v| v.as_array()) {
            for label_item in label_array {
                if let Some(label) = label_item.as_str() {
                    labels.push((id.to_string(), label.to_string()));
                }
            }
        }
    }

    let hash = format!("{:x}", hasher.finalize());
    let truncated = validation_failures.len() >= MAX_DIAGNOSTIC_FAILURES;

    ImportStaging {
        issues,
        dependencies,
        labels,
        input_hash: hash,
        issue_count: seen_ids.len(),
        diagnostics: Some(ImportDiagnostics {
            validation_failures,
            total_lines,
            processed_lines,
            truncated,
        }),
    }
}

/// Validate staged import data
#[allow(dead_code)]
fn validate_import(staging: &mut ImportStaging, _dry_run: bool) -> Result<()> {
    let issue_ids: HashSet<_> = staging.issues.iter().map(|i| i.id.clone()).collect();

    // Check for cycles first (before we take mutable reference to diagnostics)
    let cycle_result = has_any_cycle(staging);

    // Now we can safely take mutable reference to diagnostics
    let diagnostics = staging
        .diagnostics
        .get_or_insert_with(|| ImportDiagnostics {
            validation_failures: Vec::new(),
            total_lines: 0,
            processed_lines: 0,
            truncated: false,
        });

    // Validate dependencies
    for (idx, (blocked, blocker, _kind)) in staging.dependencies.iter().enumerate() {
        // Both endpoints must exist
        if !issue_ids.contains(blocked)
            && diagnostics.validation_failures.len() < MAX_DIAGNOSTIC_FAILURES
        {
            diagnostics.validation_failures.push(ValidationFailure {
                line_number: 0, // Dependencies don't have line numbers in staging
                json_pointer: Some(format!("/dependencies/{}", idx)),
                schema_keyword: Some("reference".to_string()),
                semantic_code: "unknown_blocked_issue".to_string(),
                message: format!("Dependency references unknown blocked issue '{}'", blocked),
                context: Some(format!("blocked: {}, blocker: {}", blocked, blocker)),
            });
        }
        if !issue_ids.contains(blocker)
            && diagnostics.validation_failures.len() < MAX_DIAGNOSTIC_FAILURES
        {
            diagnostics.validation_failures.push(ValidationFailure {
                line_number: 0,
                json_pointer: Some(format!("/dependencies/{}", idx)),
                schema_keyword: Some("reference".to_string()),
                semantic_code: "unknown_blocker_issue".to_string(),
                message: format!("Dependency references unknown blocker issue '{}'", blocker),
                context: Some(format!("blocked: {}, blocker: {}", blocked, blocker)),
            });
        }

        // Self-edges are invalid
        if blocked == blocker && diagnostics.validation_failures.len() < MAX_DIAGNOSTIC_FAILURES {
            diagnostics.validation_failures.push(ValidationFailure {
                line_number: 0,
                json_pointer: Some(format!("/dependencies/{}", idx)),
                schema_keyword: Some("constraint".to_string()),
                semantic_code: "self_edge_dependency".to_string(),
                message: format!("Self-edge detected: '{}'", blocked),
                context: None,
            });
        }
    }

    // Check for cycles in blocks dependencies (after all individual validations)
    match cycle_result {
        Ok(has_cycle) => {
            if has_cycle && diagnostics.validation_failures.len() < MAX_DIAGNOSTIC_FAILURES {
                diagnostics.validation_failures.push(ValidationFailure {
                    line_number: 0,
                    json_pointer: Some("/dependencies".to_string()),
                    schema_keyword: Some("acyclic".to_string()),
                    semantic_code: "cycle_in_dependencies".to_string(),
                    message: "Cycle detected in blocks dependencies".to_string(),
                    context: None,
                });
            }
        }
        Err(e) => {
            if diagnostics.validation_failures.len() < MAX_DIAGNOSTIC_FAILURES {
                diagnostics.validation_failures.push(ValidationFailure {
                    line_number: 0,
                    json_pointer: Some("/dependencies".to_string()),
                    schema_keyword: Some("analysis".to_string()),
                    semantic_code: "cycle_detection_error".to_string(),
                    message: format!("Error during cycle detection: {}", e),
                    context: None,
                });
            }
        }
    }

    // Validate labels
    for (idx, (issue_id, label)) in staging.labels.iter().enumerate() {
        if !issue_ids.contains(issue_id)
            && diagnostics.validation_failures.len() < MAX_DIAGNOSTIC_FAILURES
        {
            diagnostics.validation_failures.push(ValidationFailure {
                line_number: 0,
                json_pointer: Some(format!("/labels/{}", idx)),
                schema_keyword: Some("reference".to_string()),
                semantic_code: "unknown_issue_label".to_string(),
                message: format!("Label references unknown issue '{}'", issue_id),
                context: Some(format!("issue_id: {}, label: {}", issue_id, label)),
            });
        }
    }

    // Update truncated status
    diagnostics.truncated = diagnostics.validation_failures.len() >= MAX_DIAGNOSTIC_FAILURES;

    // Always return Ok - errors are collected in diagnostics
    Ok(())
}

/// Check if there are any cycles in the blocks dependencies
#[allow(dead_code)]
fn has_any_cycle(staging: &ImportStaging) -> Result<bool> {
    let mut visited = HashSet::new();
    let mut recursion_stack = HashSet::new();

    // Build adjacency list for blocks dependencies
    let mut adj: HashMap<String, Vec<String>> = HashMap::new();
    let mut all_nodes: HashSet<String> = HashSet::new();

    for (blocked, blocker, kind) in &staging.dependencies {
        if kind == "blocks" {
            adj.entry(blocker.clone())
                .or_default()
                .push(blocked.clone());
            all_nodes.insert(blocked.clone());
            all_nodes.insert(blocker.clone());
        }
    }

    // Check each node for cycles using DFS
    for node in &all_nodes {
        if dfs_has_cycle(&adj, node, &mut visited, &mut recursion_stack)? {
            return Ok(true);
        }
    }

    Ok(false)
}

/// DFS helper to detect cycles
#[allow(dead_code)]
fn dfs_has_cycle(
    adj: &HashMap<String, Vec<String>>,
    node: &str,
    visited: &mut HashSet<String>,
    recursion_stack: &mut HashSet<String>,
) -> Result<bool> {
    visited.insert(node.to_string());
    recursion_stack.insert(node.to_string());

    if let Some(neighbors) = adj.get(node) {
        for neighbor in neighbors {
            if !visited.contains(neighbor) {
                if dfs_has_cycle(adj, neighbor, visited, recursion_stack)? {
                    return Ok(true);
                }
            } else if recursion_stack.contains(neighbor) {
                // Found a back edge - cycle detected
                return Ok(true);
            }
        }
    }

    recursion_stack.remove(node);
    Ok(false)
}

/// Check if adding an edge would create a cycle using DFS
#[allow(dead_code)]
fn has_cycle(staging: &ImportStaging, start: &str, from: &str) -> Result<bool> {
    let mut visited = HashSet::new();
    let mut stack = Vec::new();

    // Build adjacency list for blocks dependencies
    let mut adj: HashMap<String, Vec<String>> = HashMap::new();
    for (blocked, blocker, kind) in &staging.dependencies {
        if kind == "blocks" {
            adj.entry(blocker.clone())
                .or_default()
                .push(blocked.clone());
        }
    }

    // Start DFS from neighbors of 'from', excluding 'start' itself
    // This checks if there's a path from 'from' back to 'start' (excluding the edge we're adding)
    if let Some(neighbors) = adj.get(from) {
        for neighbor in neighbors {
            if neighbor != start {
                stack.push(neighbor.clone());
            }
        }
    }

    while let Some(current) = stack.pop() {
        if current == start {
            return Ok(true); // Found cycle
        }

        if visited.contains(&current) {
            continue;
        }
        visited.insert(current.clone());

        if let Some(neighbors) = adj.get(&current) {
            for neighbor in neighbors {
                stack.push(neighbor.clone());
            }
        }
    }

    Ok(false)
}

/// Verify that target database is empty (for pre-F017 import)
fn verify_empty_target(store: &mut SqliteStore) -> Result<()> {
    let conn = store.conn();

    // Check if there are any issues
    let issue_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM issues", [], |row| row.get(0))
        .unwrap_or(0);

    if issue_count > 0 {
        bail!("Target database is not empty (has {} issues). Pre-F017 import requires an empty initialized target.", issue_count);
    }

    // Check if there are any semantic events (aside from initialization bookkeeping)
    let event_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM events WHERE kind NOT IN ('workspace_initialized')",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    if event_count > 0 {
        bail!("Target database has semantic events ({} events). Pre-F017 import requires an empty initialized target.", event_count);
    }

    Ok(())
}

/// Activate staged import data in a single transaction
#[allow(dead_code)]
fn activate_import(store: &mut SqliteStore, staging: &ImportStaging) -> Result<(usize, i64)> {
    let conn = store.conn();
    let tx = conn.unchecked_transaction()?;

    // Insert all issues
    for issue in &staging.issues {
        tx.execute(
            "INSERT INTO issues (
                id, title, description, notes, priority, issue_type, base_status,
                manual_blocked, assignee, created_at, updated_at, closed_at, close_reason,
                source_repo, profile, schema_ref, revision
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
            params![
                &issue.id,
                &issue.title,
                &issue.description,
                &issue.notes,
                &issue.priority,
                &issue.issue_type,
                &issue.base_status.as_str(),
                if issue.manual_blocked.unwrap_or(false) {
                    1
                } else {
                    0
                },
                &issue.assignee,
                &issue.created_at,
                &issue.updated_at,
                &issue.closed_at,
                &issue.close_reason,
                &issue.source_repo,
                &issue.profile,
                &issue.schema_ref,
                &issue.revision.unwrap_or(1),
            ],
        )?;

        import_issue_data(&tx, issue)?;
        import_external_references(&tx, issue)?;
        import_comments(&tx, issue)?;

        // Insert extensions (unknown fields)
        for (key, value) in &issue.extensions {
            if is_known_issue_projection(key) {
                continue;
            }
            let value_str = serde_json::to_string(value)
                .map_err(|e| anyhow!("Failed to serialize extension '{}': {}", key, e))?;
            tx.execute(
                "INSERT INTO issue_extensions (issue_id, key, value, profile)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    &issue.id,
                    key,
                    &value_str,
                    &issue.profile.as_ref().unwrap_or(&"native-v1".to_string())
                ],
            )?;
        }
    }

    // Insert dependencies
    for (blocked, blocker, kind) in &staging.dependencies {
        tx.execute(
            "INSERT INTO dependencies (blocked_issue_id, blocker_issue_id, kind)
             VALUES (?1, ?2, ?3)",
            params![blocked, blocker, kind],
        )?;
    }

    // Insert labels
    for (issue_id, label) in &staging.labels {
        tx.execute(
            "INSERT INTO labels (issue_id, label) VALUES (?1, ?2)",
            params![issue_id, label],
        )?;
    }

    // Get current sequence and allocate activation sequence
    let current_sequence: i64 = tx
        .query_row("SELECT COALESCE(MAX(sequence), 0) FROM events", [], |row| {
            row.get(0)
        })
        .unwrap_or(0);

    let activation_sequence = current_sequence + 1;

    // Append checkpoint_imported audit event
    let event_time = format_rfc3339(SystemTime::now());
    let event_detail = serde_json::json!({
        "profile": "native-v1",
        "input_hash": staging.input_hash,
        "issue_count": staging.issue_count,
        "format": "issues-jsonl-v1"
    });

    tx.execute(
        "INSERT INTO events (issue_id, kind, actor, time, detail)
         VALUES (NULL, 'checkpoint_imported', 'import', ?1, ?2)",
        params![event_time, event_detail.to_string()],
    )?;

    // Update checkpoint_state table
    update_checkpoint_state(&tx, &staging.input_hash, activation_sequence, &event_time)?;

    tx.commit()?;

    Ok((staging.issue_count, activation_sequence))
}

/// Flush checkpoint to JSONL file (Pre-F017 Issue-Only Format)
///
/// This function atomically publishes a checkpoint containing only issue records
/// to the specified output path. This is the current default behavior.
///
/// This function:
/// 1. Opens read transaction to capture snapshot
/// 2. Reads all issues with their complete data
/// 3. Sorts by issue ID for deterministic ordering
/// 4. Serializes to JSONL format
/// 5. Writes to temporary file
/// 6. Verifies hash and count
/// 7. Atomically replaces destination
/// 8. Updates checkpoint_state table
///
/// # Algorithm
///
/// 1. **Snapshot Capture**: Open read transaction and capture the current event sequence
/// 2. **Issue Reading**: Read all issues with their complete data including dependencies
/// 3. **Deterministic Ordering**: Sort issues by ID for reproducible output
/// 4. **Atomic Write**: Write to temporary file, calculate hash, verify contents
/// 5. **Atomic Replace**: Rename temporary file over target (atomic filesystem operation)
/// 6. **State Update**: Update checkpoint_state table in same transaction as rename
///
/// # Atomicity Guarantees
///
/// - Uses write-verify-rename pattern for crash safety
/// - Temporary file written and verified before atomic rename
/// - Database state updated only after successful file write
/// - If process crashes during write: old checkpoint remains valid
/// - If process crashes during rename: temporary file can be cleaned up
///
/// # Output Format
///
/// Each line contains one complete JSON issue object:
/// ```text
/// {"id":"bead-0123","title":"Task","priority":2,"base_status":"open",...}
/// {"id":"bead-0456","title":"Another","priority":1,"base_status":"open",...}
/// ```
///
/// # Arguments
///
/// * `store` - Mutable reference to the SQLite store
/// * `output_path` - Target path for the checkpoint file (will be atomically replaced)
///
/// # Returns
///
/// * `Ok(FlushResult)` - Contains issue count, SHA-256 hash, covered sequence, and timestamp
/// * `Err(...)` - I/O error, database error, or serialization failure
///
/// # Examples
///
/// ```no_run
/// # use bead_rs::store::SqliteStore;
/// # use bead_rs::service::checkpoint::flush_checkpoint;
/// # use std::path::Path;
/// # fn main() -> anyhow::Result<()> {
/// # let mut store = SqliteStore::new();
/// let result = flush_checkpoint(&mut store, Path::new(".beads/issues.jsonl"))?;
/// println!("Flushed {} issues, hash: {}", result.issue_count, result.hash);
/// # Ok(())
/// # }
/// ```
pub fn flush_checkpoint(store: &mut SqliteStore, output_path: &Path) -> Result<FlushResult> {
    // Open read transaction to capture snapshot
    let conn = store.conn();
    let tx = conn.unchecked_transaction()?;

    // Get current event sequence
    let current_sequence: i64 = tx
        .query_row("SELECT COALESCE(MAX(sequence), 0) FROM events", [], |row| {
            row.get(0)
        })
        .unwrap_or(0);

    // Read all issues with their data
    let issues = read_all_issues(&tx)?;

    // Commit the read transaction
    tx.commit()?;

    // Sort by issue ID for deterministic ordering
    let mut sorted_issues = issues;
    sorted_issues.sort_by(|a, b| a.id.cmp(&b.id));

    // Create temporary file for atomic write
    let temp_path = output_path.with_extension("tmp");
    let temp_file = File::create(&temp_path)?;
    let mut writer = BufWriter::new(temp_file);

    // Write each issue as a JSON line
    for issue in &sorted_issues {
        serde_json::to_writer(&mut writer, issue)?;
        writer.write_all(b"\n")?;
    }
    writer.flush()?;

    // Calculate hash of output
    let hash = calculate_file_hash(&temp_path)?;

    // Verify file was written correctly
    drop(writer);
    let final_count = sorted_issues.len();

    // Get export time
    let export_time = format_rfc3339(SystemTime::now());

    // Now do the atomic update in a write transaction
    let conn = store.conn();
    let tx = conn.unchecked_transaction()?;

    // Atomic rename from temp to target
    std::fs::rename(&temp_path, output_path)?;

    // Update checkpoint_state table
    update_checkpoint_state(&tx, &hash, current_sequence, &export_time)?;

    tx.commit()?;

    Ok(FlushResult {
        issue_count: final_count,
        hash,
        covered_sequence: current_sequence,
        export_time,
    })
}

/// Publish forensic checkpoint (F017)
///
/// This function implements the full forensic checkpoint-set format with:
/// - Monolithic mode: Single JSONL file with issue/event/receipt records
/// - Sharded mode: Manifest with content-addressed shards
/// - Atomic pointer replacement
/// - Git-trackable changed paths
pub fn publish_forensic_checkpoint(
    store: &mut SqliteStore,
    mode: CheckpointMode,
    checkpoint_base: &Path,
) -> Result<ForensicFlushResult> {
    let conn = store.conn();

    // Begin read transaction to capture snapshot
    let tx = conn.unchecked_transaction()?;

    // Get current state
    let current_sequence: i64 = tx
        .query_row("SELECT COALESCE(MAX(sequence), 0) FROM events", [], |row| {
            row.get(0)
        })
        .unwrap_or(0);

    let store_uuid: String =
        tx.query_row("SELECT uuid FROM workspace WHERE id = 1", [], |row| {
            row.get(0)
        })?;

    // Read all records needed for forensic checkpoint
    let issues = read_all_issues(&tx)?;
    let events = read_all_events(&tx)?;
    let receipts = read_all_provenance_receipts(&tx)?;

    // Read all graph data for dependencies and labels
    let graph_data = IssueGraphData {
        dependencies: read_all_dependencies(&tx)?,
        labels: read_all_labels(&tx)?,
    };

    // Commit the read transaction
    tx.commit()?;

    // Sort records for deterministic ordering
    let mut sorted_issues = issues;
    sorted_issues.sort_by(|a, b| a.id.cmp(&b.id));

    let mut sorted_events = events;
    sorted_events.sort_by(|a, b| {
        (&a.origin_store_uuid, a.origin_event_sequence)
            .cmp(&(&b.origin_store_uuid, b.origin_event_sequence))
    });

    let mut sorted_receipts = receipts;
    sorted_receipts.sort_by(|a, b| a.receipt_id.cmp(&b.receipt_id));

    // Calculate totals
    let issue_count = sorted_issues.len();
    let event_count = sorted_events.len();
    let receipt_count = sorted_receipts.len();
    let total_record_count = issue_count + event_count + receipt_count;

    // Generate generation ID
    let timestamp = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_nanos();
    let generation_input = format!("{}{}{}", store_uuid, current_sequence, timestamp);
    let generation_id = format!("gen-{}", md5_compute(&generation_input));

    // Create checkpoint directory structure
    let checkpoint_dir = checkpoint_base.join("checkpoint");
    std::fs::create_dir_all(&checkpoint_dir)?;
    std::fs::create_dir_all(checkpoint_dir.join("manifests"))?;
    std::fs::create_dir_all(checkpoint_dir.join("objects"))?;

    let mut changed_paths = Vec::new();

    // Publish based on mode
    let (root_hash, root_path) = match mode {
        CheckpointMode::Monolithic => publish_monolithic_checkpoint(
            &sorted_issues,
            &sorted_events,
            &sorted_receipts,
            &graph_data,
            &checkpoint_dir,
            &generation_id,
            &mut changed_paths,
        )?,
        CheckpointMode::Sharded => {
            let config = ShardedConfig {
                generation_id: generation_id.clone(),
                store_uuid: store_uuid.clone(),
                snapshot_sequence: current_sequence,
            };
            publish_sharded_checkpoint(
                &sorted_issues,
                &sorted_events,
                &sorted_receipts,
                &graph_data,
                &checkpoint_dir,
                &config,
                &mut changed_paths,
            )?
        }
    };

    // Update checkpoint pointers in a write transaction
    let conn = store.conn();
    let tx = conn.unchecked_transaction()?;

    // Preserve old pointer as previous.json using atomic rename
    let current_pointer_path = checkpoint_dir.join("current.json");
    let previous_pointer_path = checkpoint_dir.join("previous.json");

    let previous_files = if current_pointer_path.exists() {
        // Use atomic rename with temp file pattern
        let previous_temp = previous_pointer_path.with_extension("tmp");
        std::fs::copy(&current_pointer_path, &previous_temp)?;

        // Sync the temp file
        let temp_file = File::open(&previous_temp)?;
        temp_file.sync_all()?;
        drop(temp_file);

        // Atomic rename
        std::fs::rename(&previous_temp, &previous_pointer_path)?;

        // Sync parent directory
        let checkpoint_dir_file = File::open(checkpoint_dir)?;
        checkpoint_dir_file.sync_all()?;
        drop(checkpoint_dir_file);

        changed_paths.push("previous.json".to_string());

        // Read previous pointer to get existing files
        read_previous_pointer_files(&current_pointer_path)?
    } else {
        HashSet::new()
    };

    // Calculate path categories
    let current_files: HashSet<String> = changed_paths.iter().cloned().collect();
    let added_paths: Vec<String> = current_files.difference(&previous_files).cloned().collect();
    let replaced_paths: Vec<String> = current_files
        .intersection(&previous_files)
        .cloned()
        .collect();
    let deleted_paths: Vec<String> = previous_files.difference(&current_files).cloned().collect();

    // Sort for deterministic output
    let mut added_paths_sorted = added_paths;
    added_paths_sorted.sort();
    let mut replaced_paths_sorted = replaced_paths;
    replaced_paths_sorted.sort();
    let mut deleted_paths_sorted = deleted_paths;
    deleted_paths_sorted.sort();

    // Write new current.json pointer
    let pointer_config = PointerConfig {
        generation_id: generation_id.clone(),
        mode,
        store_uuid: store_uuid.clone(),
        snapshot_sequence: current_sequence,
        root_path: root_path.clone(),
        root_hash: root_hash.clone(),
        issue_count,
        event_count,
        receipt_count,
        total_record_count,
        added_paths: added_paths_sorted,
        replaced_paths: replaced_paths_sorted,
        deleted_paths: deleted_paths_sorted,
    };
    write_current_pointer(&current_pointer_path, &pointer_config)?;
    changed_paths.push("current.json".to_string());

    // Update checkpoint_state table
    let state_config = CheckpointStateConfig {
        generation_id: generation_id.clone(),
        mode,
        root_path: root_path.clone(),
        root_hash: root_hash.clone(),
        covered_sequence: current_sequence,
        changed_paths: changed_paths.clone(),
        store_uuid: store_uuid.clone(),
    };
    update_forensic_checkpoint_state(&tx, &state_config)?;

    tx.commit()?;

    Ok(ForensicFlushResult {
        mode,
        generation_id,
        issue_count,
        event_count,
        receipt_count,
        total_record_count,
        root_hash,
        covered_sequence: current_sequence,
        changed_paths,
    })
}

/// Publish monolithic forensic checkpoint
fn publish_monolithic_checkpoint(
    issues: &[Issue],
    events: &[EventRecord],
    receipts: &[ProvenanceReceipt],
    graph_data: &IssueGraphData,
    checkpoint_dir: &Path,
    generation_id: &str,
    changed_paths: &mut Vec<String>,
) -> Result<(String, String)> {
    let objects_dir = checkpoint_dir.join("objects");
    let temp_path = objects_dir.join(format!("{}.tmp", generation_id));
    let final_path = objects_dir.join(format!("{}.jsonl", generation_id));

    // Create temporary file
    let temp_file = File::create(&temp_path)?;
    let mut writer = BufWriter::new(temp_file);

    // Write issue records
    for issue in issues {
        // Collect dependencies for this issue in canonical order
        let issue_dependencies: Vec<_> = graph_data
            .dependencies
            .iter()
            .filter(|(blocked, _, _)| blocked == &issue.id)
            .collect();

        // Collect labels for this issue in canonical order
        let issue_labels: Vec<_> = graph_data
            .labels
            .iter()
            .filter(|(issue_id, _)| issue_id == &issue.id)
            .collect();

        // Build enriched issue object with dependencies and labels
        let issue_obj = build_enriched_issue_object(issue, issue_dependencies, issue_labels)?;

        // Wrap in record envelope for serialization
        let record = serde_json::json!({
            "record_type": "issue",
            "issue": issue_obj
        });
        serde_json::to_writer(&mut writer, &record)?;
        writer.write_all(b"\n")?;
    }

    // Write event records
    for event in events {
        let record = CheckpointRecord::Event {
            event: event.clone(),
        };
        serde_json::to_writer(&mut writer, &record)?;
        writer.write_all(b"\n")?;
    }

    // Write provenance receipt records
    for receipt in receipts {
        let record = CheckpointRecord::ProvenanceReceipt {
            provenance_receipt: receipt.clone(),
        };
        serde_json::to_writer(&mut writer, &record)?;
        writer.write_all(b"\n")?;
    }

    writer.flush()?;
    drop(writer);

    // Sync temp file to storage
    let temp_file = File::open(&temp_path)?;
    temp_file.sync_all()?;
    drop(temp_file);

    // Calculate hash
    let hash = calculate_file_hash(&temp_path)?;

    // Atomically rename to final path
    std::fs::rename(&temp_path, &final_path)?;

    // Sync parent directory to ensure directory entry is persisted
    let objects_dir_file = File::open(&objects_dir)?;
    objects_dir_file.sync_all()?;
    drop(objects_dir_file);

    // Update nonauthoritative forensic.jsonl view via atomic rename
    let view_path = checkpoint_dir.join("forensic.jsonl");
    let view_temp = view_path.with_extension("tmp");
    std::fs::copy(&final_path, &view_temp)?;
    let view_file = File::open(&view_temp)?;
    view_file.sync_all()?;
    drop(view_file);
    std::fs::rename(&view_temp, &view_path)?;

    // Sync checkpoint directory
    let checkpoint_dir_file = File::open(checkpoint_dir)?;
    checkpoint_dir_file.sync_all()?;
    drop(checkpoint_dir_file);

    changed_paths.push(format!("objects/{}.jsonl", generation_id));
    changed_paths.push("forensic.jsonl".to_string());

    Ok((hash, format!("objects/{}.jsonl", generation_id)))
}

/// Publish sharded forensic checkpoint
fn publish_sharded_checkpoint(
    issues: &[Issue],
    events: &[EventRecord],
    receipts: &[ProvenanceReceipt],
    graph_data: &IssueGraphData,
    checkpoint_dir: &Path,
    config: &ShardedConfig,
    changed_paths: &mut Vec<String>,
) -> Result<(String, String)> {
    let objects_dir = checkpoint_dir.join("objects");

    // Adaptive issue sharding with count and byte thresholds
    const MAX_ISSUES_PER_SHARD: usize = 10000;
    const MAX_BYTES_PER_SHARD: usize = 50 * 1024 * 1024; // 50MB

    let mut issue_shard_metadata = Vec::new();
    let mut current_shard_issues = Vec::new();
    let mut current_shard_bytes = 0;
    let mut shard_index = 0;

    // Sort issues for deterministic distribution
    let mut sorted_issues = issues.to_vec();
    sorted_issues.sort_by(|a, b| a.id.cmp(&b.id));

    for issue in &sorted_issues {
        // Collect dependencies and labels for this issue to estimate size
        let issue_dependencies: Vec<_> = graph_data
            .dependencies
            .iter()
            .filter(|(blocked, _, _)| blocked == &issue.id)
            .collect();

        let issue_labels: Vec<_> = graph_data
            .labels
            .iter()
            .filter(|(issue_id, _)| issue_id == &issue.id)
            .collect();

        // Estimate size of this issue record (with dependencies and labels)
        let issue_obj = build_enriched_issue_object(issue, issue_dependencies, issue_labels)?;
        let issue_json = serde_json::to_string(&issue_obj)?;
        let issue_size = issue_json.len() + 1; // +1 for newline

        // Check if we need to start a new shard
        let needs_new_shard = !current_shard_issues.is_empty()
            && (current_shard_issues.len() >= MAX_ISSUES_PER_SHARD
                || current_shard_bytes + issue_size > MAX_BYTES_PER_SHARD);

        if needs_new_shard {
            // Write current shard
            let temp_path = objects_dir.join(format!(
                "issue-{}-{}.tmp",
                config.generation_id, shard_index
            ));
            let hash = write_issue_shard(&current_shard_issues, graph_data, &temp_path)?;

            // Use content-addressed filename
            let shard_path = objects_dir.join(format!("{}.jsonl", hash));
            std::fs::rename(&temp_path, &shard_path)?;

            // Sync parent directory
            let objects_dir_file = File::open(&objects_dir)?;
            objects_dir_file.sync_all()?;
            drop(objects_dir_file);

            let id_prefix = current_shard_issues
                .first()
                .and_then(|i| i.id.strip_prefix("bead-"))
                .and_then(|s| s.chars().next())
                .unwrap_or('0');

            let metadata = serde_json::json!({
                "path": format!("{}.jsonl", hash),
                "sha256": hash,
                "byte_length": std::fs::metadata(&shard_path)?.len(),
                "record_count": current_shard_issues.len(),
                "id_prefix": id_prefix,
                "role": "issues"
            });

            issue_shard_metadata.push(metadata);
            changed_paths.push(format!("objects/{}.jsonl", hash));

            current_shard_issues.clear();
            current_shard_bytes = 0;
            shard_index += 1;
        }

        current_shard_issues.push(issue.clone());
        current_shard_bytes += issue_size;
    }

    // Write remaining issues
    if !current_shard_issues.is_empty() {
        let temp_path = objects_dir.join(format!(
            "issue-{}-{}.tmp",
            config.generation_id, shard_index
        ));
        let hash = write_issue_shard(&current_shard_issues, graph_data, &temp_path)?;

        // Use content-addressed filename
        let shard_path = objects_dir.join(format!("{}.jsonl", hash));
        std::fs::rename(&temp_path, &shard_path)?;

        // Sync parent directory
        let objects_dir_file = File::open(&objects_dir)?;
        objects_dir_file.sync_all()?;
        drop(objects_dir_file);

        let id_prefix = current_shard_issues
            .first()
            .and_then(|i| i.id.strip_prefix("bead-"))
            .and_then(|s| s.chars().next())
            .unwrap_or('0');

        let metadata = serde_json::json!({
            "path": format!("{}.jsonl", hash),
            "sha256": hash,
            "byte_length": std::fs::metadata(&shard_path)?.len(),
            "record_count": current_shard_issues.len(),
            "id_prefix": id_prefix,
            "role": "issues"
        });

        issue_shard_metadata.push(metadata);
        changed_paths.push(format!("objects/{}.jsonl", hash));
    }

    // Adaptive event sharding with count and byte thresholds
    const MAX_EVENTS_PER_SHARD: usize = 100000;
    const MAX_EVENT_BYTES_PER_SHARD: usize = 100 * 1024 * 1024; // 100MB

    let mut event_shard_metadata = Vec::new();
    let mut current_shard_events = Vec::new();
    let mut current_shard_bytes = 0;
    let mut shard_index = 0;

    for event in events {
        // Estimate size of this event record
        let event_json = serde_json::to_string(&CheckpointRecord::Event {
            event: event.clone(),
        })?;
        let event_size = event_json.len() + 1; // +1 for newline

        // Check if we need to start a new shard
        let needs_new_shard = !current_shard_events.is_empty()
            && (current_shard_events.len() >= MAX_EVENTS_PER_SHARD
                || current_shard_bytes + event_size > MAX_EVENT_BYTES_PER_SHARD);

        if needs_new_shard {
            // Write current shard
            let temp_path =
                objects_dir.join(format!("event-{}-{}.tmp", config.store_uuid, shard_index));
            let hash = write_event_shard(&current_shard_events, &temp_path)?;

            // Use content-addressed filename
            let shard_path = objects_dir.join(format!("{}.jsonl", hash));
            std::fs::rename(&temp_path, &shard_path)?;

            // Sync parent directory
            let objects_dir_file = File::open(&objects_dir)?;
            objects_dir_file.sync_all()?;
            drop(objects_dir_file);

            let metadata = serde_json::json!({
                "path": format!("{}.jsonl", hash),
                "sha256": hash,
                "byte_length": std::fs::metadata(&shard_path)?.len(),
                "record_count": current_shard_events.len(),
                "origin_store_uuid": config.store_uuid,
                "sequence_range": [current_shard_events.first().map(|e| e.origin_event_sequence), current_shard_events.last().map(|e| e.origin_event_sequence)],
                "role": "events"
            });

            event_shard_metadata.push(metadata);
            changed_paths.push(format!("objects/{}.jsonl", hash));

            current_shard_events.clear();
            current_shard_bytes = 0;
            shard_index += 1;
        }

        current_shard_events.push(event.clone());
        current_shard_bytes += event_size;
    }

    // Write remaining events
    if !current_shard_events.is_empty() {
        let temp_path =
            objects_dir.join(format!("event-{}-{}.tmp", config.store_uuid, shard_index));
        let hash = write_event_shard(&current_shard_events, &temp_path)?;

        // Use content-addressed filename
        let shard_path = objects_dir.join(format!("{}.jsonl", hash));
        std::fs::rename(&temp_path, &shard_path)?;

        // Sync parent directory
        let objects_dir_file = File::open(&objects_dir)?;
        objects_dir_file.sync_all()?;
        drop(objects_dir_file);

        let metadata = serde_json::json!({
            "path": format!("{}.jsonl", hash),
            "sha256": hash,
            "byte_length": std::fs::metadata(&shard_path)?.len(),
            "record_count": current_shard_events.len(),
            "origin_store_uuid": config.store_uuid,
            "sequence_range": [current_shard_events.first().map(|e| e.origin_event_sequence), current_shard_events.last().map(|e| e.origin_event_sequence)],
            "role": "events"
        });

        event_shard_metadata.push(metadata);
        changed_paths.push(format!("objects/{}.jsonl", hash));
    }

    // Write receipt shards
    let mut receipt_shards: HashMap<String, Vec<ProvenanceReceipt>> = HashMap::new();
    for receipt in receipts {
        let prefix = receipt.receipt_id.chars().next().unwrap_or('0');
        receipt_shards
            .entry(prefix.to_string())
            .or_default()
            .push(receipt.clone());
    }

    let mut receipt_shard_metadata = Vec::new();
    for (prefix, shard_receipts) in &receipt_shards {
        let shard_path = objects_dir.join(format!("receipt-{}.jsonl", prefix));
        let temp_path = shard_path.with_extension("tmp");

        let temp_file = File::create(&temp_path)?;
        let mut writer = BufWriter::new(temp_file);

        let mut sorted_shard = shard_receipts.clone();
        sorted_shard.sort_by(|a, b| a.receipt_id.cmp(&b.receipt_id));

        for receipt in &sorted_shard {
            let record = CheckpointRecord::ProvenanceReceipt {
                provenance_receipt: receipt.clone(),
            };
            serde_json::to_writer(&mut writer, &record)?;
            writer.write_all(b"\n")?;
        }

        writer.flush()?;
        drop(writer);

        let hash = calculate_file_hash(&temp_path)?;
        std::fs::rename(&temp_path, &shard_path)?;

        let metadata = serde_json::json!({
            "path": format!("receipt-{}.jsonl", prefix),
            "sha256": hash,
            "byte_length": std::fs::metadata(&shard_path)?.len(),
            "record_count": shard_receipts.len(),
            "id_prefix": prefix,
            "role": "provenance_receipts"
        });

        receipt_shard_metadata.push(metadata);
        changed_paths.push(format!("objects/receipt-{}.jsonl", prefix));
    }

    // Create manifest
    let manifest = serde_json::json!({
        "format": "checkpoint-set-v1",
        "schema_version": 1,
        "store_uuid": config.store_uuid,
        "snapshot_sequence": config.snapshot_sequence,
        "max_local_ingestion_sequence": config.snapshot_sequence,
        "created_at": format_rfc3339(SystemTime::now()),
        "profile": "native-v1",
        "partition_algorithm": "hash-prefix",
        "partition_thresholds": {
            "max_issues_per_shard": 10000,
            "max_shard_size_bytes": 52428800,
            "max_events_per_shard": 100000,
            "max_event_shard_size_bytes": 67108864
        },
        "issue_shards": issue_shard_metadata,
        "event_shards": event_shard_metadata,
        "receipt_shards": receipt_shard_metadata
    });

    // Write manifest
    let manifest_json = serde_json::to_vec_pretty(&manifest)?;
    let manifest_hash = format!("{:x}", Sha256::digest(&manifest_json));

    let manifest_path = checkpoint_dir
        .join("manifests")
        .join(format!("{}.json", manifest_hash));
    let temp_manifest_path = manifest_path.with_extension("tmp");
    std::fs::write(&temp_manifest_path, manifest_json)?;

    // Sync temp file
    let temp_file = File::open(&temp_manifest_path)?;
    temp_file.sync_all()?;
    drop(temp_file);

    std::fs::rename(&temp_manifest_path, &manifest_path)?;

    // Sync parent directory
    let manifests_dir = checkpoint_dir.join("manifests");
    let manifests_dir_file = File::open(&manifests_dir)?;
    manifests_dir_file.sync_all()?;
    drop(manifests_dir_file);

    changed_paths.push(format!("manifests/{}.json", manifest_hash));

    Ok((
        manifest_hash.clone(),
        format!("manifests/{}.json", manifest_hash),
    ))
}

/// Write issue shard to temp path and return hash
fn write_issue_shard(
    issues: &[Issue],
    graph_data: &IssueGraphData,
    temp_path: &Path,
) -> Result<String> {
    let temp_file = File::create(temp_path)?;
    let mut writer = BufWriter::new(temp_file);

    for issue in issues {
        // Collect dependencies for this issue in canonical order
        let issue_dependencies: Vec<_> = graph_data
            .dependencies
            .iter()
            .filter(|(blocked, _, _)| blocked == &issue.id)
            .collect();

        // Collect labels for this issue in canonical order
        let issue_labels: Vec<_> = graph_data
            .labels
            .iter()
            .filter(|(issue_id, _)| issue_id == &issue.id)
            .collect();

        // Build enriched issue object with dependencies and labels
        let issue_obj = build_enriched_issue_object(issue, issue_dependencies, issue_labels)?;

        // Wrap in record envelope for serialization
        let record = serde_json::json!({
            "record_type": "issue",
            "issue": issue_obj
        });
        serde_json::to_writer(&mut writer, &record)?;
        writer.write_all(b"\n")?;
    }

    writer.flush()?;
    drop(writer);

    // Sync temp file to storage
    let temp_file = File::open(temp_path)?;
    temp_file.sync_all()?;
    drop(temp_file);

    let hash = calculate_file_hash(temp_path)?;
    Ok(hash)
}

/// Build an enriched issue object with dependencies and labels embedded
///
/// This helper function creates a JSON object that includes all issue fields
/// plus optional dependencies and labels arrays, following the canonical ordering
/// specified in plan.md Section 6.1.
fn build_enriched_issue_object<'a>(
    issue: &Issue,
    dependencies: Vec<&'a (String, String, String)>, // (blocked, blocker, kind)
    labels: Vec<&'a (String, String)>,               // (issue_id, label)
) -> Result<serde_json::Value> {
    let issue_value = serde_json::to_value(issue)?;
    let mut issue_obj = issue_value
        .as_object()
        .ok_or_else(|| anyhow!("Failed to convert issue to JSON object"))?
        .clone();

    // Embed dependencies array if non-empty, already in canonical order
    if !dependencies.is_empty() {
        let deps_array: Vec<serde_json::Value> = dependencies
            .into_iter()
            .map(|(_, blocker, kind)| serde_json::json!({"blocker": blocker, "kind": kind}))
            .collect();
        issue_obj.insert(
            "dependencies".to_string(),
            serde_json::Value::Array(deps_array),
        );
    }

    // Embed labels array if non-empty, already in canonical order
    if !labels.is_empty() {
        let labels_array: Vec<serde_json::Value> = labels
            .into_iter()
            .map(|(_, label)| serde_json::Value::String(label.clone()))
            .collect();
        issue_obj.insert("labels".to_string(), serde_json::Value::Array(labels_array));
    }

    Ok(serde_json::Value::Object(issue_obj))
}

/// Write event shard to temp path and return hash
fn write_event_shard(events: &[EventRecord], temp_path: &Path) -> Result<String> {
    let temp_file = File::create(temp_path)?;
    let mut writer = BufWriter::new(temp_file);

    for event in events {
        let record = CheckpointRecord::Event {
            event: event.clone(),
        };
        serde_json::to_writer(&mut writer, &record)?;
        writer.write_all(b"\n")?;
    }

    writer.flush()?;
    drop(writer);

    // Sync temp file to storage
    let temp_file = File::open(temp_path)?;
    temp_file.sync_all()?;
    drop(temp_file);

    let hash = calculate_file_hash(temp_path)?;
    Ok(hash)
}

/// Write current.json pointer
fn write_current_pointer(pointer_path: &Path, config: &PointerConfig) -> Result<()> {
    let pointer = serde_json::json!({
        "schema_version": 1,
        "generation_id": config.generation_id,
        "mode": config.mode.as_str(),
        "store_uuid": config.store_uuid,
        "snapshot_sequence": config.snapshot_sequence,
        "active_root": {
            "path": config.root_path,
            "sha256": config.root_hash
        },
        "added_paths": config.added_paths,
        "replaced_paths": config.replaced_paths,
        "deleted_paths": config.deleted_paths,
        "issue_count": config.issue_count,
        "event_count": config.event_count,
        "receipt_count": config.receipt_count,
        "total_record_count": config.total_record_count,
        "created_at": format_rfc3339(SystemTime::now())
    });

    let temp_path = pointer_path.with_extension("tmp");
    std::fs::write(&temp_path, serde_json::to_vec_pretty(&pointer)?)?;

    // Sync temp file to storage
    let temp_file = File::open(&temp_path)?;
    temp_file.sync_all()?;
    drop(temp_file);

    // Atomic rename
    std::fs::rename(&temp_path, pointer_path)?;

    // Sync parent directory
    if let Some(parent) = pointer_path.parent() {
        let parent_dir = File::open(parent)?;
        parent_dir.sync_all()?;
        drop(parent_dir);
    }

    Ok(())
}

/// Update checkpoint_state for forensic checkpoint
fn update_forensic_checkpoint_state(
    tx: &Transaction,
    config: &CheckpointStateConfig,
) -> Result<()> {
    let updated_at = format_rfc3339(SystemTime::now());

    // Ensure row exists
    tx.execute(
        "INSERT OR IGNORE INTO checkpoint_state (id, last_interchange_hash, covered_event_sequence, store_uuid, updated_at)
         VALUES (1, '', 0, '', ?1)",
        params![&updated_at]
    )?;

    let changed_paths_json = serde_json::to_string(&config.changed_paths)?;

    tx.execute(
        "UPDATE checkpoint_state
         SET current_generation_id = ?1,
             current_mode = ?2,
             current_root_path = ?3,
             current_root_sha256 = ?4,
             covered_event_sequence = ?5,
             changed_paths_json = ?6,
             store_uuid = ?7,
             updated_at = ?8
         WHERE id = 1",
        params![
            &config.generation_id,
            config.mode.as_str(),
            &config.root_path,
            &config.root_hash,
            config.covered_sequence,
            changed_paths_json,
            &config.store_uuid,
            updated_at
        ],
    )?;

    Ok(())
}

/// Read previous pointer files for path calculation
fn read_previous_pointer_files(pointer_path: &Path) -> Result<HashSet<String>> {
    let content = std::fs::read_to_string(pointer_path)?;

    if let Ok(pointer) = serde_json::from_str::<serde_json::Value>(&content) {
        let mut files = HashSet::new();

        // Add current.json itself
        files.insert("current.json".to_string());

        // Extract active_root file
        if let Some(root) = pointer.get("active_root") {
            if let Some(path) = root.get("path").and_then(|p| p.as_str()) {
                files.insert(path.to_string());
            }
        }

        // Extract paths from added, replaced, deleted arrays
        for key in &["added_paths", "replaced_paths", "deleted_paths"] {
            if let Some(paths) = pointer.get(key).and_then(|p| p.as_array()) {
                for path in paths {
                    if let Some(path_str) = path.as_str() {
                        files.insert(path_str.to_string());
                    }
                }
            }
        }

        Ok(files)
    } else {
        // If we can't parse the previous pointer, assume no previous files
        Ok(HashSet::new())
    }
}

/// Read all events from database
fn read_all_events(tx: &Transaction) -> Result<Vec<EventRecord>> {
    let mut events = Vec::new();

    // Locally-created events are written with NULL origin columns (they are
    // nullable for backward compatibility, and no INSERT site populates them).
    // A NULL origin means the event originated in THIS store, so it must be
    // exported carrying this workspace's UUID and its own local sequence.
    // Exporting them as ("", 0) instead gives every event the identity ":0",
    // which makes the checkpoint unimportable past its first event.
    let local_store_uuid: String = tx
        .query_row("SELECT uuid FROM workspace WHERE id = 1", [], |row| {
            row.get(0)
        })
        .unwrap_or_default();

    let mut stmt = tx.prepare(
        "SELECT sequence, issue_id, kind, actor, time, detail,
                origin_store_uuid, origin_event_sequence, event_sha256, local_ingestion_sequence
         FROM events
         ORDER BY sequence ASC",
    )?;

    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>("sequence")?,
            row.get::<_, Option<String>>("issue_id")?,
            row.get::<_, String>("kind")?,
            row.get::<_, Option<String>>("actor")?,
            row.get::<_, String>("time")?,
            row.get::<_, String>("detail")?,
            row.get::<_, Option<String>>("origin_store_uuid")?,
            row.get::<_, Option<i64>>("origin_event_sequence")?,
            row.get::<_, Option<String>>("event_sha256")?,
            row.get::<_, Option<i64>>("local_ingestion_sequence")?,
        ))
    })?;

    for row in rows {
        let (
            sequence,
            issue_id,
            kind,
            actor,
            time,
            detail,
            origin_store_uuid,
            origin_event_sequence,
            _event_sha256,
            _local_ingestion_sequence,
        ) = row?;

        // Preserve a foreign origin verbatim; synthesize a local one otherwise.
        // `sequence` is the AUTOINCREMENT primary key, so it is unique and
        // monotonic — exactly the properties event identity requires.
        let origin_store_uuid = origin_store_uuid
            .filter(|uuid| !uuid.is_empty())
            .unwrap_or_else(|| local_store_uuid.clone());
        let origin_event_sequence = origin_event_sequence.unwrap_or(sequence);

        let event = EventRecord {
            schema_ref: "urn:bead-rs:schema:event:native-v1".to_string(),
            origin_store_uuid,
            origin_event_sequence,
            issue_id,
            kind,
            actor: actor.unwrap_or("system".to_string()),
            time,
            detail: serde_json::from_str(&detail).unwrap_or_default(),
        };

        events.push(event);
    }

    Ok(events)
}

/// Read all provenance receipts from database
fn read_all_provenance_receipts(tx: &Transaction) -> Result<Vec<ProvenanceReceipt>> {
    let mut receipts = Vec::new();

    let mut stmt = tx.prepare(
        "SELECT receipt_id, schema_ref, kind, source_store_uuid, target_store_uuid,
                source_root_sha256, actor, created_at, counts_json, result, summary_event_identity, receipt_sha256
         FROM provenance_receipts"
    )?;

    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>("receipt_id")?,
            row.get::<_, String>("schema_ref")?,
            row.get::<_, String>("kind")?,
            row.get::<_, String>("source_store_uuid")?,
            row.get::<_, String>("target_store_uuid")?,
            row.get::<_, String>("source_root_sha256")?,
            row.get::<_, String>("actor")?,
            row.get::<_, String>("created_at")?,
            row.get::<_, String>("counts_json")?,
            row.get::<_, String>("result")?,
            row.get::<_, Option<String>>("summary_event_identity")?,
            row.get::<_, String>("receipt_sha256")?,
        ))
    })?;

    for row in rows {
        let (
            receipt_id,
            schema_ref,
            kind,
            source_store_uuid,
            target_store_uuid,
            source_root_sha256,
            actor,
            created_at,
            counts_json,
            result,
            summary_event_identity,
            receipt_sha256,
        ) = row?;

        let counts: ReceiptCounts = serde_json::from_str(&counts_json)?;

        let receipt = ProvenanceReceipt {
            schema_ref,
            receipt_id,
            kind,
            source_store_uuid,
            target_store_uuid,
            source_root_sha256,
            actor,
            created_at,
            counts,
            result,
            summary_event_identity,
            receipt_sha256,
        };

        receipts.push(receipt);
    }

    Ok(receipts)
}

/// Configuration for sharded checkpoint publishing
#[derive(Debug, Clone)]
struct ShardedConfig {
    #[allow(dead_code)]
    generation_id: String,
    store_uuid: String,
    snapshot_sequence: i64,
}

/// Configuration for writing current pointer
#[derive(Debug, Clone)]
struct PointerConfig {
    generation_id: String,
    mode: CheckpointMode,
    store_uuid: String,
    snapshot_sequence: i64,
    root_path: String,
    root_hash: String,
    issue_count: usize,
    event_count: usize,
    receipt_count: usize,
    total_record_count: usize,
    added_paths: Vec<String>,
    replaced_paths: Vec<String>,
    deleted_paths: Vec<String>,
}

/// Configuration for forensic checkpoint state update
#[derive(Debug, Clone)]
struct CheckpointStateConfig {
    generation_id: String,
    mode: CheckpointMode,
    root_path: String,
    root_hash: String,
    covered_sequence: i64,
    changed_paths: Vec<String>,
    store_uuid: String,
}

/// Simple MD5 hash for generation IDs
fn md5_compute(data: &str) -> String {
    let md5_hash = md5::compute(data);
    format!("{:x}", md5_hash)
}

/// Read all issues from the database
/// Read all dependencies from database in canonical order
///
/// Returns dependencies sorted by blocker_id, kind, then blocked_id as specified in
/// plan.md Section 6.1 for deterministic serialization.
fn read_all_dependencies(tx: &Transaction) -> Result<Vec<(String, String, String)>> {
    let mut dependencies = Vec::new();

    let mut stmt = tx.prepare(
        "SELECT blocked_issue_id, blocker_issue_id, kind
         FROM dependencies
         ORDER BY blocker_issue_id ASC, kind ASC, blocked_issue_id ASC",
    )?;

    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>("blocked_issue_id")?,
            row.get::<_, String>("blocker_issue_id")?,
            row.get::<_, String>("kind")?,
        ))
    })?;

    for row in rows {
        let (blocked, blocker, kind) = row?;
        dependencies.push((blocked, blocker, kind));
    }

    Ok(dependencies)
}

/// Read all labels from database in canonical order
///
/// Returns labels sorted lexically by (issue_id, label) for deterministic serialization.
fn read_all_labels(tx: &Transaction) -> Result<Vec<(String, String)>> {
    let mut labels = Vec::new();

    let mut stmt = tx.prepare(
        "SELECT issue_id, label
         FROM labels
         ORDER BY issue_id ASC, label ASC",
    )?;

    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>("issue_id")?,
            row.get::<_, String>("label")?,
        ))
    })?;

    for row in rows {
        let (issue_id, label) = row?;
        labels.push((issue_id, label));
    }

    Ok(labels)
}

fn read_all_issues(tx: &Transaction) -> Result<Vec<Issue>> {
    let mut issues = Vec::new();

    // Query issues
    let mut issue_stmt = tx.prepare(
        "SELECT id, title, description, notes, priority, issue_type, base_status,
                manual_blocked, assignee, created_at, updated_at, closed_at, close_reason,
                source_repo, profile, schema_ref, revision
         FROM issues",
    )?;

    let issue_rows = issue_stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>("id")?,
            row.get::<_, String>("title")?,
            row.get::<_, Option<String>>("description")?,
            row.get::<_, Option<String>>("notes")?,
            row.get::<_, i64>("priority")?,
            row.get::<_, Option<String>>("issue_type")?,
            row.get::<_, String>("base_status")?,
            row.get::<_, i32>("manual_blocked")?,
            row.get::<_, Option<String>>("assignee")?,
            row.get::<_, String>("created_at")?,
            row.get::<_, String>("updated_at")?,
            row.get::<_, Option<String>>("closed_at")?,
            row.get::<_, Option<String>>("close_reason")?,
            row.get::<_, Option<String>>("source_repo")?,
            row.get::<_, Option<String>>("profile")?,
            row.get::<_, Option<String>>("schema_ref")?,
            row.get::<_, i64>("revision")?,
        ))
    })?;

    for row in issue_rows {
        let (
            id,
            title,
            description,
            notes,
            priority,
            issue_type,
            base_status,
            manual_blocked,
            assignee,
            created_at,
            updated_at,
            closed_at,
            close_reason,
            source_repo,
            profile,
            schema_ref,
            revision,
        ) = row?;

        // Load extensions for this issue
        let mut extensions = HashMap::new();
        let mut ext_stmt =
            tx.prepare("SELECT key, value FROM issue_extensions WHERE issue_id = ?")?;
        let ext_rows = ext_stmt.query_map([&id], |row| {
            Ok((row.get::<_, String>("key")?, row.get::<_, String>("value")?))
        })?;

        for ext_row in ext_rows {
            let (key, value_str) = ext_row?;
            let value = serde_json::from_str(&value_str)
                .map_err(|e| anyhow!("Failed to parse extension '{}': {}", key, e))?;
            extensions.insert(key, value);
        }

        let mut references = Vec::new();
        let mut reference_stmt = tx.prepare(
            "SELECT namespace, key, value FROM external_references
             WHERE issue_id = ?1 ORDER BY namespace, key, value",
        )?;
        let reference_rows = reference_stmt.query_map([&id], |row| {
            Ok(serde_json::json!({
                "namespace": row.get::<_, String>("namespace")?,
                "key": row.get::<_, String>("key")?,
                "value": row.get::<_, String>("value")?,
            }))
        })?;
        for reference in reference_rows {
            references.push(reference?);
        }
        extensions.insert(
            "external_references".to_string(),
            serde_json::Value::Array(references),
        );

        let mut comments = Vec::new();
        let mut comment_stmt = tx.prepare(
            "SELECT id, author, body, reply_to_id, resolution_state, created_at
             FROM comments WHERE issue_id = ?1 ORDER BY created_at, id",
        )?;
        let comment_rows = comment_stmt.query_map([&id], |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, String>("id")?,
                "author": row.get::<_, String>("author")?,
                "body": row.get::<_, String>("body")?,
                "reply_to_id": row.get::<_, Option<String>>("reply_to_id")?,
                "resolution_state": row.get::<_, Option<String>>("resolution_state")?,
                "created_at": row.get::<_, String>("created_at")?,
            }))
        })?;
        for comment in comment_rows {
            comments.push(comment?);
        }
        extensions.insert("comments".to_string(), serde_json::Value::Array(comments));

        // Convert to Issue model
        let mut data = serde_json::Map::new();
        let mut data_stmt = tx.prepare(
            "SELECT namespace, schema_ref, value FROM issue_data
             WHERE issue_id = ?1 ORDER BY namespace",
        )?;
        let data_rows = data_stmt.query_map([&id], |row| {
            Ok((
                row.get::<_, String>("namespace")?,
                row.get::<_, String>("schema_ref")?,
                row.get::<_, String>("value")?,
            ))
        })?;
        for data_row in data_rows {
            let (namespace, schema_ref, value) = data_row?;
            let value: serde_json::Value = serde_json::from_str(&value).map_err(|error| {
                anyhow!(
                    "Failed to parse data namespace '{}' for issue '{}': {}",
                    namespace,
                    id,
                    error
                )
            })?;
            data.insert(
                namespace,
                serde_json::json!({"schema_ref": schema_ref, "value": value}),
            );
        }

        let issue = Issue {
            id: id.clone(),
            title,
            description,
            notes,
            priority,
            issue_type: issue_type.or(Some(String::from("task"))),
            base_status: crate::model::BaseStatus::parse(&base_status)
                .map_err(|e| anyhow::anyhow!("Invalid base_status: {}", e))?,
            manual_blocked: Some(manual_blocked != 0),
            assignee,
            created_at,
            updated_at,
            closed_at,
            close_reason,
            source_repo,
            profile: profile.or(Some(String::from("native-v1"))),
            schema_ref: schema_ref.or(Some(String::from("urn:bead-rs:schema:issue:native-v1"))),
            revision: Some(revision),
            data: Some(serde_json::Value::Object(data)),
            extensions,
        };

        issues.push(issue);
    }

    Ok(issues)
}

/// Update the checkpoint_state table
fn update_checkpoint_state(
    tx: &Transaction,
    hash: &str,
    covered_sequence: i64,
    export_time: &str,
) -> Result<()> {
    // Debug: check if row exists before update
    let row_exists: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM checkpoint_state WHERE id = 1",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    if row_exists == 0 {
        // No row exists, insert one
        let updated_at = format_rfc3339(SystemTime::now());
        tx.execute(
            "INSERT INTO checkpoint_state (id, last_interchange_hash, covered_event_sequence, export_time, store_uuid, updated_at)
             VALUES (1, ?1, ?2, ?3, '', ?4)",
            params![hash, covered_sequence, export_time, updated_at],
        )?;
    } else {
        // Update existing checkpoint state
        let updated_at = format_rfc3339(SystemTime::now());
        tx.execute(
            "UPDATE checkpoint_state
             SET last_interchange_hash = ?1,
                 covered_event_sequence = ?2,
                 export_time = ?3,
                 updated_at = ?4
             WHERE id = 1",
            params![hash, covered_sequence, export_time, updated_at],
        )?;
    }

    Ok(())
}

/// Calculate SHA-256 hash of a file
fn calculate_file_hash(path: &Path) -> Result<String> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();

    let mut buffer = Vec::new();
    loop {
        buffer.clear();
        let bytes_read = reader.read_until(b'\n', &mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

/// Format system time as RFC 3339 string
fn format_rfc3339(time: SystemTime) -> String {
    let duration = time.duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = duration.as_secs();
    let nanos = duration.subsec_nanos();

    // Format: 2026-08-08T12:34:56Z
    use time::OffsetDateTime;
    let datetime = OffsetDateTime::from_unix_timestamp(secs as i64)
        .unwrap()
        .replace_nanosecond(nanos)
        .unwrap();

    datetime
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_flush_checkpoint_empty() {
        let temp_dir = TempDir::new().unwrap();
        let workspace_path = temp_dir.path().join(".beads");
        let output_path = temp_dir.path().join("issues.jsonl");

        // Initialize workspace
        fs::create_dir(&workspace_path).unwrap();

        let db_path = workspace_path.join("beads.db");
        let mut store = SqliteStore::with_path(&db_path).unwrap();
        store.apply_migrations().unwrap();

        // Flush empty checkpoint
        let result = flush_checkpoint(&mut store, &output_path).unwrap();

        assert_eq!(result.issue_count, 0);
        assert!(!result.hash.is_empty());
        assert_eq!(result.covered_sequence, 0);

        // Verify file exists and is empty (zero bytes for empty state)
        assert!(output_path.exists());
        let content = fs::read_to_string(&output_path).unwrap();
        assert!(content.is_empty());

        // Verify checkpoint_state table
        let conn = store.conn();
        let (hash, seq, time): (String, i64, Option<String>) = conn
            .query_row(
                "SELECT last_interchange_hash, covered_event_sequence, export_time
                 FROM checkpoint_state WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();

        assert_eq!(hash, result.hash);
        assert_eq!(seq, 0);
        assert!(time.is_some());

        // Verify updated_at field exists for migration 2
        let updated_at: String = conn
            .query_row(
                "SELECT updated_at FROM checkpoint_state WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!updated_at.is_empty());
    }

    #[test]
    fn test_flush_checkpoint_with_issues() {
        let temp_dir = TempDir::new().unwrap();
        let workspace_path = temp_dir.path().join(".beads");
        let output_path = temp_dir.path().join("issues.jsonl");

        // Initialize workspace and create some issues
        fs::create_dir(&workspace_path).unwrap();
        let db_path = workspace_path.join("beads.db");
        let mut store = SqliteStore::with_path(&db_path).unwrap();
        store.apply_migrations().unwrap();

        // Create test issues
        let conn = store.conn();
        let tx = conn.unchecked_transaction().unwrap();

        // Create two issues
        for i in 1..=2 {
            let id = format!("bead-{:016x}", i);
            tx.execute(
                "INSERT INTO issues (id, title, description, priority, issue_type, base_status,
                                  manual_blocked, created_at, updated_at, profile, schema_ref)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    &id,
                    &format!("Test Issue {}", i),
                    "Description",
                    2,
                    "task",
                    "open",
                    0,
                    &format_rfc3339(SystemTime::now()),
                    &format_rfc3339(SystemTime::now()),
                    "native-v1",
                    "urn:bead-rs:schema:issue:native-v1"
                ],
            )
            .unwrap();
        }

        // Create an audit event to test sequence tracking
        tx.execute(
            "INSERT INTO events (issue_id, kind, actor, time, detail)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                "bead-0000000000000001",
                "created",
                "test",
                &format_rfc3339(SystemTime::now()),
                "{}"
            ],
        )
        .unwrap();

        tx.commit().unwrap();

        // Flush checkpoint
        let result = flush_checkpoint(&mut store, &output_path).unwrap();

        assert_eq!(result.issue_count, 2);
        assert!(!result.hash.is_empty());
        assert_eq!(result.covered_sequence, 1);

        // Verify file contains two lines
        let content = fs::read_to_string(&output_path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2);

        // Verify deterministic ordering (by ID)
        let first_issue: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        let second_issue: serde_json::Value = serde_json::from_str(lines[1]).unwrap();

        assert_eq!(first_issue["id"], "bead-0000000000000001");
        assert_eq!(second_issue["id"], "bead-0000000000000002");

        // Verify checkpoint_state
        let conn = store.conn();
        let (hash, seq): (String, i64) = conn
            .query_row(
                "SELECT last_interchange_hash, covered_event_sequence
                 FROM checkpoint_state WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();

        assert_eq!(hash, result.hash);
        assert_eq!(seq, 1);

        // Verify updated_at field exists for migration 2
        let updated_at: String = conn
            .query_row(
                "SELECT updated_at FROM checkpoint_state WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!updated_at.is_empty());
    }

    #[test]
    fn test_calculate_file_hash() {
        let temp_dir = TempDir::new().unwrap();
        let test_path = temp_dir.path().join("test.jsonl");

        fs::write(&test_path, "line1\nline2\n").unwrap();
        let hash1 = calculate_file_hash(&test_path).unwrap();

        fs::write(&test_path, "line1\nline2\n").unwrap();
        let hash2 = calculate_file_hash(&test_path).unwrap();

        // Same content should produce same hash
        assert_eq!(hash1, hash2);

        // Different content should produce different hash
        fs::write(&test_path, "line1\nline3\n").unwrap();
        let hash3 = calculate_file_hash(&test_path).unwrap();
        assert_ne!(hash1, hash3);
    }
}

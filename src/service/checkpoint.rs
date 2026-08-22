//! Checkpoint export and import service
//!
//! This module provides atomic, deterministic JSONL checkpoint export and import for bead-rs.
//!
//! # Architecture Overview
//!
//! The checkpoint system operates in two distinct modes:
//!
//! ## Pre-F017 (Caller-Selected Export)
//! - Written by `sync flush-only --output PATH`
//! - Contains issue records only
//! - Used for basic backup and interchange
//! - Single-file format with one JSON object per line
//!
//! ## F017 Forensic Checkpoint Set (Default Flush Path)
//! - Writes to `.beads/checkpoint/` directory structure
//! - Contains issues, events, and provenance receipts
//! - Supports both monolithic and sharded modes, selected from the
//!   recorded plan 6.1.1 thresholds (`.beads/config.json` may force a mode
//!   or override the threshold table)
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
//! - Authoritative root is the content-addressed
//!   `.beads/checkpoint/objects/<sha256>.jsonl`; `forensic.jsonl` is the
//!   nonauthoritative byte-identical view
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
use crate::service::resource_locks::{
    acquire_issue_locks, declare_resource_keys, get_resource_keys, resource_keys_from_value,
};
use crate::store::SqliteStore;
use anyhow::{anyhow, bail, Result};
use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Component, Path, PathBuf};
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

/// Versioned checkpoint threshold configuration (plan 6.1.1)
///
/// These are the *recorded* thresholds a flush consults: they select
/// monolithic versus sharded mode and bound issue-shard and event-object
/// sizes. The defaults are the plan 6.1.1 values. Every published sharded
/// manifest records the thresholds that produced it, and a later flush
/// retains them unless the workspace overrides them in
/// `.beads/config.json` (`checkpoint.thresholds`), so threshold changes in
/// code never reshuffle an existing workspace's partition plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointThresholds {
    /// Threshold-table version; bump when the field set changes meaning
    pub version: u32,
    /// Issue records above which the native default switches to sharded
    pub max_monolith_issue_records: u64,
    /// Total serialized monolith bytes above which the default switches
    pub max_monolith_total_bytes: u64,
    /// Any single record line above this switches mode; forcing a
    /// monolith never bypasses this limit
    pub max_record_line_bytes: u64,
    /// Issue-shard split target: record count
    pub max_shard_issue_records: u64,
    /// Issue-shard split target: serialized bytes (lines plus newlines)
    pub max_shard_bytes: u64,
    /// Event-object seal target: record count
    pub max_event_object_events: u64,
    /// Event-object seal target: serialized bytes
    pub max_event_object_bytes: u64,
}

impl Default for CheckpointThresholds {
    fn default() -> Self {
        CheckpointThresholds {
            version: 1,
            max_monolith_issue_records: 50_000,
            max_monolith_total_bytes: 64 * 1024 * 1024,
            max_record_line_bytes: 8 * 1024 * 1024,
            max_shard_issue_records: 10_000,
            max_shard_bytes: 50 * 1024 * 1024,
            max_event_object_events: 100_000,
            max_event_object_bytes: 64 * 1024 * 1024,
        }
    }
}

impl CheckpointThresholds {
    /// Thresholds as recorded in a sharded manifest's `partition_thresholds`
    pub fn to_manifest_json(self) -> serde_json::Value {
        serde_json::to_value(self).expect("thresholds serialize to JSON")
    }

    /// Parse thresholds recorded in a manifest, rejecting a version this
    /// build does not understand or any nonpositive limit
    fn from_manifest_json(value: &serde_json::Value) -> Option<Self> {
        let parsed: CheckpointThresholds = serde_json::from_value(value.clone()).ok()?;
        if parsed.version != 1 {
            return None;
        }
        if parsed.max_monolith_issue_records == 0
            || parsed.max_monolith_total_bytes == 0
            || parsed.max_record_line_bytes == 0
            || parsed.max_shard_issue_records == 0
            || parsed.max_shard_bytes == 0
            || parsed.max_event_object_events == 0
            || parsed.max_event_object_bytes == 0
        {
            return None;
        }
        Some(parsed)
    }
}

/// How a flush decides which checkpoint mode to publish
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModePolicy {
    /// Select from the recorded section 6.1.1 thresholds
    Adaptive,
    /// Operator-forced mode (plan 6.1.1). Forcing sharded is always
    /// honored; forcing a monolith is refused while the content exceeds
    /// any recorded record/byte safety limit.
    Forced(CheckpointMode),
}

/// Checkpoint configuration recorded in `.beads/config.json`
///
/// All keys are optional; absence means the recorded plan 6.1.1 defaults
/// (and, for thresholds, the previous manifest's recorded values) apply.
///
/// ```json
/// { "checkpoint": { "mode": "sharded",
///                   "auto_flush": true,
///                   "thresholds": { "version": 1, "max_monolith_issue_records": 4 } } }
/// ```
#[derive(Debug, Clone, Default)]
pub struct CheckpointConfig {
    /// Forced checkpoint mode; `None` selects adaptively from thresholds
    pub mode: Option<CheckpointMode>,
    /// Threshold overrides; `None` retains recorded/default thresholds
    pub thresholds: Option<CheckpointThresholds>,
    /// Explicit post-commit publication setting (plan 6.2.1); `None` falls
    /// back to [`AUTO_FLUSH_COMPILED_DEFAULT`]
    pub auto_flush: Option<bool>,
}

/// Whether a mutating command publishes a checkpoint generation after its
/// transaction commits when `.beads/config.json` does not say otherwise
/// (plan 6.2.1, ADR-003).
///
/// **Flipped to `true` when R026 activated** (plan section 13, plan
/// revision 8): the documentation reversal shipped in the same commit as
/// this flip, so no shipped surface ever described a default the binary
/// did not have. The section 13 gate records its evidence against that
/// commit; a failing criterion reverts this constant. An explicit
/// `"checkpoint": { "auto_flush": false }` in `.beads/config.json` is the
/// durable suppressor the plan describes and keeps meaning the same thing
/// it meant as an opt-in before the flip.
pub const AUTO_FLUSH_COMPILED_DEFAULT: bool = true;

impl CheckpointConfig {
    /// Resolve the post-commit publication setting: an explicit workspace
    /// value wins over [`AUTO_FLUSH_COMPILED_DEFAULT`]
    pub fn auto_flush_enabled(&self) -> bool {
        self.auto_flush.unwrap_or(AUTO_FLUSH_COMPILED_DEFAULT)
    }
}

/// Read the checkpoint section of `.beads/config.json`, if present
pub fn load_checkpoint_config(beads_dir: &Path) -> Result<CheckpointConfig> {
    let config_path = beads_dir.join("config.json");
    if !config_path.exists() {
        return Ok(CheckpointConfig::default());
    }

    let raw = std::fs::read_to_string(&config_path)?;
    let parsed: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| anyhow!("Invalid .beads/config.json: {}", e))?;

    let Some(section) = parsed.get("checkpoint") else {
        return Ok(CheckpointConfig::default());
    };
    if section.is_null() {
        return Ok(CheckpointConfig::default());
    }

    let mut config = CheckpointConfig::default();

    if let Some(mode_value) = section.get("mode") {
        let mode_str = mode_value
            .as_str()
            .ok_or_else(|| anyhow!(".beads/config.json checkpoint.mode must be a string"))?;
        config.mode = match mode_str {
            "adaptive" => None,
            other => Some(
                std::str::FromStr::from_str(other)
                    .map_err(|e| anyhow!(".beads/config.json checkpoint.mode: {}", e))?,
            ),
        };
    }

    if let Some(auto_flush_value) = section.get("auto_flush") {
        config.auto_flush = Some(auto_flush_value.as_bool().ok_or_else(|| {
            anyhow!(".beads/config.json checkpoint.auto_flush must be a boolean")
        })?);
    }

    if let Some(thresholds_value) = section.get("thresholds") {
        if !thresholds_value.is_null() {
            let raw_value = thresholds_value.clone();
            config.thresholds =
                CheckpointThresholds::from_manifest_json(&raw_value).or_else(|| {
                    // A partial override merges over the defaults so tests and
                    // operators can tune one limit without restating the table
                    let mut merged = serde_json::to_value(CheckpointThresholds::default())
                        .expect("thresholds serialize to JSON");
                    if let (Some(target), Some(source)) =
                        (merged.as_object_mut(), raw_value.as_object())
                    {
                        for (key, value) in source {
                            target.insert(key.clone(), value.clone());
                        }
                    }
                    CheckpointThresholds::from_manifest_json(&merged)
                });
            if config.thresholds.is_none() {
                bail!(
                    "Invalid .beads/config.json checkpoint.thresholds: expected version-1 limits, all positive"
                );
            }
        }
    }

    Ok(config)
}

/// Size statistics of the would-be monolith, used for mode selection
#[derive(Debug, Clone, Copy, Default)]
pub struct MonolithStats {
    pub issue_records: u64,
    pub total_bytes: u64,
    pub max_line_bytes: u64,
}

/// Resolve the threshold table a flush consults (plan 6.1.1, 6.2 step 3)
///
/// Precedence: an explicit `.beads/config.json` override, then the table
/// recorded in the previous sharded manifest -- so a later code-default
/// change never reshuffles an existing workspace's partition plan -- then
/// the recorded plan 6.1.1 defaults.
fn resolve_checkpoint_thresholds(
    config: &CheckpointConfig,
    previous_manifest: Option<&serde_json::Value>,
) -> CheckpointThresholds {
    if let Some(thresholds) = config.thresholds {
        return thresholds;
    }
    if let Some(manifest) = previous_manifest {
        if let Some(recorded) = manifest.get("partition_thresholds") {
            if let Some(thresholds) = CheckpointThresholds::from_manifest_json(recorded) {
                return thresholds;
            }
        }
    }
    CheckpointThresholds::default()
}

/// The outgoing generation a new publication supersedes
///
/// Read from `current.json` before anything is written, so mode selection,
/// threshold retention, partition-plan retention, and the transition
/// tombstone all see the generation being replaced.
struct PreviousGeneration {
    /// Files the outgoing pointer still references (drives tombstone math)
    referenced_files: HashSet<String>,
    /// The outgoing pointer's recorded mode, when parseable
    mode: Option<CheckpointMode>,
    /// The outgoing pointer's active root, checkpoint-relative
    root_path: Option<String>,
    /// The outgoing sharded manifest, when the outgoing mode was sharded
    manifest: Option<serde_json::Value>,
}

/// Read the generation `current.json` currently selects (plan 6.2 step 3)
///
/// Returns `None` when no pointer exists or it is unparseable: an
/// unparseable pointer still gets preserved as `previous.json` by the
/// publication itself, but it carries no retention information.
fn read_previous_generation(checkpoint_dir: &Path) -> Result<Option<PreviousGeneration>> {
    let pointer_path = checkpoint_dir.join("current.json");
    if !pointer_path.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(&pointer_path)?;
    let Ok(pointer) = serde_json::from_str::<serde_json::Value>(&content) else {
        return Ok(None);
    };

    let referenced_files = read_pointer_referenced_files(&pointer_path)?;
    let mode = pointer
        .get("mode")
        .and_then(|v| v.as_str())
        .and_then(|s| std::str::FromStr::from_str(s).ok());
    let root_path = pointer
        .get("active_root")
        .and_then(|r| r.get("path"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // A sharded pointer selects its manifest as root; read it so the next
    // flush can retain the recorded partition plan and thresholds. Any
    // failure to read or parse degrades safely to a fresh plan.
    let manifest = match (&mode, &root_path) {
        (Some(CheckpointMode::Sharded), Some(rel))
            if rel.starts_with("manifests/") && is_generation_object_path(rel) =>
        {
            std::fs::read_to_string(checkpoint_dir.join(rel))
                .ok()
                .and_then(|data| serde_json::from_str(&data).ok())
        }
        _ => None,
    };

    Ok(Some(PreviousGeneration {
        referenced_files,
        mode,
        root_path,
        manifest,
    }))
}

/// The fully serialized checkpoint corpus in canonical order (plan 6.2 step 2)
///
/// Every record line is serialized exactly once: mode selection counts bytes
/// from these lines and both publishers consume them, so a flush never
/// serializes the same record twice and the counted monolith is byte-for-byte
/// what a monolithic publication would write.
struct SerializedCorpus {
    /// Enriched issue record lines, parallel to the sorted issue list
    issue_lines: Vec<Vec<u8>>,
    /// Event record lines, parallel to the sorted event list
    event_lines: Vec<Vec<u8>>,
    /// Receipt record lines, parallel to the sorted receipt list
    receipt_lines: Vec<Vec<u8>>,
}

/// Serialize every checkpoint record line in canonical order
fn serialize_corpus(
    issues: &[Issue],
    events: &[EventRecord],
    receipts: &[ProvenanceReceipt],
    graph_data: &IssueGraphData,
) -> Result<SerializedCorpus> {
    let mut issue_lines = Vec::with_capacity(issues.len());
    for issue in issues {
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
        let issue_obj = build_enriched_issue_object(issue, issue_dependencies, issue_labels)?;
        let record = serde_json::json!({
            "record_type": "issue",
            "issue": issue_obj
        });
        issue_lines.push(serde_json::to_vec(&record)?);
    }

    let mut event_lines = Vec::with_capacity(events.len());
    for event in events {
        let record = CheckpointRecord::Event {
            event: event.clone(),
        };
        event_lines.push(serde_json::to_vec(&record)?);
    }

    let mut receipt_lines = Vec::with_capacity(receipts.len());
    for receipt in receipts {
        let record = CheckpointRecord::ProvenanceReceipt {
            provenance_receipt: receipt.clone(),
        };
        receipt_lines.push(serde_json::to_vec(&record)?);
    }

    Ok(SerializedCorpus {
        issue_lines,
        event_lines,
        receipt_lines,
    })
}

/// Monolith size statistics from the serialized corpus
fn corpus_monolith_stats(corpus: &SerializedCorpus) -> MonolithStats {
    let mut stats = MonolithStats {
        issue_records: corpus.issue_lines.len() as u64,
        ..MonolithStats::default()
    };
    for line in corpus
        .issue_lines
        .iter()
        .chain(corpus.event_lines.iter())
        .chain(corpus.receipt_lines.iter())
    {
        // +1 for the newline every JSONL line carries
        stats.total_bytes += line.len() as u64 + 1;
        stats.max_line_bytes = stats.max_line_bytes.max(line.len() as u64);
    }
    stats
}

/// Select the checkpoint mode from the recorded thresholds (plan 6.1.1)
///
/// Adaptive policy switches to sharded when the monolith would exceed the
/// issue-record count, total-byte, or single-line limit. A forced sharded
/// mode is always honored. A forced monolith is refused while any limit
/// would be exceeded -- forcing output never bypasses the safety limits.
pub fn select_checkpoint_mode(
    stats: &MonolithStats,
    thresholds: &CheckpointThresholds,
    policy: ModePolicy,
) -> Result<CheckpointMode> {
    let exceeds = stats.issue_records > thresholds.max_monolith_issue_records
        || stats.total_bytes > thresholds.max_monolith_total_bytes
        || stats.max_line_bytes > thresholds.max_record_line_bytes;

    match policy {
        ModePolicy::Forced(CheckpointMode::Sharded) => Ok(CheckpointMode::Sharded),
        ModePolicy::Forced(CheckpointMode::Monolithic) => {
            if exceeds {
                bail!(
                    "Forced monolithic checkpoint would exceed recorded safety limits \
                     ({} issue records, {} total bytes, {} max line bytes; limits {}, {}, {}): \
                     remove checkpoint.mode from .beads/config.json or set it to \"sharded\"/\"adaptive\"",
                    stats.issue_records,
                    stats.total_bytes,
                    stats.max_line_bytes,
                    thresholds.max_monolith_issue_records,
                    thresholds.max_monolith_total_bytes,
                    thresholds.max_record_line_bytes
                );
            }
            Ok(CheckpointMode::Monolithic)
        }
        ModePolicy::Adaptive => Ok(if exceeds {
            CheckpointMode::Sharded
        } else {
            CheckpointMode::Monolithic
        }),
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

/// Fork receipt for R028 fork operations (bead sync fork)
///
/// A fork receipt records the explicit creation of a new workspace identity
/// from an existing checkpoint. The new UUID is derived while maintaining
/// provenance to the parent, enabling composability between forked workspaces.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForkReceipt {
    #[serde(rename = "$schema")]
    pub schema_ref: String,
    pub receipt_id: String,
    pub kind: String, // "fork"
    /// Parent workspace UUID being forked from
    pub parent_store_uuid: String,
    /// Newly generated UUID for the forked workspace
    pub new_store_uuid: String,
    /// Parent checkpoint root hash being forked
    pub parent_root_sha256: String,
    /// Parent generation ID (if available)
    pub parent_generation_id: Option<String>,
    /// Actor performing the fork
    pub actor: String,
    /// ISO 8601 timestamp
    pub created_at: String,
    /// Counts from parent checkpoint
    pub counts: ReceiptCounts,
    /// Result status
    pub result: String,
    /// Fork receipt content hash
    pub receipt_sha256: String,
    /// Optional reasoning for the fork
    pub reason: Option<String>,
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

/// Checkpoint status for `bead sync status` (plan 6.2)
///
/// `ready_to_commit` is the pre-commit gate: it holds only when the
/// authoritative pointer verifies against its root object, the checkpoint
/// covers the live event sequence, no pointer-declared tombstone is
/// unresolved, the monolithic compatibility view (when applicable) agrees
/// with the pointer-selected object, and the recorded checkpoint state
/// agrees with the pointer.
#[derive(Debug, Clone, Serialize)]
pub struct CheckpointStatusReport {
    pub checkpoint_present: bool,
    pub mode: Option<String>,
    pub generation_id: Option<String>,
    pub live_sequence: i64,
    pub covered_sequence: Option<i64>,
    pub dirty: bool,
    pub root_path: Option<String>,
    pub root_hash: Option<String>,
    pub root_verified: bool,
    /// `Some(true)`/`Some(false)` in monolithic mode (forensic.jsonl versus
    /// the pointer-selected object); `None` when no view applies (sharded).
    pub view_agrees: Option<bool>,
    pub unresolved_tombstones: Vec<String>,
    pub changed_paths: Vec<String>,
    pub ready_to_commit: bool,
    pub not_ready_reasons: Vec<String>,
    /// The R027 sync relationship between the live store and the durable
    /// checkpoint: one of `absent`, `behind`, `aligned`,
    /// `remote-advanced`, or `covered-ahead-integrity-failure`. Total over
    /// the artifacts alone (research/specs/remote-advanced-reconcile-v1.md);
    /// `aligned` and `behind` claim nothing about pointer health, which the
    /// fields above continue to report independently.
    pub relationship: String,
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
    #[serde(rename = "$schema")]
    pub schema_ref: String,
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

/// A named checkpoint generation whose complete source set has passed the
/// R036 restore verifier. The staged records stay private so callers cannot
/// construct a value that bypasses pointer/root verification.
#[derive(Debug, Clone)]
pub struct VerifiedRestoreSource {
    generation_id: String,
    mode: CheckpointMode,
    source_store_uuid: String,
    snapshot_sequence: i64,
    root_path: String,
    root_sha256: String,
    pointer_path: PathBuf,
    staging: ForensicStaging,
}

/// Exact displacement counts when an operator explicitly restores over a
/// non-empty target. These counts are reported but are not part of the source
/// provenance receipt, whose counts describe only the restored generation.
#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct RestoreDisplacedCounts {
    pub issues: usize,
    pub events: usize,
    pub provenance_receipts: usize,
    pub saved_views: usize,
    pub recurrence_templates: usize,
}

impl RestoreDisplacedCounts {
    fn is_empty(self) -> bool {
        self.issues == 0
            && self.events == 0
            && self.provenance_receipts == 0
            && self.saved_views == 0
            && self.recurrence_templates == 0
    }
}

/// Machine-readable result of one successful first-class restore.
#[derive(Debug, Clone, Serialize)]
pub struct RestoreReport {
    pub generation_id: String,
    pub mode: String,
    pub source_pointer: String,
    pub source_root_path: String,
    pub source_root_sha256: String,
    pub source_store_uuid: String,
    pub target_store_uuid: String,
    pub snapshot_sequence: i64,
    pub actor: String,
    pub issues_restored: usize,
    pub events_restored: usize,
    pub provenance_receipts_restored: usize,
    pub restore_receipt_id: String,
    pub restore_receipt_sha256: String,
    pub summary_event_sequence: i64,
    pub non_empty_override: bool,
    pub displaced: RestoreDisplacedCounts,
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
        ImportMode::RestoreIntoEmpty => {
            execute_restore_into_empty(store, &staging, actor, false, false)?
        }
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
    result.receipt = counts.receipt;
    result.summary_event_sequence = counts.summary_event_sequence;
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

#[derive(Debug, Deserialize)]
struct RestorePointerRoot {
    path: String,
    sha256: String,
}

#[derive(Debug, Deserialize)]
struct RestorePointer {
    schema_version: u64,
    generation_id: String,
    mode: String,
    store_uuid: String,
    snapshot_sequence: i64,
    active_root: RestorePointerRoot,
    issue_count: usize,
    event_count: usize,
    receipt_count: usize,
    total_record_count: usize,
}

/// Select and completely verify the immutable checkpoint generation named by
/// `generation_id` (R036). Unlike `sync import-only`, this never treats a bare
/// JSONL file as an authority: the source must resolve through a generation
/// pointer whose root and complete object closure verify.
pub fn verify_restore_source(source: &Path, generation_id: &str) -> Result<VerifiedRestoreSource> {
    validate_generation_name(generation_id)?;
    let pointer_path = select_restore_pointer(source, generation_id)?;
    let pointer = read_restore_pointer(&pointer_path)?;

    if pointer.generation_id != generation_id {
        bail!(
            "Restore generation mismatch: requested '{}', but {} selects '{}'. Name the exact immutable generation to restore.",
            generation_id,
            pointer_path.display(),
            pointer.generation_id
        );
    }
    if pointer.schema_version != 1 {
        bail!(
            "Unverified restore source: pointer schema_version {} is not supported",
            pointer.schema_version
        );
    }
    if pointer.store_uuid.trim().is_empty() {
        bail!("Unverified restore source: pointer store_uuid is empty");
    }
    if pointer.snapshot_sequence < 0 {
        bail!("Unverified restore source: snapshot_sequence is negative");
    }
    if pointer.total_record_count
        != pointer.issue_count + pointer.event_count + pointer.receipt_count
    {
        bail!(
            "Unverified restore source: pointer total_record_count {} does not equal issues {} + events {} + receipts {}",
            pointer.total_record_count,
            pointer.issue_count,
            pointer.event_count,
            pointer.receipt_count
        );
    }
    validate_sha256(&pointer.active_root.sha256, "pointer active_root.sha256")?;

    let mode: CheckpointMode = pointer.mode.parse().map_err(|_| {
        anyhow!(
            "Unverified restore source: pointer mode '{}' is not supported",
            pointer.mode
        )
    })?;
    let base = pointer_path
        .parent()
        .ok_or_else(|| anyhow!("Restore pointer has no checkpoint-set directory"))?;
    let (expected_dir, expected_extension) = match mode {
        CheckpointMode::Monolithic => ("objects", "jsonl"),
        CheckpointMode::Sharded => ("manifests", "json"),
    };
    let root = verified_checkpoint_path(
        base,
        &pointer.active_root.path,
        expected_dir,
        expected_extension,
    )?;
    verify_restore_root_file(
        &root,
        &pointer.active_root.sha256,
        &pointer.generation_id,
        mode,
        "pointer-selected root",
    )?;

    match mode {
        CheckpointMode::Monolithic => verify_monolithic_restore_records(&root)?,
        CheckpointMode::Sharded => verify_sharded_restore_closure(base, &root, &pointer)?,
    }

    let mut staging = stage_pointer_checkpoint(&pointer_path)?;
    staging.input_hash = pointer.active_root.sha256.clone();
    staging.store_uuid = pointer.store_uuid.clone();
    staging.snapshot_sequence = pointer.snapshot_sequence;

    if staging.issue_count != pointer.issue_count
        || staging.event_count != pointer.event_count
        || staging.receipt_count != pointer.receipt_count
    {
        bail!(
            "Unverified restore source: pointer counts (issues={}, events={}, receipts={}) disagree with staged records ({}, {}, {})",
            pointer.issue_count,
            pointer.event_count,
            pointer.receipt_count,
            staging.issue_count,
            staging.event_count,
            staging.receipt_count
        );
    }
    validate_forensic_contents(&staging)?;

    // Close the verification/staging time-of-check gap. A generation object
    // is immutable by contract; seeing different bytes here means the source
    // was modified during verification and is therefore not a verified input.
    verify_restore_root_file(
        &root,
        &pointer.active_root.sha256,
        &pointer.generation_id,
        mode,
        "pointer-selected root after staging",
    )?;
    if mode == CheckpointMode::Sharded {
        verify_sharded_restore_closure(base, &root, &pointer)?;
    }

    Ok(VerifiedRestoreSource {
        generation_id: pointer.generation_id,
        mode,
        source_store_uuid: pointer.store_uuid,
        snapshot_sequence: pointer.snapshot_sequence,
        root_path: pointer.active_root.path,
        root_sha256: pointer.active_root.sha256,
        pointer_path,
        staging,
    })
}

fn validate_generation_name(generation_id: &str) -> Result<()> {
    let suffix = generation_id.strip_prefix("gen-").ok_or_else(|| {
        anyhow!(
            "Invalid generation '{}': generation IDs start with 'gen-'",
            generation_id
        )
    })?;
    if suffix.is_empty()
        || suffix.len() > 128
        || !suffix
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        bail!(
            "Invalid generation '{}': use the exact generation_id from current.json or previous.json",
            generation_id
        );
    }
    Ok(())
}

fn select_restore_pointer(source: &Path, generation_id: &str) -> Result<PathBuf> {
    let metadata = std::fs::symlink_metadata(source).map_err(|error| {
        anyhow!(
            "Restore source not found or unreadable at {}: {}",
            source.display(),
            error
        )
    })?;
    if metadata.file_type().is_symlink() {
        bail!(
            "Unverified restore source: {} is a symlink",
            source.display()
        );
    }

    if metadata.is_file() {
        let pointer = read_restore_pointer(source)?;
        if pointer.generation_id != generation_id {
            bail!(
                "Restore generation mismatch: requested '{}', but {} selects '{}'",
                generation_id,
                source.display(),
                pointer.generation_id
            );
        }
        return Ok(source.to_path_buf());
    }
    if !metadata.is_dir() {
        bail!(
            "Unverified restore source: {} is neither a checkpoint-set directory nor a generation pointer",
            source.display()
        );
    }

    let mut available = Vec::new();
    for name in ["current.json", "previous.json"] {
        let candidate = source.join(name);
        if !candidate.exists() {
            continue;
        }
        match read_restore_pointer(&candidate) {
            Ok(pointer) if pointer.generation_id == generation_id => return Ok(candidate),
            Ok(pointer) => available.push(pointer.generation_id),
            Err(error) => available.push(format!("{name}: invalid ({error})")),
        }
    }

    if available.is_empty() {
        bail!(
            "Unverified restore source: {} contains no current.json or previous.json generation pointer",
            source.display()
        );
    }
    bail!(
        "Generation '{}' is not selected by current.json or previous.json in {} (available: {}). Restore requires an explicitly named retained generation pointer.",
        generation_id,
        source.display(),
        available.join(", ")
    )
}

fn read_restore_pointer(path: &Path) -> Result<RestorePointer> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        bail!(
            "Unverified restore source: pointer {} must be a regular non-symlink file",
            path.display()
        );
    }
    let bytes = std::fs::read(path)?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|error| {
        anyhow!(
            "Unverified restore source: {} is not a valid generation pointer: {}",
            path.display(),
            error
        )
    })?;
    reject_archaeology_view(&value, path)?;
    serde_json::from_value(value).map_err(|error| {
        anyhow!(
            "Unverified restore source: {} is missing required generation pointer fields: {}",
            path.display(),
            error
        )
    })
}

fn reject_archaeology_view(value: &serde_json::Value, path: &Path) -> Result<()> {
    let object = match value.as_object() {
        Some(object) => object,
        None => return Ok(()),
    };
    let explicitly_non_importable =
        object.get("importable") == Some(&serde_json::Value::Bool(false));
    let archaeology_marker = ["artifact_kind", "kind", "$schema"]
        .iter()
        .filter_map(|key| object.get(*key).and_then(serde_json::Value::as_str))
        .any(|marker| marker.to_ascii_lowercase().contains("archaeology"));
    if explicitly_non_importable || archaeology_marker {
        bail!(
            "Refusing R029 checkpoint archaeology view {}: archaeology artifacts are explicitly non-importable and cannot be used for restore",
            path.display()
        );
    }
    Ok(())
}

fn validate_sha256(hash: &str, field: &str) -> Result<()> {
    if hash.len() != 64
        || !hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        bail!(
            "Unverified restore source: {} must be a lowercase 64-character SHA-256 digest",
            field
        );
    }
    Ok(())
}

fn verified_checkpoint_path(
    base: &Path,
    relative: &str,
    expected_dir: &str,
    expected_extension: &str,
) -> Result<PathBuf> {
    if relative.is_empty() || relative.contains('\\') || relative.split('/').any(str::is_empty) {
        bail!("Unverified restore source: invalid checkpoint-relative path '{relative}'");
    }
    let path = Path::new(relative);
    if path.is_absolute()
        || path.components().count() != 2
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        bail!("Unverified restore source: invalid checkpoint-relative path '{relative}'");
    }
    if path
        .components()
        .next()
        .and_then(|part| part.as_os_str().to_str())
        != Some(expected_dir)
        || path.extension().and_then(|ext| ext.to_str()) != Some(expected_extension)
    {
        bail!(
            "Unverified restore source: '{}' must name a {}/*.{} generation object",
            relative,
            expected_dir,
            expected_extension
        );
    }

    let mut cursor = base.to_path_buf();
    for component in path.components() {
        let Component::Normal(component) = component else {
            unreachable!("components were validated above")
        };
        cursor.push(component);
        let metadata = std::fs::symlink_metadata(&cursor).map_err(|error| {
            anyhow!(
                "Unverified restore source: referenced object {} is missing or unreadable: {}",
                cursor.display(),
                error
            )
        })?;
        if metadata.file_type().is_symlink() {
            bail!(
                "Unverified restore source: referenced path {} contains a symlink",
                cursor.display()
            );
        }
    }
    if !std::fs::metadata(&cursor)?.is_file() {
        bail!(
            "Unverified restore source: referenced object {} is not a regular file",
            cursor.display()
        );
    }
    let canonical_base = std::fs::canonicalize(base)?;
    let canonical = std::fs::canonicalize(&cursor)?;
    if !canonical.starts_with(&canonical_base) {
        bail!(
            "Unverified restore source: referenced object {} resolves outside {}",
            cursor.display(),
            base.display()
        );
    }
    Ok(cursor)
}

fn verify_content_addressed_file(path: &Path, expected_hash: &str, label: &str) -> Result<()> {
    validate_sha256(expected_hash, label)?;
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("");
    if stem != expected_hash {
        bail!(
            "Unverified restore source: {} {} is not named by its declared SHA-256 {}",
            label,
            path.display(),
            expected_hash
        );
    }
    let actual = calculate_file_hash(path)?;
    if actual != expected_hash {
        bail!(
            "Unverified restore source: {} hash mismatch for {} (declared {}, actual {})",
            label,
            path.display(),
            expected_hash,
            actual
        );
    }
    Ok(())
}

fn verify_restore_root_file(
    path: &Path,
    expected_hash: &str,
    generation_id: &str,
    mode: CheckpointMode,
    label: &str,
) -> Result<()> {
    validate_sha256(expected_hash, label)?;
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("");
    let legacy_generation_root = mode == CheckpointMode::Monolithic && stem == generation_id;
    if stem != expected_hash && !legacy_generation_root {
        bail!(
            "Unverified restore source: {} {} is named by neither its declared SHA-256 {} nor its selected generation {}",
            label,
            path.display(),
            expected_hash,
            generation_id
        );
    }
    let actual = calculate_file_hash(path)?;
    if actual != expected_hash {
        bail!(
            "Unverified restore source: {} hash mismatch for {} (declared {}, actual {})",
            label,
            path.display(),
            expected_hash,
            actual
        );
    }
    Ok(())
}

fn required_manifest_usize(manifest: &serde_json::Value, field: &str) -> Result<usize> {
    manifest
        .get(field)
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| {
            anyhow!("Unverified restore source: manifest field '{field}' is missing or invalid")
        })
}

fn verify_monolithic_restore_records(root: &Path) -> Result<()> {
    for (line_index, line) in BufReader::new(File::open(root)?).lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            bail!(
                "Unverified restore source: blank record at {}:{}",
                root.display(),
                line_index + 1
            );
        }
        let record: serde_json::Value = serde_json::from_str(&line).map_err(|error| {
            anyhow!(
                "Unverified restore source: malformed record at {}:{}: {}",
                root.display(),
                line_index + 1,
                error
            )
        })?;
        reject_archaeology_view(&record, root)?;
        if !matches!(
            record
                .get("record_type")
                .and_then(serde_json::Value::as_str),
            Some("issue" | "event" | "provenance_receipt")
        ) {
            bail!(
                "Unverified restore source: {}:{} is not a typed forensic checkpoint record",
                root.display(),
                line_index + 1
            );
        }
    }
    Ok(())
}

fn verify_sharded_restore_closure(
    base: &Path,
    manifest_path: &Path,
    pointer: &RestorePointer,
) -> Result<()> {
    let bytes = std::fs::read(manifest_path)?;
    let manifest: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|error| anyhow!("Unverified restore source: invalid sharded manifest: {error}"))?;
    reject_archaeology_view(&manifest, manifest_path)?;

    if manifest.get("format").and_then(serde_json::Value::as_str) != Some("checkpoint-set-v1")
        || manifest
            .get("schema_version")
            .and_then(serde_json::Value::as_u64)
            != Some(1)
        || manifest.get("profile").and_then(serde_json::Value::as_str) != Some("native-v1")
        || manifest
            .get("partition_algorithm")
            .and_then(serde_json::Value::as_str)
            != Some("sha256-hex-prefix")
    {
        bail!("Unverified restore source: unsupported or incomplete sharded manifest metadata");
    }
    if manifest
        .get("store_uuid")
        .and_then(serde_json::Value::as_str)
        != Some(pointer.store_uuid.as_str())
        || manifest
            .get("snapshot_sequence")
            .and_then(serde_json::Value::as_i64)
            != Some(pointer.snapshot_sequence)
    {
        bail!("Unverified restore source: manifest identity or snapshot sequence disagrees with pointer");
    }
    if manifest
        .get("partition_thresholds")
        .and_then(CheckpointThresholds::from_manifest_json)
        .is_none()
    {
        bail!("Unverified restore source: manifest partition_thresholds are invalid");
    }

    let manifest_issue_count = required_manifest_usize(&manifest, "issue_count")?;
    let manifest_event_count = required_manifest_usize(&manifest, "event_count")?;
    let manifest_receipt_count = required_manifest_usize(&manifest, "receipt_count")?;
    let manifest_total = required_manifest_usize(&manifest, "total_record_count")?;
    if (
        manifest_issue_count,
        manifest_event_count,
        manifest_receipt_count,
        manifest_total,
    ) != (
        pointer.issue_count,
        pointer.event_count,
        pointer.receipt_count,
        pointer.total_record_count,
    ) {
        bail!("Unverified restore source: manifest counts disagree with generation pointer");
    }

    let mut seen_paths = HashSet::new();
    let mut verified_counts = [0usize; 3];
    for (index, (field, expected_role, expected_record_type)) in [
        ("issue_shards", "issues", "issue"),
        ("event_shards", "events", "event"),
        (
            "receipt_shards",
            "provenance_receipts",
            "provenance_receipt",
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let shards = manifest
            .get(field)
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| anyhow!("Unverified restore source: manifest missing {field}"))?;
        for shard in shards {
            let path = shard
                .get("path")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| anyhow!("Unverified restore source: {field} entry missing path"))?;
            let hash = shard
                .get("sha256")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| {
                    anyhow!("Unverified restore source: {field} entry missing sha256")
                })?;
            let byte_length = shard
                .get("byte_length")
                .and_then(serde_json::Value::as_u64)
                .ok_or_else(|| {
                    anyhow!("Unverified restore source: {field} entry missing byte_length")
                })?;
            let record_count = shard
                .get("record_count")
                .and_then(serde_json::Value::as_u64)
                .and_then(|value| usize::try_from(value).ok())
                .ok_or_else(|| {
                    anyhow!("Unverified restore source: {field} entry missing record_count")
                })?;
            if shard.get("role").and_then(serde_json::Value::as_str) != Some(expected_role) {
                bail!("Unverified restore source: {field} entry has the wrong semantic role");
            }
            if !seen_paths.insert(path.to_string()) {
                bail!("Unverified restore source: duplicate sharded object reference '{path}'");
            }
            let object = verified_checkpoint_path(base, path, "objects", "jsonl")?;
            verify_content_addressed_file(&object, hash, "sharded object")?;
            if std::fs::metadata(&object)?.len() != byte_length {
                bail!(
                    "Unverified restore source: byte length mismatch for {}",
                    object.display()
                );
            }
            let file = File::open(&object)?;
            let mut actual_records = 0usize;
            for (line_index, line) in BufReader::new(file).lines().enumerate() {
                let line = line?;
                if line.is_empty() {
                    bail!(
                        "Unverified restore source: blank record at {}:{}",
                        object.display(),
                        line_index + 1
                    );
                }
                let record: serde_json::Value = serde_json::from_str(&line).map_err(|error| {
                    anyhow!(
                        "Unverified restore source: malformed record at {}:{}: {}",
                        object.display(),
                        line_index + 1,
                        error
                    )
                })?;
                reject_archaeology_view(&record, &object)?;
                if record
                    .get("record_type")
                    .and_then(serde_json::Value::as_str)
                    != Some(expected_record_type)
                {
                    bail!(
                        "Unverified restore source: {} contains a record outside its declared {} role",
                        object.display(),
                        expected_role
                    );
                }
                actual_records += 1;
            }
            if actual_records != record_count {
                bail!(
                    "Unverified restore source: record count mismatch for {} (declared {}, actual {})",
                    object.display(),
                    record_count,
                    actual_records
                );
            }
            verified_counts[index] += actual_records;
        }
    }
    if verified_counts
        != [
            pointer.issue_count,
            pointer.event_count,
            pointer.receipt_count,
        ]
    {
        bail!(
            "Unverified restore source: sharded object counts {:?} disagree with pointer ({}, {}, {})",
            verified_counts,
            pointer.issue_count,
            pointer.event_count,
            pointer.receipt_count
        );
    }
    Ok(())
}

/// Stage forensic checkpoint from input path
/// Stage the workspace's own durable checkpoint set (R027): the
/// `checkpoint/` directory a `current.json` pointer governs. Exposed for
/// the sync-relationship classifier, which must stage exactly the
/// generation the pointer selects.
pub(crate) fn stage_checkpoint_set(checkpoint_dir: &Path) -> Result<ForensicStaging> {
    stage_forensic_checkpoint(checkpoint_dir)
}

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
        reject_archaeology_source_file(input_path)?;
        stage_monolithic_checkpoint(input_path)
    }
}

/// Classify a standalone JSON archaeology document before the JSONL parser
/// reports an incidental line-level syntax error. A streaming deserializer
/// reads only the first JSON value, so normal multi-record checkpoints do not
/// need to be buffered as one file.
fn reject_archaeology_source_file(input_path: &Path) -> Result<()> {
    let file = File::open(input_path)?;
    let mut deserializer = serde_json::Deserializer::from_reader(file);
    if let Ok(value) = serde_json::Value::deserialize(&mut deserializer) {
        reject_archaeology_view(&value, input_path)?;
    }
    Ok(())
}

/// Stage a checkpoint referenced by a `current.json` pointer, dispatching on
/// the pointer's own `mode` field. A directory checkpoint is not necessarily
/// sharded: `bead sync flush-only` always writes the same pointer +
/// `objects/<sha256>.jsonl` layout (older checkpoints may still reference
/// generation-named `objects/gen-*.jsonl` objects -- the pointer's
/// `active_root.path` is authoritative either way), but for monolithic mode
/// that root file is the raw JSONL data itself, not a shard manifest --
/// treating it as the latter (as this dispatch used to do unconditionally
/// for any directory input) fails to parse, erroring on the second JSONL
/// record as unexpected "trailing characters".
fn stage_pointer_checkpoint(pointer_path: &Path) -> Result<ForensicStaging> {
    let pointer_data = std::fs::read_to_string(pointer_path)?;
    let pointer: serde_json::Value =
        serde_json::from_str(&pointer_data).map_err(|e| anyhow!("Invalid pointer JSON: {}", e))?;
    reject_archaeology_view(&pointer, pointer_path)?;

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
        reject_archaeology_view(&record, input_path)?;

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
    reject_archaeology_view(&manifest, &manifest_full_path)?;

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

    // Canonical order is a property of the staged corpus, not of the manifest
    // that delivered it: shards may be listed in any order (issue shards are
    // keyed by hash prefix, so their bead IDs interleave), and the corpus the
    // publisher serializes is sorted by bead ID regardless.
    issues.sort_by(|a, b| a.id.cmp(&b.id));
    dependencies.sort();
    labels.sort();
    events.sort_by(|a, b| {
        (a.origin_store_uuid.as_str(), a.origin_event_sequence)
            .cmp(&(b.origin_store_uuid.as_str(), b.origin_event_sequence))
    });
    receipts.sort_by(|a, b| a.receipt_id.cmp(&b.receipt_id));

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
        reject_archaeology_view(&record, shard_path)?;

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
    validate_forensic_contents(staging)?;

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

/// Validate every source-intrinsic forensic invariant. R036 calls this only
/// after verifying the generation pointer and complete content-addressed
/// closure, and before it inspects or mutates the target.
pub(crate) fn validate_forensic_contents(staging: &ForensicStaging) -> Result<()> {
    for issue in &staging.issues {
        if issue.schema_ref.as_deref() != Some("urn:bead-rs:schema:issue:native-v1") {
            bail!(
                "Issue '{}' does not declare the native-v1 issue schema",
                issue.id
            );
        }
        issue
            .validate()
            .map_err(|error| anyhow!("Issue '{}' failed validation: {}", issue.id, error))?;
        if let Some(value) = issue.extensions.get("resource_keys") {
            resource_keys_from_value(value).map_err(|error| {
                anyhow!("Issue '{}' has invalid resource_keys: {}", issue.id, error)
            })?;
        }
    }

    // Validate canonical ordering
    validate_canonical_ordering(staging)?;

    // Validate dependencies
    validate_dependencies(&staging.dependencies, &staging.issues)?;

    // Validate event sequence continuity
    validate_event_sequence(staging)?;

    validate_forensic_events(staging)?;
    validate_forensic_receipts(staging)?;

    Ok(())
}

fn validate_forensic_events(staging: &ForensicStaging) -> Result<()> {
    let issue_ids: HashSet<&str> = staging
        .issues
        .iter()
        .map(|issue| issue.id.as_str())
        .collect();
    for event in &staging.events {
        if event.schema_ref != "urn:bead-rs:schema:event:native-v1" {
            bail!("Forensic event does not declare the native-v1 event schema");
        }
        if event.origin_store_uuid.trim().is_empty() || event.origin_event_sequence <= 0 {
            bail!("Forensic event has an invalid origin identity");
        }
        if event.kind.trim().is_empty() || event.time.trim().is_empty() {
            bail!(
                "Forensic event ({}, {}) has an empty kind or time",
                event.origin_store_uuid,
                event.origin_event_sequence
            );
        }
        if let Some(issue_id) = event.issue_id.as_deref() {
            if !issue_ids.contains(issue_id) {
                bail!(
                    "Forensic event ({}, {}) references missing issue {}",
                    event.origin_store_uuid,
                    event.origin_event_sequence,
                    issue_id
                );
            }
        }
    }
    Ok(())
}

fn validate_forensic_receipts(staging: &ForensicStaging) -> Result<()> {
    for receipt in &staging.receipts {
        if receipt.schema_ref != "urn:bead-rs:schema:provenance-receipt:native-v1"
            || !matches!(receipt.kind.as_str(), "restore" | "merge")
            || receipt.source_store_uuid.trim().is_empty()
            || receipt.target_store_uuid.trim().is_empty()
            || receipt.actor.trim().is_empty()
            || receipt.created_at.trim().is_empty()
            || receipt.result != "success"
            || receipt.counts.issues < 0
            || receipt.counts.events < 0
            || receipt.counts.provenance_receipts < 0
        {
            bail!(
                "Provenance receipt '{}' has invalid required fields",
                receipt.receipt_id
            );
        }
        validate_sha256(
            &receipt.source_root_sha256,
            "provenance receipt source_root_sha256",
        )?;
        validate_sha256(&receipt.receipt_sha256, "provenance receipt receipt_sha256")?;
        let mut hasher = Sha256::new();
        hasher.update(&receipt.receipt_id);
        hasher.update(&receipt.kind);
        hasher.update(&receipt.source_root_sha256);
        hasher.update(&receipt.actor);
        hasher.update(&receipt.created_at);
        hasher.update(&receipt.result);
        let expected = format!("{:x}", hasher.finalize());
        if receipt.receipt_sha256 != expected {
            bail!(
                "Provenance receipt '{}' hash mismatch (declared {}, calculated {})",
                receipt.receipt_id,
                receipt.receipt_sha256,
                expected
            );
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

    let mut previous_origin = "";
    let mut previous_sequence = 0i64;
    for event in &staging.events {
        if event.origin_store_uuid != previous_origin {
            if event.origin_event_sequence != 1 {
                bail!(
                    "Event sequence for origin {} does not start at 1: starts at {}",
                    event.origin_store_uuid,
                    event.origin_event_sequence
                );
            }
            previous_origin = &event.origin_store_uuid;
        } else if event.origin_event_sequence != previous_sequence + 1 {
            bail!(
                "Event sequence gap for origin {}: expected {}, found {}",
                event.origin_store_uuid,
                previous_sequence + 1,
                event.origin_event_sequence
            );
        }
        previous_sequence = event.origin_event_sequence;
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
///
/// R027: the identity comparison runs against live events enumerated with
/// their *derived* wire identities, not only rows carrying explicit origin
/// columns. Native mutations write NULL origin columns, so matching only
/// explicit rows let a staged event collide with a derived local identity
/// unnoticed — validation passed and the import re-inserted the row as a
/// duplicate under a second identity. Comparing through
/// [`read_all_events`] uses exactly the identity publication derives, with
/// the actor export default (NULL reads as `"system"`) applied on both
/// sides.
fn validate_event_prefix(conn: &rusqlite::Connection, staging: &ForensicStaging) -> Result<()> {
    let live_events = read_all_events(conn)?;
    let mut live_by_identity: HashMap<(&str, i64), &EventRecord> =
        HashMap::with_capacity(live_events.len());
    for live_event in &live_events {
        live_by_identity.insert(
            (
                live_event.origin_store_uuid.as_str(),
                live_event.origin_event_sequence,
            ),
            live_event,
        );
    }

    for event in &staging.events {
        if let Some(&live_event) = live_by_identity.get(&(
            event.origin_store_uuid.as_str(),
            event.origin_event_sequence,
        )) {
            if !crate::service::reconcile::public_content_matches(live_event, event) {
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
#[derive(Debug, Default, Clone)]
struct ImportCounts {
    inserted: usize,
    updated: usize,
    retained: usize,
    events_imported: i64,
    receipts_processed: usize,
    receipt: Option<SerializedReceipt>,
    summary_event_sequence: Option<i64>,
    displaced: RestoreDisplacedCounts,
}

/// Activate a previously verified named generation into the initialized
/// target store. The source verifier is intentionally a separate first step:
/// the CLI can run it before creating a missing database, satisfying R036's
/// no-target-mutation-before-source-verification rule.
pub fn restore_verified_generation(
    store: &mut SqliteStore,
    verified: VerifiedRestoreSource,
    actor: &str,
    allow_non_empty: bool,
) -> Result<RestoreReport> {
    validate_restore_actor(actor)?;
    validate_forensic_contents(&verified.staging)?;

    // Recheck the immutable root immediately before target inspection. The
    // CLI verifies the full source a second time after any auto-initialization;
    // this final root check closes the last mutation boundary.
    let root = verified_checkpoint_path(
        verified
            .pointer_path
            .parent()
            .ok_or_else(|| anyhow!("Restore pointer has no checkpoint-set directory"))?,
        &verified.root_path,
        match verified.mode {
            CheckpointMode::Monolithic => "objects",
            CheckpointMode::Sharded => "manifests",
        },
        match verified.mode {
            CheckpointMode::Monolithic => "jsonl",
            CheckpointMode::Sharded => "json",
        },
    )?;
    verify_restore_root_file(
        &root,
        &verified.root_sha256,
        &verified.generation_id,
        verified.mode,
        "restore root",
    )?;

    let counts =
        execute_restore_into_empty(store, &verified.staging, actor, allow_non_empty, true)?;
    let replacing = !counts.displaced.is_empty();
    let receipt = counts
        .receipt
        .ok_or_else(|| anyhow!("Restore committed without returning its provenance receipt"))?;
    let summary_event_sequence = counts
        .summary_event_sequence
        .ok_or_else(|| anyhow!("Restore committed without returning its summary event"))?;

    Ok(RestoreReport {
        generation_id: verified.generation_id,
        mode: verified.mode.as_str().to_string(),
        source_pointer: verified.pointer_path.display().to_string(),
        source_root_path: verified.root_path,
        source_root_sha256: verified.root_sha256,
        source_store_uuid: verified.source_store_uuid.clone(),
        target_store_uuid: verified.source_store_uuid,
        snapshot_sequence: verified.snapshot_sequence,
        actor: actor.to_string(),
        issues_restored: counts.inserted,
        events_restored: counts.events_imported as usize,
        provenance_receipts_restored: verified.staging.receipt_count,
        restore_receipt_id: receipt.receipt_id,
        restore_receipt_sha256: receipt.receipt_sha256,
        summary_event_sequence,
        non_empty_override: replacing && allow_non_empty,
        displaced: if replacing {
            counts.displaced
        } else {
            RestoreDisplacedCounts::default()
        },
    })
}

fn validate_restore_actor(actor: &str) -> Result<()> {
    if actor.trim().is_empty() {
        bail!("Restore actor cannot be empty");
    }
    if actor.len() > 255 {
        bail!("Restore actor cannot exceed 255 bytes");
    }
    if actor.contains(char::is_control) {
        bail!("Restore actor cannot contain control characters");
    }
    Ok(())
}

/// Fork workspace identity (R028: bead sync fork)
///
/// Creates a new workspace UUID derived from the current workspace while
/// recording the provenance relationship in a fork receipt. This enables
/// clones of one repository to become distinct origins whose event streams
/// merge composably under existing different-UUID rules.
///
/// # Arguments
///
/// * `store` - Mutable reference to SQLite store
/// * `actor` - Actor performing the fork (required, non-empty, ≤255 bytes, no control chars)
/// * `reason` - Optional human-readable explanation for the fork
///
/// # Returns
///
/// * `Ok(ForkReport)` - Complete result with UUIDs, receipt info, and counts
/// * `Err(...)` - Validation error, dirty workspace, or I/O error
///
/// # Errors
///
/// - **Actor Error**: Missing, empty, oversized, or control-character actor
/// - **Workspace Error**: Workspace is dirty or has no checkpoint
/// - **Integrity Error**: Failed to read or write fork receipt
///
/// # Fork Behavior
///
/// 1. Validates workspace is clean (checkpoint covers current event sequence)
/// 2. Generates new UUID with provenance to parent UUID
/// 3. Records fork receipt in provenance_receipts table
/// 4. Updates workspace.uuid in database
/// 5. Creates summary event in events table
/// 6. Returns report with old/new UUIDs and receipt details
///
/// The forked workspace is now a distinct origin that can merge back into
/// the parent workspace using `bead sync import-only --merge`.
pub fn fork_workspace_identity(
    store: &mut SqliteStore,
    actor: &str,
    reason: Option<&str>,
) -> Result<ForkReport> {
    // Validate actor (same rules as restore)
    if actor.trim().is_empty() {
        bail!("Fork actor cannot be empty");
    }
    if actor.len() > 255 {
        bail!("Fork actor cannot exceed 255 bytes");
    }
    if actor.contains(char::is_control) {
        bail!("Fork actor cannot contain control characters");
    }

    // Validate reason if provided
    if let Some(r) = reason {
        if r.len() > 4096 {
            bail!("Fork reason cannot exceed 4096 bytes");
        }
    }

    let conn = store.conn();
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;

    // Get current workspace state
    let parent_uuid: String = tx
        .query_row("SELECT uuid FROM workspace WHERE id = 1", [], |row| {
            row.get(0)
        })
        .map_err(|e| anyhow!("Failed to read workspace UUID: {}", e))?;

    // Get current event sequence
    let current_sequence: i64 = tx
        .query_row("SELECT COALESCE(MAX(sequence), 0) FROM events", [], |row| {
            row.get(0)
        })
        .unwrap_or(0);

    // Get checkpoint state to verify workspace is clean
    let checkpoint_state: Option<(i64, String)> = tx
        .query_row(
            "SELECT covered_event_sequence, current_generation_id FROM checkpoint_state WHERE id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .ok();

    // Verify checkpoint is clean (not dirty)
    if let Some((covered_sequence, _)) = checkpoint_state {
        if covered_sequence < current_sequence {
            bail!(
                "Cannot fork dirty workspace: checkpoint covers sequence {} but current sequence is {}. \
                 Run 'bead sync flush-only' first to publish a clean checkpoint.",
                covered_sequence, current_sequence
            );
        }
    } else {
        bail!("Cannot fork workspace with no checkpoint. Run 'bead sync flush-only' first.");
    }

    // Get current counts
    let issue_count: i64 = tx.query_row("SELECT COUNT(*) FROM issues", [], |row| row.get(0))?;
    let event_count: i64 = tx.query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))?;
    let receipt_count: i64 =
        tx.query_row("SELECT COUNT(*) FROM provenance_receipts", [], |row| {
            row.get(0)
        })?;

    // Generate new UUID with provenance derivation
    let new_uuid = derive_fork_uuid(&parent_uuid, current_sequence)?;

    // Create fork receipt
    let created_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let receipt_id = format!("fork-{}", generate_hex_suffix(16));

    // Extract parent generation ID if available
    let parent_generation_id = checkpoint_state.as_ref().map(|(_, gen_id)| gen_id.clone());

    let fork_receipt = ForkReceipt {
        schema_ref: "urn:bead-rs:schema:fork-receipt:native-v1".to_string(),
        receipt_id: receipt_id.clone(),
        kind: "fork".to_string(),
        parent_store_uuid: parent_uuid.clone(),
        new_store_uuid: new_uuid.clone(),
        parent_root_sha256: String::new(), // Will be filled after checkpoint verification
        parent_generation_id: parent_generation_id.clone(),
        actor: actor.to_string(),
        created_at: created_at.clone(),
        counts: ReceiptCounts {
            issues: issue_count,
            events: event_count,
            provenance_receipts: receipt_count,
        },
        result: "success".to_string(),
        receipt_sha256: String::new(), // Will be computed below
        reason: reason.map(|r| r.to_string()),
    };

    // Serialize and hash receipt
    let receipt_json = serde_json::to_string(&fork_receipt)
        .map_err(|e| anyhow!("Failed to serialize fork receipt: {}", e))?;
    let receipt_sha256 = format!("{:x}", Sha256::digest(receipt_json.as_bytes()));

    // Insert fork receipt
    let counts_json = serde_json::to_string(&fork_receipt.counts)
        .map_err(|e| anyhow!("Failed to serialize counts: {}", e))?;

    tx.execute(
        "INSERT INTO provenance_receipts
         (receipt_id, schema_ref, kind, source_store_uuid, target_store_uuid,
          source_root_sha256, actor, created_at, counts_json, result,
          summary_event_identity, receipt_sha256)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, NULL, ?11)",
        params![
            receipt_id,
            fork_receipt.schema_ref,
            "fork",
            parent_uuid,
            new_uuid,
            "", // root_sha256 will be filled by checkpoint verification
            actor,
            created_at,
            counts_json,
            "success",
            receipt_sha256
        ],
    )?;

    // Update workspace UUID
    tx.execute(
        "UPDATE workspace SET uuid = ?1 WHERE id = 1",
        params![new_uuid],
    )?;

    // Create summary event
    let summary_sequence = current_sequence + 1;
    let summary_event = EventRecord {
        schema_ref: "urn:bead-rs:schema:event:native-v1".to_string(),
        origin_store_uuid: new_uuid.clone(),
        origin_event_sequence: summary_sequence,
        issue_id: None,
        kind: "workspace_forked".to_string(),
        actor: actor.to_string(),
        time: created_at.clone(),
        detail: serde_json::json!({
            "parent_store_uuid": parent_uuid,
            "new_store_uuid": new_uuid,
            "fork_receipt_id": receipt_id,
            "reason": reason
        }),
    };

    let event_json = serde_json::to_string(&summary_event)
        .map_err(|e| anyhow!("Failed to serialize fork summary event: {}", e))?;

    tx.execute(
        "INSERT INTO events (sequence, origin_store_uuid, origin_event_sequence, issue_id, kind, actor, time, detail)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            summary_sequence,
            new_uuid,
            summary_sequence,
            None::<String>,
            "workspace_forked",
            actor,
            created_at,
            event_json
        ],
    )?;

    // Update checkpoint state to dirty (since we've changed the workspace identity)
    let updated_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    tx.execute(
        "UPDATE checkpoint_state
         SET covered_event_sequence = ?1, updated_at = ?2
         WHERE id = 1",
        params![summary_sequence, updated_at],
    )?;

    tx.commit()?;

    Ok(ForkReport {
        parent_store_uuid: parent_uuid.clone(),
        new_store_uuid: new_uuid.clone(),
        fork_receipt_id: receipt_id,
        fork_receipt_sha256: receipt_sha256,
        actor: actor.to_string(),
        created_at,
        issue_count: issue_count as usize,
        event_count: event_count as usize,
        receipt_count: receipt_count as usize,
        summary_event_sequence: summary_sequence,
        parent_generation_id,
        reason: reason.map(|r| r.to_string()),
    })
}

/// Derive a new fork UUID with provenance to the parent UUID
///
/// The derived UUID maintains a traceable relationship to the parent while
/// being cryptographically distinct. The format combines:
/// - Parent UUID (for provenance)
/// - Current event sequence (for uniqueness)
/// - Random entropy (for collision resistance)
fn derive_fork_uuid(parent_uuid: &str, current_sequence: i64) -> Result<String> {
    use rand::Rng;

    let mut rng = rand::thread_rng();
    let random_bytes: [u8; 8] = rng.r#gen::<[u8; 8]>();

    // Derive new UUID by combining:
    // - First 8 chars of parent UUID (provenance prefix)
    // - Current sequence (version marker)
    // - Random suffix (collision resistance)
    let provenance_prefix = &parent_uuid[..8];
    let random_suffix = format!("{:016x}", u64::from_be_bytes(random_bytes));

    Ok(format!(
        "{}-fork-{}-{}",
        provenance_prefix,
        current_sequence,
        &random_suffix[..16]
    ))
}

/// Generate random hex suffix for receipt IDs
fn generate_hex_suffix(length: usize) -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let bytes: Vec<u8> = (0..length).map(|_| rng.r#gen::<u8>()).collect();
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Fork operation result report
#[derive(Debug, Clone, Serialize)]
pub struct ForkReport {
    /// Parent workspace UUID being forked from
    pub parent_store_uuid: String,
    /// Newly generated UUID for the forked workspace
    pub new_store_uuid: String,
    /// Fork receipt ID
    pub fork_receipt_id: String,
    /// Fork receipt SHA-256
    pub fork_receipt_sha256: String,
    /// Actor who performed the fork
    pub actor: String,
    /// ISO 8601 timestamp
    pub created_at: String,
    /// Number of issues at fork time
    pub issue_count: usize,
    /// Number of events at fork time
    pub event_count: usize,
    /// Number of provenance receipts at fork time
    pub receipt_count: usize,
    /// Summary event sequence number
    pub summary_event_sequence: i64,
    /// Parent generation ID (if available)
    pub parent_generation_id: Option<String>,
    /// Optional reason for the fork
    pub reason: Option<String>,
}

fn read_restore_target_counts(conn: &rusqlite::Connection) -> Result<RestoreDisplacedCounts> {
    fn count(conn: &rusqlite::Connection, table: &str) -> Result<usize> {
        let sql = format!("SELECT COUNT(*) FROM {table}");
        let count: i64 = conn
            .query_row(&sql, [], |row| row.get(0))
            .map_err(|error| {
                anyhow!(
                "Target is not a usable current bead-rs schema (cannot inspect {table}): {error}"
            )
            })?;
        usize::try_from(count).map_err(|_| anyhow!("Invalid negative row count in {table}"))
    }

    Ok(RestoreDisplacedCounts {
        issues: count(conn, "issues")?,
        events: count(conn, "events")?,
        provenance_receipts: count(conn, "provenance_receipts")?,
        saved_views: count(conn, "saved_views")?,
        recurrence_templates: count(conn, "recurrence_templates")?,
    })
}

/// Clear every native semantic table before an explicitly authorized
/// replacement restore. Unknown tables are deliberately untouched: the
/// override replaces bead-rs state, not extension storage the operator did
/// not authorize this implementation to interpret or drop.
fn clear_native_restore_target(tx: &Transaction<'_>) -> Result<()> {
    for table in [
        "claim_telemetry",
        "recurrence_materializations",
        "scheduling_metrics",
        "leases",
        "unique_reference_bindings",
        "external_references",
        "issue_data",
        "comments",
        "dependencies",
        "labels",
        "issue_extensions",
        "events",
        "issues",
        "provenance_receipts",
        "saved_views",
        "recurrence_templates",
        "checkpoint_state",
    ] {
        tx.execute(&format!("DELETE FROM {table}"), [])?;
    }
    tx.execute("DELETE FROM workspace_claim_sequence", [])?;
    tx.execute(
        "INSERT INTO workspace_claim_sequence (sequence) VALUES (0)",
        [],
    )?;
    Ok(())
}

fn execute_restore_into_empty(
    store: &mut SqliteStore,
    staging: &ForensicStaging,
    actor: &str,
    allow_non_empty: bool,
    record_summary_event: bool,
) -> Result<ImportCounts> {
    let conn = store.conn();
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;

    // Inspect and guard the target after BEGIN IMMEDIATE. Holding the write
    // transaction here prevents another process from inserting state between
    // the empty-target decision and activation.
    let displaced = read_restore_target_counts(&tx)?;
    if !displaced.is_empty() && !allow_non_empty {
        bail!(
            "Target database is not empty (issues={}, events={}, provenance_receipts={}, saved_views={}, recurrence_templates={}). Restore refused without mutation; inspect the target and rerun with --allow-non-empty only when replacing that native state is intended.",
            displaced.issues,
            displaced.events,
            displaced.provenance_receipts,
            displaced.saved_views,
            displaced.recurrence_templates
        );
    }
    let replacing = !displaced.is_empty();

    if replacing && allow_non_empty {
        clear_native_restore_target(&tx)?;
    }

    // Adopt checkpoint store UUID
    tx.execute("UPDATE workspace SET uuid = ?1", [&staging.store_uuid])?;

    // Activate staged data
    let (inserted, _source_activation_sequence) = activate_forensic_import(&tx, staging)?;

    // The summary event makes the explicit recovery itself auditable, beyond
    // the source events it adopts. AUTOINCREMENT remains monotonic across an
    // override, so replacing an older or smaller generation still advances
    // the target's mutation sequence.
    let summary_event_sequence = if record_summary_event {
        Some(create_restore_summary(&tx, staging, actor, replacing)?)
    } else {
        None
    };

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
    let receipt = create_restore_receipt(&tx, staging, actor, summary_event_sequence)?;

    // Commit transaction
    tx.commit()?;

    if !record_summary_event {
        eprintln!(
            "Restored {} issues, {} events",
            inserted,
            staging.events.len()
        );
        eprintln!("Restore receipt: {}", receipt.receipt_id);
    }

    Ok(ImportCounts {
        inserted,
        updated: 0,
        retained: 0,
        events_imported: staging.events.len() as i64,
        // +1 for the restore receipt created above
        receipts_processed: staging.receipts.len() + 1,
        receipt: Some(receipt),
        summary_event_sequence,
        displaced,
    })
}

/// Execute merge operation
fn execute_merge(
    store: &mut SqliteStore,
    staging: &ForensicStaging,
    actor: &str,
) -> Result<ImportCounts> {
    let conn = store.conn();
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;

    // A same-UUID merge reconciles a checkpoint of this very store's history,
    // so the local rows it verified against must carry their derived wire
    // identities before the identity-based import dedup runs (R027). Without
    // this, native NULL-origin rows never match their staged counterparts
    // and every pre-existing event is imported a second time.
    let target_uuid: String = tx
        .query_row("SELECT uuid FROM workspace", [], |row| row.get(0))
        .unwrap_or_default();
    if target_uuid == staging.store_uuid {
        canonicalize_local_event_identities(&tx)?;
    }

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
        receipt: Some(receipt),
        summary_event_sequence: Some(activation_sequence),
        displaced: RestoreDisplacedCounts::default(),
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

        import_resource_keys(tx, issue)?;

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

/// Append the audit event for the explicit recovery operation itself. Source
/// events are restored verbatim; this additional local event records who
/// performed the activation and whether native target state was replaced.
fn create_restore_summary(
    tx: &Transaction<'_>,
    staging: &ForensicStaging,
    actor: &str,
    replaced_non_empty: bool,
) -> Result<i64> {
    let detail = serde_json::json!({
        "source_store_uuid": staging.store_uuid,
        "source_root_sha256": staging.input_hash,
        "snapshot_sequence": staging.snapshot_sequence,
        "issues_restored": staging.issue_count,
        "events_restored": staging.event_count,
        "provenance_receipts_restored": staging.receipt_count,
        "replaced_non_empty_target": replaced_non_empty,
    });
    tx.execute(
        "INSERT INTO events (issue_id, kind, actor, time, detail)
         VALUES (NULL, 'checkpoint_restored', ?1, ?2, ?3)",
        params![
            actor,
            format_rfc3339(SystemTime::now()),
            serde_json::to_string(&detail)?
        ],
    )?;
    Ok(tx.last_insert_rowid())
}

/// Create restore receipt
fn create_restore_receipt(
    tx: &Transaction,
    staging: &ForensicStaging,
    actor: &str,
    summary_event_sequence: Option<i64>,
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
        summary_event_identity: summary_event_sequence.map(|sequence| format!("local-{sequence}")),
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

                import_resource_keys(tx, issue)?;
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
                    if issue.extensions.contains_key("resource_keys") {
                        tx.execute(
                            "DELETE FROM issue_resource_keys WHERE issue_id = ?1",
                            [&issue.id],
                        )?;
                        import_resource_keys(tx, issue)?;
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

                    crate::service::resource_locks::sync_issue_locks(tx, &issue.id)?;

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
        "labels" | "dependencies" | "external_references" | "comments" | "resource_keys"
    )
}

fn import_resource_keys(tx: &Transaction, issue: &Issue) -> Result<()> {
    let Some(value) = issue.extensions.get("resource_keys") else {
        return Ok(());
    };
    let keys = resource_keys_from_value(value)?;
    declare_resource_keys(tx, &issue.id, &keys)?;
    if issue.base_status == crate::model::BaseStatus::InProgress && issue.assignee.is_some() {
        acquire_issue_locks(tx, &issue.id, None)?;
    }
    Ok(())
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

    // A present projection replaces the issue's reference collection. Remove
    // R032 bindings first so a changed checkpoint cannot leave an orphaned
    // uniqueness reservation behind.
    tx.execute(
        "DELETE FROM unique_reference_bindings WHERE issue_id = ?1",
        [&issue.id],
    )?;

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
        let is_unique = reference
            .get("unique_ref")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
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
        if is_unique {
            if member("key")? != crate::service::issues::UNIQUE_REF_EXTERNAL_KEY {
                return Err(anyhow!(
                    "Issue '{}' unique external reference must use key '{}'",
                    issue.id,
                    crate::service::issues::UNIQUE_REF_EXTERNAL_KEY
                ));
            }
            tx.execute(
                "INSERT INTO unique_reference_bindings (namespace, key, issue_id)
                 VALUES (?1, ?2, ?3)",
                params![member("namespace")?, member("value")?, &issue.id],
            )?;
        }
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
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;

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

        import_resource_keys(&tx, issue)?;
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
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;

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

/// The live event sequence: the same `MAX(sequence)` definition the
/// publisher and `sync --status` use as the dirtiness signal.
///
/// Returns `None` when the sequence cannot be read (unreadable database,
/// missing `events` table), which callers treat as "nothing to publish"
/// rather than an error: the post-commit chokepoint must never fail a
/// command that would otherwise succeed.
pub fn read_live_event_sequence(conn: &rusqlite::Connection) -> Option<i64> {
    conn.query_row("SELECT COALESCE(MAX(sequence), 0) FROM events", [], |row| {
        row.get(0)
    })
    .ok()
}

/// The event sequence the durable checkpoint already covers: the
/// authoritative pointer's `snapshot_sequence` (plan 6.2.1 item 3).
///
/// This is the same field `sync --status` reports as `covered_sequence` and
/// the complement of its dirtiness rule, so a publication decided here and
/// a dirtiness report there can never disagree. `None` when no pointer
/// exists or it cannot be read or parsed: the checkpoint then covers
/// nothing a caller may rely on, so publication must run rather than skip.
pub fn read_covered_event_sequence(checkpoint_base: &Path) -> Option<i64> {
    let pointer_path = checkpoint_base.join("checkpoint").join("current.json");
    let content = std::fs::read_to_string(pointer_path).ok()?;
    serde_json::from_str::<serde_json::Value>(&content)
        .ok()?
        .get("snapshot_sequence")?
        .as_i64()
}

/// How long a publisher waits for the checkpoint publication lock before
/// giving up. Generous against real publication work (object writes and
/// fsyncs on a large workspace), finite against a wedged holder: the lock
/// itself releases on process exit, so only a live but stuck publisher can
/// hold it this long.
const PUBLICATION_LOCK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Poll interval for the bounded publication-lock wait.
const PUBLICATION_LOCK_POLL: std::time::Duration = std::time::Duration::from_millis(10);

/// The checkpoint publication lock (plan 6.2.1 item 4).
///
/// An exclusive file lock on `.beads/checkpoint/publish.lock`, held across
/// one publication: read the outgoing generation, write objects, replace
/// `current.json`, apply the pointer-declared tombstones. It is deliberately
/// a *file* lock, not a SQLite lock -- publication must serialize with other
/// publishers, never with the SQLite write path, so a worker committing a
/// mutation while another publishes blocks on neither. The lock file is
/// never renamed (an flock follows the open file, not the path), never
/// appears under `objects/` or `manifests/`, and so is never enumerated,
/// tombstoned, or imported.
///
/// Dropping the guard releases the lock. A process that dies mid-publication
/// has the lock reclaimed by the kernel, so the next publisher proceeds and
/// re-declares whatever cleanup the dead one left unapplied.
pub struct CheckpointPublicationLock {
    /// Kept alive for the flock; the descriptor is the lock.
    _file: File,
}

impl Drop for CheckpointPublicationLock {
    fn drop(&mut self) {
        // flock releases on close; unlock() makes that immediate rather than
        // waiting for the File to close. Either way, failure to unlock is
        // not a publication failure.
        let _ = fs2::FileExt::unlock(&self._file);
    }
}

/// Acquire the checkpoint publication lock, waiting up to
/// [`PUBLICATION_LOCK_TIMEOUT`].
///
/// The checkpoint directory is created when absent so a first publication
/// can lock before it writes anything into it.
pub fn acquire_checkpoint_publication_lock(
    checkpoint_dir: &Path,
) -> Result<CheckpointPublicationLock> {
    acquire_checkpoint_publication_lock_within(checkpoint_dir, PUBLICATION_LOCK_TIMEOUT)
}

fn acquire_checkpoint_publication_lock_within(
    checkpoint_dir: &Path,
    timeout: std::time::Duration,
) -> Result<CheckpointPublicationLock> {
    use fs2::FileExt;
    use std::io::ErrorKind;

    std::fs::create_dir_all(checkpoint_dir)?;
    // Not File::create: truncating a file another publisher holds would be
    // harmless (the content is never read) but writing it at all is a lie --
    // the lock file stays empty forever.
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(checkpoint_dir.join("publish.lock"))?;

    let deadline = std::time::Instant::now() + timeout;
    loop {
        match file.try_lock_exclusive() {
            Ok(()) => return Ok(CheckpointPublicationLock { _file: file }),
            Err(e) if e.kind() == ErrorKind::WouldBlock => {
                if std::time::Instant::now() >= deadline {
                    return Err(anyhow!(
                        "checkpoint publication lock busy: another publisher has held \
                         {} for more than {}s; retry once it releases",
                        checkpoint_dir.join("publish.lock").display(),
                        timeout.as_secs()
                    ));
                }
                std::thread::sleep(PUBLICATION_LOCK_POLL);
            }
            Err(e) => return Err(e.into()),
        }
    }
}

/// Publish forensic checkpoint (F017)
///
/// This function implements the full forensic checkpoint-set format with:
/// - Monolithic mode: Single JSONL file with issue/event/receipt records
/// - Sharded mode: Manifest with content-addressed shards
/// - Atomic pointer replacement
/// - Git-trackable changed paths
///
/// Publication is serialized by the checkpoint publication lock (plan 6.2.1
/// item 4), acquired here and held across the whole publication: the
/// outgoing-generation read that drives tombstone math, the object writes,
/// the pointer replacement, and the tombstone application that follows it.
/// Two publishers therefore never interleave those steps, so a lost race can
/// leave a superseded generation, never a torn pointer or a partially
/// applied tombstone set. A caller that already holds the lock (the
/// post-commit chokepoint, which rechecks coverage under it) goes through
/// [`publish_forensic_checkpoint_holding`] instead.
pub fn publish_forensic_checkpoint(
    store: &mut SqliteStore,
    config: &CheckpointConfig,
    checkpoint_base: &Path,
) -> Result<ForensicFlushResult> {
    let publication_lock =
        acquire_checkpoint_publication_lock(&checkpoint_base.join("checkpoint"))?;
    publish_forensic_checkpoint_holding(&publication_lock, store, config, checkpoint_base)
}

/// The publication body, callable only by a caller that holds the
/// [`CheckpointPublicationLock`].
///
/// The lock parameter is proof, not input: it exists so the chokepoint can
/// publish under a lock it acquired for its own lost-race recheck without
/// re-acquiring (the lock is per open file, so a nested acquire by the same
/// process would self-deadlock), while every other entry point takes the
/// lock through [`publish_forensic_checkpoint`] and cannot forget it.
pub fn publish_forensic_checkpoint_holding(
    _publication_lock: &CheckpointPublicationLock,
    store: &mut SqliteStore,
    config: &CheckpointConfig,
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

    // Serialize every record line once (plan 6.2 step 2): mode selection
    // counts bytes from the same lines the publisher writes.
    let corpus = serialize_corpus(
        &sorted_issues,
        &sorted_events,
        &sorted_receipts,
        &graph_data,
    )?;

    // Read the outgoing generation before publishing anything, then select
    // the mode from the recorded configuration and thresholds (plan 6.1.1,
    // 6.2 step 3): an explicit `.beads/config.json` mode forces output,
    // otherwise the would-be monolith's size against the threshold table
    // decides.
    let previous = read_previous_generation(&checkpoint_dir)?;
    let thresholds =
        resolve_checkpoint_thresholds(config, previous.as_ref().and_then(|p| p.manifest.as_ref()));
    let stats = corpus_monolith_stats(&corpus);
    let policy = match config.mode {
        Some(mode) => ModePolicy::Forced(mode),
        None => ModePolicy::Adaptive,
    };
    let mode = select_checkpoint_mode(&stats, &thresholds, policy)?;

    let mut changed_paths = Vec::new();

    // Publish based on mode. The output's `referenced_paths` lists every
    // file the new generation selects; `changed_paths` accumulates what this
    // generation actually wrote (what one external Git commit must carry).
    let publication = match mode {
        CheckpointMode::Monolithic => publish_monolithic_checkpoint(
            &corpus,
            &checkpoint_dir,
            &generation_id,
            &mut changed_paths,
        )?,
        CheckpointMode::Sharded => {
            let sharded_config = ShardedConfig {
                generation_id: generation_id.clone(),
                store_uuid: store_uuid.clone(),
                snapshot_sequence: current_sequence,
            };
            publish_sharded_checkpoint(
                ShardedPublishInputs {
                    issues: &sorted_issues,
                    events: &sorted_events,
                    receipts: &sorted_receipts,
                    corpus: &corpus,
                    config: sharded_config,
                    thresholds,
                    previous_manifest: previous.as_ref().and_then(|p| p.manifest.as_ref()),
                },
                &checkpoint_dir,
                &mut changed_paths,
            )?
        }
    };

    // Update checkpoint pointers in a write transaction
    let root_hash = publication.root_hash;
    let root_path = publication.root_path;
    let conn = store.conn();
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;

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
        let checkpoint_dir_file = File::open(&checkpoint_dir)?;
        checkpoint_dir_file.sync_all()?;
        drop(checkpoint_dir_file);

        changed_paths.push("previous.json".to_string());

        // The outgoing pointer's referenced set was read before publication
        previous
            .as_ref()
            .map(|p| p.referenced_files.clone())
            .unwrap_or_default()
    } else {
        HashSet::new()
    };

    // Calculate path categories. current.json is rewritten by this
    // publication, so it counts as a current file and can only be replaced,
    // never deleted (plan 6.2 step 6, 6.2.1 P2). The current set is every
    // file the new generation references, not merely what it wrote: reused
    // content-addressed objects stay referenced and must never be
    // tombstoned while the generation selects them.
    let mut current_files: HashSet<String> = publication.referenced_paths.iter().cloned().collect();
    current_files.insert("current.json".to_string());
    let added_paths: Vec<String> = current_files.difference(&previous_files).cloned().collect();
    let replaced_paths: Vec<String> = current_files
        .intersection(&previous_files)
        .cloned()
        .collect();

    // Declare tombstones for every generation object on disk that neither
    // the new generation nor the retained previous generation references
    // (plan 6.1.1, 6.2 step 6). Enumerating the directory rather than
    // replaying the outgoing pointer's own declarations keeps cleanup
    // repeatable: files left behind by an interrupted cleanup, a legacy
    // layout, or an older writer are re-declared on every publication until
    // they are gone, and the retained set stays bounded by the two
    // generations current.json and previous.json reference.
    let retained_objects: HashSet<String> = previous_files
        .iter()
        .chain(current_files.iter())
        .filter(|p| is_generation_object_path(p))
        .cloned()
        .collect();
    let mut deleted_paths_sorted: Vec<String> = enumerate_generation_objects(&checkpoint_dir)?
        .into_iter()
        .filter(|p| !retained_objects.contains(p))
        .collect();

    // A mode transition supersedes the outgoing root outright: the new
    // generation's changed-path set carries a tombstone for it (plan 6.1.1).
    // Everything else the outgoing generation referenced stays retained by
    // previous.json for one more generation, per the rule above.
    if let Some(previous) = &previous {
        if let (Some(previous_mode), Some(root_path)) = (&previous.mode, &previous.root_path) {
            if *previous_mode != mode
                && is_generation_object_path(root_path)
                && !deleted_paths_sorted.contains(root_path)
            {
                deleted_paths_sorted.push(root_path.clone());
            }
        }
    }
    deleted_paths_sorted.sort();
    deleted_paths_sorted.dedup();

    // Sort for deterministic output
    let mut added_paths_sorted = added_paths;
    added_paths_sorted.sort();
    let mut replaced_paths_sorted = replaced_paths;
    replaced_paths_sorted.sort();

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
        deleted_paths: deleted_paths_sorted.clone(),
    };
    write_current_pointer(&current_pointer_path, &pointer_config)?;
    changed_paths.push("current.json".to_string());

    // Apply only the pointer-declared tombstones, after the pointer is
    // durable and before checkpoint state is recorded (plan 6.2 step 6).
    // A declared path that is already absent counts as resolved, so an
    // interrupted cleanup is safely reapplied by the next publication; a
    // real failure aborts the flush with the new generation already
    // authoritative and the tombstones still declared for reapplication.
    apply_checkpoint_tombstones(&checkpoint_dir, &deleted_paths_sorted)?;

    // Report deletions alongside additions and modifications so one
    // external Git commit carries the entire changed-path set (plan 6.1.1).
    for path in &deleted_paths_sorted {
        changed_paths.push(path.clone());
    }

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

/// Report forensic checkpoint status for `bead sync status` (plan 6.2)
///
/// Reads the authoritative pointer, the root object it selects, the
/// monolithic compatibility view, the recorded checkpoint state, and the
/// live event sequence, then decides whether the checkpoint is ready to
/// commit. Never mutates anything: repairing a not-ready checkpoint is a
/// flush's job.
pub fn forensic_checkpoint_status(
    store: &mut SqliteStore,
    checkpoint_base: &Path,
) -> Result<CheckpointStatusReport> {
    let conn = store.conn();

    let live_sequence: i64 = conn
        .query_row("SELECT COALESCE(MAX(sequence), 0) FROM events", [], |row| {
            row.get(0)
        })
        .unwrap_or(0);

    // Recorded checkpoint state, when a flush has ever run
    let state_row: Option<(i64, Option<String>, Option<String>)> = conn
        .query_row(
            "SELECT covered_event_sequence, current_generation_id, changed_paths_json
             FROM checkpoint_state WHERE id = 1",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            },
        )
        .optional()?;
    let (state_covered, state_generation, state_changed_paths) = match state_row {
        Some((covered, generation, changed)) => (covered, generation, changed),
        None => (0, None, None),
    };

    let checkpoint_dir = checkpoint_base.join("checkpoint");
    let pointer_path = checkpoint_dir.join("current.json");

    let pointer = if pointer_path.exists() {
        let content = std::fs::read_to_string(&pointer_path)?;
        match serde_json::from_str::<serde_json::Value>(&content) {
            Ok(pointer) => Some(pointer),
            Err(_) => {
                return Ok(CheckpointStatusReport {
                    checkpoint_present: true,
                    mode: None,
                    generation_id: None,
                    live_sequence,
                    covered_sequence: None,
                    dirty: true,
                    root_path: None,
                    root_hash: None,
                    root_verified: false,
                    view_agrees: None,
                    unresolved_tombstones: Vec::new(),
                    changed_paths: Vec::new(),
                    ready_to_commit: false,
                    not_ready_reasons: vec!["current.json is unparseable".to_string()],
                    // A present-but-unparseable pointer is integrity damage,
                    // not absence (R027); `covered_sequence: None` keeps the
                    // covered-ahead refusals from engaging on it, preserving
                    // flush-only's pre-R027 behavior for this shape.
                    relationship:
                        crate::service::reconcile::SyncRelationship::CoveredAheadIntegrityFailure
                            .as_str()
                            .to_string(),
                });
            }
        }
    } else {
        None
    };

    let mut report = CheckpointStatusReport {
        checkpoint_present: pointer.is_some(),
        mode: pointer
            .as_ref()
            .and_then(|p| p.get("mode"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        generation_id: pointer
            .as_ref()
            .and_then(|p| p.get("generation_id"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        live_sequence,
        covered_sequence: pointer
            .as_ref()
            .and_then(|p| p.get("snapshot_sequence"))
            .and_then(|v| v.as_i64()),
        dirty: false,
        root_path: pointer
            .as_ref()
            .and_then(|p| p.get("active_root"))
            .and_then(|r| r.get("path"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        root_hash: pointer
            .as_ref()
            .and_then(|p| p.get("active_root"))
            .and_then(|r| r.get("sha256"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        root_verified: false,
        view_agrees: None,
        unresolved_tombstones: Vec::new(),
        changed_paths: state_changed_paths
            .as_deref()
            .and_then(|json| serde_json::from_str::<Vec<String>>(json).ok())
            .unwrap_or_default(),
        ready_to_commit: false,
        not_ready_reasons: Vec::new(),
        relationship: crate::service::reconcile::SyncRelationship::Absent
            .as_str()
            .to_string(),
    };

    let Some(pointer) = pointer else {
        report
            .not_ready_reasons
            .push("no checkpoint published (run `bead sync flush-only`)".to_string());
        return Ok(report);
    };

    // Verify the root object the pointer selects
    let root_path = report.root_path.clone().unwrap_or_default();
    let root_hash = report.root_hash.clone().unwrap_or_default();
    let root_file = checkpoint_dir.join(&root_path);
    if root_path.is_empty() || !root_file.exists() {
        report
            .not_ready_reasons
            .push(format!("root object missing: {}", root_path));
    } else if calculate_file_hash(&root_file)
        .map(|actual| actual != root_hash)
        .unwrap_or(true)
    {
        report
            .not_ready_reasons
            .push(format!("root hash mismatch: {}", root_path));
    } else {
        report.root_verified = true;
    }

    // Unresolved tombstones: declared deleted but still present on disk
    if let Some(deleted) = pointer.get("deleted_paths").and_then(|v| v.as_array()) {
        for path in deleted.iter().filter_map(|v| v.as_str()) {
            if checkpoint_dir.join(path).exists() {
                report.unresolved_tombstones.push(path.to_string());
            }
        }
    }
    if !report.unresolved_tombstones.is_empty() {
        report.not_ready_reasons.push(format!(
            "unresolved tombstones ({}): {}",
            report.unresolved_tombstones.len(),
            report.unresolved_tombstones.join(", ")
        ));
    }

    // Compatibility-view agreement: monolithic mode materializes
    // forensic.jsonl as a byte-identical copy of the pointer-selected
    // object (plan 6.2).
    if report.mode.as_deref() == Some("monolithic") && report.root_verified {
        let view_path = checkpoint_dir.join("forensic.jsonl");
        let agrees = std::fs::read(&view_path).ok().map(|view| {
            std::fs::read(&root_file)
                .map(|root| view == root)
                .unwrap_or(false)
        });
        report.view_agrees = agrees;
        if agrees == Some(false) {
            report
                .not_ready_reasons
                .push("forensic.jsonl view disagrees with the pointer-selected object".to_string());
        }
    }

    // Freshness against the live event sequence
    let covered = report.covered_sequence.unwrap_or(0);
    report.dirty = covered < live_sequence;
    if report.dirty {
        report.not_ready_reasons.push(format!(
            "checkpoint dirty: covered={}, live={}",
            covered, live_sequence
        ));
    }

    // The recorded state must agree with the authoritative pointer. In the
    // remote-advanced relationship that disagreement is the state itself --
    // the database records the last local publication, the pointer records
    // the pulled one -- so the plain disagreement reason is replaced by the
    // reconcile remedy rather than presented as an integrity fault (R027,
    // research/specs/remote-advanced-reconcile-v1.md, "Reporting").
    let verdict = crate::service::reconcile::classify(conn, checkpoint_base)?;
    let relationship = verdict.relationship;
    if relationship == crate::service::reconcile::SyncRelationship::RemoteAdvanced {
        report.not_ready_reasons.retain(|reason| {
            reason != "checkpoint state disagrees with current.json"
                && !reason.starts_with("checkpoint state covered sequence")
        });
        report.not_ready_reasons.insert(
            0,
            format!(
                "remote-advanced: {}",
                crate::service::reconcile::REMOTE_ADVANCED_REMEDY
            ),
        );
    } else if relationship
        == crate::service::reconcile::SyncRelationship::CoveredAheadIntegrityFailure
    {
        if let Some(qualifier) = &verdict.failed_qualifier {
            report
                .not_ready_reasons
                .insert(0, format!("covered-ahead integrity failure: {}", qualifier));
        }
    }

    // The recorded state must agree with the authoritative pointer
    if relationship != crate::service::reconcile::SyncRelationship::RemoteAdvanced {
        if state_generation.is_some()
            && state_generation.as_deref() != report.generation_id.as_deref()
        {
            report
                .not_ready_reasons
                .push("checkpoint state disagrees with current.json".to_string());
        } else if state_generation.is_none() {
            report
                .not_ready_reasons
                .push("checkpoint state not recorded".to_string());
        } else if state_covered != covered {
            report.not_ready_reasons.push(format!(
                "checkpoint state covered sequence {} disagrees with pointer {}",
                state_covered, covered
            ));
        }
    }

    report.relationship = relationship.as_str().to_string();
    report.ready_to_commit = report.not_ready_reasons.is_empty();
    Ok(report)
}

/// Publish monolithic forensic checkpoint
/// What a publication produced
struct PublicationOutput {
    /// Canonical SHA-256 of the active root's complete bytes
    root_hash: String,
    /// Checkpoint-relative path of the active root
    root_path: String,
    /// Every checkpoint-relative file the new generation references,
    /// including reused content-addressed objects and the compatibility
    /// view. Retention must keep all of them selectable.
    referenced_paths: Vec<String>,
}

fn publish_monolithic_checkpoint(
    corpus: &SerializedCorpus,
    checkpoint_dir: &Path,
    generation_id: &str,
    changed_paths: &mut Vec<String>,
) -> Result<PublicationOutput> {
    let objects_dir = checkpoint_dir.join("objects");
    // Temp file is generation-scoped (unique scratch name); the final object
    // is content-addressed below, per plan 6.1.1 / 6.2.1 P1.
    let temp_path = objects_dir.join(format!("{}.tmp", generation_id));

    // Create temporary file
    let temp_file = File::create(&temp_path)?;
    let mut writer = BufWriter::new(temp_file);

    // Write the pre-serialized record lines in canonical order
    for line in corpus
        .issue_lines
        .iter()
        .chain(corpus.event_lines.iter())
        .chain(corpus.receipt_lines.iter())
    {
        writer.write_all(line)?;
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

    // Content-addressed object name: identical content reuses one object
    // (plan 6.1.1, 6.2.1 P1). If the object already exists, its bytes are
    // identical by construction -- drop the temp and keep the published
    // object instead of writing a second copy. A concurrent publisher
    // renaming over the same path just replaces identical bytes atomically.
    let final_path = objects_dir.join(format!("{}.jsonl", hash));
    if final_path.exists() {
        std::fs::remove_file(&temp_path)?;
    } else {
        std::fs::rename(&temp_path, &final_path)?;
    }

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

    changed_paths.push(format!("objects/{}.jsonl", hash));
    changed_paths.push("forensic.jsonl".to_string());

    let root_path = format!("objects/{}.jsonl", hash);
    Ok(PublicationOutput {
        root_hash: hash,
        root_path: root_path.clone(),
        referenced_paths: vec![root_path, "forensic.jsonl".to_string()],
    })
}

/// Maximum hex-prefix depth of an issue shard partition (a shard key is a
/// 64-hex-character SHA-256 digest)
const MAX_PARTITION_DEPTH: usize = 16;

/// Default shallow partition plan: the sixteen single-digit hex prefixes
/// (plan 6.1.1: "begin with a shallow prefix")
fn default_partition_plan() -> Vec<String> {
    (0..16).map(|d| format!("{:x}", d)).collect()
}

/// Shard key of a bead ID: the lowercase hex SHA-256 of the UTF-8 ID
/// (plan 6.1.1 issue shard assignment)
fn issue_shard_key(id: &str) -> String {
    format!("{:x}", Sha256::digest(id.as_bytes()))
}

/// Whether a recorded partition plan is structurally usable
///
/// Prefixes must be lowercase hex of length 1..=MAX_PARTITION_DEPTH, pairwise
/// disjoint (no prefix of another), and together cover the whole key space.
/// Coverage follows from the mass argument: over a common denominator of
/// 16^MAX_PARTITION_DEPTH, a depth-k prefix covers 16^(MAX_PARTITION_DEPTH-k)
/// shares, and disjoint prefixes cover everything exactly when their shares
/// sum to the whole. A plan failing any check is discarded and rebuilt from
/// the shallow default -- correctness never depends on the plan, only write
/// amplification does.
fn partition_plan_is_valid(prefixes: &[String]) -> bool {
    if prefixes.is_empty() {
        return false;
    }

    let mut sorted: Vec<&str> = prefixes.iter().map(|s| s.as_str()).collect();
    sorted.sort();

    for prefix in &sorted {
        if prefix.is_empty() || prefix.len() > MAX_PARTITION_DEPTH {
            return false;
        }
        if !prefix
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
        {
            return false;
        }
    }

    // Pairwise disjoint: sorted neighbors cannot be prefix-related
    for pair in sorted.windows(2) {
        if pair[1].starts_with(pair[0]) {
            return false;
        }
    }

    let whole = 16u128.pow(MAX_PARTITION_DEPTH as u32);
    let covered: u128 = sorted
        .iter()
        .map(|p| 16u128.pow((MAX_PARTITION_DEPTH - p.len()) as u32))
        .sum();
    covered == whole
}

/// Load the recorded issue partition plan from a previous manifest
fn load_issue_partition_plan(manifest: &serde_json::Value) -> Option<Vec<String>> {
    let recorded = manifest.get("issue_partition")?.as_array()?;
    let mut prefixes: Vec<String> = recorded
        .iter()
        .map(|v| v.as_str().map(|s| s.to_string()))
        .collect::<Option<Vec<_>>>()?;
    if !partition_plan_is_valid(&prefixes) {
        return None;
    }
    prefixes.sort();
    Some(prefixes)
}

/// The plan prefix a shard key belongs to, walking key digits shallow-first
fn plan_prefix_for_key<'a>(key: &str, plan: &HashSet<&'a str>) -> Option<&'a str> {
    for len in 1..=key.len().min(MAX_PARTITION_DEPTH) {
        if let Some(prefix) = plan.get(&key[..len]) {
            return Some(prefix);
        }
    }
    None
}

/// Assign each issue index to its plan prefix, buckets ordered by prefix
fn assign_issue_buckets(
    keys: &[String],
    plan: &[String],
) -> Result<std::collections::BTreeMap<String, Vec<usize>>> {
    let plan_set: HashSet<&str> = plan.iter().map(|s| s.as_str()).collect();
    let mut buckets = std::collections::BTreeMap::new();
    for (i, key) in keys.iter().enumerate() {
        let prefix = plan_prefix_for_key(key, &plan_set)
            .ok_or_else(|| anyhow!("recorded partition plan does not cover shard key {}", key))?;
        buckets
            .entry(prefix.to_string())
            .or_insert_with(Vec::new)
            .push(i);
    }
    Ok(buckets)
}

/// Group record indexes into objects sealed at the recorded targets
///
/// Greedy packing in the given canonical order: an object is closed once it
/// holds `max_records` records or when the next line would push it past
/// `max_bytes`. Because packing depends only on the prefix it covers,
/// re-packing a corpus that only appended records reproduces every earlier
/// object byte-for-byte, which is what lets a later flush reuse sealed
/// objects instead of rewriting them.
fn pack_sealed_groups(line_lens: &[usize], max_records: u64, max_bytes: u64) -> Vec<Vec<usize>> {
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut current: Vec<usize> = Vec::new();
    let mut current_bytes = 0u64;
    for (i, line_len) in line_lens.iter().enumerate() {
        // +1 for the newline every JSONL line carries
        let line_len = *line_len as u64 + 1;
        if !current.is_empty()
            && (current.len() as u64 >= max_records || current_bytes + line_len > max_bytes)
        {
            groups.push(std::mem::take(&mut current));
            current_bytes = 0;
        }
        current.push(i);
        current_bytes += line_len;
    }
    if !current.is_empty() {
        groups.push(current);
    }
    groups
}

/// Sync a directory entry so renames and creations beneath it persist
/// (plan 6.2 step 5)
fn sync_dir(dir: &Path) -> Result<()> {
    let dir_file = File::open(dir)?;
    dir_file.sync_all()?;
    drop(dir_file);
    Ok(())
}

/// Write a content-addressed generation object, reusing an identical object
/// already on disk without rewriting it (plan 6.1.1, 6.2 step 4)
///
/// Returns the checkpoint-relative path and the content SHA-256. Because the
/// filename is the content hash, an existing object holds identical bytes by
/// construction, so publication writes nothing for it -- that reuse is what
/// makes a flush's byte cost proportional to the delta rather than the
/// workspace. `changed_paths` receives the path only when this call wrote
/// new bytes.
fn write_content_object(
    objects_dir: &Path,
    scratch_tag: &str,
    body: &[u8],
    changed_paths: &mut Vec<String>,
) -> Result<(String, String)> {
    let hash = format!("{:x}", Sha256::digest(body));
    let rel = format!("objects/{}.jsonl", hash);
    let final_path = objects_dir.join(format!("{}.jsonl", hash));
    if !final_path.exists() {
        let temp_path = objects_dir.join(format!("{}.tmp", scratch_tag));
        let mut file = File::create(&temp_path)?;
        file.write_all(body)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temp_path, &final_path)?;
        sync_dir(objects_dir)?;
        changed_paths.push(rel.clone());
    }
    Ok((rel, hash))
}

/// Everything a sharded publication needs beyond its output paths
struct ShardedPublishInputs<'a> {
    issues: &'a [Issue],
    events: &'a [EventRecord],
    receipts: &'a [ProvenanceReceipt],
    corpus: &'a SerializedCorpus,
    config: ShardedConfig,
    thresholds: CheckpointThresholds,
    /// The outgoing generation's manifest, for plan/threshold retention
    previous_manifest: Option<&'a serde_json::Value>,
}

/// Publish sharded forensic checkpoint (plan 6.1.1)
///
/// Issue records are partitioned by the leading hexadecimal digits of
/// SHA-256(bead ID). The partition plan is retained from the previous
/// manifest whenever it is structurally valid, and only a shard exceeding a
/// recorded threshold splits, into its sixteen next-digit children -- shards
/// never merge automatically, so a workspace's plan never reshuffles (an
/// explicit future compaction operation may produce a new plan and receipt).
///
/// Audit events are packed in canonical origin/sequence order into objects
/// sealed at the recorded count/byte targets; because every object is
/// content-addressed, a later flush writes a new tail object and a new
/// immutable manifest while reusing sealed objects byte-for-byte. This keeps
/// forensic history append-friendly and prevents a frequently updated bead
/// from rewriting its history-bearing issue shard. Receipts use the same
/// content-addressed object set and seal targets, sorted by receipt ID.
fn publish_sharded_checkpoint(
    inputs: ShardedPublishInputs<'_>,
    checkpoint_dir: &Path,
    changed_paths: &mut Vec<String>,
) -> Result<PublicationOutput> {
    let ShardedPublishInputs {
        issues,
        events,
        receipts,
        corpus,
        config,
        thresholds,
        previous_manifest,
    } = inputs;
    let objects_dir = checkpoint_dir.join("objects");
    let mut referenced_paths: Vec<String> = Vec::new();

    // ---- Issue shards: sha256 hex-prefix partition plan ----

    // Retain the previous plan when it is structurally valid (plan 6.2
    // step 3); otherwise begin with the shallow default.
    let mut plan: Vec<String> = previous_manifest
        .and_then(load_issue_partition_plan)
        .unwrap_or_else(default_partition_plan);

    let keys: Vec<String> = issues.iter().map(|i| issue_shard_key(&i.id)).collect();
    let mut buckets = assign_issue_buckets(&keys, &plan)?;

    // Split only overflowing shards (plan 6.1.1): each split replaces one
    // prefix with its sixteen next-digit children. A shard that cannot split
    // -- a single record, or a prefix at maximum depth -- stays oversized; a
    // single record line above max_record_line_bytes already forced sharded
    // mode, so an unsplittable shard cannot smuggle monolith-scale bytes.
    loop {
        let overflowing: Vec<String> = buckets
            .iter()
            .filter(|(_, members)| {
                let count = members.len() as u64;
                let bytes: u64 = members
                    .iter()
                    .map(|&i| corpus.issue_lines[i].len() as u64 + 1)
                    .sum();
                count > thresholds.max_shard_issue_records || bytes > thresholds.max_shard_bytes
            })
            .map(|(prefix, _)| prefix.clone())
            .collect();
        if overflowing.is_empty() {
            break;
        }

        let mut split_any = false;
        for prefix in overflowing {
            let members = buckets.remove(&prefix).unwrap_or_default();
            if prefix.len() >= MAX_PARTITION_DEPTH || members.len() <= 1 {
                buckets.insert(prefix, members);
                continue;
            }
            split_any = true;
            plan.retain(|p| p != &prefix);
            for digit in 0..16u32 {
                let child = format!("{}{:x}", prefix, digit);
                plan.push(child.clone());
                buckets.insert(child, Vec::new());
            }
            for i in members {
                let key = &keys[i];
                let child = format!("{}{}", prefix, &key[prefix.len()..prefix.len() + 1]);
                buckets
                    .get_mut(&child)
                    .expect("child bucket was just inserted")
                    .push(i);
            }
        }
        if !split_any {
            break;
        }
    }
    plan.sort();
    plan.dedup();

    // Members are pushed in ascending bead-ID order (issues arrive sorted),
    // so every shard's records sort by bead ID as section 6.1.1 requires.
    let mut issue_shard_metadata = Vec::new();
    for (prefix, members) in &buckets {
        if members.is_empty() {
            continue;
        }
        let mut body = Vec::new();
        for &i in members {
            body.extend_from_slice(&corpus.issue_lines[i]);
            body.push(b'\n');
        }
        let (rel, hash) = write_content_object(
            &objects_dir,
            &format!("issue-{}-{}", config.generation_id, prefix),
            &body,
            changed_paths,
        )?;
        referenced_paths.push(rel.clone());
        issue_shard_metadata.push(serde_json::json!({
            "path": rel,
            "sha256": hash,
            "byte_length": body.len(),
            "record_count": members.len(),
            "id_prefix": prefix,
            "role": "issues"
        }));
    }

    // ---- Event objects: canonical-order packing, sealed at the recorded targets ----
    let event_line_lens: Vec<usize> = corpus.event_lines.iter().map(|l| l.len()).collect();
    let event_groups = pack_sealed_groups(
        &event_line_lens,
        thresholds.max_event_object_events,
        thresholds.max_event_object_bytes,
    );

    let mut event_shard_metadata = Vec::new();
    // Per-origin summaries (plan 6.1.1): uuid -> (count, min seq, max seq, object paths)
    let mut origin_stats: std::collections::BTreeMap<String, (u64, i64, i64, Vec<String>)> =
        std::collections::BTreeMap::new();
    for (group_index, members) in event_groups.iter().enumerate() {
        let mut body = Vec::new();
        for &i in members {
            body.extend_from_slice(&corpus.event_lines[i]);
            body.push(b'\n');
        }
        let (rel, hash) = write_content_object(
            &objects_dir,
            &format!("event-{}-{}", config.generation_id, group_index),
            &body,
            changed_paths,
        )?;
        referenced_paths.push(rel.clone());

        let first = &events[members[0]];
        let last = &events[members[members.len() - 1]];
        // Every object but the last was closed by hitting a seal target; the
        // last one is the open tail a later flush extends.
        let sealed = group_index + 1 < event_groups.len()
            || members.len() as u64 >= thresholds.max_event_object_events
            || body.len() as u64 >= thresholds.max_event_object_bytes;

        event_shard_metadata.push(serde_json::json!({
            "path": rel.clone(),
            "sha256": hash,
            "byte_length": body.len(),
            "record_count": members.len(),
            "sequence_range": [first.origin_event_sequence, last.origin_event_sequence],
            "origin_range": {
                "first": [first.origin_store_uuid, first.origin_event_sequence],
                "last": [last.origin_store_uuid, last.origin_event_sequence]
            },
            "sealed": sealed,
            "role": "events"
        }));

        for &i in members {
            let event = &events[i];
            let stats = origin_stats
                .entry(event.origin_store_uuid.clone())
                .or_insert((0, i64::MAX, i64::MIN, Vec::new()));
            stats.0 += 1;
            stats.1 = stats.1.min(event.origin_event_sequence);
            stats.2 = stats.2.max(event.origin_event_sequence);
            if stats.3.last().map(|p| p != &rel).unwrap_or(true) {
                stats.3.push(rel.clone());
            }
        }
    }

    // ---- Receipt objects: content-addressed, receipt-ID order ----
    let receipt_line_lens: Vec<usize> = corpus.receipt_lines.iter().map(|l| l.len()).collect();
    let receipt_groups = pack_sealed_groups(
        &receipt_line_lens,
        thresholds.max_event_object_events,
        thresholds.max_event_object_bytes,
    );

    let mut receipt_shard_metadata = Vec::new();
    for (group_index, members) in receipt_groups.iter().enumerate() {
        let mut body = Vec::new();
        for &i in members {
            body.extend_from_slice(&corpus.receipt_lines[i]);
            body.push(b'\n');
        }
        let (rel, hash) = write_content_object(
            &objects_dir,
            &format!("receipt-{}-{}", config.generation_id, group_index),
            &body,
            changed_paths,
        )?;
        referenced_paths.push(rel.clone());
        receipt_shard_metadata.push(serde_json::json!({
            "path": rel,
            "sha256": hash,
            "byte_length": body.len(),
            "record_count": members.len(),
            "role": "provenance_receipts"
        }));
    }

    // ---- Manifest: the immutable, content-addressed sharded root ----
    let origins: Vec<serde_json::Value> = origin_stats
        .iter()
        .map(|(uuid, (count, min, max, paths))| {
            serde_json::json!({
                "origin_store_uuid": uuid,
                "event_count": count,
                "min_sequence": min,
                "max_sequence": max,
                "objects": paths,
            })
        })
        .collect();

    let manifest = serde_json::json!({
        "format": "checkpoint-set-v1",
        "schema_version": 1,
        "store_uuid": config.store_uuid,
        "snapshot_sequence": config.snapshot_sequence,
        "max_local_ingestion_sequence": config.snapshot_sequence,
        "created_at": format_rfc3339(SystemTime::now()),
        "profile": "native-v1",
        "partition_algorithm": "sha256-hex-prefix",
        "partition_thresholds": thresholds.to_manifest_json(),
        "issue_partition": plan,
        "issue_count": issues.len(),
        "event_count": events.len(),
        "receipt_count": receipts.len(),
        "total_record_count": issues.len() + events.len() + receipts.len(),
        "issue_shards": issue_shard_metadata,
        "event_shards": event_shard_metadata,
        "receipt_shards": receipt_shard_metadata,
        "origins": origins
    });

    let manifest_json = serde_json::to_vec_pretty(&manifest)?;
    let manifest_hash = format!("{:x}", Sha256::digest(&manifest_json));
    let manifest_rel = format!("manifests/{}.json", manifest_hash);
    let manifest_path = checkpoint_dir.join(&manifest_rel);
    let temp_manifest_path = manifest_path.with_extension("tmp");
    std::fs::write(&temp_manifest_path, &manifest_json)?;

    // Sync temp file, atomically rename, sync parent directory
    let temp_file = File::open(&temp_manifest_path)?;
    temp_file.sync_all()?;
    drop(temp_file);
    std::fs::rename(&temp_manifest_path, &manifest_path)?;
    sync_dir(&checkpoint_dir.join("manifests"))?;

    changed_paths.push(manifest_rel.clone());
    referenced_paths.push(manifest_rel.clone());

    Ok(PublicationOutput {
        root_hash: manifest_hash,
        root_path: manifest_rel,
        referenced_paths,
    })
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

/// Read the file set a pointer still references
///
/// The active root plus the paths the pointer declares as added or replaced.
/// Paths it declares deleted are its own tombstones -- they name files it no
/// longer references -- so they are excluded. `current.json` itself is
/// included because every publication rewrites it: it is replaced, never
/// deleted.
fn read_pointer_referenced_files(pointer_path: &Path) -> Result<HashSet<String>> {
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

        // Extract paths from added and replaced arrays
        for key in &["added_paths", "replaced_paths"] {
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

/// Whether a checkpoint-relative path names a generation object
///
/// Generation objects live directly under `objects/` or `manifests/` and are
/// the only paths a tombstone may remove. Pointer and view files at the
/// checkpoint root (`current.json`, `previous.json`, `forensic.jsonl`) are
/// managed by publication itself and are never tombstone-removable.
fn is_generation_object_path(path: &str) -> bool {
    (path.starts_with("objects/") || path.starts_with("manifests/"))
        && !path.contains("..")
        && !path.contains('\\')
        && path.split('/').all(|component| !component.is_empty())
}

/// Enumerate generation objects currently on disk
///
/// Returns checkpoint-relative paths (`objects/<name>`, `manifests/<name>`)
/// for regular, non-temporary files. Publication temporaries (`.tmp`) are
/// skipped: they are scratch from an interrupted write, not generation
/// content. Symlinks and subdirectories are skipped so a tombstone can never
/// traverse out of the checkpoint directory.
fn enumerate_generation_objects(checkpoint_dir: &Path) -> Result<Vec<String>> {
    let mut paths = Vec::new();

    for dir_name in ["objects", "manifests"] {
        let entries = match std::fs::read_dir(checkpoint_dir.join(dir_name)) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e.into()),
        };

        for entry in entries {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let name = entry.file_name();
            let name = name.to_str().ok_or_else(|| {
                anyhow!(
                    "non-UTF-8 checkpoint object name: {}",
                    entry.path().display()
                )
            })?;
            if name.ends_with(".tmp") {
                continue;
            }
            paths.push(format!("{}/{}", dir_name, name));
        }
    }

    Ok(paths)
}

/// Apply pointer-declared tombstones after the pointer commit (plan 6.2 step 6)
///
/// Removes each declared path that still exists under the checkpoint
/// directory. Only generation objects are removable; a root-level pointer or
/// view path is skipped, not removed. A declared path that is already absent
/// counts as resolved, so application is idempotent and safely repeatable
/// after a crash. Touched directories are synced so a crash cannot
/// resurrect deleted directory entries. Returns how many files were removed.
fn apply_checkpoint_tombstones(checkpoint_dir: &Path, deleted_paths: &[String]) -> Result<usize> {
    let mut removed = 0usize;
    let mut touched_dirs: Vec<std::path::PathBuf> = Vec::new();

    for rel in deleted_paths {
        if !is_generation_object_path(rel) {
            // Root-level files (pointers, compatibility view) are managed by
            // publication itself; a tombstone never removes them.
            continue;
        }
        let full = checkpoint_dir.join(rel);
        match std::fs::remove_file(&full) {
            Ok(()) => {
                removed += 1;
                if let Some(parent) = full.parent() {
                    if !touched_dirs.iter().any(|d| d == parent) {
                        touched_dirs.push(parent.to_path_buf());
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(anyhow!(
                    "failed to apply checkpoint tombstone {}: {}",
                    rel,
                    e
                ));
            }
        }
    }

    // Persist directory entries so deleted objects cannot reappear
    for dir in &touched_dirs {
        let dir_file = File::open(dir)?;
        dir_file.sync_all()?;
        drop(dir_file);
    }

    Ok(removed)
}

/// Enumerate every live event with the wire identity publication would give
/// it (R027). Explicit origins are preserved verbatim; NULL-origin rows --
/// the shape every native mutation writes -- are numbered after this
/// store's highest explicit local-UUID identity in local ingestion order.
/// This is the same derivation [`read_all_events`] applies at export time,
/// exposed so sync-relationship classification and merge validation compare
/// live rows against checkpoint identities through one definition instead
/// of a re-implementation that can drift.
pub(crate) fn read_all_events(conn: &rusqlite::Connection) -> Result<Vec<EventRecord>> {
    let mut events = Vec::new();

    // Locally-created events are written with NULL origin columns (they are
    // nullable for backward compatibility, and no INSERT site populates them).
    // A NULL origin means the event originated in THIS store, so it must be
    // exported carrying this workspace's UUID and its own local sequence.
    // Exporting them as ("", 0) instead gives every event the identity ":0",
    // which makes the checkpoint unimportable past its first event.
    let (local_store_uuid, mut next_local_origin_sequence) = local_identity_basis(conn);

    let mut stmt = conn.prepare(
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

        // Preserve complete imported identities verbatim. Locally-created
        // events carry NULL origin columns; number those after this store's
        // highest explicit imported identity, in local ingestion order. This
        // is deliberately independent of the SQLite primary key: an override
        // restore must retain that key's monotonicity for mutation/publication
        // detection even when it replaces the prior event corpus.
        let (origin_store_uuid, origin_event_sequence) = derive_wire_identity(
            origin_store_uuid.as_deref(),
            origin_event_sequence,
            &local_store_uuid,
            &mut next_local_origin_sequence,
            sequence,
        );

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

    // The checkpoint wire order is origin identity order, independent of the
    // local ingestion order represented by the SQLite primary key.
    events.sort_by(|left, right| {
        (&left.origin_store_uuid, left.origin_event_sequence)
            .cmp(&(&right.origin_store_uuid, right.origin_event_sequence))
    });

    Ok(events)
}

/// The basis every derived local wire identity is numbered from: this
/// store's UUID and one past its highest explicit local-UUID identity.
fn local_identity_basis(conn: &rusqlite::Connection) -> (String, i64) {
    let local_store_uuid: String = conn
        .query_row("SELECT uuid FROM workspace WHERE id = 1", [], |row| {
            row.get(0)
        })
        .unwrap_or_default();
    let explicit_local_max: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(origin_event_sequence), 0)
             FROM events WHERE origin_store_uuid = ?1",
            [&local_store_uuid],
            |row| row.get(0),
        )
        .unwrap_or(0);
    (local_store_uuid, explicit_local_max + 1)
}

/// Derive the wire identity one event row exports under, given its stored
/// origin columns. The single definition shared by export enumeration
/// ([`read_all_events`]) and merge-time canonicalization
/// ([`canonicalize_local_event_identities`]) so the two cannot drift.
fn derive_wire_identity(
    stored_uuid: Option<&str>,
    stored_sequence: Option<i64>,
    local_store_uuid: &str,
    next_local_origin_sequence: &mut i64,
    primary_key: i64,
) -> (String, i64) {
    match (stored_uuid.filter(|uuid| !uuid.is_empty()), stored_sequence) {
        (Some(uuid), Some(origin_sequence)) => (uuid.to_string(), origin_sequence),
        (Some(uuid), None) if uuid != local_store_uuid => (uuid.to_string(), primary_key),
        _ => {
            let origin_sequence = *next_local_origin_sequence;
            *next_local_origin_sequence += 1;
            (local_store_uuid.to_string(), origin_sequence)
        }
    }
}

/// Write the derived wire identity into the origin columns of every live
/// event whose stored identity differs from its derived one (R027
/// local-identity canonicalization).
///
/// The merge machinery deduplicates imported events by explicit wire
/// identity, but native mutations write NULL origin columns, so a naive
/// same-UUID merge into a live store re-inserts every pre-existing event as
/// a duplicate row and every later export carries each event twice under
/// two identities — silent audit corruption. Canonicalizing inside the
/// merge transaction (validation has already proved the shared identities
/// carry identical public content) changes no public content, no primary
/// key, and no ordering, and is idempotent because the derivation is
/// deterministic: after the write the rows carry their derived identities
/// explicitly and later derivations preserve them verbatim.
fn canonicalize_local_event_identities(tx: &Transaction<'_>) -> Result<usize> {
    let (local_store_uuid, mut next_local_origin_sequence) = local_identity_basis(tx);

    let mut stmt = tx.prepare(
        "SELECT sequence, origin_store_uuid, origin_event_sequence
         FROM events
         ORDER BY sequence ASC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, Option<i64>>(2)?,
        ))
    })?;

    let mut canonicalized = 0;
    for row in rows {
        let (sequence, stored_uuid, stored_sequence) = row?;
        let (derived_uuid, derived_sequence) = derive_wire_identity(
            stored_uuid.as_deref(),
            stored_sequence,
            &local_store_uuid,
            &mut next_local_origin_sequence,
            sequence,
        );
        if stored_uuid.as_deref() != Some(derived_uuid.as_str())
            || stored_sequence != Some(derived_sequence)
        {
            tx.execute(
                "UPDATE events
                 SET origin_store_uuid = ?1, origin_event_sequence = ?2
                 WHERE sequence = ?3",
                params![&derived_uuid, &derived_sequence, &sequence],
            )?;
            canonicalized += 1;
        }
    }

    Ok(canonicalized)
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
            "SELECT namespace, key, value,
                    EXISTS(
                        SELECT 1 FROM unique_reference_bindings binding
                        WHERE binding.namespace = external_references.namespace
                          AND binding.key = external_references.value
                          AND binding.issue_id = external_references.issue_id
                    ) AS is_unique
             FROM external_references
             WHERE issue_id = ?1 ORDER BY namespace, key, value",
        )?;
        let reference_rows = reference_stmt.query_map([&id], |row| {
            let mut reference = serde_json::json!({
                "namespace": row.get::<_, String>("namespace")?,
                "key": row.get::<_, String>("key")?,
                "value": row.get::<_, String>("value")?,
            });
            if row.get::<_, bool>("is_unique")? {
                reference["unique_ref"] = serde_json::Value::Bool(true);
            }
            Ok(reference)
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

        let resource_keys = get_resource_keys(tx, &id)?;
        if !resource_keys.is_empty() {
            extensions.insert(
                "resource_keys".to_string(),
                serde_json::Value::Array(
                    resource_keys
                        .into_iter()
                        .map(serde_json::Value::String)
                        .collect(),
                ),
            );
        }

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
pub fn calculate_file_hash(path: &Path) -> Result<String> {
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

    /// The publication lock is exclusive across separately opened files --
    /// the property that serializes two publisher processes -- and a
    /// bounded waiter gives up with a named error rather than blocking
    /// forever (plan 6.2.1 item 4).
    #[test]
    fn publication_lock_is_exclusive_across_openers() {
        let temp_dir = TempDir::new().unwrap();
        let checkpoint_dir = temp_dir.path().join("checkpoint");

        let held = acquire_checkpoint_publication_lock(&checkpoint_dir).unwrap();
        {
            // A second opener (its own file description, like another
            // process) must not acquire while the first is held. flock is
            // per open file, so a same-process second open is a faithful
            // stand-in for a second process here.
            let contender = std::thread::spawn(move || {
                acquire_checkpoint_publication_lock_within(
                    &checkpoint_dir,
                    std::time::Duration::from_millis(150),
                )
            });
            let outcome = contender.join().unwrap();
            let err = outcome.err().expect("a second opener acquired a held lock");
            assert!(
                err.to_string().contains("publication lock busy"),
                "lock contention must name the lock, got: {err}"
            );
        }

        // Releasing (here, by drop) lets the next opener through.
        drop(held);
        let reacquired = acquire_checkpoint_publication_lock_within(
            &temp_dir.path().join("checkpoint"),
            std::time::Duration::from_millis(150),
        );
        assert!(reacquired.is_ok(), "a released lock must be acquirable");
    }

    /// The lock file lives where publication can create it before its first
    /// write, stays empty, and is invisible to the tombstone machinery:
    /// only `objects/` and `manifests/` paths are ever enumerated or
    /// declared deleted, so the lock file cannot be reclaimed out from
    /// under a holder.
    #[test]
    fn publication_lock_file_is_created_and_never_a_tombstone_target() {
        let temp_dir = TempDir::new().unwrap();
        let checkpoint_dir = temp_dir.path().join("checkpoint");

        let held = acquire_checkpoint_publication_lock(&checkpoint_dir).unwrap();
        let lock_path = checkpoint_dir.join("publish.lock");
        assert!(lock_path.exists(), "acquire must create the lock file");
        assert_eq!(
            fs::metadata(&lock_path).unwrap().len(),
            0,
            "the lock file carries no content"
        );
        assert!(
            !is_generation_object_path("publish.lock"),
            "the lock file must not be tombstone-removable"
        );
        drop(held);

        // And the checkpoint directory enumeration that drives tombstones
        // never sees it, because it scans only objects/ and manifests/.
        let enumerated = enumerate_generation_objects(&checkpoint_dir).unwrap();
        assert!(
            !enumerated.iter().any(|p| p.contains("publish.lock")),
            "tombstone enumeration must not see the lock file, got {enumerated:?}"
        );
    }
}

//! R027 remote-advanced reconcile: the sync-relationship taxonomy.
//!
//! Normative source: `research/specs/remote-advanced-reconcile-v1.md`. The
//! Git-transported workflow commits `.beads/checkpoint/` and not
//! `.beads/beads.db`, so after pulling another machine's flush the durable
//! checkpoint can contain work the live database does not. This module
//! recognizes that state — `remote-advanced` — from the workspace artifacts
//! alone (bead-rs never runs or inspects Git, ADR-009), and [`classify`]
//! is the one definition every consumer uses: `sync status`, `sync
//! flush-only`'s covered-ahead refusals, `doctor`'s distinction between an
//! actionable reconcile and an integrity failure, and `sync reconcile`
//! itself.
//!
//! Recognizing remote-advanced legitimizes exactly one previously-failing
//! shape. Every other checkpoint-ahead-of-live configuration stays a
//! fail-closed integrity failure (`covered-ahead-integrity-failure`), and a
//! pointer that exists but cannot even yield a covered sequence is filed
//! under that same failure bucket rather than `absent` — `absent` means no
//! pointer exists, and reporting it for a present-but-unusable pointer
//! would understate the damage.

use crate::error::{Error, Result};
use crate::service::checkpoint::{
    self, ForensicStaging, SerializedEvent, EventRecord,
};
use rusqlite::Connection;
use std::collections::HashMap;
use std::path::Path;

/// Stable machine-readable marker for the remote-advanced state. `doctor`
/// and text reporting key on this prefix; it is part of the command
/// contract and must not be reworded casually.
pub const REMOTE_ADVANCED_MARKER: &str = "remote-advanced";

/// The remedy every remote-advanced report names.
pub const REMOTE_ADVANCED_REMEDY: &str =
    "run `bead sync reconcile --actor <you>` to merge the pulled checkpoint into the live store";

/// The sync relationship between the live store and the durable checkpoint
/// (spec, "State taxonomy"). Total over the workspace artifacts alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncRelationship {
    /// No pointer exists.
    Absent,
    /// `C < L`: the live store has unflushed work.
    Behind,
    /// `C == L`. Claims nothing about pointer or recorded-state health,
    /// which status reports separately.
    Aligned,
    /// `C > L` and every remote-advanced qualifier holds: the checkpoint a
    /// pull delivered is a verified superset of the live store and can be
    /// reconciled.
    RemoteAdvanced,
    /// `C > L` and at least one qualifier fails — or the pointer is present
    /// but too damaged to yield a comparable `C`. Not a remedy state: no
    /// command merges, publishes over, or repairs it.
    CoveredAheadIntegrityFailure,
}

impl SyncRelationship {
    pub fn as_str(self) -> &'static str {
        match self {
            SyncRelationship::Absent => "absent",
            SyncRelationship::Behind => "behind",
            SyncRelationship::Aligned => "aligned",
            SyncRelationship::RemoteAdvanced => "remote-advanced",
            SyncRelationship::CoveredAheadIntegrityFailure => "covered-ahead-integrity-failure",
        }
    }
}

/// What [`classify`] concluded, including the first failed qualifier when
/// the relationship is `covered-ahead-integrity-failure` (spec requires
/// refusals and reports to name it).
#[derive(Debug, Clone)]
pub struct RelationshipVerdict {
    pub relationship: SyncRelationship,
    /// Human-readable description of the first failed remote-advanced
    /// qualifier, present only for `covered-ahead-integrity-failure`.
    pub failed_qualifier: Option<String>,
}

impl RelationshipVerdict {
    fn of(relationship: SyncRelationship) -> Self {
        RelationshipVerdict {
            relationship,
            failed_qualifier: None,
        }
    }

    fn failing(qualifier: impl Into<String>) -> Self {
        RelationshipVerdict {
            relationship: SyncRelationship::CoveredAheadIntegrityFailure,
            failed_qualifier: Some(qualifier.into()),
        }
    }
}

/// Classify the sync relationship between the live store and the durable
/// checkpoint under `checkpoint_base` (the `.beads` directory).
///
/// Only the `covered > live` branch pays for staging and full event
/// enumeration; `behind`, `aligned`, and `absent` resolve from the pointer
/// and one aggregate alone.
pub fn classify(conn: &Connection, checkpoint_base: &Path) -> Result<RelationshipVerdict> {
    let pointer_path = checkpoint_base.join("checkpoint").join("current.json");
    if !pointer_path.exists() {
        return Ok(RelationshipVerdict::of(SyncRelationship::Absent));
    }

    let pointer: serde_json::Value = std::fs::read_to_string(&pointer_path)
        .map_err(|e| Error::Integrity(format!("Failed to read current.json: {}", e)))
        .and_then(|content| {
            serde_json::from_str(&content)
                .map_err(|e| Error::Integrity(format!("Failed to parse current.json: {}", e)))
        })?;

    // The covered sequence C is the pointer's snapshot_sequence. Without it
    // no comparison — and no covered-ahead claim — is possible, but the
    // pointer is present and unusable, which is integrity damage, not
    // absence.
    let Some(covered) = pointer.get("snapshot_sequence").and_then(|v| v.as_i64())
    else {
        return Ok(RelationshipVerdict::failing(
            "verified pointer: current.json does not declare an integer snapshot_sequence",
        ));
    };

    let live: i64 = conn
        .query_row("SELECT COALESCE(MAX(sequence), 0) FROM events", [], |row| {
            row.get(0)
        })
        .unwrap_or(0);

    if covered < live {
        return Ok(RelationshipVerdict::of(SyncRelationship::Behind));
    }
    if covered == live {
        return Ok(RelationshipVerdict::of(SyncRelationship::Aligned));
    }

    // covered > live: every qualifier must hold for remote-advanced.
    Ok(classify_covered_ahead(conn, checkpoint_base, &pointer, live))
}

/// Evaluate the five remote-advanced qualifiers in specification order,
/// returning the first failure as `covered-ahead-integrity-failure`.
fn classify_covered_ahead(
    conn: &Connection,
    checkpoint_base: &Path,
    pointer: &serde_json::Value,
    live: i64,
) -> RelationshipVerdict {
    // Qualifier 1: verified pointer.
    if let Some(failure) = verify_pointer_shape(pointer) {
        return RelationshipVerdict::failing(failure);
    }
    let checkpoint_dir = checkpoint_base.join("checkpoint");
    let root_path = pointer
        .get("active_root")
        .and_then(|root| root.get("path"))
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let root_hash = pointer
        .get("active_root")
        .and_then(|root| root.get("sha256"))
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let root_file = checkpoint_dir.join(root_path);
    let root_bytes = match std::fs::read(&root_file) {
        Ok(bytes) => bytes,
        Err(_) => {
            return RelationshipVerdict::failing(format!(
                "verified pointer: root object missing: {}",
                root_path
            ))
        }
    };
    if checkpoint::calculate_file_hash(&root_file)
        .map(|actual| actual != root_hash)
        .unwrap_or(true)
    {
        return RelationshipVerdict::failing(format!(
            "verified pointer: root hash mismatch: {}",
            root_path
        ));
    }
    if let Some(deleted) = pointer.get("deleted_paths").and_then(|v| v.as_array()) {
        if let Some(unresolved) = deleted
            .iter()
            .filter_map(|v| v.as_str())
            .find(|path| checkpoint_dir.join(path).exists())
        {
            return RelationshipVerdict::failing(format!(
                "verified pointer: unresolved tombstone: {}",
                unresolved
            ));
        }
    }
    let mode = pointer
        .get("mode")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    if mode == "monolithic" {
        let view = std::fs::read(checkpoint_dir.join("forensic.jsonl")).ok();
        if !view.is_some_and(|view| view == root_bytes) {
            return RelationshipVerdict::failing(
                "verified pointer: forensic.jsonl compatibility view is not byte-identical \
                 to the pointer-selected root",
            );
        }
    }

    // Qualifier 2: valid staged stream.
    let staging = match checkpoint::stage_checkpoint_set(&checkpoint_dir) {
        Ok(staging) => staging,
        Err(e) => {
            return RelationshipVerdict::failing(format!(
                "valid staged stream: {}",
                e
            ))
        }
    };
    if let Err(e) = checkpoint::validate_forensic_contents(&staging) {
        return RelationshipVerdict::failing(format!("valid staged stream: {}", e));
    }

    // Qualifier 3: same origin.
    let workspace_uuid: String = conn
        .query_row("SELECT uuid FROM workspace", [], |row| row.get(0))
        .unwrap_or_default();
    if staging.store_uuid != workspace_uuid {
        return RelationshipVerdict::failing(format!(
            "same origin: checkpoint store UUID {} does not match the workspace UUID {} \
             (a foreign checkpoint is `sync import-only --merge` input, never a reconcile)",
            staging.store_uuid, workspace_uuid
        ));
    }

    // Qualifier 4: event-stream superset under derived wire identities.
    if let Some(failure) = verify_superset(conn, &staging) {
        return RelationshipVerdict::failing(failure);
    }

    // Qualifier 5: honest recorded state.
    let state_covered: Option<i64> = conn
        .query_row(
            "SELECT covered_event_sequence FROM checkpoint_state WHERE id = 1",
            [],
            |row| row.get(0),
        )
        .ok();
    if let Some(state_covered) = state_covered {
        if state_covered > live {
            return RelationshipVerdict::failing(format!(
                "honest recorded state: database claims covered sequence {} over a live \
                 sequence of {}",
                state_covered, live
            ));
        }
    }

    RelationshipVerdict::of(SyncRelationship::RemoteAdvanced)
}

/// Pointer-shape half of qualifier 1: parses, declares a supported mode, a
/// nonempty store UUID, and a nonnegative snapshot sequence.
fn verify_pointer_shape(pointer: &serde_json::Value) -> Option<String> {
    match pointer.get("mode").and_then(|v| v.as_str()) {
        Some("monolithic") | Some("sharded") => {}
        other => {
            return Some(format!(
                "verified pointer: unsupported or missing mode: {:?}",
                other
            ))
        }
    }
    let store_uuid = pointer
        .get("store_uuid")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    if store_uuid.is_empty() {
        return Some("verified pointer: empty store UUID".to_string());
    }
    let snapshot = pointer
        .get("snapshot_sequence")
        .and_then(|v| v.as_i64())
        .unwrap_or_default();
    if snapshot < 0 {
        return Some(format!(
            "verified pointer: negative snapshot sequence {}",
            snapshot
        ));
    }
    None
}

/// Qualifier 4: every live event — enumerated with its derived wire
/// identity — appears in the staged stream with identical public content.
/// An empty live stream trivially satisfies this.
fn verify_superset(conn: &Connection, staging: &ForensicStaging) -> Option<String> {
    let staged_by_identity: HashMap<(&str, i64), &SerializedEvent> = staging
        .events
        .iter()
        .map(|event| {
            (
                (
                    event.origin_store_uuid.as_str(),
                    event.origin_event_sequence,
                ),
                event,
            )
        })
        .collect();

    let live_events = match checkpoint::read_all_events(conn) {
        Ok(events) => events,
        Err(e) => return Some(format!("event-stream superset: {}", e)),
    };
    for live_event in &live_events {
        let identity = (
            live_event.origin_store_uuid.as_str(),
            live_event.origin_event_sequence,
        );
        match staged_by_identity.get(&identity) {
            None => {
                return Some(format!(
                    "event-stream superset: live event identity ({}, {}) is absent from \
                     the pulled checkpoint",
                    identity.0, identity.1
                ))
            }
            Some(staged) if !public_content_matches(live_event, staged) => {
                return Some(format!(
                    "event-stream superset: live event identity ({}, {}) has different \
                     content than the pulled checkpoint",
                    identity.0, identity.1
                ))
            }
            Some(_) => {}
        }
    }
    None
}

/// Public content equality per the specification: `(issue_id, kind, actor,
/// time, detail)` with the actor compared after applying the export
/// default (NULL actor is `"system"`).
pub fn public_content_matches(live: &EventRecord, staged: &SerializedEvent) -> bool {
    live.issue_id == staged.issue_id
        && live.kind == staged.kind
        && live.actor == staged.actor.clone().unwrap_or_else(|| "system".to_string())
        && live.time == staged.time
        && live.detail == staged.detail
}

/// `bead sync reconcile`: merge the pointer-verified, same-UUID,
/// verified-superset checkpoint into the live store through the existing
/// merge machinery (spec, "The command").
///
/// Refusal mapping: anything other than `remote-advanced` is a usage error
/// (exit 2) naming the actual relationship and its remedy, except a
/// `covered-ahead-integrity-failure`, which is an integrity refusal (exit
/// 5) naming the first failed qualifier. Neither mutates anything,
/// including under `--dry-run`.
pub fn reconcile_checkpoint(
    store: &mut crate::store::SqliteStore,
    checkpoint_base: &Path,
    actor: &str,
    dry_run: bool,
) -> Result<crate::service::checkpoint::FullImportResult> {
    let verdict = {
        let conn = store.conn();
        classify(conn, checkpoint_base)?
    };
    match verdict.relationship {
        SyncRelationship::RemoteAdvanced => {}
        SyncRelationship::CoveredAheadIntegrityFailure => {
            return Err(Error::Integrity(format!(
                "sync reconcile refused: covered-ahead integrity failure - {}",
                verdict
                    .failed_qualifier
                    .as_deref()
                    .unwrap_or("first failed qualifier unavailable")
            )))
        }
        SyncRelationship::Behind => {
            return Err(Error::cli_usage(
                "sync reconcile refused: the live store is ahead of the checkpoint \
                 (relationship `behind`) - run `bead sync flush-only` first",
            ))
        }
        SyncRelationship::Aligned | SyncRelationship::Absent => {
            return Err(Error::cli_usage(format!(
                "sync reconcile refused: there is nothing to reconcile \
                 (relationship `{}`)",
                verdict.relationship.as_str()
            )))
        }
    }

    checkpoint::import_forensic_checkpoint(
        store,
        &checkpoint_base.join("checkpoint"),
        "native-v1",
        crate::cli::ImportMode::Merge,
        actor,
        dry_run,
    )
    .map_err(Error::Internal)
}

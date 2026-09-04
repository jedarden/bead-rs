//! Atomic historical-redaction transaction (ADR-015, BR-T16).
//!
//! This module owns the destructive semantic step only. It locates one
//! scanner fingerprint, revalidates it under an IMMEDIATE transaction,
//! replaces exactly that byte range with the fixed marker, and commits the
//! finding, receipt, epoch, anti-resurrection tombstone, and audit event as
//! one unit. BR-T17 owns CLI exposure and sanitized checkpoint publication.

use crate::model::redaction::{
    FieldSelector, FindingSeverity, PublicationState, RedactionEpoch, RedactionError,
    RedactionExtensions, RedactionFinding, RedactionReceipt, ResurrectionTombstone,
    REDACTION_MARKER, SCHEMA_REDACTION_EPOCH, SCHEMA_REDACTION_FIELD_SELECTOR,
    SCHEMA_REDACTION_FINDING, SCHEMA_REDACTION_RECEIPT, SCHEMA_REDACTION_TOMBSTONE,
};
use crate::scan::{self, Disposition, Field, Mode, ScanConfig, Tier};
use crate::service::checkpoint::{acquire_checkpoint_publication_lock, CheckpointPublicationLock};
use crate::service::secret_diagnostics::{find_live_finding, LiveFindingLocation};
use crate::store::SqliteStore;
use fs2::FileExt;
use rusqlite::types::Value as SqlValue;
use rusqlite::{params, params_from_iter, OptionalExtension, Transaction, TransactionBehavior};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::ErrorKind;
use std::path::Path;
use std::time::{Duration, Instant};

const LOCK_TIMEOUT: Duration = Duration::from_secs(30);
const LOCK_POLL: Duration = Duration::from_millis(10);

/// Result of one semantic redaction transaction.
#[derive(Debug, Clone, Serialize)]
pub struct RedactionOutcome {
    /// Durable nonsecret receipt for the replacement.
    pub receipt: RedactionReceipt,
    /// True when an identical request returned an already committed receipt.
    pub is_replay: bool,
}

/// Non-mutating plan returned by `bead redact --dry-run`.
#[derive(Debug, Clone, Serialize)]
pub struct RedactionPreview {
    pub finding_fingerprint: String,
    pub ruleset_version: u32,
    pub rule_id: String,
    pub selector: FieldSelector,
    pub prior_record_hash: String,
    pub sanitized_record_hash: String,
    pub affected_issue_revision: Option<i64>,
    pub replacement_marker: &'static str,
    pub previous_generation_reset: bool,
}

/// Load one durable receipt for `bead redact --resume` without exposing any
/// removed bytes.
pub fn load_redaction_receipt(
    conn: &rusqlite::Connection,
    receipt_id: &str,
) -> Result<RedactionReceipt, RedactionError> {
    if receipt_id.len() != 64
        || !receipt_id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(RedactionError::Usage(
            "receipt ID must be 64-character lowercase SHA-256 hex".to_string(),
        ));
    }
    read_receipts(conn, "receipt_id", receipt_id)?
        .into_iter()
        .next()
        .ok_or_else(|| RedactionError::NotFound("redaction receipt does not exist".to_string()))
}

/// Proof that both exceptional-maintenance locks are held in canonical order.
///
/// BR-T17 keeps this guard alive through sanitized checkpoint publication so
/// no ordinary publisher can install the dirty predecessor as `previous` in
/// the gap after the SQLite transaction commits.
pub struct RedactionLocks {
    _maintenance: MaintenanceLock,
    publication: CheckpointPublicationLock,
}

impl RedactionLocks {
    /// Borrow the already-held publication lock for the BR-T17 publisher.
    pub fn checkpoint_publication_lock(&self) -> &CheckpointPublicationLock {
        &self.publication
    }
}

/// Acquire workspace-maintenance first, then checkpoint-publication.
pub fn acquire_redaction_locks(workspace_root: &Path) -> Result<RedactionLocks, RedactionError> {
    let maintenance = acquire_maintenance_lock(&workspace_root.join(".beads"))?;
    let publication = acquire_checkpoint_publication_lock(
        &workspace_root.join(".beads/checkpoint"),
    )
    .map_err(|_| {
        RedactionError::Integrity("could not acquire checkpoint publication lock".to_string())
    })?;
    Ok(RedactionLocks {
        _maintenance: maintenance,
        publication,
    })
}

/// Apply one fingerprint-selected historical redaction.
///
/// The caller supplies no matched value, range, selector, replacement, SQL,
/// or path. Both file locks are held before the live row is read; the SQLite
/// write lock is then acquired before lookup and remains held through commit.
pub fn redact_finding(
    store: &mut SqliteStore,
    workspace_root: &Path,
    fingerprint: &str,
    actor: &str,
    reason: &str,
) -> Result<RedactionOutcome, RedactionError> {
    let locks = acquire_redaction_locks(workspace_root)?;
    redact_finding_holding(store, &locks, fingerprint, actor, reason)
}

/// Apply the semantic transaction while caller-owned maintenance and
/// publication locks remain held.
pub fn redact_finding_holding(
    store: &mut SqliteStore,
    _locks: &RedactionLocks,
    fingerprint: &str,
    actor: &str,
    reason: &str,
) -> Result<RedactionOutcome, RedactionError> {
    validate_request(fingerprint, actor, reason)?;
    let expected = find_live_finding(store.conn(), fingerprint)
        .map_err(|_| integrity("could not scan live redaction targets"))?;
    redact_in_transaction(
        store.conn(),
        fingerprint,
        actor,
        reason,
        false,
        expected.as_ref(),
    )
}

/// Revalidate and describe one redaction without committing any change.
pub fn preview_redaction_holding(
    store: &mut SqliteStore,
    _locks: &RedactionLocks,
    fingerprint: &str,
    actor: &str,
    reason: &str,
) -> Result<RedactionPreview, RedactionError> {
    validate_request(fingerprint, actor, reason)?;
    let tx = Transaction::new_unchecked(store.conn(), TransactionBehavior::Immediate)
        .map_err(|_| integrity("could not open redaction preview transaction"))?;
    let location = find_live_finding(&tx, fingerprint)
        .map_err(|_| integrity("could not scan live redaction targets"))?
        .ok_or_else(|| {
            RedactionError::NotFound("no current live finding matches that fingerprint".to_string())
        })?;
    let target = target_spec(location.table, location.field).ok_or_else(|| {
        RedactionError::Conflict(format!(
            "finding addresses unsupported field {}.{}",
            location.table, location.field
        ))
    })?;
    let current = read_target_text(&tx, &location)?;
    let finding = scan::scan(
        &ScanConfig::new(Mode::Advisory),
        &location.finding.selector,
        &[Field::new(location.field, &current)],
    )
    .findings
    .into_iter()
    .find(|finding| finding.fingerprint == fingerprint)
    .ok_or_else(|| RedactionError::Conflict("finding changed during dry-run".to_string()))?;
    if current.get(finding.start..finding.end).is_none() {
        return Err(RedactionError::Conflict(
            "finding byte range is no longer a UTF-8 boundary".to_string(),
        ));
    }

    let (columns, values) = read_target_record(&tx, &location, target.integrity_hash_column)?;
    let prior_record_hash = hash_target_values(location.table, &columns, &values);
    let mut sanitized = String::with_capacity(
        current.len() - (finding.end - finding.start) + REDACTION_MARKER.len(),
    );
    sanitized.push_str(&current[..finding.start]);
    sanitized.push_str(REDACTION_MARKER);
    sanitized.push_str(&current[finding.end..]);
    let mut sanitized_values = values;
    let field_index = columns
        .iter()
        .position(|column| column == location.field)
        .ok_or_else(|| integrity("redaction preview target field disappeared"))?;
    sanitized_values[field_index] = SqlValue::Text(sanitized);
    if target.is_issue_row {
        let revision_index = columns
            .iter()
            .position(|column| column == "revision")
            .ok_or_else(|| integrity("issue redaction preview has no revision"))?;
        let revision = match &sanitized_values[revision_index] {
            SqlValue::Integer(revision) => *revision,
            _ => return Err(integrity("issue redaction preview has invalid revision")),
        };
        sanitized_values[revision_index] = SqlValue::Integer(revision + 1);
    }
    let sanitized_record_hash = hash_target_values(location.table, &columns, &sanitized_values);
    let affected_issue_revision = associated_issue_id(&tx, &location, target.issue_id_column)?
        .map(|issue_id| {
            tx.query_row(
                "SELECT revision + 1 FROM issues WHERE id = ?1",
                [issue_id],
                |row| row.get(0),
            )
            .map_err(|_| integrity("could not preview affected issue revision"))
        })
        .transpose()?;
    let selector = FieldSelector {
        schema_ref: SCHEMA_REDACTION_FIELD_SELECTOR.to_string(),
        record_kind: location.table.to_string(),
        origin_identity: location.finding.selector,
        field_path: location.field.to_string(),
        byte_start: finding.start as i64,
        byte_length: (finding.end - finding.start) as i64,
        prior_record_hash: prior_record_hash.clone(),
        extensions: RedactionExtensions::new(),
    };
    Ok(RedactionPreview {
        finding_fingerprint: fingerprint.to_string(),
        ruleset_version: finding.ruleset_version,
        rule_id: finding.rule_id,
        selector,
        prior_record_hash,
        sanitized_record_hash,
        affected_issue_revision,
        replacement_marker: REDACTION_MARKER,
        previous_generation_reset: true,
    })
}

fn validate_request(fingerprint: &str, actor: &str, reason: &str) -> Result<(), RedactionError> {
    if fingerprint.len() != 64
        || !fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(RedactionError::Usage(
            "finding fingerprint must be 64-character lowercase SHA-256 hex".to_string(),
        ));
    }
    crate::model::redaction::validate_actor(actor)?;
    crate::model::redaction::validate_reason(reason)?;

    let config = ScanConfig::new(Mode::Enforce);
    let report = scan::scan(
        &config,
        "redaction:request",
        &[Field::new("reason", reason)],
    );
    if let Some(finding) = report.blocking.first() {
        return Err(RedactionError::Usage(format!(
            "redaction reason contains a blocking finding (rule {}, fingerprint {}); matched bytes are not shown",
            finding.rule_id, finding.fingerprint
        )));
    }
    Ok(())
}

fn redact_in_transaction(
    conn: &rusqlite::Connection,
    fingerprint: &str,
    actor: &str,
    reason: &str,
    fail_before_commit: bool,
    expected: Option<&LiveFindingLocation>,
) -> Result<RedactionOutcome, RedactionError> {
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
        .map_err(|_| integrity("could not open redaction transaction"))?;

    if let Some(existing) = read_receipt_by_fingerprint(&tx, fingerprint)? {
        if existing.actor != actor || existing.reason != reason {
            return Err(RedactionError::Conflict(
                "finding already has a receipt for a different request".to_string(),
            ));
        }
        return Ok(RedactionOutcome {
            receipt: existing,
            is_replay: true,
        });
    }

    let location = find_live_finding(&tx, fingerprint)
        .map_err(|_| integrity("could not scan live redaction targets"))?;
    if let Some(expected) = expected {
        let current = location.as_ref().ok_or_else(|| {
            RedactionError::Conflict("finding changed before the redaction transaction".to_string())
        })?;
        if current.table != expected.table
            || current.field != expected.field
            || current.identity_values != expected.identity_values
            || current.finding.start != expected.finding.start
            || current.finding.end != expected.finding.end
        {
            return Err(RedactionError::Conflict(
                "finding selector changed before the redaction transaction".to_string(),
            ));
        }
    }
    let location = location.ok_or_else(|| {
        RedactionError::NotFound("no current live finding matches that fingerprint".to_string())
    })?;
    let target = target_spec(location.table, location.field).ok_or_else(|| {
        RedactionError::Conflict(format!(
            "finding addresses unsupported field {}.{}",
            location.table, location.field
        ))
    })?;

    let current = read_target_text(&tx, &location)?;
    let revalidated = scan::scan(
        &ScanConfig::new(Mode::Advisory),
        &location.finding.selector,
        &[Field::new(location.field, &current)],
    )
    .findings
    .into_iter()
    .find(|finding| finding.fingerprint == fingerprint)
    .ok_or_else(|| {
        RedactionError::Conflict("finding changed before the redaction transaction".to_string())
    })?;
    if revalidated.start != location.finding.start
        || revalidated.end != location.finding.end
        || revalidated.rule_id != location.finding.rule_id
    {
        return Err(RedactionError::Conflict(
            "finding selector changed before the redaction transaction".to_string(),
        ));
    }
    if current.get(revalidated.start..revalidated.end).is_none() {
        return Err(RedactionError::Conflict(
            "finding byte range is no longer a UTF-8 boundary".to_string(),
        ));
    }

    let prior_record_hash = hash_target_record(&tx, &location, target.integrity_hash_column)?;
    let mut sanitized = String::with_capacity(
        current.len() - (revalidated.end - revalidated.start) + REDACTION_MARKER.len(),
    );
    sanitized.push_str(&current[..revalidated.start]);
    sanitized.push_str(REDACTION_MARKER);
    sanitized.push_str(&current[revalidated.end..]);

    let affected_issue_id = associated_issue_id(&tx, &location, target.issue_id_column)?;
    update_target(&tx, &location, &sanitized, target.is_issue_row)?;
    let affected_issue_revision = if let Some(issue_id) = affected_issue_id.as_deref() {
        if !target.is_issue_row {
            let changed = tx
                .execute(
                    "UPDATE issues SET revision = revision + 1 WHERE id = ?1",
                    [issue_id],
                )
                .map_err(|_| integrity("could not advance affected issue revision"))?;
            if changed != 1 {
                return Err(integrity("affected issue disappeared during redaction"));
            }
        }
        Some(
            tx.query_row(
                "SELECT revision FROM issues WHERE id = ?1",
                [issue_id],
                |row| row.get(0),
            )
            .map_err(|_| integrity("could not read affected issue revision"))?,
        )
    } else {
        None
    };

    let sanitized_record_hash = hash_target_record(&tx, &location, target.integrity_hash_column)?;
    if prior_record_hash == sanitized_record_hash {
        return Err(integrity("redaction did not change the target record hash"));
    }
    update_integrity_hash(
        &tx,
        &location,
        target.integrity_hash_column,
        &sanitized_record_hash,
    )?;

    let timestamp = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|_| integrity("could not format redaction timestamp"))?;
    let selector = FieldSelector {
        schema_ref: SCHEMA_REDACTION_FIELD_SELECTOR.to_string(),
        record_kind: location.table.to_string(),
        origin_identity: location.finding.selector.clone(),
        field_path: location.field.to_string(),
        byte_start: revalidated.start as i64,
        byte_length: (revalidated.end - revalidated.start) as i64,
        prior_record_hash: prior_record_hash.clone(),
        extensions: RedactionExtensions::new(),
    };
    let finding = RedactionFinding {
        schema_ref: SCHEMA_REDACTION_FINDING.to_string(),
        fingerprint: fingerprint.to_string(),
        ruleset_version: revalidated.ruleset_version,
        rule_id: revalidated.rule_id.clone(),
        selector: selector.clone(),
        severity: if revalidated.tier == Tier::Blocking
            && revalidated.disposition == Disposition::Confirmed
        {
            FindingSeverity::Blocking
        } else {
            FindingSeverity::Advisory
        },
        detected_at: timestamp.clone(),
        extensions: RedactionExtensions::new(),
    };
    finding.validate()?;

    let receipt_id = RedactionReceipt::canonical_identity(
        fingerprint,
        revalidated.ruleset_version,
        &revalidated.rule_id,
        &selector,
        &sanitized_record_hash,
        actor,
        reason,
        &timestamp,
        affected_issue_revision,
    );
    let receipt_ids = vec![receipt_id.clone()];
    let epoch_id = RedactionEpoch::identity_for(&receipt_ids);
    let receipt = RedactionReceipt {
        schema_ref: SCHEMA_REDACTION_RECEIPT.to_string(),
        receipt_id: receipt_id.clone(),
        finding_fingerprint: fingerprint.to_string(),
        ruleset_version: revalidated.ruleset_version,
        rule_id: revalidated.rule_id,
        selector: selector.clone(),
        prior_record_hash: prior_record_hash.clone(),
        sanitized_record_hash: sanitized_record_hash.clone(),
        actor: actor.to_string(),
        reason: reason.to_string(),
        redacted_at: timestamp.clone(),
        affected_issue_revision,
        publication_state: PublicationState::Committed,
        resulting_generation_id: None,
        epoch_id: Some(epoch_id.clone()),
        extensions: RedactionExtensions::new(),
    };
    receipt.validate()?;
    receipt.verify_identity()?;
    let epoch = RedactionEpoch {
        schema_ref: SCHEMA_REDACTION_EPOCH.to_string(),
        epoch_id: epoch_id.clone(),
        receipt_ids,
        publication_state: PublicationState::Committed,
        resulting_generation_id: None,
        previous_generation_reset: false,
        superseded_generations: Vec::new(),
        opened_at: timestamp.clone(),
        published_at: None,
        extensions: RedactionExtensions::new(),
    };
    epoch.validate()?;
    let tombstone_id = ResurrectionTombstone::identity_for(
        &selector.record_kind,
        &selector.origin_identity,
        &selector.field_path,
        &prior_record_hash,
        fingerprint,
        &epoch_id,
    );
    let tombstone = ResurrectionTombstone {
        schema_ref: SCHEMA_REDACTION_TOMBSTONE.to_string(),
        tombstone_id,
        record_kind: selector.record_kind.clone(),
        origin_identity: selector.origin_identity.clone(),
        field_path: selector.field_path.clone(),
        prior_record_hash: prior_record_hash.clone(),
        finding_fingerprint: fingerprint.to_string(),
        epoch_id: epoch_id.clone(),
        created_at: timestamp.clone(),
        extensions: RedactionExtensions::new(),
    };
    tombstone.validate()?;

    insert_records(&tx, &finding, &receipt, &epoch, &tombstone)?;
    append_audit_event(
        &tx,
        affected_issue_id.as_deref(),
        actor,
        &timestamp,
        &receipt,
        &epoch_id,
    )?;

    if fail_before_commit {
        return Err(integrity("injected failure before redaction commit"));
    }
    tx.commit()
        .map_err(|_| integrity("could not commit redaction transaction"))?;
    Ok(RedactionOutcome {
        receipt,
        is_replay: false,
    })
}

#[derive(Clone, Copy)]
struct TargetSpec {
    is_issue_row: bool,
    issue_id_column: Option<&'static str>,
    integrity_hash_column: Option<&'static str>,
}

fn target_spec(table: &str, field: &str) -> Option<TargetSpec> {
    let allowed = match table {
        "issues" => matches!(field, "title" | "description" | "notes" | "close_reason"),
        "events" => field == "detail",
        "comments" => field == "body",
        "issue_data" | "external_references" => field == "value",
        "recurrence_templates" => matches!(
            field,
            "title" | "description" | "base_title_template" | "base_description" | "labels_json"
        ),
        "attempt_outcomes" => matches!(
            field,
            "reason" | "evidence_refs_json" | "model" | "harness" | "harness_version"
        ),
        "provenance_receipts" => field == "actor",
        _ => false,
    };
    if !allowed {
        return None;
    }
    Some(TargetSpec {
        is_issue_row: table == "issues",
        issue_id_column: match table {
            "issues" => Some("id"),
            "comments" | "issue_data" | "external_references" | "attempt_outcomes" => {
                Some("issue_id")
            }
            _ => None,
        },
        integrity_hash_column: match table {
            "events" => Some("event_sha256"),
            "provenance_receipts" => Some("receipt_sha256"),
            _ => None,
        },
    })
}

fn read_target_text(
    tx: &Transaction<'_>,
    location: &LiveFindingLocation,
) -> Result<String, RedactionError> {
    let sql = format!(
        "SELECT {} FROM {} WHERE {}",
        location.field,
        location.table,
        identity_predicate(location.identity_fields, 1)
    );
    tx.query_row(
        &sql,
        params_from_iter(location.identity_values.iter()),
        |row| row.get(0),
    )
    .optional()
    .map_err(|_| integrity("could not read redaction target"))?
    .ok_or_else(|| RedactionError::Conflict("redaction target no longer exists".to_string()))
}

fn associated_issue_id(
    tx: &Transaction<'_>,
    location: &LiveFindingLocation,
    issue_id_column: Option<&str>,
) -> Result<Option<String>, RedactionError> {
    let Some(column) = issue_id_column else {
        return Ok(None);
    };
    let sql = format!(
        "SELECT {column} FROM {} WHERE {}",
        location.table,
        identity_predicate(location.identity_fields, 1)
    );
    tx.query_row(
        &sql,
        params_from_iter(location.identity_values.iter()),
        |row| row.get(0),
    )
    .optional()
    .map_err(|_| integrity("could not resolve affected issue"))?
    .ok_or_else(|| RedactionError::Conflict("redaction target no longer exists".to_string()))
    .map(Some)
}

fn update_target(
    tx: &Transaction<'_>,
    location: &LiveFindingLocation,
    sanitized: &str,
    is_issue_row: bool,
) -> Result<(), RedactionError> {
    let revision = if is_issue_row {
        ", revision = revision + 1"
    } else {
        ""
    };
    let sql = format!(
        "UPDATE {} SET {} = ?1{} WHERE {}",
        location.table,
        location.field,
        revision,
        identity_predicate(location.identity_fields, 2)
    );
    let mut values = Vec::with_capacity(location.identity_values.len() + 1);
    values.push(SqlValue::Text(sanitized.to_string()));
    values.extend(location.identity_values.iter().cloned());
    let changed = tx
        .execute(&sql, params_from_iter(values.iter()))
        .map_err(|_| integrity("could not replace redaction target"))?;
    if changed != 1 {
        return Err(RedactionError::Conflict(
            "redaction target changed before replacement".to_string(),
        ));
    }
    Ok(())
}

fn update_integrity_hash(
    tx: &Transaction<'_>,
    location: &LiveFindingLocation,
    hash_column: Option<&str>,
    record_hash: &str,
) -> Result<(), RedactionError> {
    let Some(column) = hash_column else {
        return Ok(());
    };
    let value = if location.table == "provenance_receipts" {
        provenance_receipt_hash(tx, location)?
    } else {
        record_hash.to_string()
    };
    let sql = format!(
        "UPDATE {} SET {column} = ?1 WHERE {}",
        location.table,
        identity_predicate(location.identity_fields, 2)
    );
    let mut values = Vec::with_capacity(location.identity_values.len() + 1);
    values.push(SqlValue::Text(value));
    values.extend(location.identity_values.iter().cloned());
    let changed = tx
        .execute(&sql, params_from_iter(values.iter()))
        .map_err(|_| integrity("could not update target integrity hash"))?;
    if changed != 1 {
        return Err(integrity(
            "target disappeared while updating its integrity hash",
        ));
    }
    Ok(())
}

fn provenance_receipt_hash(
    tx: &Transaction<'_>,
    location: &LiveFindingLocation,
) -> Result<String, RedactionError> {
    let sql = format!(
        "SELECT receipt_id, kind, source_root_sha256, actor, created_at, result
         FROM provenance_receipts WHERE {}",
        identity_predicate(location.identity_fields, 1)
    );
    let parts: (String, String, String, String, String, String) = tx
        .query_row(
            &sql,
            params_from_iter(location.identity_values.iter()),
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .map_err(|_| integrity("could not recompute provenance receipt hash"))?;
    let mut hasher = Sha256::new();
    hasher.update(parts.0);
    hasher.update(parts.1);
    hasher.update(parts.2);
    hasher.update(parts.3);
    hasher.update(parts.4);
    hasher.update(parts.5);
    Ok(format!("{:x}", hasher.finalize()))
}

fn hash_target_record(
    tx: &Transaction<'_>,
    location: &LiveFindingLocation,
    excluded_hash_column: Option<&str>,
) -> Result<String, RedactionError> {
    let (columns, values) = read_target_record(tx, location, excluded_hash_column)?;
    Ok(hash_target_values(location.table, &columns, &values))
}

fn read_target_record(
    tx: &Transaction<'_>,
    location: &LiveFindingLocation,
    excluded_hash_column: Option<&str>,
) -> Result<(Vec<String>, Vec<SqlValue>), RedactionError> {
    let mut column_statement = tx
        .prepare(&format!("PRAGMA table_info({})", location.table))
        .map_err(|_| integrity("could not enumerate redaction target columns"))?;
    let columns = column_statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|_| integrity("could not enumerate redaction target columns"))?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|_| integrity("could not enumerate redaction target columns"))?;
    let columns: Vec<String> = columns
        .into_iter()
        .filter(|column| Some(column.as_str()) != excluded_hash_column)
        .collect();
    if columns.is_empty()
        || columns.iter().any(|column| {
            !column
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        })
    {
        return Err(integrity("redaction target has an unsafe column layout"));
    }
    let sql = format!(
        "SELECT {} FROM {} WHERE {}",
        columns.join(", "),
        location.table,
        identity_predicate(location.identity_fields, 1)
    );
    let values: Vec<SqlValue> = tx
        .query_row(
            &sql,
            params_from_iter(location.identity_values.iter()),
            |row| {
                (0..columns.len())
                    .map(|index| row.get(index))
                    .collect::<rusqlite::Result<Vec<_>>>()
            },
        )
        .map_err(|_| integrity("could not hash redaction target"))?;
    Ok((columns, values))
}

fn hash_target_values(table: &str, columns: &[String], values: &[SqlValue]) -> String {
    let mut hasher = Sha256::new();
    hash_part(&mut hasher, b"bead-rs-redaction-record-v1");
    hash_part(&mut hasher, table.as_bytes());
    for (column, value) in columns.iter().zip(values) {
        hash_part(&mut hasher, column.as_bytes());
        match value {
            SqlValue::Null => hash_part(&mut hasher, b"null"),
            SqlValue::Integer(value) => {
                hash_part(&mut hasher, b"integer");
                hash_part(&mut hasher, &value.to_be_bytes());
            }
            SqlValue::Real(value) => {
                hash_part(&mut hasher, b"real");
                hash_part(&mut hasher, &value.to_bits().to_be_bytes());
            }
            SqlValue::Text(value) => {
                hash_part(&mut hasher, b"text");
                hash_part(&mut hasher, value.as_bytes());
            }
            SqlValue::Blob(value) => {
                hash_part(&mut hasher, b"blob");
                hash_part(&mut hasher, value);
            }
        }
    }
    format!("{:x}", hasher.finalize())
}

fn hash_part(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
}

fn identity_predicate(fields: &[&str], first_parameter: usize) -> String {
    fields
        .iter()
        .enumerate()
        .map(|(index, field)| format!("{field} IS ?{}", first_parameter + index))
        .collect::<Vec<_>>()
        .join(" AND ")
}

fn insert_records(
    tx: &Transaction<'_>,
    finding: &RedactionFinding,
    receipt: &RedactionReceipt,
    epoch: &RedactionEpoch,
    tombstone: &ResurrectionTombstone,
) -> Result<(), RedactionError> {
    let finding_extensions = encode_extensions(&finding.extensions)?;
    let finding_selector_extensions = encode_extensions(&finding.selector.extensions)?;
    tx.execute(
        "INSERT OR IGNORE INTO redaction_findings (
            fingerprint, ruleset_version, rule_id, record_kind,
            origin_identity, field_path, byte_start, byte_length,
            prior_record_hash, severity, detected_at, extensions_json,
            selector_extensions_json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            finding.fingerprint,
            finding.ruleset_version,
            finding.rule_id,
            finding.selector.record_kind,
            finding.selector.origin_identity,
            finding.selector.field_path,
            finding.selector.byte_start,
            finding.selector.byte_length,
            finding.selector.prior_record_hash,
            finding.severity.as_str(),
            finding.detected_at,
            finding_extensions,
            finding_selector_extensions,
        ],
    )
    .map_err(|_| integrity("could not store redaction finding"))?;
    let finding_matches: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM redaction_findings
             WHERE fingerprint = ?1 AND ruleset_version = ?2 AND rule_id = ?3
               AND record_kind = ?4 AND origin_identity = ?5 AND field_path = ?6
               AND byte_start = ?7 AND byte_length = ?8 AND prior_record_hash = ?9
               AND severity = ?10 AND extensions_json = ?11
               AND selector_extensions_json = ?12",
            params![
                finding.fingerprint,
                finding.ruleset_version,
                finding.rule_id,
                finding.selector.record_kind,
                finding.selector.origin_identity,
                finding.selector.field_path,
                finding.selector.byte_start,
                finding.selector.byte_length,
                finding.selector.prior_record_hash,
                finding.severity.as_str(),
                finding_extensions,
                finding_selector_extensions,
            ],
            |row| row.get(0),
        )
        .map_err(|_| integrity("could not verify stored redaction finding"))?;
    if finding_matches != 1 {
        return Err(RedactionError::Conflict(
            "finding fingerprint collides with different stored metadata".to_string(),
        ));
    }

    let receipt_extensions = encode_extensions(&receipt.extensions)?;
    let receipt_selector_extensions = encode_extensions(&receipt.selector.extensions)?;
    tx.execute(
        "INSERT INTO redaction_receipts (
            receipt_id, finding_fingerprint, ruleset_version, rule_id,
            record_kind, origin_identity, field_path, byte_start, byte_length,
            prior_record_hash, sanitized_record_hash, actor, reason,
            redacted_at, affected_issue_revision, publication_state,
            resulting_generation_id, epoch_id, extensions_json,
            selector_extensions_json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                   ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
        params![
            receipt.receipt_id,
            receipt.finding_fingerprint,
            receipt.ruleset_version,
            receipt.rule_id,
            receipt.selector.record_kind,
            receipt.selector.origin_identity,
            receipt.selector.field_path,
            receipt.selector.byte_start,
            receipt.selector.byte_length,
            receipt.prior_record_hash,
            receipt.sanitized_record_hash,
            receipt.actor,
            receipt.reason,
            receipt.redacted_at,
            receipt.affected_issue_revision,
            receipt.publication_state.as_str(),
            receipt.resulting_generation_id,
            receipt.epoch_id,
            receipt_extensions,
            receipt_selector_extensions,
        ],
    )
    .map_err(|_| integrity("could not store redaction receipt"))?;

    let receipt_ids = serde_json::to_string(&epoch.receipt_ids)
        .map_err(|_| integrity("could not encode redaction epoch"))?;
    let superseded = serde_json::to_string(&epoch.superseded_generations)
        .map_err(|_| integrity("could not encode redaction epoch"))?;
    let epoch_extensions = encode_extensions(&epoch.extensions)?;
    tx.execute(
        "INSERT INTO redaction_epochs (
            epoch_id, publication_state, receipt_ids_json,
            resulting_generation_id, previous_generation_reset,
            superseded_generations_json, opened_at, published_at,
            extensions_json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            epoch.epoch_id,
            epoch.publication_state.as_str(),
            receipt_ids,
            epoch.resulting_generation_id,
            epoch.previous_generation_reset,
            superseded,
            epoch.opened_at,
            epoch.published_at,
            epoch_extensions,
        ],
    )
    .map_err(|_| integrity("could not store redaction epoch"))?;

    let tombstone_extensions = encode_extensions(&tombstone.extensions)?;
    tx.execute(
        "INSERT INTO redaction_tombstones (
            tombstone_id, record_kind, origin_identity, field_path,
            prior_record_hash, finding_fingerprint, epoch_id, created_at,
            extensions_json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            tombstone.tombstone_id,
            tombstone.record_kind,
            tombstone.origin_identity,
            tombstone.field_path,
            tombstone.prior_record_hash,
            tombstone.finding_fingerprint,
            tombstone.epoch_id,
            tombstone.created_at,
            tombstone_extensions,
        ],
    )
    .map_err(|_| integrity("could not store redaction tombstone"))?;
    Ok(())
}

fn append_audit_event(
    tx: &Transaction<'_>,
    issue_id: Option<&str>,
    actor: &str,
    timestamp: &str,
    receipt: &RedactionReceipt,
    epoch_id: &str,
) -> Result<(), RedactionError> {
    let detail = serde_json::json!({
        "$schema": "urn:bead-rs:schema:redaction-event:native-v1",
        "receipt_id": receipt.receipt_id,
        "finding_fingerprint": receipt.finding_fingerprint,
        "ruleset_version": receipt.ruleset_version,
        "rule_id": receipt.rule_id,
        "record_kind": receipt.selector.record_kind,
        "origin_identity": receipt.selector.origin_identity,
        "field_path": receipt.selector.field_path,
        "byte_start": receipt.selector.byte_start,
        "byte_length": receipt.selector.byte_length,
        "prior_record_hash": receipt.prior_record_hash,
        "sanitized_record_hash": receipt.sanitized_record_hash,
        "epoch_id": epoch_id,
    });
    tx.execute(
        "INSERT INTO events (issue_id, kind, actor, time, detail)
         VALUES (?1, 'historical_redaction', ?2, ?3, ?4)",
        params![issue_id, actor, timestamp, detail.to_string()],
    )
    .map_err(|_| integrity("could not append historical-redaction audit event"))?;
    Ok(())
}

fn read_receipt_by_fingerprint(
    tx: &rusqlite::Connection,
    fingerprint: &str,
) -> Result<Option<RedactionReceipt>, RedactionError> {
    let mut receipts = read_receipts(tx, "finding_fingerprint", fingerprint)?;
    match receipts.len() {
        0 => Ok(None),
        1 => Ok(receipts.pop()),
        _ => Err(integrity("one finding has multiple redaction receipts")),
    }
}

fn read_receipts(
    conn: &rusqlite::Connection,
    identity_column: &str,
    identity: &str,
) -> Result<Vec<RedactionReceipt>, RedactionError> {
    debug_assert!(matches!(
        identity_column,
        "receipt_id" | "finding_fingerprint"
    ));
    let sql = format!(
        "SELECT receipt_id, finding_fingerprint, ruleset_version, rule_id,
                record_kind, origin_identity, field_path, byte_start,
                byte_length, prior_record_hash, sanitized_record_hash,
                actor, reason, redacted_at, affected_issue_revision,
                publication_state, resulting_generation_id, epoch_id,
                extensions_json, selector_extensions_json
         FROM redaction_receipts WHERE {identity_column} = ?1
         ORDER BY receipt_id LIMIT 2"
    );
    let mut statement = conn
        .prepare(&sql)
        .map_err(|_| integrity("could not query existing redaction receipt"))?;
    let rows = statement
        .query_map([identity], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, i64>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, String>(10)?,
                row.get::<_, String>(11)?,
                row.get::<_, String>(12)?,
                row.get::<_, String>(13)?,
                row.get::<_, Option<i64>>(14)?,
                row.get::<_, String>(15)?,
                row.get::<_, Option<String>>(16)?,
                row.get::<_, Option<String>>(17)?,
                row.get::<_, String>(18)?,
                row.get::<_, String>(19)?,
            ))
        })
        .map_err(|_| integrity("could not query existing redaction receipt"))?;
    let mut receipts = Vec::new();
    for row in rows {
        let (
            receipt_id,
            finding_fingerprint,
            ruleset_version,
            rule_id,
            record_kind,
            origin_identity,
            field_path,
            byte_start,
            byte_length,
            prior_record_hash,
            sanitized_record_hash,
            actor,
            reason,
            redacted_at,
            affected_issue_revision,
            publication_state,
            resulting_generation_id,
            epoch_id,
            extensions_json,
            selector_extensions_json,
        ) = row.map_err(|_| integrity("could not read existing redaction receipt"))?;
        let receipt = RedactionReceipt {
            schema_ref: SCHEMA_REDACTION_RECEIPT.to_string(),
            receipt_id,
            finding_fingerprint,
            ruleset_version: ruleset_version
                .try_into()
                .map_err(|_| integrity("stored redaction ruleset version is invalid"))?,
            rule_id,
            selector: FieldSelector {
                schema_ref: SCHEMA_REDACTION_FIELD_SELECTOR.to_string(),
                record_kind,
                origin_identity,
                field_path,
                byte_start,
                byte_length,
                prior_record_hash: prior_record_hash.clone(),
                extensions: decode_extensions(&selector_extensions_json)?,
            },
            prior_record_hash,
            sanitized_record_hash,
            actor,
            reason,
            redacted_at,
            affected_issue_revision,
            publication_state: PublicationState::parse(&publication_state)?,
            resulting_generation_id,
            epoch_id,
            extensions: decode_extensions(&extensions_json)?,
        };
        receipt.validate()?;
        receipt.verify_identity()?;
        receipts.push(receipt);
    }
    Ok(receipts)
}

fn encode_extensions(extensions: &RedactionExtensions) -> Result<String, RedactionError> {
    serde_json::to_string(extensions)
        .map_err(|_| integrity("could not encode redaction extensions"))
}

fn decode_extensions(value: &str) -> Result<RedactionExtensions, RedactionError> {
    serde_json::from_str(value).map_err(|_| integrity("stored redaction extensions are invalid"))
}

struct MaintenanceLock {
    file: File,
}

impl Drop for MaintenanceLock {
    fn drop(&mut self) {
        let _ = fs2::FileExt::unlock(&self.file);
    }
}

fn acquire_maintenance_lock(beads_dir: &Path) -> Result<MaintenanceLock, RedactionError> {
    std::fs::create_dir_all(beads_dir)
        .map_err(|_| integrity("could not create workspace maintenance lock directory"))?;
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(beads_dir.join("maintenance.lock"))
        .map_err(|_| integrity("could not open workspace maintenance lock"))?;
    let deadline = Instant::now() + LOCK_TIMEOUT;
    loop {
        match file.try_lock_exclusive() {
            Ok(()) => return Ok(MaintenanceLock { file }),
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    return Err(integrity("workspace maintenance lock is busy"));
                }
                std::thread::sleep(LOCK_POLL);
            }
            Err(_) => return Err(integrity("could not acquire workspace maintenance lock")),
        }
    }
}

fn integrity(message: &str) -> RedactionError {
    RedactionError::Integrity(message.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::migrations;

    fn connection() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        migrations::apply_migrations(&conn).unwrap();
        conn.execute(
            "INSERT INTO workspace (id, uuid, prefix, layout_version, created_at)
             VALUES (1, 'redaction-test-store', 'redact', 1, '2026-09-03T00:00:00Z')",
            [],
        )
        .unwrap();
        conn
    }

    fn shaped_value() -> String {
        ["AK", "IA", "7Q9W2E4R6T8Y1U3I"].concat()
    }

    fn insert_issue(conn: &rusqlite::Connection, description: &str) {
        conn.execute(
            "INSERT INTO issues (
                id, title, description, notes, priority, issue_type,
                base_status, created_at, updated_at, revision
             ) VALUES ('redact-1', 'title', ?1, 'notes', 2, 'task', 'open',
                       '2026-09-03T00:00:00Z', '2026-09-03T00:00:00Z', 1)",
            [description],
        )
        .unwrap();
    }

    fn issue_fingerprint(conn: &rusqlite::Connection) -> String {
        crate::service::secret_diagnostics::scan_live_findings(conn)
            .expect("scan live rows")
            .into_iter()
            .find(|finding| finding.field_path == "description")
            .unwrap()
            .fingerprint
    }

    #[test]
    fn failure_before_commit_rolls_back_every_effect() {
        let conn = connection();
        let value = shaped_value();
        insert_issue(&conn, &value);
        let fingerprint = issue_fingerprint(&conn);
        let error = redact_in_transaction(
            &conn,
            &fingerprint,
            "operator",
            "remove exposed credential",
            true,
            None,
        )
        .unwrap_err();
        assert!(matches!(error, RedactionError::Integrity(_)));

        let description: String = conn
            .query_row(
                "SELECT description FROM issues WHERE id = 'redact-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let counts: (i64, i64, i64) = conn
            .query_row(
                "SELECT
                    (SELECT COUNT(*) FROM redaction_receipts),
                    (SELECT COUNT(*) FROM redaction_tombstones),
                    (SELECT COUNT(*) FROM events)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(description, value);
        assert_eq!(counts, (0, 0, 0));
    }

    #[test]
    fn stale_preflight_location_conflicts_without_mutation() {
        let conn = connection();
        let value = shaped_value();
        insert_issue(&conn, &value);
        let fingerprint = issue_fingerprint(&conn);
        let expected = find_live_finding(&conn, &fingerprint).unwrap().unwrap();
        conn.execute(
            "UPDATE issues SET description = 'changed before transaction' WHERE id = 'redact-1'",
            [],
        )
        .unwrap();

        let error = redact_in_transaction(
            &conn,
            &fingerprint,
            "operator",
            "remove exposed credential",
            false,
            Some(&expected),
        )
        .unwrap_err();
        assert!(matches!(error, RedactionError::Conflict(_)));
        let receipts: i64 = conn
            .query_row("SELECT COUNT(*) FROM redaction_receipts", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(receipts, 0);
    }

    #[test]
    fn secret_bearing_reason_is_rejected_without_echoing_it() {
        let value = shaped_value();
        let error =
            validate_request(&"a".repeat(64), "operator", &format!("because {value}")).unwrap_err();
        let rendered = format!("{error:?} {error}");
        assert!(matches!(error, RedactionError::Usage(_)));
        assert!(!rendered.contains(&value));
    }
}

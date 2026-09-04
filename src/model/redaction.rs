//! Redaction storage model for the audited historical-redaction contract.
//!
//! This module implements the durable record types behind ADR-015 and the
//! `historical-redaction-v1` specification: scanner findings, acknowledgments,
//! field selectors, redaction receipts and epochs, and anti-resurrection
//! tombstones.
//!
//! The governing invariant of the v1-defined fields is that **the removed
//! bytes are never representable**. A redaction record can name where content
//! was (a selector), what it was (a fingerprint of a scanner rule match), and
//! what the surrounding record hashed to before and after — no defined field
//! holds the matched value, and the fixed replacement marker is the only
//! replacement content the module defines.
//!
//! Every record preserves unknown fields from newer writers in an opaque,
//! bounded extension map. This satisfies the native checkpoint round-trip
//! contract without giving extensions any v1 semantics or including them in
//! canonical identities. Extensions remain scanner-visible recovery input;
//! the sanitized-publication gate must reject secret-bearing extensions just
//! as it rejects secret-bearing fields in any other record. The complementary
//! guarantee — that a record type this reader does not know about at all is
//! rejected rather than silently dropped — lives in the checkpoint record
//! dispatcher, because silently dropping a tombstone is exactly how redacted
//! bytes come back.
//!
//! See: research/specs/historical-redaction-v1.md

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt;

/// Opaque additive fields preserved for forward-compatible checkpoint replay.
pub type RedactionExtensions = BTreeMap<String, serde_json::Value>;

/// Maximum serialized size of one record's additive fields.
pub const MAX_EXTENSION_BYTES: usize = 4 * 1024 * 1024;

/// Schema identity for a stored scanner finding.
pub const SCHEMA_REDACTION_FINDING: &str = "urn:bead-rs:schema:redaction-finding:native-v1";

/// Schema identity for an advisory-finding acknowledgment.
pub const SCHEMA_REDACTION_ACKNOWLEDGMENT: &str =
    "urn:bead-rs:schema:redaction-acknowledgment:native-v1";

/// Schema identity for the field selector embedded in findings and receipts.
pub const SCHEMA_REDACTION_FIELD_SELECTOR: &str =
    "urn:bead-rs:schema:redaction-field-selector:native-v1";

/// Schema identity for a committed redaction receipt.
pub const SCHEMA_REDACTION_RECEIPT: &str = "urn:bead-rs:schema:redaction-receipt:native-v1";

/// Schema identity for a redaction publication epoch.
pub const SCHEMA_REDACTION_EPOCH: &str = "urn:bead-rs:schema:redaction-epoch:native-v1";

/// Schema identity for a durable anti-resurrection tombstone.
pub const SCHEMA_REDACTION_TOMBSTONE: &str = "urn:bead-rs:schema:redaction-tombstone:native-v1";

/// Schema identity for the `historical_redaction` audit event detail.
pub const SCHEMA_REDACTION_EVENT: &str = "urn:bead-rs:schema:redaction-event:native-v1";

/// The only replacement content this contract ever writes.
///
/// A redaction replaces a finding's exact byte range with this marker and
/// changes no other byte. It is fixed by the specification, so every reader
/// — scanner, conformance harness, or a human reviewing a diff — sees the
/// same string and no redaction can smuggle in chosen replacement text.
pub const REDACTION_MARKER: &str = "[REDACTED:bead-rs]";

/// Longest accepted operator-supplied reason, in bytes.
///
/// A reason travels into the receipt, the audit event, and the published
/// checkpoint, so it is bounded rather than unbounded operator text.
pub const MAX_REASON_BYTES: usize = 1024;

/// Longest accepted actor identity, in bytes.
pub const MAX_ACTOR_BYTES: usize = 255;

/// Longest accepted field path, in bytes.
pub const MAX_FIELD_PATH_BYTES: usize = 256;

/// Publication lifecycle of a receipt or the epoch that carries it.
///
/// `Committed` means the semantic redaction is durable in SQLite but its
/// sanitized checkpoint generation has not been published; that is the
/// resumable state `bead redact --resume` targets. `Published` means the
/// sanitized pointer pair is durable. `Discarded` means the epoch published
/// without this receipt's target surviving revalidation — recorded, never
/// deleted, so the audit trail stays complete.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicationState {
    /// Durable in SQLite, checkpoint generation not yet published.
    Committed,
    /// Sanitized generation published and the pointer pair durable.
    Published,
    /// The epoch published without this target; kept for the audit trail.
    Discarded,
}

impl PublicationState {
    /// Parse from the serialized form, rejecting unknown states.
    pub fn parse(s: &str) -> Result<Self, RedactionError> {
        match s {
            "committed" => Ok(PublicationState::Committed),
            "published" => Ok(PublicationState::Published),
            "discarded" => Ok(PublicationState::Discarded),
            _ => Err(RedactionError::Usage(format!(
                "Unknown publication state: {}",
                s
            ))),
        }
    }

    /// Canonical serialized form.
    pub fn as_str(&self) -> &'static str {
        match self {
            PublicationState::Committed => "committed",
            PublicationState::Published => "published",
            PublicationState::Discarded => "discarded",
        }
    }

    /// Whether the semantic change is durable but the checkpoint is not.
    ///
    /// Only a committed-and-unpublished receipt is resumable; replaying a
    /// published receipt is a no-op, and a discarded one must not be retried.
    pub fn is_unpublished(&self) -> bool {
        matches!(self, PublicationState::Committed)
    }
}

impl fmt::Display for PublicationState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Scanner severity class of a finding.
///
/// A blocking finding is rejected outright by a mutating command under
/// ADR-014; an advisory one is reported and may be acknowledged. Severity
/// travels with the finding so a stored receipt records how certain the
/// match was without recording what matched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingSeverity {
    /// High-confidence match; the mutation is refused.
    Blocking,
    /// Lower-confidence match; reported and acknowledged, not refused.
    Advisory,
}

impl FindingSeverity {
    /// Parse from the serialized form, rejecting unknown severities.
    pub fn parse(s: &str) -> Result<Self, RedactionError> {
        match s {
            "blocking" => Ok(FindingSeverity::Blocking),
            "advisory" => Ok(FindingSeverity::Advisory),
            _ => Err(RedactionError::Usage(format!(
                "Unknown finding severity: {}",
                s
            ))),
        }
    }

    /// Canonical serialized form.
    pub fn as_str(&self) -> &'static str {
        match self {
            FindingSeverity::Blocking => "blocking",
            FindingSeverity::Advisory => "advisory",
        }
    }
}

impl fmt::Display for FindingSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Where in a record a finding's bytes sit.
///
/// A selector is the address of removed content, never the content: record
/// kind, the record's origin identity, a dotted field path, and a byte range
/// within that field's UTF-8 encoding, plus the hash of the whole record as
/// it stood before the redaction. Byte offsets are byte counts into the
/// field, not character counts, so a selector survives non-ASCII content and
/// still selects the same bytes on revalidation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldSelector {
    /// Schema identity of the selector envelope.
    #[serde(rename = "$schema")]
    pub schema_ref: String,

    /// Kind of record addressed (`issue`, `event`, `comment`, …).
    pub record_kind: String,

    /// Origin identity of the addressed record.
    ///
    /// An issue addresses by ID; an event addresses by
    /// `<origin_store_uuid>:<origin_event_sequence>`, which is the identity
    /// that survives a restore into a different local sequence.
    pub origin_identity: String,

    /// Dotted path to the field within the record (e.g. `description`,
    /// `detail.api_key`). Segment names are lowercase identifiers; numeric
    /// segments address sequence and map elements.
    pub field_path: String,

    /// First byte of the finding within the field, zero-based.
    pub byte_start: i64,

    /// Length of the finding in bytes.
    pub byte_length: i64,

    /// SHA-256 of the addressed record as it stood before redaction.
    ///
    /// This is the stale-detection key: if the record no longer hashes to
    /// this value the range no longer holds the matched bytes and the
    /// redaction conflicts instead of mutating.
    pub prior_record_hash: String,

    /// Additive fields written by a newer schema version.
    #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extensions: RedactionExtensions,
}

impl FieldSelector {
    /// Validate the selector, including that the byte range is well-formed.
    pub fn validate(&self) -> Result<(), RedactionError> {
        validate_exact_schema_ref(
            &self.schema_ref,
            SCHEMA_REDACTION_FIELD_SELECTOR,
            "selector schema_ref",
        )?;
        validate_record_kind(&self.record_kind)?;
        validate_origin_identity(&self.origin_identity)?;
        validate_field_path(&self.field_path)?;
        validate_hash(&self.prior_record_hash, "selector prior_record_hash")?;
        validate_extensions(
            &self.extensions,
            &[
                "$schema",
                "record_kind",
                "origin_identity",
                "field_path",
                "byte_start",
                "byte_length",
                "prior_record_hash",
            ],
            "selector",
        )?;

        if self.byte_start < 0 {
            return Err(RedactionError::Usage(
                "selector byte_start cannot be negative".to_string(),
            ));
        }
        if self.byte_length <= 0 {
            return Err(RedactionError::Usage(
                "selector byte_length must be positive".to_string(),
            ));
        }
        if self.byte_start.saturating_add(self.byte_length) > MAX_FIELD_BYTES as i64 {
            return Err(RedactionError::Usage(format!(
                "selector byte range exceeds the {}-byte field bound",
                MAX_FIELD_BYTES
            )));
        }
        Ok(())
    }

    /// Canonical identity of the addressed location.
    ///
    /// Two selectors address the same place exactly when this string is
    /// equal, so it keys both tombstones and exactly-once checks without
    /// relying on struct field order.
    pub fn location_key(&self) -> String {
        format!(
            "{}\u{1f}{}\u{1f}{}",
            self.record_kind, self.origin_identity, self.field_path
        )
    }
}

/// Upper bound on a selectable field, in bytes.
///
/// Every operator-text field the store accepts is already bounded well below
/// this; the bound exists so a selector can never describe a range wider
/// than any real field, which would let a malformed receipt claim content it
/// did not address.
pub const MAX_FIELD_BYTES: usize = 4 * 1024 * 1024;

/// A stored scanner finding.
///
/// Findings are the input to a redaction: the caller supplies only the
/// fingerprint, and the store revalidates it against a live finding before
/// anything is replaced. A finding records the rule that matched and where,
/// never the value that matched.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedactionFinding {
    /// Schema identity of the finding record.
    #[serde(rename = "$schema")]
    pub schema_ref: String,

    /// Canonical fingerprint of this finding.
    ///
    /// The only handle a caller ever holds: it is derived from the rule and
    /// the selector, so naming a finding in argv leaks nothing about the
    /// matched bytes.
    pub fingerprint: String,

    /// Version of the scanner ruleset that produced the finding.
    pub ruleset_version: u32,

    /// Identifier of the individual rule that matched.
    pub rule_id: String,

    /// Where the matched bytes sit.
    pub selector: FieldSelector,

    /// Whether the finding blocks mutation or is advisory only.
    pub severity: FindingSeverity,

    /// When the scan produced the finding (RFC 3339).
    pub detected_at: String,

    /// Additive fields written by a newer schema version.
    #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extensions: RedactionExtensions,
}

impl RedactionFinding {
    /// Validate the finding, including its embedded selector.
    pub fn validate(&self) -> Result<(), RedactionError> {
        validate_exact_schema_ref(
            &self.schema_ref,
            SCHEMA_REDACTION_FINDING,
            "finding schema_ref",
        )?;
        validate_fingerprint(&self.fingerprint)?;
        if self.ruleset_version == 0 {
            return Err(RedactionError::Usage(
                "ruleset_version must be positive".to_string(),
            ));
        }
        validate_nonempty_bounded(&self.rule_id, "rule_id", 128)?;
        self.selector.validate()?;
        validate_timestamp(&self.detected_at, "finding detected_at")?;
        validate_extensions(
            &self.extensions,
            &[
                "$schema",
                "fingerprint",
                "ruleset_version",
                "rule_id",
                "selector",
                "severity",
                "detected_at",
            ],
            "finding",
        )
    }
}

/// An acknowledgment of an advisory finding.
///
/// Acknowledging records that a lower-confidence match was reviewed and
/// accepted, so a scanner run can distinguish "known and accepted" from
/// "not yet looked at" without anyone storing the matched value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedactionAcknowledgment {
    /// Schema identity of the acknowledgment record.
    #[serde(rename = "$schema")]
    pub schema_ref: String,

    /// Fingerprint of the acknowledged finding.
    pub fingerprint: String,

    /// Identity of whoever acknowledged it.
    pub actor: String,

    /// Nonsecret, bounded reason for accepting the finding.
    pub reason: String,

    /// When it was acknowledged (RFC 3339).
    pub acknowledged_at: String,

    /// Additive fields written by a newer schema version.
    #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extensions: RedactionExtensions,
}

impl RedactionAcknowledgment {
    /// Validate the acknowledgment.
    pub fn validate(&self) -> Result<(), RedactionError> {
        validate_exact_schema_ref(
            &self.schema_ref,
            SCHEMA_REDACTION_ACKNOWLEDGMENT,
            "acknowledgment schema_ref",
        )?;
        validate_fingerprint(&self.fingerprint)?;
        validate_actor(&self.actor)?;
        validate_reason(&self.reason)?;
        validate_timestamp(&self.acknowledged_at, "acknowledgment acknowledged_at")?;
        validate_extensions(
            &self.extensions,
            &[
                "$schema",
                "fingerprint",
                "actor",
                "reason",
                "acknowledged_at",
            ],
            "acknowledgment",
        )
    }
}

/// A committed historical redaction.
///
/// The receipt is the accountability record for one replaced byte range. It
/// carries the rule and selector, the record hash before and after, who
/// ordered it and why, and the publication state of the epoch that carries
/// it. It never carries the removed bytes, and it is the record an exactly-
/// once replay returns instead of mutating a second time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedactionReceipt {
    /// Schema identity of the receipt record.
    #[serde(rename = "$schema")]
    pub schema_ref: String,

    /// Canonical receipt identity.
    ///
    /// Derived from [`RedactionReceipt::canonical_identity`], so the same
    /// semantic redaction always produces the same receipt ID and a replay
    /// is detectable as a duplicate rather than applied twice.
    pub receipt_id: String,

    /// Fingerprint of the finding that was redacted.
    pub finding_fingerprint: String,

    /// Version of the ruleset that produced the finding.
    pub ruleset_version: u32,

    /// Identifier of the rule that matched.
    pub rule_id: String,

    /// Where the replaced bytes sat.
    pub selector: FieldSelector,

    /// SHA-256 of the record before the replacement.
    ///
    /// Duplicates `selector.prior_record_hash` at the receipt's own level so
    /// the before/after pair is readable as one fact without parsing the
    /// selector; the two must always agree.
    pub prior_record_hash: String,

    /// SHA-256 of the record after the replacement.
    pub sanitized_record_hash: String,

    /// Identity of whoever ordered the redaction.
    pub actor: String,

    /// Nonsecret, bounded reason for the redaction.
    pub reason: String,

    /// When the redaction committed (RFC 3339).
    pub redacted_at: String,

    /// Issue revision the affected issue advanced to, when the redaction
    /// changed an issue materialization.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub affected_issue_revision: Option<i64>,

    /// Publication state of this receipt within its epoch.
    pub publication_state: PublicationState,

    /// Sanitized checkpoint generation that published this receipt.
    ///
    /// A generation ID is used instead of its content hash: embedding the
    /// hash of a checkpoint inside a record contained by that checkpoint
    /// would create an unsatisfiable self-reference.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resulting_generation_id: Option<String>,

    /// Epoch that owns this receipt's publication, once one is open.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub epoch_id: Option<String>,

    /// Additive fields written by a newer schema version.
    #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extensions: RedactionExtensions,
}

impl RedactionReceipt {
    /// Validate the receipt and its internal consistency.
    pub fn validate(&self) -> Result<(), RedactionError> {
        validate_exact_schema_ref(
            &self.schema_ref,
            SCHEMA_REDACTION_RECEIPT,
            "receipt schema_ref",
        )?;
        validate_hash(&self.receipt_id, "receipt_id")?;
        validate_fingerprint(&self.finding_fingerprint)?;
        if self.ruleset_version == 0 {
            return Err(RedactionError::Usage(
                "ruleset_version must be positive".to_string(),
            ));
        }
        validate_nonempty_bounded(&self.rule_id, "rule_id", 128)?;
        self.selector.validate()?;
        validate_hash(&self.prior_record_hash, "prior_record_hash")?;
        validate_hash(&self.sanitized_record_hash, "sanitized_record_hash")?;
        validate_actor(&self.actor)?;
        validate_reason(&self.reason)?;
        validate_timestamp(&self.redacted_at, "receipt redacted_at")?;
        if let Some(revision) = self.affected_issue_revision {
            if revision < 1 {
                return Err(RedactionError::Usage(
                    "affected_issue_revision must be positive".to_string(),
                ));
            }
        }
        if self.prior_record_hash == self.sanitized_record_hash {
            return Err(RedactionError::Integrity(
                "receipt records an identical prior and sanitized record hash".to_string(),
            ));
        }
        if self.prior_record_hash != self.selector.prior_record_hash {
            return Err(RedactionError::Integrity(
                "receipt prior_record_hash disagrees with its selector".to_string(),
            ));
        }
        if let Some(epoch) = &self.epoch_id {
            validate_hash(epoch, "epoch_id")?;
        }
        if let Some(generation) = &self.resulting_generation_id {
            validate_generation_id(generation, "receipt resulting_generation_id")?;
        }
        if self.publication_state == PublicationState::Published
            && self.resulting_generation_id.is_none()
        {
            return Err(RedactionError::Integrity(
                "published receipt records no resulting generation".to_string(),
            ));
        }
        validate_extensions(
            &self.extensions,
            &[
                "$schema",
                "receipt_id",
                "finding_fingerprint",
                "ruleset_version",
                "rule_id",
                "selector",
                "prior_record_hash",
                "sanitized_record_hash",
                "actor",
                "reason",
                "redacted_at",
                "affected_issue_revision",
                "publication_state",
                "resulting_generation_id",
                "epoch_id",
            ],
            "receipt",
        )
    }

    /// The canonical identity of a redaction.
    ///
    /// Every field that makes two redactions the same redaction, in a fixed
    /// order, NUL-separated. Publication state and epoch linkage are
    /// deliberately excluded: a receipt that is later published, or resumed,
    /// keeps the identity it committed with, which is what lets `--resume`
    /// and an exact replay both find the same row.
    #[allow(clippy::too_many_arguments)]
    pub fn canonical_identity(
        finding_fingerprint: &str,
        ruleset_version: u32,
        rule_id: &str,
        selector: &FieldSelector,
        sanitized_record_hash: &str,
        actor: &str,
        reason: &str,
        redacted_at: &str,
        affected_issue_revision: Option<i64>,
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(b"redaction-receipt-v1");
        hasher.update(b"\0");
        for part in [
            finding_fingerprint,
            rule_id,
            selector.record_kind.as_str(),
            selector.origin_identity.as_str(),
            selector.field_path.as_str(),
            selector.prior_record_hash.as_str(),
            &selector.byte_start.to_string(),
            &selector.byte_length.to_string(),
            sanitized_record_hash,
            actor,
            reason,
            redacted_at,
            &affected_issue_revision
                .map(|revision| revision.to_string())
                .unwrap_or_default(),
        ] {
            hasher.update(part.as_bytes());
            hasher.update(b"\0");
        }
        hasher.update(ruleset_version.to_be_bytes());
        hasher.update(b"\0");
        format!("{:x}", hasher.finalize())
    }

    /// Recompute this receipt's own canonical identity.
    ///
    /// Returns an error when the stored `receipt_id` does not match, so a
    /// receipt whose identity was tampered with is caught on read rather
    /// than trusted.
    pub fn verify_identity(&self) -> Result<(), RedactionError> {
        let expected = Self::canonical_identity(
            &self.finding_fingerprint,
            self.ruleset_version,
            &self.rule_id,
            &self.selector,
            &self.sanitized_record_hash,
            &self.actor,
            &self.reason,
            &self.redacted_at,
            self.affected_issue_revision,
        );
        if expected != self.receipt_id {
            return Err(RedactionError::Integrity(format!(
                "receipt identity mismatch (declared {}, canonical {})",
                self.receipt_id, expected
            )));
        }
        Ok(())
    }
}

/// A redaction publication epoch.
///
/// One epoch covers every receipt published together as one sanitized
/// generation set. It records whether the dirty previous generation was
/// deliberately reset — the exceptional `previous.json` behaviour ADR-015
/// allows — and which superseded objects were tombstoned afterwards, so a
/// later audit can tell an ordinary publication from a redaction publication
/// without recovering anything secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedactionEpoch {
    /// Schema identity of the epoch record.
    #[serde(rename = "$schema")]
    pub schema_ref: String,

    /// Canonical epoch identity.
    pub epoch_id: String,

    /// Receipts carried by this epoch, in canonical order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub receipt_ids: Vec<String>,

    /// Publication state of the epoch as a whole.
    pub publication_state: PublicationState,

    /// Identity of the sanitized generation this epoch published, once known.
    ///
    /// This is deliberately the generation ID rather than a root hash; the
    /// epoch itself is part of the content-addressed root, so storing that
    /// root in the epoch would be self-referential.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resulting_generation_id: Option<String>,

    /// Whether the dirty previous generation was reset instead of retained.
    pub previous_generation_reset: bool,

    /// Generation identities superseded by this epoch.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub superseded_generations: Vec<String>,

    /// When the epoch was opened (RFC 3339).
    pub opened_at: String,

    /// When the epoch's pointer pair became durable, once published.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_at: Option<String>,

    /// Additive fields written by a newer schema version.
    #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extensions: RedactionExtensions,
}

impl RedactionEpoch {
    /// Validate the epoch.
    pub fn validate(&self) -> Result<(), RedactionError> {
        validate_exact_schema_ref(&self.schema_ref, SCHEMA_REDACTION_EPOCH, "epoch schema_ref")?;
        validate_hash(&self.epoch_id, "epoch_id")?;
        if self.receipt_ids.is_empty() {
            return Err(RedactionError::Usage(
                "epoch must contain at least one receipt_id".to_string(),
            ));
        }
        for receipt_id in &self.receipt_ids {
            validate_hash(receipt_id, "epoch receipt_id")?;
        }
        if !self.receipt_ids.windows(2).all(|pair| pair[0] < pair[1]) {
            return Err(RedactionError::Usage(
                "epoch receipt_ids must be sorted and unique".to_string(),
            ));
        }
        for generation in &self.superseded_generations {
            if generation.trim().is_empty() || generation.len() > 128 {
                return Err(RedactionError::Usage(
                    "epoch superseded generation identity is empty or over 128 bytes".to_string(),
                ));
            }
        }
        if let Some(generation) = &self.resulting_generation_id {
            validate_generation_id(generation, "epoch resulting_generation_id")?;
        }
        validate_timestamp(&self.opened_at, "epoch opened_at")?;
        if let Some(published) = &self.published_at {
            validate_timestamp(published, "epoch published_at")?;
            if self.publication_state == PublicationState::Committed {
                return Err(RedactionError::Integrity(
                    "epoch records a publication time while still committed".to_string(),
                ));
            }
        }
        if self.publication_state == PublicationState::Published && self.published_at.is_none() {
            return Err(RedactionError::Integrity(
                "epoch is published but records no publication time".to_string(),
            ));
        }
        if self.publication_state == PublicationState::Published
            && self.resulting_generation_id.is_none()
        {
            return Err(RedactionError::Integrity(
                "epoch is published but records no resulting generation".to_string(),
            ));
        }
        let expected = Self::identity_for(&self.receipt_ids);
        if self.epoch_id != expected {
            return Err(RedactionError::Integrity(format!(
                "epoch identity mismatch (declared {}, canonical {})",
                self.epoch_id, expected
            )));
        }
        validate_extensions(
            &self.extensions,
            &[
                "$schema",
                "epoch_id",
                "receipt_ids",
                "publication_state",
                "resulting_generation_id",
                "previous_generation_reset",
                "superseded_generations",
                "opened_at",
                "published_at",
            ],
            "epoch",
        )
    }

    /// Derive the epoch identity from the receipts it carries.
    ///
    /// An epoch is identified by its receipt set, so reopening publication
    /// for the same set finds the same epoch rather than opening a second
    /// one over the same redactions.
    pub fn identity_for(receipt_ids: &[String]) -> String {
        let mut sorted: Vec<&String> = receipt_ids.iter().collect();
        sorted.sort();
        let mut hasher = Sha256::new();
        hasher.update(b"redaction-epoch-v1");
        hasher.update(b"\0");
        for receipt_id in sorted {
            hasher.update(receipt_id.as_bytes());
            hasher.update(b"\0");
        }
        format!("{:x}", hasher.finalize())
    }
}

/// A durable anti-resurrection tombstone.
///
/// Recovery precedence is the reason this type exists: an older valid
/// checkpoint is otherwise able to reintroduce redacted bytes, because the
/// bytes are simply what that checkpoint recorded. A tombstone names the
/// location and the pre-redaction record hash, so an import, merge,
/// reconcile, or restore can recognize incoming pre-redaction content and
/// refuse to let it overwrite the sanitized record.
///
/// Tombstones are keyed by origin record identity rather than local row IDs
/// so they keep working across a restore into a different workspace, where
/// local sequence numbers are reassigned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResurrectionTombstone {
    /// Schema identity of the tombstone record.
    #[serde(rename = "$schema")]
    pub schema_ref: String,

    /// Canonical tombstone identity.
    pub tombstone_id: String,

    /// Kind of record the tombstone guards.
    pub record_kind: String,

    /// Origin identity of the guarded record.
    pub origin_identity: String,

    /// Field path the redaction touched.
    pub field_path: String,

    /// SHA-256 of the record as it stood before redaction.
    ///
    /// Incoming content hashing to this value is pre-redaction content and
    /// is refused; the sanitized record hashes differently, so it passes.
    pub prior_record_hash: String,

    /// Fingerprint of the finding that drove the redaction.
    pub finding_fingerprint: String,

    /// Epoch whose publication made this tombstone durable.
    pub epoch_id: String,

    /// When the tombstone became durable (RFC 3339).
    pub created_at: String,

    /// Additive fields written by a newer schema version.
    #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extensions: RedactionExtensions,
}

impl ResurrectionTombstone {
    /// Validate the tombstone.
    pub fn validate(&self) -> Result<(), RedactionError> {
        validate_exact_schema_ref(
            &self.schema_ref,
            SCHEMA_REDACTION_TOMBSTONE,
            "tombstone schema_ref",
        )?;
        validate_hash(&self.tombstone_id, "tombstone_id")?;
        validate_record_kind(&self.record_kind)?;
        validate_origin_identity(&self.origin_identity)?;
        validate_field_path(&self.field_path)?;
        validate_hash(&self.prior_record_hash, "tombstone prior_record_hash")?;
        validate_fingerprint(&self.finding_fingerprint)?;
        validate_hash(&self.epoch_id, "tombstone epoch_id")?;
        validate_timestamp(&self.created_at, "tombstone created_at")?;
        let expected = Self::identity_for(
            &self.record_kind,
            &self.origin_identity,
            &self.field_path,
            &self.prior_record_hash,
            &self.finding_fingerprint,
            &self.epoch_id,
        );
        if self.tombstone_id != expected {
            return Err(RedactionError::Integrity(format!(
                "tombstone identity mismatch (declared {}, canonical {})",
                self.tombstone_id, expected
            )));
        }
        validate_extensions(
            &self.extensions,
            &[
                "$schema",
                "tombstone_id",
                "record_kind",
                "origin_identity",
                "field_path",
                "prior_record_hash",
                "finding_fingerprint",
                "epoch_id",
                "created_at",
            ],
            "tombstone",
        )
    }

    /// The recovery-precedence key for incoming content.
    ///
    /// Recovery compares this key for incoming records against durable
    /// tombstones: a match means the incoming bytes predate a known
    /// redaction.
    pub fn precedence_key(&self) -> String {
        format!(
            "{}\u{1f}{}\u{1f}{}\u{1f}{}",
            self.record_kind, self.origin_identity, self.field_path, self.prior_record_hash
        )
    }

    /// Derive the tombstone identity from its guarding facts.
    ///
    /// Derived so the same redaction recovered into a fresh workspace
    /// produces the same tombstone, and so a tombstone cannot be re-keyed to
    /// guard a different location without changing its identity.
    pub fn identity_for(
        record_kind: &str,
        origin_identity: &str,
        field_path: &str,
        prior_record_hash: &str,
        finding_fingerprint: &str,
        epoch_id: &str,
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(b"redaction-tombstone-v1");
        hasher.update(b"\0");
        for part in [
            record_kind,
            origin_identity,
            field_path,
            prior_record_hash,
            finding_fingerprint,
            epoch_id,
        ] {
            hasher.update(part.as_bytes());
            hasher.update(b"\0");
        }
        format!("{:x}", hasher.finalize())
    }
}

/// Validation and storage errors for redaction records.
///
/// Exit codes follow the `historical-redaction-v1` output contract: 2 for a
/// rejected actor, reason, or fingerprint; 3 for a missing target; 4 for a
/// stale target or semantic conflict; 1 for an internal or committed-but-
/// unpublished failure.
#[derive(Debug, Clone, thiserror::Error)]
pub enum RedactionError {
    /// Invalid actor, reason, fingerprint, or selector (exit 2).
    #[error("Usage error: {0}")]
    Usage(String),

    /// Addressed record or finding does not exist (exit 3).
    #[error("Not found: {0}")]
    NotFound(String),

    /// Stale range, hash, or fingerprint; no mutation applied (exit 4).
    #[error("Conflict: {0}")]
    Conflict(String),

    /// Receipt identity, hash, or state inconsistency (exit 1).
    #[error("Integrity error: {0}")]
    Integrity(String),
}

impl RedactionError {
    /// Map to the contract's exit code.
    pub fn exit_code(&self) -> i32 {
        match self {
            RedactionError::Usage(_) => 2,
            RedactionError::NotFound(_) => 3,
            RedactionError::Conflict(_) => 4,
            RedactionError::Integrity(_) => 1,
        }
    }
}

/// Validate a `$schema` identity against the redaction schema namespace.
fn validate_exact_schema_ref(
    value: &str,
    expected: &str,
    label: &str,
) -> Result<(), RedactionError> {
    validate_nonempty_bounded(value, label, 128)?;
    if value != expected {
        return Err(RedactionError::Usage(format!(
            "{} must be {} (got {})",
            label, expected, value
        )));
    }
    Ok(())
}

fn validate_timestamp(value: &str, label: &str) -> Result<(), RedactionError> {
    time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
        .map(|_| ())
        .map_err(|_| RedactionError::Usage(format!("{} must be RFC 3339", label)))
}

fn validate_generation_id(value: &str, label: &str) -> Result<(), RedactionError> {
    validate_nonempty_bounded(value, label, 128)?;
    if value.chars().any(char::is_control) {
        return Err(RedactionError::Usage(format!(
            "{} cannot contain control characters",
            label
        )));
    }
    Ok(())
}

fn validate_extensions(
    extensions: &RedactionExtensions,
    known_fields: &[&str],
    label: &str,
) -> Result<(), RedactionError> {
    if let Some(collision) = extensions
        .keys()
        .find(|key| known_fields.contains(&key.as_str()))
    {
        return Err(RedactionError::Integrity(format!(
            "{label} extension collides with known field {collision}"
        )));
    }
    let encoded = serde_json::to_vec(extensions).map_err(|error| {
        RedactionError::Integrity(format!("{label} extensions cannot be encoded: {error}"))
    })?;
    if encoded.len() > MAX_EXTENSION_BYTES {
        return Err(RedactionError::Usage(format!(
            "{label} extensions exceed the {MAX_EXTENSION_BYTES}-byte bound"
        )));
    }
    Ok(())
}

/// Validate a finding fingerprint.
///
/// A fingerprint is a lowercase hex SHA-256. Anything else cannot have come
/// from the scanner, and accepting it would let a caller address a finding
/// that was never produced.
fn validate_fingerprint(value: &str) -> Result<(), RedactionError> {
    validate_sha256_hex(value)
}

/// Validate a SHA-256 hex digest.
fn validate_hash(value: &str, label: &str) -> Result<(), RedactionError> {
    validate_sha256_hex(value).map_err(|_| {
        RedactionError::Usage(format!(
            "{} is not a lowercase hex SHA-256: {}",
            label, value
        ))
    })
}

fn validate_sha256_hex(value: &str) -> Result<(), RedactionError> {
    if value.len() != 64 {
        return Err(RedactionError::Usage(format!(
            "expected a 64-character SHA-256 hex digest, got {} characters",
            value.len()
        )));
    }
    if !value.bytes().all(|byte| byte.is_ascii_hexdigit())
        || value.bytes().any(|byte| byte.is_ascii_uppercase())
    {
        return Err(RedactionError::Usage(
            "expected a lowercase hex SHA-256 digest".to_string(),
        ));
    }
    Ok(())
}

/// Validate an actor identity.
pub(crate) fn validate_actor(value: &str) -> Result<(), RedactionError> {
    validate_nonempty_bounded(value, "actor", MAX_ACTOR_BYTES)?;
    if value.chars().any(char::is_control) {
        return Err(RedactionError::Usage(
            "actor cannot contain control characters".to_string(),
        ));
    }
    Ok(())
}

/// Validate an operator-supplied reason.
///
/// A reason is the one free-text field a redaction record carries, so it is
/// bounded, stripped of control characters, and required to be nonempty —
/// an unexplained destructive repair is not auditable. Whether the reason
/// itself contains a secret is the scanner's judgement (the caller's own
/// ruleset revalidates it before the mutation is offered); the storage layer
/// enforces only structure, so it never has to guess at content.
pub(crate) fn validate_reason(value: &str) -> Result<(), RedactionError> {
    validate_nonempty_bounded(value, "reason", MAX_REASON_BYTES)?;
    if value.chars().any(char::is_control) {
        return Err(RedactionError::Usage(
            "reason cannot contain control characters".to_string(),
        ));
    }
    Ok(())
}

/// Validate a record kind.
fn validate_record_kind(value: &str) -> Result<(), RedactionError> {
    validate_nonempty_bounded(value, "record_kind", 32)?;
    if !value
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    {
        return Err(RedactionError::Usage(format!(
            "record_kind must be lowercase letters, digits, or underscores: {}",
            value
        )));
    }
    Ok(())
}

/// Validate a record origin identity.
fn validate_origin_identity(value: &str) -> Result<(), RedactionError> {
    validate_nonempty_bounded(value, "origin_identity", 512)?;
    if value.chars().any(char::is_control) {
        return Err(RedactionError::Usage(
            "origin_identity cannot contain control characters".to_string(),
        ));
    }
    Ok(())
}

/// Validate a dotted field path.
///
/// Segments are lowercase identifiers or nonnegative decimal indexes,
/// joined by `.`. Constraining the shape keeps a field path unambiguous as
/// a selector component and stops it from doubling as a filesystem path or
/// an SQL fragment.
fn validate_field_path(value: &str) -> Result<(), RedactionError> {
    validate_nonempty_bounded(value, "field_path", MAX_FIELD_PATH_BYTES)?;
    let segment_ok = |segment: &str| {
        !segment.is_empty()
            && segment.len() <= 64
            && (segment.bytes().all(|byte| byte.is_ascii_digit())
                || (segment
                    .chars()
                    .next()
                    .map(|c| c.is_ascii_lowercase() || c == '_')
                    .unwrap_or(false)
                    && segment
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')))
    };
    if !value.split('.').all(segment_ok) {
        return Err(RedactionError::Usage(format!(
            "field_path must be dot-separated lowercase identifiers or indexes: {}",
            value
        )));
    }
    Ok(())
}

/// Require a nonempty, byte-bounded string.
fn validate_nonempty_bounded(
    value: &str,
    label: &str,
    max_bytes: usize,
) -> Result<(), RedactionError> {
    if value.trim().is_empty() {
        return Err(RedactionError::Usage(format!("{} cannot be empty", label)));
    }
    if value.len() > max_bytes {
        return Err(RedactionError::Usage(format!(
            "{} cannot exceed {} bytes (got {})",
            label,
            max_bytes,
            value.len()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn selector() -> FieldSelector {
        FieldSelector {
            schema_ref: SCHEMA_REDACTION_FIELD_SELECTOR.to_string(),
            record_kind: "issue".to_string(),
            origin_identity: "beadrs-402c24d7".to_string(),
            field_path: "description".to_string(),
            byte_start: 12,
            byte_length: 32,
            prior_record_hash: "a".repeat(64),
            extensions: RedactionExtensions::new(),
        }
    }

    fn receipt() -> RedactionReceipt {
        RedactionReceipt {
            schema_ref: SCHEMA_REDACTION_RECEIPT.to_string(),
            receipt_id: RedactionReceipt::canonical_identity(
                &"b".repeat(64),
                1,
                "provider-token",
                &selector(),
                &"c".repeat(64),
                "operator",
                "remove leaked credential",
                "2026-09-03T00:00:00Z",
                Some(7),
            ),
            finding_fingerprint: "b".repeat(64),
            ruleset_version: 1,
            rule_id: "provider-token".to_string(),
            selector: selector(),
            prior_record_hash: "a".repeat(64),
            sanitized_record_hash: "c".repeat(64),
            actor: "operator".to_string(),
            reason: "remove leaked credential".to_string(),
            redacted_at: "2026-09-03T00:00:00Z".to_string(),
            affected_issue_revision: Some(7),
            publication_state: PublicationState::Committed,
            resulting_generation_id: None,
            epoch_id: None,
            extensions: RedactionExtensions::new(),
        }
    }

    #[test]
    fn marker_is_the_specified_fixed_string() {
        assert_eq!(REDACTION_MARKER, "[REDACTED:bead-rs]");
    }

    #[test]
    fn receipt_identity_is_a_function_of_semantic_fields() {
        let first = receipt();
        let mut second = receipt();
        second.receipt_id = String::new();
        second.receipt_id = RedactionReceipt::canonical_identity(
            &second.finding_fingerprint,
            second.ruleset_version,
            &second.rule_id,
            &second.selector,
            &second.sanitized_record_hash,
            &second.actor,
            &second.reason,
            &second.redacted_at,
            second.affected_issue_revision,
        );
        assert_eq!(first.receipt_id, second.receipt_id);
    }

    #[test]
    fn receipt_identity_ignores_publication_state_and_epoch() {
        let mut published = receipt();
        published.publication_state = PublicationState::Published;
        published.epoch_id = Some("d".repeat(64));
        assert_eq!(receipt().receipt_id, published.receipt_id);
    }

    #[test]
    fn receipt_identity_distinguishes_a_different_reason() {
        let other = RedactionReceipt::canonical_identity(
            &"b".repeat(64),
            1,
            "provider-token",
            &selector(),
            &"c".repeat(64),
            "operator",
            "a different reason",
            "2026-09-03T00:00:00Z",
            Some(7),
        );
        assert_ne!(receipt().receipt_id, other);
    }

    #[test]
    fn receipt_verify_identity_accepts_and_rejects() {
        assert!(receipt().verify_identity().is_ok());

        let mut tampered = receipt();
        tampered.reason = "changed after the fact".to_string();
        assert!(matches!(
            tampered.verify_identity(),
            Err(RedactionError::Integrity(_))
        ));
    }

    #[test]
    fn receipt_rejects_matching_prior_and_sanitized_hashes() {
        let mut unchanged = receipt();
        unchanged.sanitized_record_hash = unchanged.prior_record_hash.clone();
        assert!(matches!(
            unchanged.validate(),
            Err(RedactionError::Integrity(_))
        ));
    }

    #[test]
    fn receipt_rejects_selector_hash_disagreement() {
        let mut disagreeing = receipt();
        disagreeing.selector.prior_record_hash = "e".repeat(64);
        assert!(matches!(
            disagreeing.validate(),
            Err(RedactionError::Integrity(_))
        ));
    }

    #[test]
    fn finding_accepts_the_scanners_opaque_fingerprint() {
        let finding = RedactionFinding {
            schema_ref: SCHEMA_REDACTION_FINDING.to_string(),
            fingerprint: "b".repeat(64),
            ruleset_version: 1,
            rule_id: "provider-token".to_string(),
            selector: selector(),
            severity: FindingSeverity::Blocking,
            detected_at: "2026-09-03T00:00:00Z".to_string(),
            extensions: RedactionExtensions::new(),
        };
        assert!(finding.validate().is_ok());
        assert_eq!(finding.fingerprint, "b".repeat(64));
    }

    #[test]
    fn selector_rejects_a_negative_or_empty_range() {
        let mut negative = selector();
        negative.byte_start = -1;
        assert!(negative.validate().is_err());

        let mut empty = selector();
        empty.byte_length = 0;
        assert!(empty.validate().is_err());
    }

    #[test]
    fn selector_rejects_a_range_wider_than_any_field() {
        let mut wide = selector();
        wide.byte_start = 0;
        wide.byte_length = MAX_FIELD_BYTES as i64 + 1;
        assert!(wide.validate().is_err());
    }

    #[test]
    fn field_path_accepts_identifiers_and_indexes_only() {
        let mut good = selector();
        good.field_path = "detail.recurrences.0.text".to_string();
        assert!(good.validate().is_ok());

        for path in ["", "..", "Detail", "description;drop", "a b"] {
            let mut bad = selector();
            bad.field_path = path.to_string();
            assert!(bad.validate().is_err(), "accepted {:?}", path);
        }
    }

    #[test]
    fn tombstone_precedence_key_covers_location_and_prior_hash() {
        let mut tombstone = ResurrectionTombstone {
            schema_ref: SCHEMA_REDACTION_TOMBSTONE.to_string(),
            tombstone_id: String::new(),
            record_kind: "issue".to_string(),
            origin_identity: "beadrs-402c24d7".to_string(),
            field_path: "description".to_string(),
            prior_record_hash: "a".repeat(64),
            finding_fingerprint: "b".repeat(64),
            epoch_id: "d".repeat(64),
            created_at: "2026-09-03T00:00:00Z".to_string(),
            extensions: RedactionExtensions::new(),
        };
        tombstone.tombstone_id = ResurrectionTombstone::identity_for(
            &tombstone.record_kind,
            &tombstone.origin_identity,
            &tombstone.field_path,
            &tombstone.prior_record_hash,
            &tombstone.finding_fingerprint,
            &tombstone.epoch_id,
        );
        assert!(tombstone.validate().is_ok());

        let mut different_hash = tombstone.clone();
        different_hash.prior_record_hash = "1".repeat(64);
        assert_ne!(
            tombstone.precedence_key(),
            different_hash.precedence_key(),
            "a sanitized record must not match a pre-redaction tombstone key"
        );
    }

    #[test]
    fn tombstone_identity_is_stable_across_recovery() {
        let derived = ResurrectionTombstone::identity_for(
            "issue",
            "beadrs-402c24d7",
            "description",
            &"a".repeat(64),
            &"b".repeat(64),
            &"d".repeat(64),
        );
        assert_eq!(derived, derived);
        assert_eq!(derived.len(), 64);
    }

    #[test]
    fn epoch_identity_is_order_independent_and_content_sensitive() {
        let one = RedactionEpoch::identity_for(&["a".repeat(64), "b".repeat(64)]);
        let two = RedactionEpoch::identity_for(&["b".repeat(64), "a".repeat(64)]);
        assert_eq!(one, two);

        let three = RedactionEpoch::identity_for(&["a".repeat(64)]);
        assert_ne!(one, three);
    }

    #[test]
    fn epoch_rejects_published_without_a_root_or_time() {
        let epoch = |state: PublicationState, generation: Option<String>, at: Option<String>| {
            let receipt_ids = vec!["f".repeat(64)];
            RedactionEpoch {
                schema_ref: SCHEMA_REDACTION_EPOCH.to_string(),
                epoch_id: RedactionEpoch::identity_for(&receipt_ids),
                receipt_ids,
                publication_state: state,
                resulting_generation_id: generation,
                previous_generation_reset: true,
                superseded_generations: vec![],
                opened_at: "2026-09-03T00:00:00Z".to_string(),
                published_at: at,
                extensions: RedactionExtensions::new(),
            }
        };

        assert!(epoch(PublicationState::Committed, None, None)
            .validate()
            .is_ok());
        assert!(epoch(
            PublicationState::Published,
            Some("gen-sanitized".to_string()),
            Some("2026-09-03T01:00:00Z".to_string())
        )
        .validate()
        .is_ok());
        assert!(matches!(
            epoch(PublicationState::Published, None, Some(String::new())).validate(),
            Err(RedactionError::Usage(_))
        ));
        assert!(matches!(
            epoch(
                PublicationState::Published,
                None,
                Some("2026-09-03T01:00:00Z".to_string())
            )
            .validate(),
            Err(RedactionError::Integrity(_))
        ));
    }

    #[test]
    fn reason_is_bounded_and_free_of_control_characters() {
        assert!(validate_reason("rotate the credential").is_ok());
        assert!(validate_reason("").is_err());
        assert!(validate_reason("   ").is_err());
        assert!(validate_reason(&"x".repeat(MAX_REASON_BYTES + 1)).is_err());
        assert!(validate_reason("line one\nline two").is_err());
    }

    #[test]
    fn actor_is_bounded() {
        assert!(validate_actor("needle-worker-alpha").is_ok());
        assert!(validate_actor("").is_err());
        assert!(validate_actor(&"a".repeat(MAX_ACTOR_BYTES + 1)).is_err());
    }

    #[test]
    fn hashes_must_be_lowercase_hex_sha256() {
        assert!(validate_hash(&"a".repeat(64), "x").is_ok());
        assert!(validate_hash(&"A".repeat(64), "x").is_err());
        assert!(validate_hash(&"g".repeat(64), "x").is_err());
        assert!(validate_hash(&"a".repeat(63), "x").is_err());
    }

    #[test]
    fn schema_refs_must_stay_in_the_redaction_namespace() {
        assert!(
            validate_exact_schema_ref(SCHEMA_REDACTION_RECEIPT, SCHEMA_REDACTION_RECEIPT, "x")
                .is_ok()
        );
        assert!(validate_exact_schema_ref(
            "urn:bead-rs:schema:provenance-receipt:native-v1",
            SCHEMA_REDACTION_RECEIPT,
            "x"
        )
        .is_err());
        assert!(validate_exact_schema_ref("", SCHEMA_REDACTION_RECEIPT, "x").is_err());
    }

    #[test]
    fn publication_state_round_trips() {
        for state in [
            PublicationState::Committed,
            PublicationState::Published,
            PublicationState::Discarded,
        ] {
            assert_eq!(PublicationState::parse(state.as_str()).unwrap(), state);
        }
        assert!(PublicationState::parse("publishing").is_err());
        assert!(PublicationState::Committed.is_unpublished());
        assert!(!PublicationState::Published.is_unpublished());
    }

    #[test]
    fn records_survive_a_serde_round_trip_and_tolerate_unknown_fields() {
        // A newer writer may add fields this reader has never heard of.
        // Tolerating them is what makes an additive schema change safe.
        let receipt = receipt();
        let mut json = serde_json::to_value(&receipt).unwrap();
        json["some_future_field"] = serde_json::json!({"nested": [1, 2, 3]});
        let parsed: RedactionReceipt = serde_json::from_value(json).unwrap();
        assert_eq!(
            parsed.extensions.get("some_future_field"),
            Some(&serde_json::json!({"nested": [1, 2, 3]}))
        );
        let tombstone = ResurrectionTombstone {
            schema_ref: SCHEMA_REDACTION_TOMBSTONE.to_string(),
            tombstone_id: "f".repeat(64),
            record_kind: "event".to_string(),
            origin_identity: "uuid:3".to_string(),
            field_path: "detail".to_string(),
            prior_record_hash: "a".repeat(64),
            finding_fingerprint: "b".repeat(64),
            epoch_id: "d".repeat(64),
            created_at: "2026-09-03T00:00:00Z".to_string(),
            extensions: RedactionExtensions::new(),
        };
        let mut tombstone_json = serde_json::to_value(&tombstone).unwrap();
        tombstone_json["future"] = serde_json::json!(true);
        let parsed: ResurrectionTombstone = serde_json::from_value(tombstone_json).unwrap();
        assert_eq!(
            parsed.extensions.get("future"),
            Some(&serde_json::json!(true))
        );
    }

    #[test]
    fn no_known_v1_record_field_carries_arbitrary_removed_content() {
        // The contract's headline guarantee, checked structurally: every
        // serialized key across every durable record type must be one of a
        // fixed set that cannot hold a removed value. Adding a free-text
        // field later means adding it to this list deliberately, not
        // silently.
        let allowed: &[&str] = &[
            "$schema",
            "acknowledged_at",
            "actor",
            "affected_issue_revision",
            "byte_length",
            "byte_start",
            "created_at",
            "detected_at",
            "epoch_id",
            "field_path",
            "finding_fingerprint",
            "fingerprint",
            "finding",
            "finding_fingerprint",
            "opened_at",
            "prior_record_hash",
            "publication_state",
            "published_at",
            "reason",
            "receipt_id",
            "receipt_ids",
            "record_kind",
            "redacted_at",
            "rule_id",
            "ruleset_version",
            "sanitized_record_hash",
            "resulting_generation_id",
            "schema_ref",
            "selector",
            "severity",
            "superseded_generations",
            "tombstone_id",
            "origin_identity",
            "previous_generation_reset",
        ];

        let selector = selector();
        let records: Vec<serde_json::Value> = vec![
            serde_json::to_value(&selector).unwrap(),
            serde_json::to_value(&RedactionFinding {
                schema_ref: SCHEMA_REDACTION_FINDING.to_string(),
                fingerprint: "b".repeat(64),
                ruleset_version: 1,
                rule_id: "provider-token".to_string(),
                selector: selector.clone(),
                severity: FindingSeverity::Blocking,
                detected_at: "2026-09-03T00:00:00Z".to_string(),
                extensions: RedactionExtensions::new(),
            })
            .unwrap(),
            serde_json::to_value(&RedactionAcknowledgment {
                schema_ref: SCHEMA_REDACTION_ACKNOWLEDGMENT.to_string(),
                fingerprint: "b".repeat(64),
                actor: "operator".to_string(),
                reason: "accepted".to_string(),
                acknowledged_at: "2026-09-03T00:00:00Z".to_string(),
                extensions: RedactionExtensions::new(),
            })
            .unwrap(),
            serde_json::to_value(receipt()).unwrap(),
            serde_json::to_value(&RedactionEpoch {
                schema_ref: SCHEMA_REDACTION_EPOCH.to_string(),
                epoch_id: "d".repeat(64),
                receipt_ids: vec!["f".repeat(64)],
                publication_state: PublicationState::Committed,
                resulting_generation_id: None,
                previous_generation_reset: true,
                superseded_generations: vec!["gen-abc".to_string()],
                opened_at: "2026-09-03T00:00:00Z".to_string(),
                published_at: None,
                extensions: RedactionExtensions::new(),
            })
            .unwrap(),
            serde_json::to_value(&ResurrectionTombstone {
                schema_ref: SCHEMA_REDACTION_TOMBSTONE.to_string(),
                tombstone_id: "f".repeat(64),
                record_kind: "issue".to_string(),
                origin_identity: "beadrs-402c24d7".to_string(),
                field_path: "description".to_string(),
                prior_record_hash: "a".repeat(64),
                finding_fingerprint: "b".repeat(64),
                epoch_id: "d".repeat(64),
                created_at: "2026-09-03T00:00:00Z".to_string(),
                extensions: RedactionExtensions::new(),
            })
            .unwrap(),
        ];

        for record in &records {
            let object = record.as_object().expect("records serialize to objects");
            for key in object.keys() {
                assert!(
                    allowed.contains(&key.as_str()),
                    "field {:?} is not on the reviewed allowlist; confirm it cannot carry removed bytes",
                    key
                );
            }
        }

        // The one free-text field a record carries is bounded, and no field
        // is named for content.
        for banned in ["content", "value", "matched", "removed_bytes", "secret"] {
            assert!(
                !allowed.contains(&banned),
                "{} must never become a record field",
                banned
            );
        }
    }

    #[test]
    fn error_exit_codes_follow_the_contract() {
        assert_eq!(RedactionError::Usage("x".into()).exit_code(), 2);
        assert_eq!(RedactionError::NotFound("x".into()).exit_code(), 3);
        assert_eq!(RedactionError::Conflict("x".into()).exit_code(), 4);
        assert_eq!(RedactionError::Integrity("x".into()).exit_code(), 1);
    }
}

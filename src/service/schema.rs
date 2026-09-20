//! Immutable public schema registry.
//!
//! Both capability negotiation and `bead schema list` project this registry,
//! preventing those discovery surfaces from drifting apart.

use crate::error::{Error, Result};
use crate::model::redaction::{
    SCHEMA_REDACTION_ACKNOWLEDGMENT, SCHEMA_REDACTION_EPOCH, SCHEMA_REDACTION_FIELD_SELECTOR,
    SCHEMA_REDACTION_FINDING, SCHEMA_REDACTION_RECEIPT, SCHEMA_REDACTION_TOMBSTONE,
};
use crate::service::capabilities::SchemaEntry;
use serde_json::{json, Map, Value};
use std::collections::HashSet;

/// Version of the native field-guide document emitted by
/// `bead schema explain`. Bump when the guide's typed shape or any documented
/// semantic changes incompatibly; snapshot and conformance tests pin this
/// value, and the `field_guide` JSON Schema carries it as a `const`.
pub const FIELD_GUIDE_VERSION: i64 = 3;

/// Artifact identity carried by every `bead schema explain` response, per the
/// accepted field-guide contract (`research/specs/native-field-guide-v1.md`).
pub const FIELD_GUIDE_SCHEMA_REF: &str = "urn:bead-rs:schema:field-guide:native-v1";

struct Descriptor {
    schema_ref: &'static str,
    document_kind: &'static str,
    readable: bool,
    writable: bool,
    validate: bool,
    consume: &'static [&'static str],
    emit: &'static [&'static str],
}

const DESCRIPTORS: &[Descriptor] = &[
    Descriptor {
        schema_ref: SCHEMA_REDACTION_ACKNOWLEDGMENT,
        document_kind: "redaction_acknowledgment",
        readable: true,
        writable: true,
        validate: true,
        consume: &["checkpoint-set-v1"],
        emit: &["checkpoint-set-v1"],
    },
    Descriptor {
        schema_ref: SCHEMA_REDACTION_EPOCH,
        document_kind: "redaction_epoch",
        readable: true,
        writable: true,
        validate: true,
        consume: &["checkpoint-set-v1"],
        emit: &["checkpoint-set-v1"],
    },
    Descriptor {
        schema_ref: SCHEMA_REDACTION_FIELD_SELECTOR,
        document_kind: "redaction_field_selector",
        readable: true,
        writable: true,
        validate: true,
        consume: &[],
        emit: &[],
    },
    Descriptor {
        schema_ref: SCHEMA_REDACTION_FINDING,
        document_kind: "redaction_finding",
        readable: true,
        writable: true,
        validate: true,
        consume: &["checkpoint-set-v1"],
        emit: &["checkpoint-set-v1"],
    },
    Descriptor {
        schema_ref: SCHEMA_REDACTION_RECEIPT,
        document_kind: "redaction_receipt",
        readable: true,
        writable: true,
        validate: true,
        consume: &["checkpoint-set-v1"],
        emit: &["checkpoint-set-v1"],
    },
    Descriptor {
        schema_ref: SCHEMA_REDACTION_TOMBSTONE,
        document_kind: "redaction_tombstone",
        readable: true,
        writable: true,
        validate: true,
        consume: &["checkpoint-set-v1"],
        emit: &["checkpoint-set-v1"],
    },
    Descriptor {
        schema_ref: "urn:bead-rs:schema:attempt-outcome:native-v1",
        document_kind: "attempt_outcome",
        readable: true,
        writable: true,
        validate: true,
        consume: &[],
        emit: &["checkpoint-set-v1"],
    },
    Descriptor {
        schema_ref: "urn:bead-rs:schema:capabilities:native-v1",
        document_kind: "capabilities",
        readable: true,
        writable: true,
        validate: true,
        consume: &[],
        emit: &["capabilities"],
    },
    Descriptor {
        schema_ref: "urn:bead-rs:schema:checkpoint-manifest:native-v1",
        document_kind: "checkpoint_manifest",
        readable: true,
        writable: true,
        validate: true,
        consume: &["checkpoint-set-v1"],
        emit: &["checkpoint-set-v1"],
    },
    Descriptor {
        schema_ref: "urn:bead-rs:schema:checkpoint-pointer:native-v1",
        document_kind: "checkpoint_pointer",
        readable: true,
        writable: true,
        validate: true,
        consume: &["checkpoint-set-v1"],
        emit: &["checkpoint-set-v1"],
    },
    Descriptor {
        schema_ref: "urn:bead-rs:schema:event:native-v1",
        document_kind: "audit_event",
        readable: true,
        writable: true,
        validate: true,
        consume: &[],
        emit: &["checkpoint-set-v1"],
    },
    Descriptor {
        schema_ref: FIELD_GUIDE_SCHEMA_REF,
        document_kind: "field_guide",
        readable: true,
        writable: true,
        validate: true,
        consume: &[],
        emit: &["schema.explain"],
    },
    Descriptor {
        schema_ref: "urn:bead-rs:schema:issue:native-v1",
        document_kind: "issue",
        readable: true,
        writable: true,
        validate: true,
        consume: &["sync.import-only"],
        emit: &["checkpoint-set-v1", "sync.flush-only"],
    },
    Descriptor {
        schema_ref: "urn:bead-rs:schema:provenance-receipt:native-v1",
        document_kind: "provenance_receipt",
        readable: true,
        writable: true,
        validate: true,
        consume: &["checkpoint-set-v1"],
        emit: &["checkpoint-set-v1"],
    },
    Descriptor {
        schema_ref: "urn:bead-rs:schema:resolve-receipt:native-v1",
        document_kind: "resolve_receipt",
        readable: true,
        writable: true,
        validate: true,
        consume: &[],
        emit: &["resolve"],
    },
    Descriptor {
        schema_ref: "urn:bead-rs:schema:resolve-request:native-v1",
        document_kind: "resolve_request",
        readable: true,
        writable: true,
        validate: true,
        consume: &["resolve"],
        emit: &[],
    },
];

pub fn schema_catalog() -> Result<Vec<SchemaEntry>> {
    let mut seen = HashSet::new();
    let mut entries = Vec::with_capacity(DESCRIPTORS.len());
    for descriptor in DESCRIPTORS {
        if !seen.insert(descriptor.schema_ref) {
            return Err(Error::Integrity(format!(
                "Duplicate schema identity in registry: {}",
                descriptor.schema_ref
            )));
        }
        entries.push(SchemaEntry {
            schema_ref: descriptor.schema_ref.to_string(),
            document_kind: descriptor.document_kind.to_string(),
            validate: descriptor.validate,
            readable: descriptor.readable,
            writable: descriptor.writable,
            lossy: None,
            consume: descriptor
                .consume
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
            emit: descriptor
                .emit
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
        });
    }
    entries.sort_by(|left, right| left.schema_ref.cmp(&right.schema_ref));
    Ok(entries)
}

fn descriptor(schema_ref: &str) -> Result<&'static Descriptor> {
    DESCRIPTORS
        .iter()
        .find(|descriptor| descriptor.schema_ref == schema_ref)
        .ok_or_else(|| Error::cli_usage(format!("Unsupported schema identity: {schema_ref}")))
}

fn names(kind: &str) -> &'static [&'static str] {
    match kind {
        "issue" => &[
            "id",
            "title",
            "revision",
            "description",
            "notes",
            "priority",
            "base_status",
            "manual_blocked",
            "assignee",
            "claim_epoch",
            "issue_type",
            "created_at",
            "updated_at",
            "closed_at",
            "close_reason",
            "source_repo",
            "profile",
            "schema_ref",
            "data",
            "labels",
            "dependencies",
            "comments",
            "external_references",
        ],
        "audit_event" => &[
            "$schema",
            "origin_store_uuid",
            "origin_event_sequence",
            "issue_id",
            "kind",
            "actor",
            "time",
            "detail",
        ],
        "provenance_receipt" => &[
            "$schema",
            "receipt_id",
            "kind",
            "source_store_uuid",
            "target_store_uuid",
            "source_root_sha256",
            "actor",
            "created_at",
            "counts",
            "result",
            "summary_event_identity",
            "receipt_sha256",
        ],
        "capabilities" => &[
            "contract",
            "implementation",
            "version",
            "store_layout",
            "atomic_claim",
            "priorities",
            "statuses",
            "checkpoint_modes",
            "checkpoint_formats",
            "logical_revision",
            "schema_ref",
            "schemas",
            "commands",
            // Additive R026 handshake (plan section 11): optional because it
            // is absent until the compiled automatic-flush default flips on
            "auto_flush",
            // Additive ADR-018 handshake: post-publication staging of the
            // verified checkpoint fileset, optional for the same reason
            "auto_stage",
            "attempt_summary",
            "secret_scan",
            "historical_redaction",
        ],
        "attempt_outcome" => &[
            "$schema",
            "attempt_id",
            "issue_id",
            "outcome",
            "action",
            "reason",
            "canonical_request_hash",
            "resulting_issue_revision",
            "resulting_state",
            "resulting_attempt_tier",
            "receipt_id",
            "actor",
            "created_at",
            "evidence_refs",
            "model",
            "harness",
            "harness_version",
        ],
        "resolve_receipt" => &[
            "receipt_id",
            "canonical_request_hash",
            "issue_id",
            "attempt_id",
            "resulting_issue_revision",
            "resulting_state",
            "resulting_attempt_tier",
            "created_at",
            "is_replay",
        ],
        "resolve_request" => &[
            "attempt_id",
            "issue_id",
            "outcome",
            "action",
            "reason",
            "if_revision",
            "fencing_token",
            "evidence_refs",
            "actor",
            "model",
            "harness",
            "harness_version",
        ],
        "redaction_field_selector" => &[
            "$schema",
            "record_kind",
            "origin_identity",
            "field_path",
            "byte_start",
            "byte_length",
            "prior_record_hash",
        ],
        "redaction_finding" => &[
            "$schema",
            "fingerprint",
            "ruleset_version",
            "rule_id",
            "selector",
            "severity",
            "detected_at",
        ],
        "redaction_acknowledgment" => &[
            "$schema",
            "fingerprint",
            "actor",
            "reason",
            "acknowledged_at",
        ],
        "redaction_receipt" => &[
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
        "redaction_epoch" => &[
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
        "redaction_tombstone" => &[
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
        "checkpoint_pointer" => &[
            "schema_version",
            "generation_id",
            "mode",
            "store_uuid",
            "snapshot_sequence",
            "active_root",
            "added_paths",
            "replaced_paths",
            "deleted_paths",
            "issue_count",
            "event_count",
            "receipt_count",
            "attempt_outcome_count",
            "redaction_record_count",
            "total_record_count",
            "created_at",
        ],
        "checkpoint_manifest" => &[
            "format",
            "schema_version",
            "store_uuid",
            "snapshot_sequence",
            "max_local_ingestion_sequence",
            "created_at",
            "profile",
            "partition_algorithm",
            "partition_thresholds",
            "issue_shards",
            "event_shards",
            "receipt_shards",
            "attempt_outcome_count",
            "redaction_record_count",
            "attempt_outcome_shards",
            "redaction_shards",
        ],
        "field_guide" => &[
            "schema_ref",
            "guide_version",
            "describes_schema_refs",
            "documents",
            "fields",
            "additional_properties",
            "lifecycle",
            "derived_state",
            "events",
            "operations",
            "rehydration",
            "known_implementation_deviations",
        ],
        _ => &[],
    }
}

fn property_schema(kind: &str, name: &str) -> Value {
    let timestamp = || json!({"type":"string", "format":"date-time"});
    match (kind, name) {
        (_, "$schema") | (_, "schema_ref") => json!({"type":"string", "format":"uri"}),
        ("issue", "id") => json!({"type":"string", "minLength":1, "maxLength":255}),
        ("issue", "title") => json!({"type":"string", "minLength":1, "maxLength":4096}),
        ("issue", "revision") => json!({"type":"integer", "minimum":1, "default":1}),
        ("issue", "description") | ("issue", "notes") => {
            json!({"type":["string","null"], "maxLength":4194304})
        }
        ("issue", "priority") => json!({"type":"integer", "minimum":0, "maximum":4, "default":2}),
        ("issue", "base_status") => {
            json!({"type":"string", "enum":["open","in_progress","deferred","closed"], "default":"open"})
        }
        ("issue", "manual_blocked") => json!({"type":["boolean","null"], "default":false}),
        ("issue", "claim_epoch") => json!({"type":["integer","null"], "minimum":1}),
        ("issue", "assignee")
        | ("issue", "issue_type")
        | ("issue", "close_reason")
        | ("issue", "source_repo")
        | ("issue", "profile") => json!({"type":["string","null"]}),
        ("issue", "created_at") | ("issue", "updated_at") => timestamp(),
        ("issue", "closed_at") => json!({"type":["string","null"], "format":"date-time"}),
        ("issue", "data") => json!({"type":["object","null"]}),
        ("issue", "labels")
        | ("issue", "dependencies")
        | ("issue", "comments")
        | ("issue", "external_references") => json!({"type":"array"}),
        ("audit_event", "origin_event_sequence") => json!({"type":"integer", "minimum":1}),
        ("audit_event", "issue_id") | ("audit_event", "actor") => json!({"type":["string","null"]}),
        ("audit_event", "time") => timestamp(),
        ("audit_event", "detail") => json!({}),
        ("audit_event", "kind") => {
            json!({"type":"string", "enum":["updated","claimed","released","reopened","closed","assignment_cleared"]})
        }
        ("provenance_receipt", "summary_event_identity") => json!({"type":["string","null"]}),
        ("provenance_receipt", "counts") => {
            json!({"type":"object", "required":["issues","events","provenance_receipts"], "properties":{"issues":{"type":"integer","minimum":0},"events":{"type":"integer","minimum":0},"provenance_receipts":{"type":"integer","minimum":0}}, "additionalProperties":false})
        }
        ("checkpoint_pointer", "schema_version") => json!({"const":1}),
        ("checkpoint_pointer", "snapshot_sequence")
        | ("checkpoint_pointer", "issue_count")
        | ("checkpoint_pointer", "event_count")
        | ("checkpoint_pointer", "receipt_count")
        | ("checkpoint_pointer", "attempt_outcome_count")
        | ("checkpoint_pointer", "redaction_record_count")
        | ("checkpoint_pointer", "total_record_count") => json!({"type":"integer", "minimum":0}),
        ("checkpoint_pointer", "mode") => json!({"type":"string", "enum":["monolithic","sharded"]}),
        ("checkpoint_pointer", "active_root") => {
            json!({"type":"object", "required":["path","sha256"], "properties":{"path":{"type":"string"},"sha256":{"type":"string"}}, "additionalProperties":false})
        }
        ("checkpoint_pointer", "added_paths")
        | ("checkpoint_pointer", "replaced_paths")
        | ("checkpoint_pointer", "deleted_paths") => {
            json!({"type":"array", "items":{"type":"string"}})
        }
        ("checkpoint_manifest", "schema_version") => json!({"const":1}),
        ("checkpoint_manifest", "snapshot_sequence")
        | ("checkpoint_manifest", "max_local_ingestion_sequence") => {
            json!({"type":"integer", "minimum":0})
        }
        ("checkpoint_manifest", "issue_shards")
        | ("checkpoint_manifest", "event_shards")
        | ("checkpoint_manifest", "receipt_shards")
        | ("checkpoint_manifest", "attempt_outcome_shards")
        | ("checkpoint_manifest", "redaction_shards") => json!({"type":"array"}),
        ("checkpoint_manifest", "attempt_outcome_count")
        | ("checkpoint_manifest", "redaction_record_count") => {
            json!({"type":"integer", "minimum":0})
        }
        ("checkpoint_manifest", "partition_thresholds") => json!({"type":"object"}),
        ("capabilities", "store_layout") => json!({"type":"integer", "minimum":1}),
        ("capabilities", "atomic_claim")
        | ("capabilities", "logical_revision")
        | ("capabilities", "auto_flush")
        | ("capabilities", "auto_stage")
        | ("capabilities", "attempt_summary") => {
            json!({"type":"boolean"})
        }
        ("capabilities", "statuses")
        | ("capabilities", "checkpoint_modes")
        | ("capabilities", "checkpoint_formats")
        | ("capabilities", "schemas")
        | ("capabilities", "commands") => json!({"type":"array"}),
        ("capabilities", "priorities") => json!({"type":"object"}),
        ("capabilities", "secret_scan") | ("capabilities", "historical_redaction") => {
            json!({"type":"object"})
        }
        ("field_guide", "guide_version") => json!({"const": FIELD_GUIDE_VERSION}),
        ("attempt_outcome", "attempt_id") => {
            json!({"type":"string", "minLength":1, "maxLength":255})
        }
        ("attempt_outcome", "issue_id") => json!({"type":"string", "minLength":1, "maxLength":255}),
        ("attempt_outcome", "outcome") => {
            json!({"type":"string", "enum":["verified_success","work_failure","infrastructure_failure","cancelled","indeterminate"]})
        }
        ("attempt_outcome", "action") => {
            json!({"type":"string", "enum":["close","release","quarantine","block","none"]})
        }
        ("attempt_outcome", "reason") => json!({"type":["string","null"], "maxLength":4194304}),
        ("attempt_outcome", "canonical_request_hash") => {
            json!({"type":"string", "minLength":64, "maxLength":64})
        }
        ("attempt_outcome", "resulting_state") => {
            json!({"type":"string", "enum":["open","in_progress","deferred","closed"]})
        }
        ("attempt_outcome", "resulting_issue_revision") => json!({"type":"integer", "minimum":1}),
        ("attempt_outcome", "resulting_attempt_tier") => {
            json!({"type":"integer", "minimum":0, "maximum":3})
        }
        ("attempt_outcome", "receipt_id") => json!({"type":"string", "minLength":1}),
        ("attempt_outcome", "actor") => json!({"type":"string", "minLength":1, "maxLength":255}),
        ("attempt_outcome", "created_at") => timestamp(),
        ("attempt_outcome", "evidence_refs") => json!({"type":"array", "items":{"type":"string"}}),
        ("attempt_outcome", "model")
        | ("attempt_outcome", "harness")
        | ("attempt_outcome", "harness_version") => json!({"type":["string","null"]}),
        ("resolve_receipt", "receipt_id") => json!({"type":"string", "minLength":1}),
        ("resolve_receipt", "canonical_request_hash") => {
            json!({"type":"string", "minLength":64, "maxLength":64})
        }
        ("resolve_receipt", "issue_id") => json!({"type":"string", "minLength":1, "maxLength":255}),
        ("resolve_receipt", "attempt_id") => {
            json!({"type":"string", "minLength":1, "maxLength":255})
        }
        ("resolve_receipt", "resulting_issue_revision") => json!({"type":"integer", "minimum":1}),
        ("resolve_receipt", "resulting_state") => json!({"type":"string"}),
        ("resolve_receipt", "resulting_attempt_tier") => {
            json!({"type":"integer", "minimum":0, "maximum":3})
        }
        ("resolve_receipt", "created_at") => timestamp(),
        ("resolve_receipt", "is_replay") => json!({"type":"boolean"}),
        ("resolve_request", "attempt_id") => {
            json!({"type":"string", "minLength":1, "maxLength":255})
        }
        ("resolve_request", "issue_id") => json!({"type":"string", "minLength":1, "maxLength":255}),
        ("resolve_request", "outcome") => {
            json!({"type":"string", "enum":["verified_success","work_failure","infrastructure_failure","cancelled","indeterminate"]})
        }
        ("resolve_request", "action") => {
            json!({"type":["string","null"], "enum":["close","release","quarantine","block","none"]})
        }
        ("resolve_request", "reason") => json!({"type":["string","null"], "maxLength":4194304}),
        ("resolve_request", "if_revision") => json!({"type":["integer","null"], "minimum":1}),
        ("resolve_request", "fencing_token") => json!({"type":["string","null"]}),
        ("resolve_request", "evidence_refs") => json!({"type":"array", "items":{"type":"string"}}),
        ("resolve_request", "actor") => json!({"type":"string", "minLength":1, "maxLength":255}),
        ("resolve_request", "model")
        | ("resolve_request", "harness")
        | ("resolve_request", "harness_version") => json!({"type":["string","null"]}),
        ("redaction_field_selector", "byte_start") => {
            json!({"type":"integer", "minimum":0, "maximum":4194303})
        }
        ("redaction_field_selector", "byte_length") => {
            json!({"type":"integer", "minimum":1, "maximum":4194304})
        }
        ("redaction_field_selector", "prior_record_hash")
        | ("redaction_finding", "fingerprint")
        | ("redaction_acknowledgment", "fingerprint")
        | ("redaction_receipt", "receipt_id")
        | ("redaction_receipt", "finding_fingerprint")
        | ("redaction_receipt", "prior_record_hash")
        | ("redaction_receipt", "sanitized_record_hash")
        | ("redaction_epoch", "epoch_id")
        | ("redaction_tombstone", "tombstone_id")
        | ("redaction_tombstone", "prior_record_hash")
        | ("redaction_tombstone", "finding_fingerprint")
        | ("redaction_tombstone", "epoch_id") => {
            json!({"type":"string", "pattern":"^[0-9a-f]{64}$"})
        }
        ("redaction_finding", "ruleset_version") | ("redaction_receipt", "ruleset_version") => {
            json!({"type":"integer", "minimum":1})
        }
        ("redaction_finding", "selector") | ("redaction_receipt", "selector") => {
            json!({"type":"object"})
        }
        ("redaction_finding", "severity") => {
            json!({"type":"string", "enum":["blocking","advisory"]})
        }
        ("redaction_receipt", "publication_state") | ("redaction_epoch", "publication_state") => {
            json!({"type":"string", "enum":["committed","published","discarded"]})
        }
        ("redaction_receipt", "affected_issue_revision") => {
            json!({"type":["integer","null"], "minimum":1})
        }
        ("redaction_receipt", "epoch_id")
        | ("redaction_receipt", "resulting_generation_id")
        | ("redaction_epoch", "resulting_generation_id")
        | ("redaction_epoch", "published_at") => {
            json!({"type":["string","null"]})
        }
        ("redaction_epoch", "receipt_ids") => {
            json!({"type":"array", "minItems":1, "uniqueItems":true, "items":{"type":"string", "pattern":"^[0-9a-f]{64}$"}})
        }
        ("redaction_epoch", "superseded_generations") => {
            json!({"type":"array", "items":{"type":"string"}})
        }
        ("redaction_epoch", "previous_generation_reset") => json!({"type":"boolean"}),
        ("redaction_finding", "detected_at")
        | ("redaction_acknowledgment", "acknowledged_at")
        | ("redaction_receipt", "redacted_at")
        | ("redaction_epoch", "opened_at")
        | ("redaction_tombstone", "created_at") => timestamp(),
        ("redaction_acknowledgment", "reason") | ("redaction_receipt", "reason") => {
            json!({"type":"string", "minLength":1, "maxLength":1024})
        }
        ("field_guide", "describes_schema_refs")
        | ("field_guide", "documents")
        | ("field_guide", "fields")
        | ("field_guide", "operations")
        | ("field_guide", "known_implementation_deviations") => json!({"type":"array"}),
        ("field_guide", _) => json!({"type":"object"}),
        (_, "created_at") => timestamp(),
        (_, _) => json!({"type":"string"}),
    }
}

fn properties_for(kind: &str) -> Map<String, Value> {
    names(kind)
        .iter()
        .map(|name| ((*name).to_string(), property_schema(kind, name)))
        .collect()
}

fn required_for(kind: &str) -> Vec<String> {
    let optional: &[&str] = match kind {
        "issue" => &[
            "revision",
            "description",
            "notes",
            "manual_blocked",
            "assignee",
            "claim_epoch",
            "issue_type",
            "closed_at",
            "close_reason",
            "source_repo",
            "profile",
            "data",
            "labels",
            "dependencies",
            "comments",
            "external_references",
        ],
        "audit_event" => &["issue_id", "actor"],
        "provenance_receipt" => &["summary_event_identity"],
        // Optional while the R026 gate keeps the compiled default off, so a
        // document without it validates; present-when-enabled documents
        // validate against the same additive identity (plan section 11).
        // `auto_stage` is additive the same way (ADR-018)
        "capabilities" => &[
            "auto_flush",
            "auto_stage",
            "secret_scan",
            "historical_redaction",
        ],
        "attempt_outcome" => &["reason", "model", "harness", "harness_version"],
        "resolve_receipt" => &[],
        "resolve_request" => &[
            "action",
            "reason",
            "if_revision",
            "fencing_token",
            "evidence_refs",
            "model",
            "harness",
            "harness_version",
        ],
        "redaction_receipt" => &[
            "affected_issue_revision",
            "resulting_generation_id",
            "epoch_id",
        ],
        "redaction_epoch" => &[
            "resulting_generation_id",
            "superseded_generations",
            "published_at",
        ],
        "checkpoint_pointer" => &["attempt_outcome_count", "redaction_record_count"],
        "checkpoint_manifest" => &[
            "attempt_outcome_count",
            "redaction_record_count",
            "attempt_outcome_shards",
            "redaction_shards",
        ],
        _ => &[],
    };
    names(kind)
        .iter()
        .filter(|name| !optional.contains(name))
        .map(|name| (*name).to_string())
        .collect()
}

pub fn schema_document(schema_ref: &str) -> Result<Value> {
    let descriptor = descriptor(schema_ref)?;
    let properties = properties_for(descriptor.document_kind);
    let required = required_for(descriptor.document_kind);
    Ok(json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": descriptor.schema_ref,
        "title": descriptor.document_kind,
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": descriptor.document_kind == "issue"
            || descriptor.document_kind.starts_with("redaction_")
    }))
}

fn guide_documents() -> Vec<Value> {
    let docs: &[(&str, &str, &str, &[&str])] = &[
        (
            "cli_issue",
            "urn:bead-rs:schema:issue:native-v1",
            "issue",
            &[
                "assignee",
                "attempts",
                "claim_epoch",
                "comments",
                "created_at",
                "dependencies",
                "description",
                "effective_status",
                "id",
                "labels",
                "manual_blocked",
                "notes",
                "priority",
                "revision",
                "status",
                "title",
                "updated_at",
            ],
        ),
        (
            "checkpoint_issue",
            "urn:bead-rs:schema:issue:native-v1",
            "issue",
            names("issue"),
        ),
        (
            "claim_result",
            "urn:bead-rs:schema:issue:native-v1",
            "claim_result",
            &["assignee", "bead_id", "claim_epoch", "lease"],
        ),
        (
            "checkpoint_event",
            "urn:bead-rs:schema:event:native-v1",
            "audit_event",
            names("audit_event"),
        ),
        (
            "checkpoint_provenance_receipt",
            "urn:bead-rs:schema:provenance-receipt:native-v1",
            "provenance_receipt",
            names("provenance_receipt"),
        ),
    ];
    docs.iter().map(|(name, schema_ref, kind, members)| {
        let mut members: Vec<&str> = members.to_vec();
        members.sort_unstable();
        json!({"name":name,"schema_ref":schema_ref,"document_kind":kind,"transport":"public JSON","member_source":"native typed model","members":members})
    }).collect()
}

/// JSON Schema fragment for one projection member, used to derive the typed
/// `json_type` and `nullable` guide entries. Checkpoint documents reuse the
/// registry schemas; the two interactive projections carry their own shapes
/// because they are computed views, not stored records.
fn projection_property_schema(document: &str, name: &str) -> Value {
    match (document, name) {
        ("cli_issue", "assignee") => json!({"type":["string","null"]}),
        ("cli_issue", "attempts")
        | ("cli_issue", "comments")
        | ("cli_issue", "dependencies")
        | ("cli_issue", "labels") => json!({"type":"array"}),
        ("cli_issue", "claim_epoch") => json!({"type":["integer","null"], "minimum":1}),
        ("cli_issue", "created_at") | ("cli_issue", "updated_at") => {
            json!({"type":"string", "format":"date-time"})
        }
        ("cli_issue", "manual_blocked") => json!({"type":"boolean"}),
        ("cli_issue", "priority") => json!({"type":"integer", "minimum":0, "maximum":4}),
        ("cli_issue", "revision") => json!({"type":"integer", "minimum":1}),
        ("cli_issue", _) => json!({"type":"string"}),
        ("claim_result", "lease") => json!({"type":["object","null"]}),
        ("claim_result", "bead_id") | ("claim_result", "assignee") => {
            json!({"type":["string","null"]})
        }
        ("claim_result", "claim_epoch") => json!({"type":["integer","null"], "minimum":1}),
        _ => {
            let checkpoint_kind = match document {
                "checkpoint_event" => "audit_event",
                "checkpoint_provenance_receipt" => "provenance_receipt",
                _ => "issue",
            };
            property_schema(checkpoint_kind, name)
        }
    }
}

/// Curated semantics for one `(document, name)` guide entry: everything the
/// accepted field-guide contract requires per field — ownership, owning
/// operations, default, example, invariants, and the common mistake.
struct FieldSemantics {
    ownership: &'static str,
    operations: &'static [&'static str],
    has_default: bool,
    default: Value,
    example: Value,
    invariants: &'static [&'static str],
    common_mistake: &'static str,
}

/// Fallback marking a semantics-table gap. `guide_field` must never emit it:
/// the conformance tests assert every declared member of every document has
/// curated semantics, so hitting this arm is a bug, not a graceful degrade.
fn unspecified_semantics() -> FieldSemantics {
    FieldSemantics {
        ownership: "unspecified",
        operations: &[],
        has_default: false,
        default: Value::Null,
        example: Value::Null,
        invariants: &["documentation gap: this member lacks curated semantics"],
        common_mistake: "Semantics table gap: report this as a bug.",
    }
}

fn field_semantics(document: &str, name: &str) -> FieldSemantics {
    match (document, name) {
        // ---- cli_issue (interactive CLI issue projection) ------------------
        ("cli_issue", "id") => FieldSemantics {
            ownership: "system",
            operations: &["create"],
            has_default: false,
            default: Value::Null,
            example: json!("bead-18409c0e"),
            invariants: &[
                "1-255 UTF-8 bytes",
                "leading/trailing whitespace, '/', '\\', NUL, and control characters other than tab, LF, and CR are forbidden",
                "immutable",
                "preserved verbatim by native restore",
            ],
            common_mistake: "Manufacturing an ID or inferring chronology from its spelling.",
        },
        ("cli_issue", "title") => FieldSemantics {
            ownership: "caller",
            operations: &["create"],
            has_default: false,
            default: Value::Null,
            example: json!("Verify restore invariants"),
            invariants: &["1-4096 UTF-8 bytes", "immutable after create"],
            common_mistake: "Attempting update --title, which is a usage error.",
        },
        ("cli_issue", "revision") => FieldSemantics {
            ownership: "system",
            operations: &["claim", "close", "release", "reopen", "update"],
            has_default: true,
            default: json!(1),
            example: json!(4),
            invariants: &[
                "integer starting at 1, advanced by semantic mutations",
                "--if-revision on update, release, close, and reopen consumes a previously read value",
                "claim has no revision guard",
                "materialized as 1 only when an older in-memory value lacks it",
            ],
            common_mistake: "Choosing the next revision or treating it as time.",
        },
        ("cli_issue", "description") => FieldSemantics {
            ownership: "caller",
            operations: &["create"],
            has_default: true,
            default: json!(""),
            example: json!("Rehearse flush and restore."),
            invariants: &[
                "maximum 4 MiB",
                "absent checkpoint description materializes as the empty string in this projection",
                "immutable after create",
            ],
            common_mistake:
                "Treating checkpoint absence, explicit null, and projected empty text as interchangeable.",
        },
        ("cli_issue", "notes") => FieldSemantics {
            ownership: "caller",
            operations: &["update"],
            has_default: true,
            default: json!(""),
            example: json!("Reproduction captured in the reconciliation report."),
            invariants: &[
                "maximum 4 MiB",
                "checkpoint content, not a secret store",
                "absent checkpoint notes materialize as the empty string in this projection",
                "written only through update --notes, which replaces the whole field",
            ],
            common_mistake: "Expecting create --notes or assuming notes are private.",
        },
        ("cli_issue", "priority") => FieldSemantics {
            ownership: "caller",
            operations: &["create"],
            has_default: true,
            default: json!(2),
            example: json!(2),
            invariants: &[
                "integer from 0 through 4",
                "P0 urgent, P1 critical, P2 high (native default), P3 normal, P4 aspirational",
                "lower is more urgent",
                "fixed at creation",
            ],
            common_mistake: "Calling P2 normal or reversing the ordering.",
        },
        ("cli_issue", "status") => FieldSemantics {
            ownership: "derived",
            operations: &["list", "show"],
            has_default: false,
            default: Value::Null,
            example: json!("open"),
            invariants: &[
                "projects base_status; the manual_blocked overlay is exposed separately as effective_status",
                "never stored in a checkpoint issue",
                "consumers must not treat blocked as a stored base value",
            ],
            common_mistake:
                "Assuming status == open proves readiness or that a status member exists in a native checkpoint issue.",
        },
        ("cli_issue", "effective_status") => FieldSemantics {
            ownership: "derived",
            operations: &["list", "show"],
            has_default: false,
            default: Value::Null,
            example: json!("open"),
            invariants: &[
                "blocked exactly when manual_blocked is true on non-closed work; otherwise equals status",
                "never stored in a checkpoint issue",
                "readiness is stricter than effective_status == open: assignment and graph blockers also apply",
            ],
            common_mistake:
                "Reading effective_status as a stored base value or as proof of readiness.",
        },
        ("cli_issue", "manual_blocked") => FieldSemantics {
            ownership: "caller",
            operations: &["close", "reopen", "update"],
            has_default: true,
            default: json!(false),
            example: json!(false),
            invariants: &[
                "true prevents readiness",
                "cleared by close and reopen",
                "never encodes graph blocking",
            ],
            common_mistake:
                "Encoding graph blocking in this flag or assuming false proves readiness.",
        },
        ("cli_issue", "assignee") => FieldSemantics {
            ownership: "caller",
            operations: &["claim", "create", "release", "reopen", "update"],
            has_default: true,
            default: Value::Null,
            example: Value::Null,
            invariants: &[
                "explicit null when unassigned",
                "nonempty when present",
                "claim assigns and enters in_progress",
                "release applies to in_progress work; an open assigned bead uses update --clear-assignee",
                "preserved by close; cleared by reopen",
            ],
            common_mistake: "Treating assignment as authorization.",
        },
        ("cli_issue", "dependencies") => FieldSemantics {
            ownership: "caller",
            operations: &["dep.add", "dep.remove"],
            has_default: true,
            default: json!([]),
            example: json!([{"blocker":"bead-a","kind":"blocks"}]),
            invariants: &[
                "entries carry blocker and kind",
                "blocks edges reject self-edges and cycles",
                "relates_to and verifies never affect readiness",
                "a blocker blocks while its base status is not closed; a deferred blocker still blocks",
            ],
            common_mistake: "Reversing the blocked-first direction.",
        },
        ("cli_issue", "created_at") => FieldSemantics {
            ownership: "system",
            operations: &["create"],
            has_default: false,
            default: Value::Null,
            example: json!("2026-08-12T22:51:52.301697398Z"),
            invariants: &[
                "RFC 3339 UTC with nanosecond precision",
                "immutable",
                "preserved by native restore",
            ],
            common_mistake:
                "Synthesizing a source tracker's time as a native creation instant during rehydration.",
        },
        ("cli_issue", "updated_at") => FieldSemantics {
            ownership: "system",
            operations: &["claim", "close", "release", "reopen", "update"],
            has_default: false,
            default: Value::Null,
            example: json!("2026-08-12T22:53:00.000000000Z"),
            invariants: &[
                "RFC 3339 UTC with nanosecond precision",
                "advanced by semantic mutation",
                "not an optimistic concurrency token; use revision",
            ],
            common_mistake: "Using it as an optimistic concurrency token.",
        },
        ("cli_issue", "labels") => FieldSemantics {
            ownership: "caller",
            operations: &["create", "label.add", "label.remove"],
            has_default: true,
            default: json!([]),
            example: json!([]),
            invariants: &[
                "case-sensitive strings",
                "add and remove are idempotent",
            ],
            common_mistake: "Treating a label as lifecycle state.",
        },
        ("cli_issue", "attempts") => FieldSemantics {
            ownership: "system",
            operations: &[],
            has_default: true,
            default: json!([]),
            example: json!([]),
            invariants: &[
                "read-only attempt summaries recorded by the resolve/attempt subsystem",
                "empty array when no attempts exist",
                "never present in checkpoint issue records",
            ],
            common_mistake: "Manufacturing or editing attempt entries.",
        },
        ("cli_issue", "comments") => FieldSemantics {
            ownership: "system",
            operations: &[],
            has_default: true,
            default: json!([]),
            example: json!([]),
            invariants: &[
                "ordered by creation time then ID",
                "v0.1 has no public command that creates comments",
                "interactive reads expose only the projection selected by --comments",
            ],
            common_mistake:
                "Treating comments as unknown extensions or assuming an interactive omission means no durable comments exist.",
        },
        ("cli_issue", "claim_epoch") => FieldSemantics {
            ownership: "system",
            operations: &["claim"],
            has_default: false,
            default: Value::Null,
            example: json!(1),
            invariants: &[
                "absent from this projection until the first claim mints one",
                "monotonically increasing credential minted by every claim that selects work",
                "also advanced by lease renewal and claimant reassignment",
                "presented back as --fencing-token for mutations of claimed work",
                "not a revision and not a timestamp",
            ],
            common_mistake:
                "Treating the epoch as an optimistic concurrency token or assuming it is always present.",
        },
        // ---- checkpoint_issue (native checkpoint issue record) --------------
        ("checkpoint_issue", "id") => FieldSemantics {
            ownership: "system",
            operations: &["create", "sync.import-only"],
            has_default: false,
            default: Value::Null,
            example: json!("bead-18409c0e"),
            invariants: &[
                "1-255 UTF-8 bytes",
                "leading/trailing whitespace, '/', '\\', NUL, and control characters other than tab, LF, and CR are forbidden",
                "immutable",
                "preserved verbatim by native restore",
            ],
            common_mistake: "Manufacturing an ID or inferring chronology from its spelling.",
        },
        ("checkpoint_issue", "title") => FieldSemantics {
            ownership: "caller",
            operations: &["create"],
            has_default: false,
            default: Value::Null,
            example: json!("Verify restore invariants"),
            invariants: &["1-4096 UTF-8 bytes", "immutable after create"],
            common_mistake: "Attempting update --title, which is a usage error.",
        },
        ("checkpoint_issue", "revision") => FieldSemantics {
            ownership: "system",
            operations: &["claim", "close", "release", "reopen", "update"],
            has_default: true,
            default: json!(1),
            example: json!(4),
            invariants: &[
                "integer starting at 1, advanced by semantic mutations",
                "--if-revision on update, release, close, and reopen consumes a previously read value; claim has none",
                "preserved through export, restore, and merge insertion",
                "on merge, an incoming revision newer than the live token is retained; otherwise replacement advances the live token by one",
                "a stale checkpoint cannot roll the token backward",
            ],
            common_mistake:
                "Choosing the next revision or treating it as time. Released v0.1.1 reset revisions to 1.",
        },
        ("checkpoint_issue", "description") => FieldSemantics {
            ownership: "caller",
            operations: &["create"],
            has_default: false,
            default: Value::Null,
            example: json!("Rehearse flush and restore."),
            invariants: &[
                "optional nullable string; absent when create omits --description",
                "maximum 4 MiB",
            ],
            common_mistake:
                "Treating absent checkpoint description, explicit null, and projected empty text as interchangeable.",
        },
        ("checkpoint_issue", "notes") => FieldSemantics {
            ownership: "caller",
            operations: &["update"],
            has_default: true,
            default: json!(""),
            example: json!("Reproduction captured in the reconciliation report."),
            invariants: &[
                "optional nullable string with live default empty string",
                "maximum 4 MiB",
                "checkpoint content, not a secret store",
                "written only through update --notes, which replaces the whole field",
            ],
            common_mistake: "Expecting create --notes or assuming notes are private.",
        },
        ("checkpoint_issue", "priority") => FieldSemantics {
            ownership: "caller",
            operations: &["create"],
            has_default: true,
            default: json!(2),
            example: json!(2),
            invariants: &[
                "integer from 0 through 4",
                "P0 urgent, P1 critical, P2 high (native default), P3 normal, P4 aspirational",
                "lower is more urgent",
                "fixed at creation",
            ],
            common_mistake: "Calling P2 normal or reversing the ordering.",
        },
        ("checkpoint_issue", "base_status") => FieldSemantics {
            ownership: "system",
            operations: &["claim", "close", "release", "reopen", "update"],
            has_default: true,
            default: json!("open"),
            example: json!("open"),
            invariants: &[
                "enum: open, in_progress, deferred, closed",
                "closed_at is present exactly when base_status is closed",
                "blocked and ready are never stored base values",
                "import validation rejects lifecycle violations in both directions",
            ],
            common_mistake: "Storing blocked or ready as a base value.",
        },
        ("checkpoint_issue", "manual_blocked") => FieldSemantics {
            ownership: "caller",
            operations: &["close", "reopen", "update"],
            has_default: true,
            default: json!(false),
            example: json!(false),
            invariants: &[
                "optional nullable boolean with effective default false",
                "true prevents readiness",
                "cleared by close and reopen",
                "never encodes graph blocking",
            ],
            common_mistake:
                "Encoding graph blocking in this flag or assuming false proves readiness.",
        },
        ("checkpoint_issue", "assignee") => FieldSemantics {
            ownership: "caller",
            operations: &["claim", "create", "release", "reopen", "update"],
            has_default: false,
            default: Value::Null,
            example: json!("agent-name"),
            invariants: &[
                "absence when unset; an imported explicit null reserializes as absence",
                "nonempty when present",
                "claim assigns and enters in_progress",
                "release applies to in_progress work; an open assigned bead uses update --clear-assignee",
                "preserved by close; cleared by reopen",
            ],
            common_mistake: "Treating assignment as authorization.",
        },
        ("checkpoint_issue", "claim_epoch") => FieldSemantics {
            ownership: "system",
            operations: &["claim"],
            has_default: false,
            default: Value::Null,
            example: json!(1),
            invariants: &[
                "stored as 0 when no claim epoch exists and serialized as absence",
                "monotonically increasing credential minted by every claim that selects work",
                "also advanced by lease renewal and claimant reassignment",
                "presented back as --fencing-token for mutations of claimed work",
                "not a revision and not a timestamp",
            ],
            common_mistake: "Treating the epoch as an optimistic concurrency token.",
        },
        ("checkpoint_issue", "issue_type") => FieldSemantics {
            ownership: "caller",
            operations: &["create"],
            has_default: true,
            default: json!("task"),
            example: json!("task"),
            invariants: &[
                "free nonempty string with no enumerated value validation",
                "effective default task",
                "fixed at creation",
            ],
            common_mistake:
                "Attempting to update it or treating the examples in CLI help as an exhaustive enum.",
        },
        ("checkpoint_issue", "created_at") => FieldSemantics {
            ownership: "system",
            operations: &["create"],
            has_default: false,
            default: Value::Null,
            example: json!("2026-08-12T22:51:52.301697398Z"),
            invariants: &[
                "RFC 3339 UTC with nanosecond precision",
                "immutable",
                "preserved by native restore",
            ],
            common_mistake:
                "Synthesizing a source tracker's time as a native creation instant during rehydration.",
        },
        ("checkpoint_issue", "updated_at") => FieldSemantics {
            ownership: "system",
            operations: &["claim", "close", "release", "reopen", "update"],
            has_default: false,
            default: Value::Null,
            example: json!("2026-08-12T22:53:00.000000000Z"),
            invariants: &[
                "RFC 3339 UTC with nanosecond precision",
                "advanced by semantic mutation",
                "on merge, scalar issue content follows the newer updated_at",
                "not an optimistic concurrency token; use revision",
            ],
            common_mistake: "Using it as an optimistic concurrency token.",
        },
        ("checkpoint_issue", "closed_at") => FieldSemantics {
            ownership: "system",
            operations: &["close", "reopen"],
            has_default: false,
            default: Value::Null,
            example: json!("2026-08-12T23:00:00.000000000Z"),
            invariants: &[
                "present exactly when base_status is closed",
                "cleared to absence by reopen",
                "import validation rejects violations in both directions; doctor detects legacy rows without guessing repairs",
            ],
            common_mistake: "Assuming detection repaired legacy rows.",
        },
        ("checkpoint_issue", "close_reason") => FieldSemantics {
            ownership: "caller",
            operations: &["close", "reopen"],
            has_default: false,
            default: Value::Null,
            example: json!("Completed and verified"),
            invariants: &[
                "nonempty for closed issues",
                "no length bound",
                "cleared to absence by reopen",
            ],
            common_mistake: "Omitting --reason.",
        },
        ("checkpoint_issue", "source_repo") => FieldSemantics {
            ownership: "system",
            operations: &[],
            has_default: false,
            default: Value::Null,
            example: json!("forgejo:jedarden/project"),
            invariants: &[
                "no public writer in v0.1 and unreachable through normal creation",
                "never network-resolved",
            ],
            common_mistake:
                "Editing checkpoint JSON to inject provenance; use external references and the reconciliation report.",
        },
        ("checkpoint_issue", "profile") => FieldSemantics {
            ownership: "system",
            operations: &["sync.import-only"],
            has_default: true,
            default: json!("native-v1"),
            example: json!("native-v1"),
            invariants: &["version 0.1 accepts no external checkpoint profile"],
            common_mistake: "Relabeling foreign records as native.",
        },
        ("checkpoint_issue", "schema_ref") => FieldSemantics {
            ownership: "system",
            operations: &[],
            has_default: true,
            default: json!("urn:bead-rs:schema:issue:native-v1"),
            example: json!("urn:bead-rs:schema:issue:native-v1"),
            invariants: &[
                "required absolute URI on every native-v1 issue record",
                "immutable",
                "identifies the public document schema, not SQLite storage",
            ],
            common_mistake: "Silently replacing an unknown reference.",
        },
        ("checkpoint_issue", "data") => FieldSemantics {
            ownership: "caller",
            operations: &["data.set", "data.remove"],
            has_default: true,
            default: json!({}),
            example: json!({"example":{"schema_ref":"urn:example:v1","value":{}}}),
            invariants: &[
                "optional nullable object for legacy input and required in new native output, where no namespaces serialize as {}",
                "each namespace has an immutable schema reference and an arbitrary JSON value",
                "merge replaces the collection when present and preserves it when absent",
                "preserved transactionally through export and activation",
            ],
            common_mistake:
                "Editing the aggregate through issue update. Released v0.1.1 lost this table.",
        },
        ("checkpoint_issue", "labels") => FieldSemantics {
            ownership: "caller",
            operations: &["create", "label.add", "label.remove"],
            has_default: true,
            default: json!([]),
            example: json!([]),
            invariants: &[
                "case-sensitive strings",
                "add and remove are idempotent",
                "merge is additive",
            ],
            common_mistake: "Treating a label as lifecycle state.",
        },
        ("checkpoint_issue", "dependencies") => FieldSemantics {
            ownership: "caller",
            operations: &["dep.add", "dep.remove"],
            has_default: true,
            default: json!([]),
            example: json!([{"blocker":"bead-a","kind":"blocks"}]),
            invariants: &[
                "edges retain blocked ID, blocker ID, kind, and optional condition",
                "blocks edges reject self-edges and cycles",
                "merge is additive",
            ],
            common_mistake: "Reversing the blocked-first direction.",
        },
        ("checkpoint_issue", "comments") => FieldSemantics {
            ownership: "system",
            operations: &[],
            has_default: true,
            default: json!([]),
            example: json!([]),
            invariants: &[
                "optional array for legacy inputs and required in new native output, where empty serializes as []",
                "each entry carries id, author, body, nullable reply_to_id, nullable resolution_state, and created_at",
                "ordered by creation time then ID",
                "merge replaces when present and preserves when absent",
            ],
            common_mistake:
                "Treating comments as unknown extensions or assuming an interactive omission means no durable comments exist.",
        },
        ("checkpoint_issue", "external_references") => FieldSemantics {
            ownership: "caller",
            operations: &["ref.add", "ref.remove"],
            has_default: true,
            default: json!([]),
            example: json!({"namespace":"source","key":"issue-id","value":"bf-123"}),
            invariants: &[
                "optional array for legacy inputs and required in new native output, where empty serializes as []",
                "namespace, key, and value are required non-null strings",
                "merge replaces when present and preserves when absent",
            ],
            common_mistake:
                "Confusing these tracker/commit bindings with structured-data schema_ref values.",
        },
        // ---- claim_result (bead claim --json) -------------------------------
        ("claim_result", "bead_id") => FieldSemantics {
            ownership: "system",
            operations: &["claim"],
            has_default: false,
            default: Value::Null,
            example: json!("bead-18409c0e"),
            invariants: &[
                "identifies the selected issue",
                "an empty queue returns an object without a nonempty bead_id",
                "this projection does not rename bead_id to id",
            ],
            common_mistake:
                "Assuming every claim returns work, or reading the result as an issue projection.",
        },
        ("claim_result", "assignee") => FieldSemantics {
            ownership: "system",
            operations: &["claim"],
            has_default: false,
            default: Value::Null,
            example: json!("agent-name"),
            invariants: &[
                "echoes the --assignee requested by the caller",
                "always present; empty only when the caller requested an empty --assignee, which claim does not validate",
            ],
            common_mistake: "Treating assignment as authorization.",
        },
        ("claim_result", "claim_epoch") => FieldSemantics {
            ownership: "system",
            operations: &["claim"],
            has_default: false,
            default: Value::Null,
            example: json!(1),
            invariants: &[
                "minted by every claim that selects work, leased or not; the empty-queue result omits the member",
                "presented back as --fencing-token for mutations of claimed work",
                "not a revision and not a timestamp",
            ],
            common_mistake: "Treating the epoch as an issue revision.",
        },
        ("claim_result", "lease") => FieldSemantics {
            ownership: "system",
            operations: &["claim"],
            has_default: false,
            default: Value::Null,
            example: Value::Null,
            invariants: &[
                "null when the claim carries no lease",
                "a lease carries issue_id, assignee, fencing_token, and expires_at",
                "after expiry the holder cannot update, release, close, or reopen (exit 4)",
                "an expired lease cannot be renewed: renewal requires an unexpired lease, so --renew-lease reports an empty success",
                "expiry does not reopen the frontier: the issue stays claimed until an override clears it",
                "a stale or mismatched token is an exit-4 conflict",
                "a coordination guard, not an issue revision",
            ],
            common_mistake:
                "Treating the fencing token as an issue revision or assuming leases never expire.",
        },
        // ---- checkpoint_event (native event record) -------------------------
        ("checkpoint_event", "$schema") => FieldSemantics {
            ownership: "system",
            operations: &[],
            has_default: false,
            default: Value::Null,
            example: json!("urn:bead-rs:schema:event:native-v1"),
            invariants: &[
                "required non-null absolute URI",
                "event records use the $schema spelling where issue records use schema_ref",
            ],
            common_mistake: "Replacing it with the issue record's schema_ref spelling.",
        },
        ("checkpoint_event", "origin_store_uuid") => FieldSemantics {
            ownership: "system",
            operations: &[],
            has_default: false,
            default: Value::Null,
            example: json!("workspace-uuid"),
            invariants: &[
                "required non-null string identifying the originating store",
                "preserved by native restore",
            ],
            common_mistake: "Substituting the destination store identity on restore.",
        },
        ("checkpoint_event", "origin_event_sequence") => FieldSemantics {
            ownership: "system",
            operations: &[],
            has_default: false,
            default: Value::Null,
            example: json!(1),
            invariants: &[
                "required positive integer",
                "monotonically contiguous within the origin store",
                "preserved by native restore",
            ],
            common_mistake: "Renumbering origin events after restore.",
        },
        ("checkpoint_event", "issue_id") => FieldSemantics {
            ownership: "system",
            operations: &[],
            has_default: false,
            default: Value::Null,
            example: Value::Null,
            invariants: &["null for workspace-scoped events", "names the affected issue otherwise"],
            common_mistake: "Assuming every event belongs to an issue.",
        },
        ("checkpoint_event", "kind") => FieldSemantics {
            ownership: "system",
            operations: &[],
            has_default: false,
            default: Value::Null,
            example: json!("updated"),
            invariants: &[
                "required nonempty string",
                "the published JSON Schema enum lists six issue-lifecycle kinds: updated, claimed, released, reopened, closed, assignment_cleared",
                "stores write further kinds: created, lease_renewed, claim_override, label_added, label_removed, dependency_added, dependency_removed, data_set, data_removed, external_ref_added, external_ref_removed, resource_keys_added, resource_keys_removed, attempt_resolved, checkpoint_restored, checkpoint_imported, checkpoint_monolithic or checkpoint_sharded (merge import summary), workspace_forked, historical_redaction",
                "import preserves foreign kinds verbatim",
            ],
            common_mistake: "Treating an unknown kind as permission to discard the event.",
        },
        ("checkpoint_event", "actor") => FieldSemantics {
            ownership: "system",
            operations: &[],
            has_default: false,
            default: Value::Null,
            example: json!("system"),
            invariants: &[
                "producer-required but import-tolerated as null",
                "derived from operation context, not authentication",
            ],
            common_mistake: "Treating it as proof of authentication or assuming import rejects null.",
        },
        ("checkpoint_event", "time") => FieldSemantics {
            ownership: "system",
            operations: &[],
            has_default: false,
            default: Value::Null,
            example: json!("2026-08-12T22:51:52.301697398Z"),
            invariants: &[
                "required non-null RFC 3339 UTC timestamp with nanosecond precision",
                "not part of event identity",
            ],
            common_mistake: "Using it as event identity.",
        },
        ("checkpoint_event", "detail") => FieldSemantics {
            ownership: "system",
            operations: &[],
            has_default: false,
            default: Value::Null,
            example: json!({}),
            invariants: &[
                "required in producer output; import defaults an absent member to null",
                "arbitrary JSON value with no defined keys",
            ],
            common_mistake:
                "Interpreting arbitrary detail keys as durable issue fields or conflating the producer example with the import default.",
        },
        // ---- checkpoint_provenance_receipt ----------------------------------
        ("checkpoint_provenance_receipt", "$schema") => FieldSemantics {
            ownership: "system",
            operations: &[],
            has_default: false,
            default: Value::Null,
            example: json!("urn:bead-rs:schema:provenance-receipt:native-v1"),
            invariants: &["required non-null absolute URI"],
            common_mistake:
                "Confusing this receipt identity with the issue or event schema identity.",
        },
        ("checkpoint_provenance_receipt", "receipt_id") => FieldSemantics {
            ownership: "system",
            operations: &[],
            has_default: false,
            default: Value::Null,
            example: json!("0".repeat(64)),
            invariants: &[
                "64 lowercase hex characters",
                "unique per receipt",
                "covers the receipt content",
            ],
            common_mistake: "Reading the receipt as issue state.",
        },
        ("checkpoint_provenance_receipt", "kind") => FieldSemantics {
            ownership: "system",
            operations: &[],
            has_default: false,
            default: Value::Null,
            example: json!("restore"),
            invariants: &[
                "one of restore, merge, fork",
                "records system-owned restore, merge, or fork provenance",
            ],
            common_mistake: "Reading the receipt as issue state.",
        },
        ("checkpoint_provenance_receipt", "source_store_uuid") => FieldSemantics {
            ownership: "system",
            operations: &[],
            has_default: false,
            default: Value::Null,
            example: json!("workspace-uuid"),
            invariants: &["origin store identity of the applied checkpoint"],
            common_mistake: "Substituting the destination store identity.",
        },
        ("checkpoint_provenance_receipt", "target_store_uuid") => FieldSemantics {
            ownership: "system",
            operations: &[],
            has_default: false,
            default: Value::Null,
            example: json!("workspace-uuid"),
            invariants: &["store identity the checkpoint was applied to"],
            common_mistake: "Swapping source and target identities.",
        },
        ("checkpoint_provenance_receipt", "source_root_sha256") => FieldSemantics {
            ownership: "system",
            operations: &[],
            has_default: false,
            default: Value::Null,
            example: json!("0".repeat(64)),
            invariants: &[
                "64 lowercase hex characters",
                "digest of the source checkpoint root",
            ],
            common_mistake: "Recomputing or re-basing the digest on the target store.",
        },
        ("checkpoint_provenance_receipt", "actor") => FieldSemantics {
            ownership: "system",
            operations: &[],
            has_default: false,
            default: Value::Null,
            example: json!("system"),
            invariants: &["derived from the operation context"],
            common_mistake: "Treating it as proof of authentication.",
        },
        ("checkpoint_provenance_receipt", "created_at") => FieldSemantics {
            ownership: "system",
            operations: &[],
            has_default: false,
            default: Value::Null,
            example: json!("2026-08-12T22:51:52.301697398Z"),
            invariants: &[
                "RFC 3339 UTC with nanosecond precision",
                "set once when the receipt is written",
            ],
            common_mistake:
                "Using it to order operations across stores; use the counts and event identity.",
        },
        ("checkpoint_provenance_receipt", "counts") => FieldSemantics {
            ownership: "system",
            operations: &[],
            has_default: false,
            default: Value::Null,
            example: json!({"events":0,"issues":1,"provenance_receipts":0}),
            invariants: &[
                "exactly the issues, events, and provenance_receipts members, each a nonnegative integer",
            ],
            common_mistake:
                "Reading counts as live-store totals; they describe the applied checkpoint.",
        },
        ("checkpoint_provenance_receipt", "result") => FieldSemantics {
            ownership: "system",
            operations: &[],
            has_default: false,
            default: Value::Null,
            example: json!("success"),
            invariants: &["operation outcome recorded by the producing operation"],
            common_mistake: "Treating a receipt as a request to re-apply the operation.",
        },
        ("checkpoint_provenance_receipt", "summary_event_identity") => FieldSemantics {
            ownership: "system",
            operations: &[],
            has_default: false,
            default: Value::Null,
            example: Value::Null,
            invariants: &["null when the operation produced no summary event"],
            common_mistake: "Assuming every receipt carries a summary event identity.",
        },
        ("checkpoint_provenance_receipt", "receipt_sha256") => FieldSemantics {
            ownership: "system",
            operations: &[],
            has_default: false,
            default: Value::Null,
            example: json!("0".repeat(64)),
            invariants: &[
                "64 lowercase hex characters",
                "digest binding the receipt content",
            ],
            common_mistake: "Validating the digest against mutated receipt content.",
        },
        _ => unspecified_semantics(),
    }
}

fn checkpoint_document_kind(document: &str) -> &'static str {
    match document {
        "checkpoint_event" => "audit_event",
        "checkpoint_provenance_receipt" => "provenance_receipt",
        _ => "issue",
    }
}

fn guide_field(document: &str, name: &str) -> Value {
    let schema = projection_property_schema(document, name);
    let json_type = schema
        .get("type")
        .and_then(|value| {
            value.as_str().or_else(|| {
                value.as_array()?.iter().find_map(|kind| {
                    let kind = kind.as_str()?;
                    (kind != "null").then_some(kind)
                })
            })
        })
        .unwrap_or_else(|| {
            if schema.get("const").is_some() {
                "integer"
            } else {
                "json"
            }
        });
    let nullable = schema
        .get("type")
        .and_then(Value::as_array)
        .is_some_and(|types| types.iter().any(|value| value == "null"));
    let presence = match (document, name) {
        // "conditional" = the member is omitted from the JSON until a condition
        // holds; "optional" = always emitted, null when not applicable. The
        // empty-queue claim result is the shape that separates them: bead_id
        // and lease are emitted as null, claim_epoch is skipped entirely.
        ("cli_issue", "claim_epoch") => "conditional",
        ("claim_result", "bead_id") => "optional",
        ("claim_result", "claim_epoch") => "conditional",
        ("claim_result", "lease") => "optional",
        ("cli_issue", _) | ("claim_result", _) => "required",
        _ => {
            let required = required_for(checkpoint_document_kind(document))
                .iter()
                .any(|member| member == name);
            if required {
                "required"
            } else {
                "optional"
            }
        }
    };
    let semantics = field_semantics(document, name);
    let mut operations: Vec<&str> = semantics.operations.to_vec();
    operations.sort_unstable();
    json!({
        "document":document,"name":name,"json_type":json_type,"nullable":nullable,
        "presence":presence,"has_default":semantics.has_default,
        "default":semantics.default,"ownership":semantics.ownership,
        "operations":operations,"invariants":semantics.invariants,
        "example":semantics.example,"common_mistake":semantics.common_mistake
    })
}

/// Documented mutating and read operations of the native model. Exit codes
/// follow the shared contract: 0 success, 2 usage, 3 not-found, 4 conflict,
/// 5 integrity, plus 1 (internal, including post-commit publication failure)
/// reachable from any command and 6 (database busy, the workspace operation
/// lock) from any guarded mutation. Per-operation
/// `failure_exits` list the operation-specific codes. Entries are emitted
/// lexicographically sorted by name.
fn guide_operations() -> Vec<Value> {
    struct OperationSemantics {
        name: &'static str,
        ownership_effect: &'static str,
        failure_exits: &'static [i64],
        affected_fields: &'static [&'static str],
        rules: &'static [&'static str],
    }
    let ops: &[OperationSemantics] = &[
        OperationSemantics {
            name: "claim",
            ownership_effect: "assigns the caller and starts work",
            failure_exits: &[2, 3, 4],
            affected_fields: &["assignee", "base_status", "claim_epoch", "revision", "updated_at"],
            rules: &[
                "atomic selection, assignment, and transition to in_progress",
                "candidates are open, unassigned, not manually blocked, free of unfinished blocks edges, and free of conflicting held resource keys",
                "default fifo-v1 orders candidates by priority ascending, creation time ascending, then ID ascending",
                "no revision guard; its atomic transaction and optional lease fencing provide concurrency safety",
                "appends the claimed event; the default sort advances the revision (intelligent R019 policies assign without one)",
                "an empty queue returns success with no nonempty bead_id",
            ],
        },
        OperationSemantics {
            name: "close",
            ownership_effect: "terminal caller-visible transition",
            failure_exits: &[2, 3, 4],
            affected_fields: &[
                "base_status",
                "closed_at",
                "close_reason",
                "manual_blocked",
                "revision",
                "updated_at",
            ],
            rules: &[
                "requires a non-empty --reason",
                "clears manual blocking",
                "may expose dependents",
                "preserves assignment and the claim epoch; reopen clears the assignee",
                "re-closing with a different reason conflicts (exit 4)",
                "accepts a previously read revision through --if-revision",
            ],
        },
        OperationSemantics {
            name: "create",
            ownership_effect: "initializes caller-owned fields",
            failure_exits: &[2, 3, 5],
            affected_fields: &[
                "assignee",
                "base_status",
                "created_at",
                "description",
                "id",
                "issue_type",
                "labels",
                "priority",
                "revision",
                "schema_ref",
                "title",
                "updated_at",
            ],
            rules: &[
                "only public entry point for new native issues",
                "title, description, priority, assignee, issue_type, and labels are fixed at creation",
                "appends the created audit event",
            ],
        },
        OperationSemantics {
            name: "data.remove",
            ownership_effect: "removes a caller-owned namespace",
            failure_exits: &[2, 3],
            affected_fields: &["data"],
            rules: &["each namespace's schema reference is immutable"],
        },
        OperationSemantics {
            name: "data.set",
            ownership_effect: "writes a caller-owned namespace",
            failure_exits: &[2, 3],
            affected_fields: &["data"],
            rules: &["each namespace's schema reference is immutable"],
        },
        OperationSemantics {
            name: "dep.add",
            ownership_effect: "extends caller-owned graph state",
            failure_exits: &[2, 3, 4],
            affected_fields: &["dependencies"],
            rules: &[
                "blocks edges reject self-edges and cycles",
                "relates_to is informational",
                "verifies declares the blocker checks the blocked issue's work; informational for readiness",
                "--condition attaches a bounded declarative predicate",
            ],
        },
        OperationSemantics {
            name: "dep.remove",
            ownership_effect: "removes caller-owned graph state",
            failure_exits: &[2, 3],
            affected_fields: &["dependencies"],
            rules: &["removing the last active blocker can expose a dependent"],
        },
        OperationSemantics {
            name: "label.add",
            ownership_effect: "extends caller-owned collections",
            failure_exits: &[2, 3],
            affected_fields: &["labels"],
            rules: &["idempotent"],
        },
        OperationSemantics {
            name: "label.remove",
            ownership_effect: "removes from caller-owned collections",
            failure_exits: &[2, 3],
            affected_fields: &["labels"],
            rules: &["idempotent"],
        },
        OperationSemantics {
            name: "list",
            ownership_effect: "read-only projection",
            failure_exits: &[2],
            affected_fields: &[],
            rules: &["inspects the ready frontier only with --ready, without reservation"],
        },
        OperationSemantics {
            name: "ref.add",
            ownership_effect: "extends caller-owned external references",
            failure_exits: &[2, 3],
            affected_fields: &["external_references"],
            rules: &["never resolves over a network"],
        },
        OperationSemantics {
            name: "ref.remove",
            ownership_effect: "removes caller-owned external references",
            failure_exits: &[2, 3],
            affected_fields: &["external_references"],
            rules: &["never resolves over a network"],
        },
        OperationSemantics {
            name: "release",
            ownership_effect: "clears assignment and stops work",
            failure_exits: &[2, 3, 4],
            affected_fields: &["assignee", "base_status", "revision", "updated_at"],
            rules: &[
                "semantically applies to in_progress -> open",
                "an assigned open bead uses update --clear-assignee instead",
                "a no-op on unassigned open work: success without a write or event",
                "conflicts on closed and deferred work",
                "the claim epoch survives as a high-water mark",
            ],
        },
        OperationSemantics {
            name: "reopen",
            ownership_effect: "reverses closure",
            failure_exits: &[2, 3, 4],
            affected_fields: &[
                "assignee",
                "base_status",
                "closed_at",
                "close_reason",
                "manual_blocked",
                "revision",
                "updated_at",
            ],
            rules: &[
                "clears close metadata, the assignee, and manual blocking",
                "returns closed work to open; a no-op on already-open work",
                "conflicts on in_progress and deferred work",
                "the claim epoch survives as a high-water mark",
                "accepts a previously read revision through --if-revision",
            ],
        },
        OperationSemantics {
            name: "schema.explain",
            ownership_effect: "read-only guide emission",
            failure_exits: &[2],
            affected_fields: &[],
            rules: &["workspace-independent; exact catalog identities only"],
        },
        OperationSemantics {
            name: "schema.list",
            ownership_effect: "read-only catalog emission",
            failure_exits: &[2],
            affected_fields: &[],
            rules: &["workspace-independent"],
        },
        OperationSemantics {
            name: "schema.show",
            ownership_effect: "read-only schema emission",
            failure_exits: &[2],
            affected_fields: &[],
            rules: &["emits the exact immutable JSON Schema for the identity"],
        },
        OperationSemantics {
            name: "show",
            ownership_effect: "read-only projection",
            failure_exits: &[2, 3],
            affected_fields: &[],
            rules: &["emits one issue projection"],
        },
        OperationSemantics {
            name: "sync.flush-only",
            ownership_effect: "publishes durable checkpoint state",
            failure_exits: &[2, 5],
            affected_fields: &[],
            rules: &["writes the durable checkpoint from the live store", "idempotent"],
        },
        OperationSemantics {
            name: "sync.import-only",
            ownership_effect: "restores or merges checkpoint state",
            failure_exits: &[2, 4, 5],
            affected_fields: &[],
            rules: &[
                "restores into a fresh workspace or merges into an existing one",
                "performs bidirectional issue validation before activation",
            ],
        },
        OperationSemantics {
            name: "update",
            ownership_effect: "mutates caller-owned fields",
            failure_exits: &[2, 3, 4],
            affected_fields: &[
                "assignee",
                "base_status",
                "claim_epoch",
                "manual_blocked",
                "notes",
                "revision",
                "updated_at",
            ],
            rules: &[
                "accepts a previously read revision through --if-revision",
                "a held claim epoch must be presented with --fencing-token",
                "cannot modify title, description, priority, or issue_type",
                "cannot enter or leave closed (use close or reopen)",
                "--status blocked sets the manual overlay without changing base status",
                "reassigning to a different worker mints a new claim epoch",
            ],
        },
        OperationSemantics {
            name: "resolve",
            ownership_effect: "applies an attempt outcome and records it",
            failure_exits: &[2, 3, 4, 5],
            affected_fields: &[
                "assignee",
                "base_status",
                "closed_at",
                "close_reason",
                "manual_blocked",
                "revision",
                "updated_at",
            ],
            rules: &[
                "records the attempt_outcome row and the attempt_resolved event in one transaction",
                "close and release route through the lifecycle rules; block sets the manual overlay",
                "claimed work requires the claim epoch via --fencing-token for every action; close and release re-validate through the shared lifecycle rules",
                "verified_success preserves the attempt tier; work_failure advances it (tier 3 quarantines)",
                "an identical retry returns the original receipt with is_replay true and no mutation; a divergent reuse of the attempt id conflicts (exit 4)",
                "the printed receipt is not separately persisted; the durable record is the attempt_outcome row",
            ],
        },
    ];
    let mut entries: Vec<Value> = ops
        .iter()
        .map(|op| {
            json!({
                "name": op.name,
                "ownership_effect": op.ownership_effect,
                "success_exit": 0,
                "failure_exits": op.failure_exits,
                "affected_fields": op.affected_fields,
                "rules": op.rules,
            })
        })
        .collect();
    entries.sort_by(|left, right| {
        left["name"]
            .as_str()
            .unwrap_or_default()
            .cmp(right["name"].as_str().unwrap_or_default())
    });
    entries
}

fn native_field_guide() -> Value {
    let documents = guide_documents();
    let fields: Vec<Value> = documents
        .iter()
        .flat_map(|document| {
            let name = document["name"].as_str().unwrap();
            document["members"]
                .as_array()
                .unwrap()
                .iter()
                .map(move |member| guide_field(name, member.as_str().unwrap()))
        })
        .collect();
    json!({
        "schema_ref":FIELD_GUIDE_SCHEMA_REF,"guide_version":FIELD_GUIDE_VERSION,
        "describes_schema_refs":["urn:bead-rs:schema:event:native-v1","urn:bead-rs:schema:issue:native-v1","urn:bead-rs:schema:provenance-receipt:native-v1"],
        "documents":documents,"fields":fields,
        "additional_properties":{"allowed":true,"ownership":"preserved","rules":["Unknown checkpoint issue members retain exact JSON name, type, value, and null-versus-absence presence."]},
        "lifecycle":{"base_values":["closed","deferred","in_progress","open"],"allowed_transitions":["closed->open","deferred->closed","deferred->open","in_progress->closed","in_progress->deferred","in_progress->open","open->closed","open->deferred","open->in_progress"]},
        "derived_state":{"status":{"ownership":"derived","rules":["manual_blocked overlays non-closed base status as blocked"]},"ready":{"ownership":"derived","rules":["base status is open, not manually blocked, unassigned, has no unfinished blocks blocker, and holds no conflicting resource key"]},"blocked_by":{"ownership":"derived","rules":["derived from incoming blocks edges"]},"blocking":{"ownership":"derived","rules":["derived from outgoing blocks edges"]}},
        "events":{"envelope_member":"event","schema_ref_member":"$schema","identity":["origin_store_uuid","origin_event_sequence"],"ordering":["origin_store_uuid","origin_event_sequence"]},
        "operations":guide_operations(),
        "rehydration":{"source_mode":"read-only","allowed_writes":["public bead commands in a separate destination"],"forbidden_writes":["foreign SQLite","native SQLite","synthetic checkpoint JSON"],"verification":["issue reconciliation","dependency orientation","ready frontier","fresh restore"]},
        "known_implementation_deviations":[{"id":"manual-blocked-cli-projection","severity":"known","behavior":"v0.1 CLI projections expose base_status without the manual_blocked overlay","required_disposition":"Consumers must not infer readiness from status alone."}]
    })
}

pub fn schema_explanation(schema_ref: &str) -> Result<Value> {
    let descriptor = descriptor(schema_ref)?;
    if matches!(
        descriptor.document_kind,
        "issue" | "audit_event" | "provenance_receipt"
    ) {
        return Ok(native_field_guide());
    }
    let schema = schema_document(schema_ref)?;
    let members: Vec<String> = schema["properties"]
        .as_object()
        .expect("schema properties are objects")
        .keys()
        .cloned()
        .collect();
    let fields: Vec<Value> = members
        .iter()
        .map(|name| {
            json!({
                "document": descriptor.document_kind,
                "name": name,
                "json_type": "any",
                "nullable": true,
                "presence": "schema-defined",
                "has_default": false,
                "default": null,
                "ownership": "document producer",
                "operations": [],
                "invariants": [],
                "example": null,
                "common_mistake": "Inferring semantics from the member name instead of this schema identity."
            })
        })
        .collect();
    let base_values = if descriptor.document_kind == "issue" {
        json!(["closed", "deferred", "in_progress", "open"])
    } else {
        json!([])
    };
    Ok(json!({
        "schema_ref": FIELD_GUIDE_SCHEMA_REF,
        "guide_version": FIELD_GUIDE_VERSION,
        "describes_schema_refs": [descriptor.schema_ref],
        "documents": [{
            "name": descriptor.document_kind,
            "schema_ref": descriptor.schema_ref,
            "document_kind": descriptor.document_kind,
            "transport": "public JSON",
            "member_source": "typed schema registry",
            "members": members
        }],
        "fields": fields,
        "additional_properties": {
            "allowed": descriptor.document_kind == "issue",
            "ownership": "producer",
            "rules": ["Unknown issue members are preserved; other documents reject unknown members."]
        },
        "lifecycle": {"base_values": base_values, "allowed_transitions": []},
        "derived_state": {
            "status": {"ownership": "system", "rules": []},
            "ready": {"ownership": "system", "rules": []},
            "blocked_by": {"ownership": "system", "rules": []},
            "blocking": {"ownership": "system", "rules": []}
        },
        "events": {"envelope_member": "event", "schema_ref_member": "$schema", "identity": [], "ordering": []},
        "operations": [{
            "name": "schema.show",
            "ownership_effect": "read-only",
            "success_exit": 0,
            "failure_exits": [2],
            "affected_fields": [],
            "rules": ["Exact schema identities only."]
        }],
        "rehydration": {"source_mode": "read-only", "allowed_writes": [], "forbidden_writes": [], "verification": []},
        "known_implementation_deviations": []
    }))
}

/// Deterministic Markdown rendering of the typed guide value: fixed section
/// order, fields in document order, operations in their lexicographically
/// sorted array order, LF line endings, and no generated timestamp. Every
/// typed member the JSON carries is represented here.
pub fn schema_explanation_markdown(explanation: &Value) -> String {
    let mut output = format!(
        "# Native field guide v{}\n\nSchema: `{}`\n\n",
        explanation["guide_version"].as_i64().unwrap_or(1),
        explanation["schema_ref"].as_str().unwrap_or("")
    );
    output.push_str("## Documents\n\n");
    for document in explanation["documents"].as_array().into_iter().flatten() {
        output.push_str(&format!(
            "### {}\n\nSchema: `{}`\n\nMembers:\n\n",
            document["name"].as_str().unwrap_or("document"),
            document["schema_ref"].as_str().unwrap_or("")
        ));
        for member in document["members"].as_array().into_iter().flatten() {
            output.push_str(&format!("- `{}`\n", member.as_str().unwrap_or("")));
        }
        output.push('\n');
    }
    output.push_str("## Fields\n\n");
    for field in explanation["fields"].as_array().into_iter().flatten() {
        output.push_str(&format!(
            "### {}.{}\n\n- Type: `{}`{}\n- Presence: `{}`\n- Ownership: `{}`\n",
            field["document"].as_str().unwrap_or("document"),
            field["name"].as_str().unwrap_or("member"),
            field["json_type"].as_str().unwrap_or("json"),
            if field["nullable"].as_bool().unwrap_or(false) {
                " (nullable)"
            } else {
                ""
            },
            field["presence"].as_str().unwrap_or("unknown"),
            field["ownership"].as_str().unwrap_or("unknown")
        ));
        if field["has_default"].as_bool().unwrap_or(false) {
            output.push_str(&format!("- Default: `{}`\n", field["default"]));
        } else {
            output.push_str("- Default: none\n");
        }
        output.push_str(&format!(
            "- Operations: {}\n",
            field["operations"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|operation| format!("`{}`", operation.as_str().unwrap_or("")))
                .collect::<Vec<_>>()
                .join(", ")
        ));
        for invariant in field["invariants"].as_array().into_iter().flatten() {
            output.push_str(&format!(
                "- Invariant: {}\n",
                invariant.as_str().unwrap_or("")
            ));
        }
        output.push_str(&format!("- Example: `{}`\n", field["example"]));
        output.push_str(&format!(
            "- Common mistake: {}\n\n",
            field["common_mistake"].as_str().unwrap_or("")
        ));
    }
    output.push_str("## Additional properties\n\n");
    let additional = &explanation["additional_properties"];
    output.push_str(&format!(
        "- Allowed: {}\n- Ownership: `{}`\n",
        additional["allowed"].as_bool().unwrap_or(false),
        additional["ownership"].as_str().unwrap_or("unknown")
    ));
    for rule in additional["rules"].as_array().into_iter().flatten() {
        output.push_str(&format!("- {}\n", rule.as_str().unwrap_or("")));
    }
    output.push('\n');
    output.push_str("## Lifecycle\n\n- Base values:");
    let lifecycle = &explanation["lifecycle"];
    for value in lifecycle["base_values"].as_array().into_iter().flatten() {
        output.push_str(&format!(" `{}`", value.as_str().unwrap_or("")));
    }
    output.push_str("\n- Allowed transitions:");
    for value in lifecycle["allowed_transitions"]
        .as_array()
        .into_iter()
        .flatten()
    {
        output.push_str(&format!(" `{}`", value.as_str().unwrap_or("")));
    }
    output.push_str("\n\n");
    output.push_str("## Derived state\n\n");
    for name in ["status", "ready", "blocked_by", "blocking"] {
        let derived = &explanation["derived_state"][name];
        output.push_str(&format!(
            "### {name}\n\n- Ownership: `{}`\n",
            derived["ownership"].as_str().unwrap_or("unknown")
        ));
        for rule in derived["rules"].as_array().into_iter().flatten() {
            output.push_str(&format!("- {}\n", rule.as_str().unwrap_or("")));
        }
        output.push('\n');
    }
    output.push_str("## Events\n\n");
    let events = &explanation["events"];
    output.push_str(&format!(
        "- Envelope member: `{}`\n- Schema reference member: `{}`\n- Identity:",
        events["envelope_member"].as_str().unwrap_or(""),
        events["schema_ref_member"].as_str().unwrap_or("")
    ));
    for member in events["identity"].as_array().into_iter().flatten() {
        output.push_str(&format!(" `{}`", member.as_str().unwrap_or("")));
    }
    output.push_str("\n- Ordering:");
    for member in events["ordering"].as_array().into_iter().flatten() {
        output.push_str(&format!(" `{}`", member.as_str().unwrap_or("")));
    }
    output.push_str("\n\n");
    output.push_str("## Operations\n\n");
    for operation in explanation["operations"].as_array().into_iter().flatten() {
        output.push_str(&format!(
            "### {}\n\n- Ownership effect: {}\n- Success exit: `{}`\n- Failure exits:",
            operation["name"].as_str().unwrap_or("operation"),
            operation["ownership_effect"].as_str().unwrap_or(""),
            operation["success_exit"].as_i64().unwrap_or(0)
        ));
        for exit in operation["failure_exits"].as_array().into_iter().flatten() {
            output.push_str(&format!(" `{}`", exit));
        }
        output.push_str("\n- Affected fields:");
        for field in operation["affected_fields"]
            .as_array()
            .into_iter()
            .flatten()
        {
            output.push_str(&format!(" `{}`", field.as_str().unwrap_or("")));
        }
        output.push_str("\n\nRules:\n\n");
        for rule in operation["rules"].as_array().into_iter().flatten() {
            output.push_str(&format!("- {}\n", rule.as_str().unwrap_or("")));
        }
        output.push('\n');
    }
    output.push_str("## Rehydration\n\n- Source mode: ");
    let rehydration = &explanation["rehydration"];
    output.push_str(&format!(
        "`{}`\n",
        rehydration["source_mode"].as_str().unwrap_or("unknown")
    ));
    for (label, member) in [
        ("Allowed writes", "allowed_writes"),
        ("Forbidden writes", "forbidden_writes"),
        ("Verification", "verification"),
    ] {
        output.push_str(&format!("- {label}:"));
        for value in rehydration[member].as_array().into_iter().flatten() {
            output.push_str(&format!(" `{}`", value.as_str().unwrap_or("")));
        }
        output.push('\n');
    }
    output.push('\n');
    output.push_str("## Known implementation deviations\n\n");
    for deviation in explanation["known_implementation_deviations"]
        .as_array()
        .into_iter()
        .flatten()
    {
        output.push_str(&format!(
            "### {}\n\n- Severity: `{}`\n- Behavior: {}\n- Required disposition: {}\n\n",
            deviation["id"].as_str().unwrap_or("deviation"),
            deviation["severity"].as_str().unwrap_or("unknown"),
            deviation["behavior"].as_str().unwrap_or(""),
            deviation["required_disposition"].as_str().unwrap_or("")
        ));
    }
    output
}

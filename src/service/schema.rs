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
///
/// v6: the concise (per-schema) explanation path publishes curated per-member
/// semantics, defaults, and invariants and a populated lifecycle for every
/// catalog identity, replacing the `document producer` placeholder fields and
/// the empty lifecycle section (beadrs-62a57bdf).
///
/// v7: the (audit_event, kind) enum enumerates every kind the stores actually
/// write — it previously listed only the six issue-lifecycle kinds while the
/// code wrote twenty-one more, so `bead schema show` validated stores against
/// an enum they violate (beadrs-085daf20).
///
/// v8: the capabilities catalog publishes its `attempt_outcome` member — the
/// implementation struct has carried the ADR-012 handshake since it landed
/// (always emitted, `supported: true`, src/service/capabilities.rs) while the
/// published schema omitted it, so `bead schema show` under-reported the
/// documents this binary emits and rejected a member the contract's own
/// detection recipe reads (beadrs-7b6590ba).
///
/// v9: the guide's own `schema.explain` operations entry now carries worked
/// examples for both `--format` values and the two common mistakes agents hit
/// with the command (byte-exact identity resolution, concise-path
/// restrictions), and the `field_guide.guide_version` example tracks the
/// compiled constant instead of a stale literal (beadrs-bead5974).
pub const FIELD_GUIDE_VERSION: i64 = 9;

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
    // The checkpoint-set-v1 edge value names the durable checkpoint fileset
    // (its format tag), not an operation like sync.flush-only: sync both
    // emits these documents into the set and consumes them back on the next
    // publication, import, and restore, so the edges are genuinely
    // bidirectional. SchemaEntry.consume/emit values are documented as
    // operations but carry this set-format label too — dual use by design
    // (audit o1, beadrs-f8805045).
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
            "dependency_kinds",
            "checkpoint_modes",
            "checkpoint_formats",
            "logical_revision",
            "schema_ref",
            "schemas",
            "commands",
            // Additive R026 handshake (plan section 11): optional so a
            // producer whose compiled automatic-flush default is off can
            // omit it and still validate. This binary's default has been on
            // since the R026 activation flipped it, so the documents it
            // emits always carry the member
            "auto_flush",
            // Additive ADR-018 handshake: post-publication staging of the
            // verified checkpoint fileset, optional for the same reason
            "auto_stage",
            // Additive ADR-012 handshake: attempt outcome resolution
            // capabilities, optional so a producer predating attempt
            // resolution can omit it and still validate. This binary always
            // emits it with supported true (src/service/capabilities.rs),
            // and the contract's own detection recipe reads
            // .attempt_outcome.supported (src/cli.rs resolve --help)
            "attempt_outcome",
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
        // The three trailing members mirror write_current_pointer's
        // redaction extension: inserted after created_at, and only on a
        // pointer published from a redaction generation, so required_for
        // keeps them optional (audit F3, beadrs-f8805045).
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
            "redaction_epoch_id",
            "previous_generation_reset",
            "superseded_generations",
        ],
        // issue_partition, the four totals, and origins mirror the sharded
        // manifest writer. They joined the format checkpoint-set-v1 member
        // set after the first sharded publications while schema_version
        // stayed 1, so required_for keeps them optional like the other
        // additive members (audit F2, beadrs-f8805045).
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
            "issue_partition",
            "issue_count",
            "event_count",
            "receipt_count",
            "issue_shards",
            "event_shards",
            "receipt_shards",
            "attempt_outcome_count",
            "redaction_record_count",
            "total_record_count",
            "attempt_outcome_shards",
            "redaction_shards",
            "origins",
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
        // The producer's full write set, one enum value per call site. Import
        // can additionally preserve foreign kinds verbatim; those stay outside
        // the published enum by design.
        ("audit_event", "kind") => {
            json!({"type":"string", "enum":[
                "created","updated","claimed","released","reopened","closed",
                "assignment_cleared","lease_renewed","claim_override",
                "label_added","label_removed",
                "dependency_added","dependency_removed",
                "data_set","data_removed",
                "external_ref_added","external_ref_removed",
                "resource_keys_added","resource_keys_removed",
                "attempt_resolved",
                "checkpoint_restored","checkpoint_imported",
                "checkpoint_monolithic","checkpoint_sharded",
                "workspace_forked","historical_redaction","secret_acknowledged",
            ]})
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
        // Redaction-published pointers only; same shapes the redaction_epoch
        // document carries for the same values
        ("checkpoint_pointer", "redaction_epoch_id") => {
            json!({"type":"string", "pattern":"^[0-9a-f]{64}$"})
        }
        ("checkpoint_pointer", "previous_generation_reset") => json!({"type":"boolean"}),
        ("checkpoint_pointer", "superseded_generations") => {
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
        ("checkpoint_manifest", "issue_count")
        | ("checkpoint_manifest", "event_count")
        | ("checkpoint_manifest", "receipt_count")
        | ("checkpoint_manifest", "total_record_count") => {
            json!({"type":"integer", "minimum":0})
        }
        ("checkpoint_manifest", "issue_partition") => {
            json!({"type":"array", "items":{"type":"string"}})
        }
        // Per-origin event summary the sharded writer records alongside the
        // event objects it packed for that origin
        ("checkpoint_manifest", "origins") => {
            json!({"type":"array", "items":{
                "type":"object",
                "required":["origin_store_uuid","event_count","min_sequence","max_sequence","objects"],
                "properties":{
                    "origin_store_uuid":{"type":"string"},
                    "event_count":{"type":"integer","minimum":0},
                    "min_sequence":{"type":"integer","minimum":1},
                    "max_sequence":{"type":"integer","minimum":1},
                    "objects":{"type":"array","items":{"type":"string"}}
                },
                "additionalProperties":false
            }})
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
        // The native dependency write set, the same posture as the
        // audit-event kind enum above: interchange keeps foreign kinds
        // preservable, and those stay outside the published enum by design.
        ("capabilities", "dependency_kinds") => {
            json!({"type":"array", "items":{"type":"string", "enum":["blocks","relates_to","verifies"]}})
        }
        ("capabilities", "statuses")
        | ("capabilities", "checkpoint_modes")
        | ("capabilities", "checkpoint_formats")
        | ("capabilities", "schemas")
        | ("capabilities", "commands") => json!({"type":"array"}),
        ("capabilities", "priorities") => json!({"type":"object"}),
        ("capabilities", "attempt_outcome")
        | ("capabilities", "secret_scan")
        | ("capabilities", "historical_redaction") => {
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
        // Optional so a document from a producer whose compiled
        // automatic-flush default is off validates; this binary's default
        // has been on since the R026 activation flipped it, so the
        // documents it emits always carry the member, and both shapes
        // validate against the same additive identity (plan section 11).
        // `auto_stage` is additive the same way (ADR-018), and
        // `attempt_outcome` likewise (ADR-012): a producer without
        // attempt resolution omits the member entirely rather than
        // publishing supported false
        "capabilities" => &[
            "auto_flush",
            "auto_stage",
            "attempt_outcome",
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
        // The redaction members exist only on redaction-published pointers;
        // the additive counts exist only on manifests from the
        // sha256-hex-prefix era forward — both stay optional so documents
        // from producers that predate the member still validate
        "checkpoint_pointer" => &[
            "attempt_outcome_count",
            "redaction_record_count",
            "redaction_epoch_id",
            "previous_generation_reset",
            "superseded_generations",
        ],
        "checkpoint_manifest" => &[
            "attempt_outcome_count",
            "redaction_record_count",
            "attempt_outcome_shards",
            "redaction_shards",
            "issue_partition",
            "issue_count",
            "event_count",
            "receipt_count",
            "total_record_count",
            "origins",
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
                "the published JSON Schema enum enumerates every kind the stores write: created, updated, claimed, released, reopened, closed, assignment_cleared, lease_renewed, claim_override, label_added, label_removed, dependency_added, dependency_removed, data_set, data_removed, external_ref_added, external_ref_removed, resource_keys_added, resource_keys_removed, attempt_resolved, checkpoint_restored, checkpoint_imported, checkpoint_monolithic, checkpoint_sharded, workspace_forked, historical_redaction, secret_acknowledged",
                "checkpoint_monolithic and checkpoint_sharded are the merge import summary kinds, selected by checkpoint mode",
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
        // ---- redaction_finding (scanner finding persisted by redact) ------
        ("redaction_finding", "$schema") => FieldSemantics {
            ownership: "system",
            operations: &["redact"],
            has_default: false,
            default: Value::Null,
            example: json!(SCHEMA_REDACTION_FINDING),
            invariants: &[
                "exact catalog identity; any other value is a usage rejection",
                "stamped on every finding the redact commit persists",
            ],
            common_mistake: "Accepting a finding document whose schema identity is absent or renamed.",
        },
        ("redaction_finding", "fingerprint") => FieldSemantics {
            ownership: "caller",
            operations: &["redact"],
            has_default: false,
            default: Value::Null,
            example: json!("0".repeat(64)),
            invariants: &[
                "64 lowercase hex characters",
                "derived from the rule and the selector, so naming it leaks nothing about the matched bytes",
                "the only handle a caller ever holds; the store revalidates it against the addressed live bytes before anything is replaced",
            ],
            common_mistake: "Treating the fingerprint as content: it addresses the finding, it never contains it.",
        },
        ("redaction_finding", "ruleset_version") => FieldSemantics {
            ownership: "system",
            operations: &["redact"],
            has_default: false,
            default: Value::Null,
            example: json!(1),
            invariants: &[
                "positive integer stamped by the scanner that produced the finding",
                "carried onto the receipt so a stored redaction records which rule version matched",
            ],
            common_mistake: "Assuming findings from different ruleset versions address the same bytes.",
        },
        ("redaction_finding", "rule_id") => FieldSemantics {
            ownership: "system",
            operations: &["redact"],
            has_default: false,
            default: Value::Null,
            example: json!("aws-access-key-id"),
            invariants: &[
                "nonempty and at most 128 bytes",
                "identifier of the single rule that matched",
            ],
            common_mistake: "Parsing rule_id to infer severity; severity travels in its own member.",
        },
        ("redaction_finding", "selector") => FieldSemantics {
            ownership: "system",
            operations: &["redact"],
            has_default: false,
            default: Value::Null,
            example: json!({"record_kind":"issues","origin_identity":"bead-18409c0e","field_path":"description","byte_start":32,"byte_length":20,"prior_record_hash":"0".repeat(64)}),
            invariants: &[
                "addresses the matched byte range: record kind, origin identity, dotted field path, byte range, and the pre-redaction record hash",
                "carried onto the receipt; the store revalidates it under the redaction transaction before replacing anything",
            ],
            common_mistake: "Reading the selector as the secret: it is the address of the removed content, never the content.",
        },
        ("redaction_finding", "severity") => FieldSemantics {
            ownership: "system",
            operations: &["redact"],
            has_default: false,
            default: Value::Null,
            example: json!("blocking"),
            invariants: &[
                "enum: blocking, advisory",
                "blocking means a confirmed high-confidence match; under enforce mode the mutation is refused unless the exact fingerprint is acknowledged",
                "advisory matches are reported and may be acknowledged without refusing the mutation",
                "recorded so a stored receipt preserves how certain the match was without recording what matched",
            ],
            common_mistake: "Assuming severity alone decides refusal: mode and exact-fingerprint acknowledgment participate.",
        },
        ("redaction_finding", "detected_at") => FieldSemantics {
            ownership: "system",
            operations: &["redact"],
            has_default: false,
            default: Value::Null,
            example: json!("2026-09-21T12:00:00.000000000Z"),
            invariants: &[
                "RFC 3339 timestamp",
                "the scanner runs per invocation; only a redact commit persists a finding row, so this is the commit instant",
            ],
            common_mistake: "Assuming findings are persisted at scan time.",
        },
        // ---- redaction_acknowledgment (admitted-finding record) ------------
        ("redaction_acknowledgment", "$schema") => FieldSemantics {
            ownership: "system",
            operations: &["sync.import-only"],
            has_default: false,
            default: Value::Null,
            example: json!(SCHEMA_REDACTION_ACKNOWLEDGMENT),
            invariants: &[
                "exact catalog identity; any other value is a usage rejection",
                "no command in this binary authors the record locally: acknowledgment records enter a workspace through checkpoint import activation, while an invocation-level --acknowledge-secret admission appends a secret_acknowledged audit event instead",
            ],
            common_mistake: "Looking for a command that creates acknowledgment rows; the durable records travel with checkpoints.",
        },
        ("redaction_acknowledgment", "fingerprint") => FieldSemantics {
            ownership: "system",
            operations: &["sync.import-only"],
            has_default: false,
            default: Value::Null,
            example: json!("0".repeat(64)),
            invariants: &[
                "64 lowercase hex characters",
                "unique per record: activation ignores a duplicate fingerprint",
                "identifies the admitted finding without exposing it",
            ],
            common_mistake: "Storing the matched value here; the record carries the finding's handle only.",
        },
        ("redaction_acknowledgment", "actor") => FieldSemantics {
            ownership: "caller",
            operations: &["sync.import-only"],
            has_default: false,
            default: Value::Null,
            example: json!("operator"),
            invariants: &[
                "nonempty, at most 255 bytes, no control characters",
                "identity of whoever accepted the finding",
            ],
            common_mistake: "Attributing the admission to the scanner; a person or agent accepted it.",
        },
        ("redaction_acknowledgment", "reason") => FieldSemantics {
            ownership: "caller",
            operations: &["sync.import-only"],
            has_default: false,
            default: Value::Null,
            example: json!("Test fixture credential, rotation tracked elsewhere"),
            invariants: &[
                "required nonempty, at most 1024 bytes, control characters forbidden",
                "the nonsecret rationale for accepting the finding",
            ],
            common_mistake: "Writing the secret itself into the reason; the reason is auditable free text.",
        },
        ("redaction_acknowledgment", "acknowledged_at") => FieldSemantics {
            ownership: "system",
            operations: &["sync.import-only"],
            has_default: false,
            default: Value::Null,
            example: json!("2026-09-21T12:00:00.000000000Z"),
            invariants: &["RFC 3339 timestamp of the admission"],
            common_mistake: "Deriving it from the audit event time; the record carries its own instant.",
        },
        // ---- redaction_field_selector (address of removed bytes) -----------
        ("redaction_field_selector", "$schema") => FieldSemantics {
            ownership: "system",
            operations: &["redact"],
            has_default: false,
            default: Value::Null,
            example: json!(SCHEMA_REDACTION_FIELD_SELECTOR),
            invariants: &[
                "exact catalog identity; any other value is a usage rejection",
                "the selector envelope is embedded inside finding and receipt records, not stored as an independent row",
            ],
            common_mistake: "Treating the selector as a standalone stored document.",
        },
        ("redaction_field_selector", "record_kind") => FieldSemantics {
            ownership: "system",
            operations: &["redact"],
            has_default: false,
            default: Value::Null,
            example: json!("issues"),
            invariants: &[
                "lowercase letters, digits, and underscores; at most 32 bytes",
                "the kind of stored record addressed",
            ],
            common_mistake: "Assuming any record kind is addressable; the kind is the store's own record identity.",
        },
        ("redaction_field_selector", "origin_identity") => FieldSemantics {
            ownership: "system",
            operations: &["redact"],
            has_default: false,
            default: Value::Null,
            example: json!("bead-18409c0e"),
            invariants: &[
                "nonempty, at most 512 bytes, no control characters",
                "an issue addresses by ID; an event addresses by <origin_store_uuid>:<origin_event_sequence>, the identity that survives a restore into a different local sequence",
            ],
            common_mistake: "Addressing an event by its local sequence number, which a restore reassigns.",
        },
        ("redaction_field_selector", "field_path") => FieldSemantics {
            ownership: "system",
            operations: &["redact"],
            has_default: false,
            default: Value::Null,
            example: json!("description"),
            invariants: &[
                "dot-separated segments, each a lowercase identifier or nonnegative decimal index of at most 64 bytes; the whole path at most 256 bytes",
                "numeric segments address sequence and map elements",
            ],
            common_mistake: "Using the path as a filesystem path or SQL fragment; its shape keeps it unambiguous as a selector component.",
        },
        ("redaction_field_selector", "byte_start") => FieldSemantics {
            ownership: "system",
            operations: &["redact"],
            has_default: false,
            default: Value::Null,
            example: json!(32),
            invariants: &[
                "zero-based byte offset into the field's UTF-8 encoding, minimum 0",
                "byte counts, not character counts, so a selector survives non-ASCII content and still selects the same bytes on revalidation",
            ],
            common_mistake: "Reading the offset as a character position.",
        },
        ("redaction_field_selector", "byte_length") => FieldSemantics {
            ownership: "system",
            operations: &["redact"],
            has_default: false,
            default: Value::Null,
            example: json!(20),
            invariants: &[
                "positive byte count",
                "start plus length may not exceed the 4 MiB field bound, so a malformed selector cannot claim a range wider than any real field",
            ],
            common_mistake: "Selecting a range wider than the field; the bound exists to keep receipts honest about what they addressed.",
        },
        ("redaction_field_selector", "prior_record_hash") => FieldSemantics {
            ownership: "system",
            operations: &["redact"],
            has_default: false,
            default: Value::Null,
            example: json!("0".repeat(64)),
            invariants: &[
                "64 lowercase hex characters",
                "SHA-256 of the addressed record as it stood before the redaction",
                "the stale-detection key: a record that no longer hashes to this value conflicts instead of mutating",
            ],
            common_mistake: "Redacting against a stale hash; the store refuses rather than guess.",
        },
        // ---- redaction_receipt (committed historical redaction) ------------
        ("redaction_receipt", "$schema") => FieldSemantics {
            ownership: "system",
            operations: &["redact"],
            has_default: false,
            default: Value::Null,
            example: json!(SCHEMA_REDACTION_RECEIPT),
            invariants: &[
                "exact catalog identity; any other value is a usage rejection",
                "stamped on every receipt the redact commit persists and packs into checkpoints",
            ],
            common_mistake: "Accepting a receipt whose schema identity is absent or renamed.",
        },
        ("redaction_receipt", "receipt_id") => FieldSemantics {
            ownership: "system",
            operations: &["redact"],
            has_default: false,
            default: Value::Null,
            example: json!("0".repeat(64)),
            invariants: &[
                "64 lowercase hex characters: the canonical identity derived from every fact that makes two redactions the same redaction",
                "an exact replay finds the same row instead of applying twice; --resume targets the committed receipt by it",
                "publication state and epoch linkage are excluded from the identity, so a later publication keeps the identity the receipt committed with",
            ],
            common_mistake: "Recomputing a receipt ID by hand instead of replaying the redaction.",
        },
        ("redaction_receipt", "finding_fingerprint") => FieldSemantics {
            ownership: "system",
            operations: &["redact"],
            has_default: false,
            default: Value::Null,
            example: json!("0".repeat(64)),
            invariants: &[
                "64 lowercase hex characters",
                "the fingerprint of the finding that was redacted",
            ],
            common_mistake: "Expecting the matched value; the receipt carries the finding's handle.",
        },
        ("redaction_receipt", "ruleset_version") => FieldSemantics {
            ownership: "system",
            operations: &["redact"],
            has_default: false,
            default: Value::Null,
            example: json!(1),
            invariants: &[
                "positive integer",
                "the ruleset version that produced the redacted finding",
            ],
            common_mistake: "Reading it as the redaction feature's version.",
        },
        ("redaction_receipt", "rule_id") => FieldSemantics {
            ownership: "system",
            operations: &["redact"],
            has_default: false,
            default: Value::Null,
            example: json!("aws-access-key-id"),
            invariants: &["nonempty and at most 128 bytes", "the rule that matched"],
            common_mistake: "Inferring severity from the rule identifier.",
        },
        ("redaction_receipt", "selector") => FieldSemantics {
            ownership: "system",
            operations: &["redact"],
            has_default: false,
            default: Value::Null,
            example: json!({"$schema":SCHEMA_REDACTION_FIELD_SELECTOR,"record_kind":"issues","origin_identity":"bead-18409c0e","field_path":"description","byte_start":32,"byte_length":20,"prior_record_hash":"0".repeat(64)}),
            invariants: &[
                "where the replaced bytes sat, revalidated under the redaction transaction before the replacement",
            ],
            common_mistake: "Treating the stored selector as reusable against current bytes; staleness conflicts.",
        },
        ("redaction_receipt", "prior_record_hash") => FieldSemantics {
            ownership: "system",
            operations: &["redact"],
            has_default: false,
            default: Value::Null,
            example: json!("0".repeat(64)),
            invariants: &[
                "64 lowercase hex characters",
                "must equal the selector's prior_record_hash; disagreement is an integrity rejection",
                "must differ from sanitized_record_hash; a receipt recording an unchanged record is an integrity rejection",
            ],
            common_mistake: "Hand-editing one hash without the other; the pair is validated together.",
        },
        ("redaction_receipt", "sanitized_record_hash") => FieldSemantics {
            ownership: "system",
            operations: &["redact"],
            has_default: false,
            default: Value::Null,
            example: json!("0".repeat(64)),
            invariants: &[
                "64 lowercase hex characters",
                "SHA-256 of the record after the replacement; with prior_record_hash the before/after pair reads as one fact",
            ],
            common_mistake: "Validating the pair against mutated record content.",
        },
        ("redaction_receipt", "actor") => FieldSemantics {
            ownership: "caller",
            operations: &["redact"],
            has_default: false,
            default: Value::Null,
            example: json!("operator"),
            invariants: &[
                "nonempty, at most 255 bytes, no control characters",
                "supplied through bead redact --actor",
            ],
            common_mistake: "Attributing the redaction to the scanner; an actor ordered it.",
        },
        ("redaction_receipt", "reason") => FieldSemantics {
            ownership: "caller",
            operations: &["redact"],
            has_default: false,
            default: Value::Null,
            example: json!("credential rotation"),
            invariants: &[
                "required nonempty, at most 1024 bytes, control characters forbidden",
                "supplied through bead redact --reason; an unexplained destructive repair is not auditable",
            ],
            common_mistake: "Omitting or padding the reason to pass validation.",
        },
        ("redaction_receipt", "redacted_at") => FieldSemantics {
            ownership: "system",
            operations: &["redact"],
            has_default: false,
            default: Value::Null,
            example: json!("2026-09-21T12:00:00.000000000Z"),
            invariants: &[
                "RFC 3339 timestamp of the commit",
                "part of the canonical identity, so a replay of the same redaction derives the same receipt",
            ],
            common_mistake: "Reusing a timestamp across distinct redactions and expecting identity to hold.",
        },
        ("redaction_receipt", "affected_issue_revision") => FieldSemantics {
            ownership: "system",
            operations: &["redact"],
            has_default: false,
            default: Value::Null,
            example: json!(7),
            invariants: &[
                "present only when the redaction changed an issue materialization; positive",
                "the revision the affected issue advanced to",
            ],
            common_mistake: "Assuming every receipt carries one; redactions of non-issue records leave it absent.",
        },
        ("redaction_receipt", "publication_state") => FieldSemantics {
            ownership: "system",
            operations: &["redact"],
            has_default: false,
            default: Value::Null,
            example: json!("committed"),
            invariants: &[
                "enum: committed, published, discarded",
                "committed is the resumable state bead redact --resume targets",
                "discarded is recorded, never deleted, so the audit trail stays complete",
            ],
            common_mistake: "Retrying a discarded receipt or treating a published one as unapplied.",
        },
        ("redaction_receipt", "resulting_generation_id") => FieldSemantics {
            ownership: "system",
            operations: &["redact"],
            has_default: false,
            default: Value::Null,
            example: json!("gen-20260921T120000Z-a1b2c3d4"),
            invariants: &[
                "present exactly when the receipt is published; a published receipt without one is an integrity rejection",
                "a generation identity, deliberately not the root content hash: the hash of a checkpoint inside a record it contains would be a self-reference",
            ],
            common_mistake: "Expecting a content hash here.",
        },
        ("redaction_receipt", "epoch_id") => FieldSemantics {
            ownership: "system",
            operations: &["redact"],
            has_default: false,
            default: Value::Null,
            example: json!("0".repeat(64)),
            invariants: &[
                "64 lowercase hex characters when present",
                "set on every receipt this binary commits; optional in the schema so documents from producers predating the member still validate",
            ],
            common_mistake: "Opening a second epoch over the same redaction; the epoch is derived from the receipt set.",
        },
        // ---- redaction_epoch (publication epoch over a receipt set) --------
        ("redaction_epoch", "$schema") => FieldSemantics {
            ownership: "system",
            operations: &["redact"],
            has_default: false,
            default: Value::Null,
            example: json!(SCHEMA_REDACTION_EPOCH),
            invariants: &[
                "exact catalog identity; any other value is a usage rejection",
                "one epoch covers every receipt published together as one sanitized generation set",
            ],
            common_mistake: "Treating the epoch as per-receipt; it spans the published set.",
        },
        ("redaction_epoch", "epoch_id") => FieldSemantics {
            ownership: "system",
            operations: &["redact"],
            has_default: false,
            default: Value::Null,
            example: json!("0".repeat(64)),
            invariants: &[
                "64 lowercase hex characters derived from the sorted receipt set",
                "reopening publication for the same set finds the same epoch rather than opening a second one",
            ],
            common_mistake: "Minting a fresh epoch for a resumed publication.",
        },
        ("redaction_epoch", "receipt_ids") => FieldSemantics {
            ownership: "system",
            operations: &["redact"],
            has_default: false,
            default: Value::Null,
            example: json!(["0".repeat(64)]),
            invariants: &[
                "at least one receipt id, each 64 lowercase hex characters",
                "sorted and unique; an unsorted set is a usage rejection",
                "the epoch's identity is derived from exactly this set",
            ],
            common_mistake: "Reordering the set; validation rejects it and the identity would move.",
        },
        ("redaction_epoch", "publication_state") => FieldSemantics {
            ownership: "system",
            operations: &["redact"],
            has_default: false,
            default: Value::Null,
            example: json!("committed"),
            invariants: &[
                "enum: committed, published, discarded",
                "publication advances the epoch as a whole together with every receipt it carries",
            ],
            common_mistake: "Publishing part of an epoch; the set publishes as one generation.",
        },
        ("redaction_epoch", "resulting_generation_id") => FieldSemantics {
            ownership: "system",
            operations: &["redact"],
            has_default: false,
            default: Value::Null,
            example: json!("gen-20260921T120000Z-a1b2c3d4"),
            invariants: &[
                "the sanitized generation this epoch published, once known",
                "required for published epochs; a published epoch without one is an integrity rejection",
                "a generation identity, deliberately not the root content hash, for the same self-reference reason as the receipt",
            ],
            common_mistake: "Storing the root hash here; the epoch is part of the content the hash covers.",
        },
        ("redaction_epoch", "previous_generation_reset") => FieldSemantics {
            ownership: "system",
            operations: &["redact"],
            has_default: false,
            default: Value::Null,
            example: json!(false),
            invariants: &[
                "whether the dirty previous generation was deliberately reset instead of retained, the exceptional behavior ADR-015 allows",
                "epochs open with false; publication that does not retain the pre-redaction generation records true",
            ],
            common_mistake: "Reading true as data loss: the reset is the point of a redaction publication.",
        },
        ("redaction_epoch", "superseded_generations") => FieldSemantics {
            ownership: "system",
            operations: &["redact"],
            has_default: false,
            default: Value::Null,
            example: json!(["gen-20260920T120000Z-99887766"]),
            invariants: &[
                "generation identities superseded by this epoch, each nonempty and at most 128 bytes",
                "recorded so a later audit can tell an ordinary publication from a redaction publication without recovering anything secret",
            ],
            common_mistake: "Putting receipt ids here; these are generation identities.",
        },
        ("redaction_epoch", "opened_at") => FieldSemantics {
            ownership: "system",
            operations: &["redact"],
            has_default: false,
            default: Value::Null,
            example: json!("2026-09-21T12:00:00.000000000Z"),
            invariants: &[
                "RFC 3339 timestamp of the commit that opened the epoch",
                "the same commit that persisted the receipts and tombstone",
            ],
            common_mistake: "Reading it as the publication time; that is published_at.",
        },
        ("redaction_epoch", "published_at") => FieldSemantics {
            ownership: "system",
            operations: &["redact"],
            has_default: false,
            default: Value::Null,
            example: json!("2026-09-21T12:05:00.000000000Z"),
            invariants: &[
                "absent while committed; recording one is an integrity rejection",
                "required once published; a published epoch without one is an integrity rejection",
            ],
            common_mistake: "Backfilling a publication time onto a committed epoch.",
        },
        // ---- redaction_tombstone (anti-resurrection guard) ------------------
        ("redaction_tombstone", "$schema") => FieldSemantics {
            ownership: "system",
            operations: &["redact"],
            has_default: false,
            default: Value::Null,
            example: json!(SCHEMA_REDACTION_TOMBSTONE),
            invariants: &[
                "exact catalog identity; any other value is a usage rejection",
                "tombstones ride every checkpoint publication so recovery into a fresh workspace keeps the guard",
            ],
            common_mistake: "Dropping a tombstone on import: silently losing one is how redacted bytes come back.",
        },
        ("redaction_tombstone", "tombstone_id") => FieldSemantics {
            ownership: "system",
            operations: &["redact"],
            has_default: false,
            default: Value::Null,
            example: json!("0".repeat(64)),
            invariants: &[
                "64 lowercase hex characters derived from the guarding facts",
                "the same redaction recovered into a fresh workspace derives the same identity",
                "a tombstone cannot be re-keyed to guard a different location without changing its identity",
            ],
            common_mistake: "Re-keying a tombstone to a new location; the identity pins the guarded facts.",
        },
        ("redaction_tombstone", "record_kind") => FieldSemantics {
            ownership: "system",
            operations: &["redact"],
            has_default: false,
            default: Value::Null,
            example: json!("issues"),
            invariants: &[
                "lowercase letters, digits, and underscores; at most 32 bytes",
                "the kind of record the tombstone guards",
            ],
            common_mistake: "Assuming the kind is free-form.",
        },
        ("redaction_tombstone", "origin_identity") => FieldSemantics {
            ownership: "system",
            operations: &["redact"],
            has_default: false,
            default: Value::Null,
            example: json!("bead-18409c0e"),
            invariants: &[
                "nonempty, at most 512 bytes, no control characters",
                "keyed by origin record identity rather than local row IDs, so the guard keeps working across a restore that reassigns local sequences",
            ],
            common_mistake: "Guarding by local row id; a restore reassigns those.",
        },
        ("redaction_tombstone", "field_path") => FieldSemantics {
            ownership: "system",
            operations: &["redact"],
            has_default: false,
            default: Value::Null,
            example: json!("description"),
            invariants: &[
                "dot-separated lowercase identifier or index segments, whole path at most 256 bytes",
                "the field the redaction touched",
            ],
            common_mistake: "Broadening the guard to the whole record; the path scopes it.",
        },
        ("redaction_tombstone", "prior_record_hash") => FieldSemantics {
            ownership: "system",
            operations: &["redact"],
            has_default: false,
            default: Value::Null,
            example: json!("0".repeat(64)),
            invariants: &[
                "64 lowercase hex characters",
                "incoming content hashing to this value is pre-redaction content and is refused; the sanitized record hashes differently and passes",
            ],
            common_mistake: "Treating a hash match as corruption: it is exactly the resurrection the tombstone exists to refuse.",
        },
        ("redaction_tombstone", "finding_fingerprint") => FieldSemantics {
            ownership: "system",
            operations: &["redact"],
            has_default: false,
            default: Value::Null,
            example: json!("0".repeat(64)),
            invariants: &["64 lowercase hex characters", "the finding that drove the redaction"],
            common_mistake: "Expecting the secret; the guard carries handles, never content.",
        },
        ("redaction_tombstone", "epoch_id") => FieldSemantics {
            ownership: "system",
            operations: &["redact"],
            has_default: false,
            default: Value::Null,
            example: json!("0".repeat(64)),
            invariants: &[
                "64 lowercase hex characters",
                "the epoch whose publication made the tombstone durable",
            ],
            common_mistake: "Attributing the tombstone to the wrong epoch when auditing a generation.",
        },
        ("redaction_tombstone", "created_at") => FieldSemantics {
            ownership: "system",
            operations: &["redact"],
            has_default: false,
            default: Value::Null,
            example: json!("2026-09-21T12:00:00.000000000Z"),
            invariants: &["RFC 3339 timestamp; the same commit instant as the receipt's redacted_at"],
            common_mistake: "Reading it as the publication instant.",
        },
        // ---- attempt_outcome (durable resolve record) -----------------------
        ("attempt_outcome", "$schema") => FieldSemantics {
            ownership: "system",
            operations: &["resolve"],
            has_default: false,
            default: Value::Null,
            example: json!("urn:bead-rs:schema:attempt-outcome:native-v1"),
            invariants: &[
                "exact catalog identity; any other value is a usage rejection",
                "stamped on outcome records packed into checkpoints",
            ],
            common_mistake: "Accepting an outcome record whose schema identity is absent or renamed.",
        },
        ("attempt_outcome", "attempt_id") => FieldSemantics {
            ownership: "caller",
            operations: &["resolve"],
            has_default: false,
            default: Value::Null,
            example: json!("urn:needle:attempt:abc123"),
            invariants: &[
                "required nonempty, at most 255 bytes",
                "the caller's idempotency key: an identical retry returns the original receipt, a divergent reuse conflicts (exit 4)",
            ],
            common_mistake: "Reusing an attempt id with a different request; that is a conflict, not an update.",
        },
        ("attempt_outcome", "issue_id") => FieldSemantics {
            ownership: "caller",
            operations: &["resolve"],
            has_default: false,
            default: Value::Null,
            example: json!("bead-18409c0e"),
            invariants: &[
                "required nonempty, at most 255 bytes",
                "must name an existing issue; a missing issue is exit 3",
            ],
            common_mistake: "Resolving an issue id that was never created.",
        },
        ("attempt_outcome", "outcome") => FieldSemantics {
            ownership: "caller",
            operations: &["resolve"],
            has_default: false,
            default: Value::Null,
            example: json!("verified_success"),
            invariants: &[
                "enum: verified_success, work_failure, infrastructure_failure, cancelled, indeterminate",
                "work_failure is the only classification that advances the attempt tier",
                "the read-time failure run resets on verified_success; infrastructure, cancellation, and indeterminate neither increment nor reset it",
            ],
            common_mistake: "Classifying a repeatable failure as infrastructure_failure to avoid tier progression.",
        },
        ("attempt_outcome", "action") => FieldSemantics {
            ownership: "caller",
            operations: &["resolve"],
            has_default: true,
            default: json!("none"),
            example: json!("close"),
            invariants: &[
                "enum: close, release, quarantine, block, none",
                "an omitted action resolves as none and is stored as none",
                "outcome-action combinations are validated; verified_success with quarantine, for example, is a usage error",
                "close requires a non-empty reason",
            ],
            common_mistake: "Assuming any outcome pairs with any action.",
        },
        ("attempt_outcome", "reason") => FieldSemantics {
            ownership: "caller",
            operations: &["resolve"],
            has_default: false,
            default: Value::Null,
            example: json!("All tests passed"),
            invariants: &[
                "absent when the request omitted it; stored as empty text in the durable row",
                "required and non-empty when the action is close",
                "the published schema bounds it at 4 MiB",
            ],
            common_mistake: "Omitting the reason on a close action; that is a usage error before any mutation.",
        },
        ("attempt_outcome", "canonical_request_hash") => FieldSemantics {
            ownership: "system",
            operations: &["resolve"],
            has_default: false,
            default: Value::Null,
            example: json!("0".repeat(64)),
            invariants: &[
                "64 lowercase hex characters",
                "SHA-256 over the canonical request: attempt id, issue id, outcome, action, reason, if-revision, fencing token, override reason, sorted evidence references, and the optional model/harness metadata",
                "the replay key: an equal hash replays the original receipt, a different hash conflicts",
            ],
            common_mistake: "Choosing the hash by hand; it is computed from the request.",
        },
        ("attempt_outcome", "resulting_issue_revision") => FieldSemantics {
            ownership: "system",
            operations: &["resolve"],
            has_default: false,
            default: Value::Null,
            example: json!(5),
            invariants: &[
                "the issue revision after the applied action: the pre-resolution revision plus one",
                "replays report the originally recorded revision",
            ],
            common_mistake: "Reading it as the revision the request saw.",
        },
        ("attempt_outcome", "resulting_state") => FieldSemantics {
            ownership: "system",
            operations: &["resolve"],
            has_default: false,
            default: Value::Null,
            example: json!("closed"),
            invariants: &[
                "the issue base status after the action: none, quarantine, and block retain it; close yields closed; release yields open",
                "release is legal only from in_progress (otherwise exit 4); closing already-closed work is a usage error",
            ],
            common_mistake: "Assuming quarantine or block changes base status; they set tier and the manual overlay.",
        },
        ("attempt_outcome", "resulting_attempt_tier") => FieldSemantics {
            ownership: "system",
            operations: &["resolve"],
            has_default: false,
            default: Value::Null,
            example: json!(0),
            invariants: &[
                "integer 0 through 3",
                "work_failure advances the stored failure count: 1 retryable, 2 struggling, 3 quarantined",
                "every other outcome leaves tier and failure count as they stood",
            ],
            common_mistake: "Expecting a success to lower the stored tier; only failures move it.",
        },
        ("attempt_outcome", "receipt_id") => FieldSemantics {
            ownership: "system",
            operations: &["resolve"],
            has_default: false,
            default: Value::Null,
            example: json!("ao-4f7a2b1c0d9e8f7a2b1c0d9e8f7a2b1c"),
            invariants: &[
                "ao- prefixed random hex identifier minted once at resolution",
                "replays return the original receipt id",
            ],
            common_mistake: "Manufacturing a receipt id; the resolver mints one.",
        },
        ("attempt_outcome", "actor") => FieldSemantics {
            ownership: "caller",
            operations: &["resolve"],
            has_default: true,
            default: json!("unknown"),
            example: json!("needle-worker-alpha"),
            invariants: &[
                "carries the request actor; the CLI supplies unknown when --actor is omitted",
                "nonempty, at most 255 bytes",
            ],
            common_mistake: "Leaving the default in audit-sensitive resolutions.",
        },
        ("attempt_outcome", "created_at") => FieldSemantics {
            ownership: "system",
            operations: &["resolve"],
            has_default: false,
            default: Value::Null,
            example: json!("2026-09-21T12:00:00.000000000Z"),
            invariants: &[
                "RFC 3339 UTC instant of the resolution",
                "replays report the original instant",
            ],
            common_mistake: "Treating it as the replay time.",
        },
        ("attempt_outcome", "evidence_refs") => FieldSemantics {
            ownership: "caller",
            operations: &["resolve"],
            has_default: false,
            default: Value::Null,
            example: json!(["s3:logs/a1b2c3d4.tar.gz"]),
            invariants: &[
                "each entry is NAMESPACE:VALUE: namespace 1-32 characters matching [a-z][a-z0-9-]*, value 1-255 bytes without control characters",
                "sorted before hashing so ordering cannot change the canonical hash",
                "serialized absent when empty",
            ],
            common_mistake: "Putting secrets in evidence references; they land in checkpoints.",
        },
        ("attempt_outcome", "model") => FieldSemantics {
            ownership: "caller",
            operations: &["resolve"],
            has_default: false,
            default: Value::Null,
            example: json!("claude-opus-5"),
            invariants: &[
                "optional telemetry string; absent when omitted",
                "an input to the canonical hash only when present",
            ],
            common_mistake: "Assuming it affects resolution semantics; it is provenance.",
        },
        ("attempt_outcome", "harness") => FieldSemantics {
            ownership: "caller",
            operations: &["resolve"],
            has_default: false,
            default: Value::Null,
            example: json!("needle"),
            invariants: &[
                "optional telemetry string; absent when omitted",
                "an input to the canonical hash only when present",
            ],
            common_mistake: "Assuming it affects resolution semantics; it is provenance.",
        },
        ("attempt_outcome", "harness_version") => FieldSemantics {
            ownership: "caller",
            operations: &["resolve"],
            has_default: false,
            default: Value::Null,
            example: json!("1.2.3"),
            invariants: &[
                "optional telemetry string; absent when omitted",
                "an input to the canonical hash only when present",
            ],
            common_mistake: "Assuming it affects resolution semantics; it is provenance.",
        },
        // ---- resolve_receipt (printed resolve result) -----------------------
        ("resolve_receipt", "receipt_id") => FieldSemantics {
            ownership: "system",
            operations: &["resolve"],
            has_default: false,
            default: Value::Null,
            example: json!("ao-4f7a2b1c0d9e8f7a2b1c0d9e8f7a2b1c"),
            invariants: &[
                "minted once at resolution; an identical retry returns the original",
                "the durable record is the attempt_outcome row; the printed receipt is not separately persisted",
            ],
            common_mistake: "Expecting the receipt to exist as its own stored document.",
        },
        ("resolve_receipt", "canonical_request_hash") => FieldSemantics {
            ownership: "system",
            operations: &["resolve"],
            has_default: false,
            default: Value::Null,
            example: json!("0".repeat(64)),
            invariants: &[
                "64 lowercase hex characters",
                "the request's canonical hash; equality with the stored hash is what proves a replay",
            ],
            common_mistake: "Recomputing it with a different field order and expecting a replay match.",
        },
        ("resolve_receipt", "issue_id") => FieldSemantics {
            ownership: "system",
            operations: &["resolve"],
            has_default: false,
            default: Value::Null,
            example: json!("bead-18409c0e"),
            invariants: &["echoed from the request"],
            common_mistake: "Treating it as independently resolved; the request named the issue.",
        },
        ("resolve_receipt", "attempt_id") => FieldSemantics {
            ownership: "system",
            operations: &["resolve"],
            has_default: false,
            default: Value::Null,
            example: json!("urn:needle:attempt:abc123"),
            invariants: &["echoed from the request", "the durable attempt_outcome row is keyed by it"],
            common_mistake: "Assuming the receipt mints a new attempt identity.",
        },
        ("resolve_receipt", "resulting_issue_revision") => FieldSemantics {
            ownership: "system",
            operations: &["resolve"],
            has_default: false,
            default: Value::Null,
            example: json!(5),
            invariants: &[
                "the issue revision after the applied action",
                "a replay reports the originally recorded revision",
            ],
            common_mistake: "Reading it as live issue state; it is the recorded result.",
        },
        ("resolve_receipt", "resulting_state") => FieldSemantics {
            ownership: "system",
            operations: &["resolve"],
            has_default: false,
            default: Value::Null,
            example: json!("closed"),
            invariants: &[
                "the issue base status after the applied action",
                "replay receipts of older rows reconstruct it from the recorded action",
            ],
            common_mistake: "Expecting the current status; query the issue for that.",
        },
        ("resolve_receipt", "resulting_attempt_tier") => FieldSemantics {
            ownership: "system",
            operations: &["resolve"],
            has_default: false,
            default: Value::Null,
            example: json!(0),
            invariants: &[
                "integer 0 through 3, as recorded for the resolution",
                "replays report the original tier",
            ],
            common_mistake: "Reading it as the issue's live tier.",
        },
        ("resolve_receipt", "created_at") => FieldSemantics {
            ownership: "system",
            operations: &["resolve"],
            has_default: false,
            default: Value::Null,
            example: json!("2026-09-21T12:00:00.000000000Z"),
            invariants: &["RFC 3339 instant of the resolution; the original instant on replay"],
            common_mistake: "Treating a replay's timestamp as fresh.",
        },
        ("resolve_receipt", "is_replay") => FieldSemantics {
            ownership: "system",
            operations: &["resolve"],
            has_default: false,
            default: Value::Null,
            example: json!(false),
            invariants: &[
                "true when the resolution is an identical retry of an already-committed attempt",
                "a replay performs no mutation and returns the original values",
            ],
            common_mistake: "Retrying with changed fields and expecting a replay; a divergent reuse conflicts (exit 4).",
        },
        // ---- resolve_request (resolve input) --------------------------------
        ("resolve_request", "attempt_id") => FieldSemantics {
            ownership: "caller",
            operations: &["resolve"],
            has_default: false,
            default: Value::Null,
            example: json!("urn:needle:attempt:abc123"),
            invariants: &[
                "required nonempty, at most 255 bytes",
                "the idempotency key the receipt and durable row are found by",
            ],
            common_mistake: "Generating a fresh attempt id per retry; each id resolves at most once.",
        },
        ("resolve_request", "issue_id") => FieldSemantics {
            ownership: "caller",
            operations: &["resolve"],
            has_default: false,
            default: Value::Null,
            example: json!("bead-18409c0e"),
            invariants: &["required; a missing issue is exit 3"],
            common_mistake: "Resolving an issue in another workspace.",
        },
        ("resolve_request", "outcome") => FieldSemantics {
            ownership: "caller",
            operations: &["resolve"],
            has_default: false,
            default: Value::Null,
            example: json!("verified_success"),
            invariants: &[
                "enum: verified_success, work_failure, infrastructure_failure, cancelled, indeterminate",
                "validated before any mutation; an unknown outcome is a usage error",
            ],
            common_mistake: "Sending a free-form outcome string.",
        },
        ("resolve_request", "action") => FieldSemantics {
            ownership: "caller",
            operations: &["resolve"],
            has_default: true,
            default: json!("none"),
            example: json!("close"),
            invariants: &[
                "enum: close, release, quarantine, block, none",
                "an omitted action resolves as none",
                "close requires a non-empty reason",
                "combination-validated against the outcome before any mutation",
            ],
            common_mistake: "Pairing an action the outcome forbids.",
        },
        ("resolve_request", "reason") => FieldSemantics {
            ownership: "caller",
            operations: &["resolve"],
            has_default: false,
            default: Value::Null,
            example: json!("All tests passed"),
            invariants: &[
                "optional; required and non-empty when the action is close",
                "a whitespace-only reason does not satisfy the close requirement",
                "part of the canonical request hash",
            ],
            common_mistake: "Omitting the reason on a close action.",
        },
        ("resolve_request", "if_revision") => FieldSemantics {
            ownership: "caller",
            operations: &["resolve"],
            has_default: false,
            default: Value::Null,
            example: json!(4),
            invariants: &[
                "optimistic guard: conflicts (exit 4) when the issue is not at this revision",
                "absent means no guard; the canonical hash records it as 0 in that case",
            ],
            common_mistake: "Treating absence as revision zero rather than as no guard.",
        },
        ("resolve_request", "fencing_token") => FieldSemantics {
            ownership: "caller",
            operations: &["resolve"],
            has_default: false,
            default: Value::Null,
            example: json!("3"),
            invariants: &[
                "while the issue is claimed, the claim-epoch credential bead claim minted; a missing or stale value conflicts (exit 4)",
                "parsed as an integer; a non-integer is a usage error",
                "close and release re-validate it through the shared lifecycle rules",
            ],
            common_mistake: "Omitting it on claimed work or treating it as a revision.",
        },
        ("resolve_request", "evidence_refs") => FieldSemantics {
            ownership: "caller",
            operations: &["resolve"],
            has_default: false,
            default: Value::Null,
            example: json!(["s3:logs/a1b2c3d4.tar.gz"]),
            invariants: &[
                "each entry is NAMESPACE:VALUE with a [a-z][a-z0-9-]* namespace of 1-32 characters and a 1-255 byte value without control characters",
                "validated before any mutation",
            ],
            common_mistake: "Smuggling secrets into evidence references; they land in checkpoints.",
        },
        ("resolve_request", "actor") => FieldSemantics {
            ownership: "caller",
            operations: &["resolve"],
            has_default: true,
            default: json!("unknown"),
            example: json!("needle-worker-alpha"),
            invariants: &[
                "required nonempty, at most 255 bytes",
                "the CLI supplies unknown when --actor is omitted",
            ],
            common_mistake: "Leaving the default in audit-sensitive resolutions.",
        },
        ("resolve_request", "model") => FieldSemantics {
            ownership: "caller",
            operations: &["resolve"],
            has_default: false,
            default: Value::Null,
            example: json!("claude-opus-5"),
            invariants: &[
                "optional telemetry string",
                "an input to the canonical hash only when present",
            ],
            common_mistake: "Changing it between retries and expecting the retry to replay.",
        },
        ("resolve_request", "harness") => FieldSemantics {
            ownership: "caller",
            operations: &["resolve"],
            has_default: false,
            default: Value::Null,
            example: json!("needle"),
            invariants: &[
                "optional telemetry string",
                "an input to the canonical hash only when present",
            ],
            common_mistake: "Changing it between retries and expecting the retry to replay.",
        },
        ("resolve_request", "harness_version") => FieldSemantics {
            ownership: "caller",
            operations: &["resolve"],
            has_default: false,
            default: Value::Null,
            example: json!("1.2.3"),
            invariants: &[
                "optional telemetry string",
                "an input to the canonical hash only when present",
            ],
            common_mistake: "Changing it between retries and expecting the retry to replay.",
        },
        // ---- capabilities (machine-readable feature advertisement) ---------
        ("capabilities", "contract") => FieldSemantics {
            ownership: "system",
            operations: &["capabilities"],
            has_default: false,
            default: Value::Null,
            example: json!("native-v1"),
            invariants: &[
                "profile-selected contract name: native-v1 for the native profile, needle-v1 for the needle profile",
                "any other profile is a validation error",
            ],
            common_mistake: "Parsing it as a workspace property; it names the implementation contract.",
        },
        ("capabilities", "implementation") => FieldSemantics {
            ownership: "system",
            operations: &["capabilities"],
            has_default: false,
            default: Value::Null,
            example: json!("bead-rs"),
            invariants: &["always bead-rs in this implementation"],
            common_mistake: "Branching on the implementation name instead of the contract.",
        },
        ("capabilities", "version") => FieldSemantics {
            ownership: "system",
            operations: &["capabilities"],
            has_default: false,
            default: Value::Null,
            example: json!("0.2.6"),
            invariants: &["the compiled package version"],
            common_mistake: "Using it as a schema version; document identities carry their own revisions.",
        },
        ("capabilities", "store_layout") => FieldSemantics {
            ownership: "system",
            operations: &["capabilities"],
            has_default: false,
            default: Value::Null,
            example: json!(1),
            invariants: &["always 1 in this implementation"],
            common_mistake: "Assuming layout numbers match across implementations.",
        },
        ("capabilities", "atomic_claim") => FieldSemantics {
            ownership: "system",
            operations: &["capabilities"],
            has_default: false,
            default: Value::Null,
            example: json!(true),
            invariants: &["always true: claims are atomic selection, assignment, and transition"],
            common_mistake: "Building around a non-atomic claim that this contract does not have.",
        },
        ("capabilities", "priorities") => FieldSemantics {
            ownership: "system",
            operations: &["capabilities"],
            has_default: false,
            default: Value::Null,
            example: json!({"min":0,"max":4,"default":2,"p4_claimable_by_fifo":true}),
            invariants: &[
                "min 0, max 4, default 2, and p4_claimable_by_fifo true in this implementation",
            ],
            common_mistake: "Reading P4 as unclaimable under fifo-v1; the advertisement says otherwise.",
        },
        ("capabilities", "statuses") => FieldSemantics {
            ownership: "system",
            operations: &["capabilities"],
            has_default: false,
            default: Value::Null,
            example: json!(["closed","deferred","in_progress","open"]),
            invariants: &[
                "the stored base-status vocabulary, sorted",
                "blocked and ready are derived views, never advertised statuses",
            ],
            common_mistake: "Expecting blocked or ready here.",
        },
        ("capabilities", "dependency_kinds") => FieldSemantics {
            ownership: "system",
            operations: &["capabilities"],
            has_default: false,
            default: Value::Null,
            example: json!(["blocks","relates_to","verifies"]),
            invariants: &[
                "the vocabulary `dep add --kind` accepts, sorted",
                "blocks gates readiness; relates_to and verifies never do",
                "interchange preserves foreign kinds outside this set",
            ],
            common_mistake:
                "Treating a verifies edge as a readiness gate; only blocks gates eligibility.",
        },
        ("capabilities", "checkpoint_modes") => FieldSemantics {
            ownership: "system",
            operations: &["capabilities"],
            has_default: false,
            default: Value::Null,
            example: json!(["monolithic","sharded"]),
            invariants: &[
                "monolithic and sharded",
                "the publisher selects per recorded thresholds unless the workspace forces a mode",
            ],
            common_mistake: "Forcing a monolith past the recorded safety limits; that is refused.",
        },
        ("capabilities", "checkpoint_formats") => FieldSemantics {
            ownership: "system",
            operations: &["capabilities"],
            has_default: false,
            default: Value::Null,
            example: json!(["issues-jsonl-v1","checkpoint-set-v1"]),
            invariants: &["issues-jsonl-v1 and checkpoint-set-v1"],
            common_mistake: "Reading the legacy single-file format as the durable fileset identity.",
        },
        ("capabilities", "logical_revision") => FieldSemantics {
            ownership: "system",
            operations: &["capabilities"],
            has_default: false,
            default: Value::Null,
            example: json!(true),
            invariants: &["always true: revision guards are supported on update, release, close, and reopen"],
            common_mistake: "Implementing timestamp-based guards against a revision-guarded store.",
        },
        ("capabilities", "schema_ref") => FieldSemantics {
            ownership: "system",
            operations: &["capabilities"],
            has_default: false,
            default: Value::Null,
            example: json!("urn:bead-rs:schema:capabilities:native-v1"),
            invariants: &["the capabilities document's own catalog identity"],
            common_mistake: "Confusing it with the schema catalog entries below.",
        },
        ("capabilities", "schemas") => FieldSemantics {
            ownership: "system",
            operations: &["capabilities"],
            has_default: false,
            default: Value::Null,
            example: json!([{"schema_ref":"urn:bead-rs:schema:issue:native-v1","document_kind":"issue"}]),
            invariants: &[
                "exactly the catalog bead schema list returns, sorted by schema identity",
                "shared state with the schema registry, so the two discovery surfaces cannot drift",
            ],
            common_mistake: "Parsing it as live workspace state; it is the compiled registry.",
        },
        ("capabilities", "commands") => FieldSemantics {
            ownership: "system",
            operations: &["capabilities"],
            has_default: false,
            default: Value::Null,
            example: json!(["claim","close","create"]),
            invariants: &["every public root command, in alphabetical order"],
            common_mistake: "Assuming subcommand names appear here; the members are root commands.",
        },
        ("capabilities", "auto_flush") => FieldSemantics {
            ownership: "system",
            operations: &["capabilities"],
            has_default: false,
            default: Value::Null,
            example: json!(true),
            invariants: &[
                "optional boolean reporting the compiled default, true since the R026 activation",
                "the workspace checkpoint.auto_flush key and --no-auto-flush change behavior, never this advertisement",
                "absent entirely from a producer whose compiled default is off",
            ],
            common_mistake: "Reading it as workspace cleanliness; sync --status is the only authority on that.",
        },
        ("capabilities", "auto_stage") => FieldSemantics {
            ownership: "system",
            operations: &["capabilities"],
            has_default: false,
            default: Value::Null,
            example: json!(true),
            invariants: &[
                "optional boolean reporting the compiled default, true since the ADR-018 feature landed",
                "the workspace checkpoint.auto_stage key changes behavior, never this advertisement",
                "absent entirely when the compiled default is off",
            ],
            common_mistake: "Reading it as a promise that every workspace is a Git repository; staging outside one is a no-op.",
        },
        ("capabilities", "attempt_outcome") => FieldSemantics {
            ownership: "system",
            operations: &["capabilities"],
            has_default: false,
            default: Value::Null,
            example: json!({"supported":true,"outcomes":["verified_success","work_failure","infrastructure_failure","cancelled","indeterminate"],"actions":["close","release","quarantine","block","none"],"replay_detection":true,"revision_guard":true,"fencing_token":true,"evidence_refs":true,"resolve_receipt_schema":"urn:bead-rs:schema:resolve-receipt:native-v1","resolve_request_schema":"urn:bead-rs:schema:resolve-request:native-v1"}),
            invariants: &[
                "optional object: supported flag, the five outcome classifications, the five lifecycle actions, replay-detection/revision-guard/fencing-token/evidence-refs support, and the resolve receipt and request schema identities",
                "a producer without the ADR-012 capability omits the member entirely, never publishes supported false; this binary always emits it with supported true",
                "the contract's own detection recipe reads .attempt_outcome.supported (bead capabilities piped through jq)",
            ],
            common_mistake: "Reading absence as supported false; pre-ADR-012 producers leave the member out of the document entirely.",
        },
        ("capabilities", "attempt_summary") => FieldSemantics {
            ownership: "system",
            operations: &["capabilities"],
            has_default: false,
            default: Value::Null,
            example: json!(true),
            invariants: &["always true: show and list JSON derive an attempt summary from durable outcome records"],
            common_mistake: "Writing attempt summaries by hand; they are read-time projections.",
        },
        ("capabilities", "secret_scan") => FieldSemantics {
            ownership: "system",
            operations: &["capabilities"],
            has_default: false,
            default: Value::Null,
            example: json!({"contract_identity":"urn:bead-rs:spec:secret-scan:v1","ruleset_version":1,"effective_mode":"enforce","blocking":true,"advisory":true,"exact_fingerprint_acknowledgment":true}),
            invariants: &[
                "optional object: contract identity, ruleset version, effective mode, blocking and advisory support, and exact-fingerprint acknowledgment",
                "effective_mode reports the workspace-discovered policy or the compiled enforce default",
                "absent from producers without the ADR-014 capability",
            ],
            common_mistake: "Reading effective_mode as per-invocation state; it is the discovered policy.",
        },
        ("capabilities", "historical_redaction") => FieldSemantics {
            ownership: "system",
            operations: &["capabilities"],
            has_default: false,
            default: Value::Null,
            example: json!({"contract":"urn:bead-rs:spec:historical-redaction:v1","doctor_findings":true,"atomic_redact":true,"anti_resurrection":true,"sanitized_generation_set":true,"resumable_publication":true}),
            invariants: &[
                "optional object advertising the ADR-015 handshake: doctor findings, atomic redact, anti-resurrection tombstones, the sanitized generation set, and resumable publication",
                "absent from producers without the capability",
            ],
            common_mistake: "Assuming redaction support without checking the handshake.",
        },
        // ---- checkpoint_manifest (sharded generation root) ------------------
        ("checkpoint_manifest", "format") => FieldSemantics {
            ownership: "system",
            operations: &["sync.flush-only"],
            has_default: false,
            default: Value::Null,
            example: json!("checkpoint-set-v1"),
            invariants: &[
                "always checkpoint-set-v1: the durable checkpoint fileset identity this manifest roots",
                "the manifest self-identifies by this member because checkpoint documents stamp no in-document schema URN",
                "the manifest file name is the SHA-256 of its own bytes: the content-addressed root",
            ],
            common_mistake: "Requiring a $schema member on a manifest; resolution goes through the checkpoint set.",
        },
        ("checkpoint_manifest", "schema_version") => FieldSemantics {
            ownership: "system",
            operations: &["sync.flush-only"],
            has_default: false,
            default: Value::Null,
            example: json!(1),
            invariants: &["always 1 in this format version"],
            common_mistake: "Reading a manifest with a different schema_version with this build's assumptions.",
        },
        ("checkpoint_manifest", "store_uuid") => FieldSemantics {
            ownership: "system",
            operations: &["sync.flush-only"],
            has_default: false,
            default: Value::Null,
            example: json!("5f0c3a2e-1b4d-4e8f-9a2b-3c5d6e7f8091"),
            invariants: &["the publishing workspace's store UUID"],
            common_mistake: "Merging manifests across stores without reconciliation; origins and sequences are per-store.",
        },
        ("checkpoint_manifest", "snapshot_sequence") => FieldSemantics {
            ownership: "system",
            operations: &["sync.flush-only"],
            has_default: false,
            default: Value::Null,
            example: json!(42),
            invariants: &["the local event sequence the publication covers"],
            common_mistake: "Reading it as a timestamp or a record count.",
        },
        ("checkpoint_manifest", "max_local_ingestion_sequence") => FieldSemantics {
            ownership: "system",
            operations: &["sync.flush-only"],
            has_default: false,
            default: Value::Null,
            example: json!(42),
            invariants: &[
                "the highest local ingestion sequence the manifest covers",
                "equals snapshot_sequence in documents this binary writes: the publication covers events through that sequence",
            ],
            common_mistake: "Expecting it to run ahead of snapshot_sequence in this build's documents.",
        },
        ("checkpoint_manifest", "created_at") => FieldSemantics {
            ownership: "system",
            operations: &["sync.flush-only"],
            has_default: false,
            default: Value::Null,
            example: json!("2026-09-21T12:00:00.000000000Z"),
            invariants: &[
                "RFC 3339 instant of the publication",
                "part of the manifest's bytes, so the same state republished lands under a new content-addressed name",
            ],
            common_mistake: "Comparing manifests by created_at alone; compare roots and sequences.",
        },
        ("checkpoint_manifest", "profile") => FieldSemantics {
            ownership: "system",
            operations: &["sync.flush-only"],
            has_default: false,
            default: Value::Null,
            example: json!("native-v1"),
            invariants: &["always native-v1 for documents this binary writes"],
            common_mistake: "Relabeling a foreign manifest as native.",
        },
        ("checkpoint_manifest", "partition_algorithm") => FieldSemantics {
            ownership: "system",
            operations: &["sync.flush-only"],
            has_default: false,
            default: Value::Null,
            example: json!("sha256-hex-prefix"),
            invariants: &[
                "sha256-hex-prefix: issue records are bucketed by hex prefixes of their shard key",
                "a build that does not recognize the recorded algorithm must not guess at the packing",
            ],
            common_mistake: "Re-deriving shards with a different algorithm and expecting the same roots.",
        },
        ("checkpoint_manifest", "partition_thresholds") => FieldSemantics {
            ownership: "system",
            operations: &["sync.flush-only"],
            has_default: false,
            default: Value::Null,
            example: json!({"version":1,"max_monolith_issue_records":10000}),
            invariants: &[
                "the recorded packing limits: monolith record/byte safety caps, shard sizes, and event-object bounds",
                "version-tagged; a build that does not understand the recorded version rejects the manifest rather than guessing",
                "any nonpositive limit invalidates the record",
            ],
            common_mistake: "Editing thresholds in a published manifest; the root hash would no longer verify.",
        },
        ("checkpoint_manifest", "issue_partition") => FieldSemantics {
            ownership: "system",
            operations: &["sync.flush-only"],
            has_default: false,
            default: Value::Null,
            example: json!(["a","b","c","d","e","f","0","1","2","3","4","5","6","7","8","9"]),
            invariants: &[
                "the prefix plan actually used: lowercase hex prefixes, pairwise disjoint, jointly covering the whole key space",
                "carried forward from the previous manifest when structurally valid; otherwise rebuilt from the shallow default",
                "correctness never depends on the plan, only write amplification does",
            ],
            common_mistake: "Assuming a fixed single prefix; the plan splits as shards overflow.",
        },
        ("checkpoint_manifest", "issue_count") => FieldSemantics {
            ownership: "system",
            operations: &["sync.flush-only"],
            has_default: false,
            default: Value::Null,
            example: json!(120),
            invariants: &[
                "issue records packed into this generation",
                "additive member: absent on manifests from producers predating it",
            ],
            common_mistake: "Reading it as live store totals; it describes the published generation.",
        },
        ("checkpoint_manifest", "event_count") => FieldSemantics {
            ownership: "system",
            operations: &["sync.flush-only"],
            has_default: false,
            default: Value::Null,
            example: json!(340),
            invariants: &[
                "event records packed into this generation",
                "additive member: absent on manifests from producers predating it",
            ],
            common_mistake: "Reading it as live store totals.",
        },
        ("checkpoint_manifest", "receipt_count") => FieldSemantics {
            ownership: "system",
            operations: &["sync.flush-only"],
            has_default: false,
            default: Value::Null,
            example: json!(2),
            invariants: &[
                "provenance receipts packed into this generation",
                "additive member: absent on manifests from producers predating it",
            ],
            common_mistake: "Reading it as live store totals.",
        },
        ("checkpoint_manifest", "attempt_outcome_count") => FieldSemantics {
            ownership: "system",
            operations: &["sync.flush-only"],
            has_default: false,
            default: Value::Null,
            example: json!(7),
            invariants: &[
                "attempt-outcome records packed into this generation",
                "additive member: absent on manifests from producers predating it",
            ],
            common_mistake: "Expecting it on a legacy manifest.",
        },
        ("checkpoint_manifest", "redaction_record_count") => FieldSemantics {
            ownership: "system",
            operations: &["sync.flush-only"],
            has_default: false,
            default: Value::Null,
            example: json!(4),
            invariants: &[
                "redaction records (findings, acknowledgments, receipts, epochs, tombstones) packed into this generation",
                "additive member: absent on manifests from producers predating it",
            ],
            common_mistake: "Expecting zero to mean no redaction support; read the generation's epoch records for that.",
        },
        ("checkpoint_manifest", "total_record_count") => FieldSemantics {
            ownership: "system",
            operations: &["sync.flush-only"],
            has_default: false,
            default: Value::Null,
            example: json!(513),
            invariants: &["the sum of the five record counts"],
            common_mistake: "Validating shard record_counts against it while ignoring the sealed-object grouping.",
        },
        ("checkpoint_manifest", "issue_shards") => FieldSemantics {
            ownership: "system",
            operations: &["sync.flush-only"],
            has_default: false,
            default: Value::Null,
            example: json!([{"path":"objects/issue-gen-1-0","sha256":"0".repeat(64),"byte_length":4096,"record_count":120,"role":"issues"}]),
            invariants: &[
                "per-object metadata: generation-relative path, content sha256, byte length, record count, and role",
                "objects are content-addressed and sealed in canonical order, so re-packing an append-only corpus reproduces earlier objects byte-for-byte",
            ],
            common_mistake: "Fetching shard objects by record id; address by path and verify by hash.",
        },
        ("checkpoint_manifest", "event_shards") => FieldSemantics {
            ownership: "system",
            operations: &["sync.flush-only"],
            has_default: false,
            default: Value::Null,
            example: json!([{"path":"objects/event-gen-1-0","sha256":"0".repeat(64),"byte_length":8192,"record_count":340,"role":"events"}]),
            invariants: &[
                "per-object metadata for packed event objects, sealed at the recorded event-object targets",
                "per-origin coverage is summarized separately under origins",
            ],
            common_mistake: "Opening every event object to learn one origin's range; query origins first.",
        },
        ("checkpoint_manifest", "receipt_shards") => FieldSemantics {
            ownership: "system",
            operations: &["sync.flush-only"],
            has_default: false,
            default: Value::Null,
            example: json!([{"path":"objects/receipt-gen-1-0","sha256":"0".repeat(64),"byte_length":1024,"record_count":2,"role":"provenance_receipts"}]),
            invariants: &["per-object metadata for the packed provenance receipts"],
            common_mistake: "Assuming receipts share objects with issues; each role packs its own objects.",
        },
        ("checkpoint_manifest", "attempt_outcome_shards") => FieldSemantics {
            ownership: "system",
            operations: &["sync.flush-only"],
            has_default: false,
            default: Value::Null,
            example: json!([{"path":"objects/attempt-outcome-gen-1-0","sha256":"0".repeat(64),"byte_length":2048,"record_count":7,"role":"attempt_outcomes"}]),
            invariants: &[
                "per-object metadata for the packed attempt-outcome records",
                "additive member: absent on manifests from producers predating it",
            ],
            common_mistake: "Expecting it on a legacy manifest.",
        },
        ("checkpoint_manifest", "redaction_shards") => FieldSemantics {
            ownership: "system",
            operations: &["sync.flush-only"],
            has_default: false,
            default: Value::Null,
            example: json!([{"path":"objects/redaction-gen-1-0","sha256":"0".repeat(64),"byte_length":2048,"record_count":4,"role":"redaction_records"}]),
            invariants: &[
                "per-object metadata for the packed redaction records",
                "additive member: absent on manifests from producers predating it",
            ],
            common_mistake: "Expecting it on a legacy manifest.",
        },
        ("checkpoint_manifest", "origins") => FieldSemantics {
            ownership: "system",
            operations: &["sync.flush-only"],
            has_default: false,
            default: Value::Null,
            example: json!([{"origin_store_uuid":"5f0c3a2e-1b4d-4e8f-9a2b-3c5d6e7f8091","event_count":340,"min_sequence":1,"max_sequence":340,"objects":["objects/event-gen-1-0"]}]),
            invariants: &[
                "per-origin event summary: origin store UUID, event count, minimum and maximum sequence, and the objects that origin's events were packed into",
                "additive member: absent on manifests from producers predating it",
                "lets a consumer reason about cross-origin sequences without opening shards",
            ],
            common_mistake: "Interleaving sequences across origins; sequence order is per origin.",
        },
        // ---- checkpoint_pointer (current.json / previous.json) --------------
        ("checkpoint_pointer", "schema_version") => FieldSemantics {
            ownership: "system",
            operations: &["sync.flush-only"],
            has_default: false,
            default: Value::Null,
            example: json!(1),
            invariants: &["always 1; a verified restore refuses any other version"],
            common_mistake: "Feeding a future pointer version to this build's restore.",
        },
        ("checkpoint_pointer", "generation_id") => FieldSemantics {
            ownership: "system",
            operations: &["sync.flush-only"],
            has_default: false,
            default: Value::Null,
            example: json!("gen-20260921T120000Z-a1b2c3d4"),
            invariants: &[
                "gen- prefixed; the suffix is 1-128 bytes of lowercase letters, digits, and hyphens",
                "the exact name restore and verify require; anything else is rejected",
            ],
            common_mistake: "Guessing a generation name; use the one current.json records.",
        },
        ("checkpoint_pointer", "mode") => FieldSemantics {
            ownership: "system",
            operations: &["sync.flush-only"],
            has_default: false,
            default: Value::Null,
            example: json!("sharded"),
            invariants: &[
                "enum: monolithic, sharded",
                "selected per the recorded thresholds unless the workspace forces a mode; forcing a monolith past the safety limits is refused",
                "a mode transition supersedes the outgoing root outright",
            ],
            common_mistake: "Assuming the mode is sticky forever; adaptive publications switch at the thresholds.",
        },
        ("checkpoint_pointer", "store_uuid") => FieldSemantics {
            ownership: "system",
            operations: &["sync.flush-only"],
            has_default: false,
            default: Value::Null,
            example: json!("5f0c3a2e-1b4d-4e8f-9a2b-3c5d6e7f8091"),
            invariants: &["the publishing workspace's store UUID"],
            common_mistake: "Restoring a pointer into a foreign store without reconciliation.",
        },
        ("checkpoint_pointer", "snapshot_sequence") => FieldSemantics {
            ownership: "system",
            operations: &["sync.flush-only"],
            has_default: false,
            default: Value::Null,
            example: json!(42),
            invariants: &["the local event sequence the publication covers"],
            common_mistake: "Using it to order operations across stores; use the counts and the roots.",
        },
        ("checkpoint_pointer", "active_root") => FieldSemantics {
            ownership: "system",
            operations: &["sync.flush-only"],
            has_default: false,
            default: Value::Null,
            example: json!({"path":"manifests/9a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b1c2d3e4f5a6b7c8d9e0f1a2b.json","sha256":"0".repeat(64)}),
            invariants: &[
                "path plus sha256 of the generation root: the content-addressed manifest in sharded mode, the object root in monolithic mode",
                "the sha256 binds the root's exact bytes and is verified before any import or restore activates it",
            ],
            common_mistake: "Trusting a root whose hash does not verify.",
        },
        ("checkpoint_pointer", "added_paths") => FieldSemantics {
            ownership: "system",
            operations: &["sync.flush-only"],
            has_default: false,
            default: Value::Null,
            example: json!(["objects/issue-gen-2-0"]),
            invariants: &[
                "sorted generation-relative paths this generation references that the previous generation's referenced set lacked",
            ],
            common_mistake: "Reading added_paths as files created by this publication; reused objects stay referenced without being re-added.",
        },
        ("checkpoint_pointer", "replaced_paths") => FieldSemantics {
            ownership: "system",
            operations: &["sync.flush-only"],
            has_default: false,
            default: Value::Null,
            example: json!(["current.json"]),
            invariants: &[
                "sorted paths referenced by both this generation and the previous one",
                "current.json itself counts as replaced by every publication",
            ],
            common_mistake: "Assuming replaced means rewritten; a reused content-addressed object is referenced by both without changing.",
        },
        ("checkpoint_pointer", "deleted_paths") => FieldSemantics {
            ownership: "system",
            operations: &["sync.flush-only"],
            has_default: false,
            default: Value::Null,
            example: json!(["objects/event-gen-1-3"]),
            invariants: &[
                "tombstone declarations: generation objects referenced by neither retained pointer",
                "applied only after the new pointer is durable; an already-absent path counts as resolved, so an interrupted cleanup is safely reapplied",
            ],
            common_mistake: "Applying tombstones before the pointer is durable.",
        },
        ("checkpoint_pointer", "issue_count") => FieldSemantics {
            ownership: "system",
            operations: &["sync.flush-only"],
            has_default: false,
            default: Value::Null,
            example: json!(120),
            invariants: &["issue records in the generation"],
            common_mistake: "Reading it as live store totals; it describes the published generation.",
        },
        ("checkpoint_pointer", "event_count") => FieldSemantics {
            ownership: "system",
            operations: &["sync.flush-only"],
            has_default: false,
            default: Value::Null,
            example: json!(340),
            invariants: &["event records in the generation"],
            common_mistake: "Reading it as live store totals.",
        },
        ("checkpoint_pointer", "receipt_count") => FieldSemantics {
            ownership: "system",
            operations: &["sync.flush-only"],
            has_default: false,
            default: Value::Null,
            example: json!(2),
            invariants: &["provenance receipts in the generation"],
            common_mistake: "Reading it as live store totals.",
        },
        ("checkpoint_pointer", "attempt_outcome_count") => FieldSemantics {
            ownership: "system",
            operations: &["sync.flush-only"],
            has_default: false,
            default: Value::Null,
            example: json!(7),
            invariants: &[
                "attempt-outcome records in the generation",
                "additive member: absent on pointers from producers predating it",
            ],
            common_mistake: "Expecting it on a legacy pointer.",
        },
        ("checkpoint_pointer", "redaction_record_count") => FieldSemantics {
            ownership: "system",
            operations: &["sync.flush-only"],
            has_default: false,
            default: Value::Null,
            example: json!(4),
            invariants: &[
                "redaction records in the generation",
                "additive member: absent on pointers from producers predating it",
            ],
            common_mistake: "Expecting it on a legacy pointer.",
        },
        ("checkpoint_pointer", "total_record_count") => FieldSemantics {
            ownership: "system",
            operations: &["sync.flush-only"],
            has_default: false,
            default: Value::Null,
            example: json!(513),
            invariants: &["the sum of the five record counts"],
            common_mistake: "Using it to order operations across stores; use the roots and sequences.",
        },
        ("checkpoint_pointer", "created_at") => FieldSemantics {
            ownership: "system",
            operations: &["sync.flush-only"],
            has_default: false,
            default: Value::Null,
            example: json!("2026-09-21T12:00:00.000000000Z"),
            invariants: &["RFC 3339 instant of the write; the pointer is rewritten by every publication"],
            common_mistake: "Comparing pointers by created_at alone; compare generation ids and roots.",
        },
        ("checkpoint_pointer", "redaction_epoch_id") => FieldSemantics {
            ownership: "system",
            operations: &["sync.flush-only"],
            has_default: false,
            default: Value::Null,
            example: json!("0".repeat(64)),
            invariants: &[
                "present only on pointers published by a redaction publication: the epoch that owns the sanitized generation",
            ],
            common_mistake: "Expecting it on ordinary publications; it marks the redaction path only.",
        },
        ("checkpoint_pointer", "previous_generation_reset") => FieldSemantics {
            ownership: "system",
            operations: &["sync.flush-only"],
            has_default: false,
            default: Value::Null,
            example: json!(true),
            invariants: &[
                "present only on redaction-published pointers, and always true there: the pre-redaction generation was deliberately not retained",
            ],
            common_mistake: "Reading false as retained on a redaction pointer; the member appears only when the dirty generation was reset.",
        },
        ("checkpoint_pointer", "superseded_generations") => FieldSemantics {
            ownership: "system",
            operations: &["sync.flush-only"],
            has_default: false,
            default: Value::Null,
            example: json!(["gen-20260920T120000Z-99887766"]),
            invariants: &[
                "generation identities superseded by the sanitized publication",
                "present only on redaction-published pointers",
            ],
            common_mistake: "Putting receipt ids here; these are generation identities.",
        },
        // ---- field_guide (this document's own identity) ---------------------
        ("field_guide", "schema_ref") => FieldSemantics {
            ownership: "system",
            operations: &["schema.explain"],
            has_default: false,
            default: Value::Null,
            example: json!(FIELD_GUIDE_SCHEMA_REF),
            invariants: &["the guide's own published identity, carried by every explanation"],
            common_mistake: "Confusing it with the identity the explanation was asked about.",
        },
        ("field_guide", "guide_version") => FieldSemantics {
            ownership: "system",
            operations: &["schema.explain"],
            has_default: false,
            default: Value::Null,
            example: json!(FIELD_GUIDE_VERSION),
            invariants: &[
                "the pinned guide version; snapshot and conformance tests pin rendered content to it",
                "a bump means the guide's typed shape or a documented semantic changed incompatibly, and the snapshots refresh in the same commit",
            ],
            common_mistake: "Parsing rendered guide content without checking the version.",
        },
        ("field_guide", "describes_schema_refs") => FieldSemantics {
            ownership: "system",
            operations: &["schema.explain"],
            has_default: false,
            default: Value::Null,
            example: json!(["urn:bead-rs:schema:issue:native-v1"]),
            invariants: &[
                "sorted, unique absolute URIs naming the identities the explanation describes",
                "the native guide describes the issue, event, and provenance-receipt identities; a concise explanation names exactly the identity asked about",
            ],
            common_mistake: "Assuming every explanation describes all three native identities.",
        },
        ("field_guide", "documents") => FieldSemantics {
            ownership: "system",
            operations: &["schema.explain"],
            has_default: false,
            default: Value::Null,
            example: json!([{"name":"checkpoint_issue","schema_ref":"urn:bead-rs:schema:issue:native-v1","document_kind":"issue","transport":"public JSON","member_source":"native typed model","members":["id","title"]}]
            ),
            invariants: &[
                "one entry per documented projection: name, schema reference, document kind, transport, member source, and the sorted member list",
            ],
            common_mistake: "Treating a member absent from the list as optional; the list is the declared set.",
        },
        ("field_guide", "fields") => FieldSemantics {
            ownership: "system",
            operations: &["schema.explain"],
            has_default: false,
            default: Value::Null,
            example: json!([{"document":"checkpoint_issue","name":"priority","json_type":"integer","presence":"required","ownership":"caller","operations":["create"],"invariants":["integer from 0 through 4"]}]),
            invariants: &[
                "exactly the flattened members of the documents array, uniquely keyed by document and name",
                "every entry carries type, presence, ownership, operations, default, example, invariants, and a common mistake",
                "no entry may carry unspecified ownership or a documentation-gap invariant; the conformance tests fail the build on either",
            ],
            common_mistake: "Treating a missing field entry as an optional member.",
        },
        ("field_guide", "additional_properties") => FieldSemantics {
            ownership: "system",
            operations: &["schema.explain"],
            has_default: false,
            default: Value::Null,
            example: json!({"allowed":true,"ownership":"preserved","rules":["Unknown checkpoint issue members retain exact JSON name, type, value, and null-versus-absence presence."]}),
            invariants: &[
                "whether unknown members are allowed and how a producer treats them",
                "agrees with the published schema's additionalProperties for every described identity",
            ],
            common_mistake: "Assuming all documents reject unknown members; issue and redaction documents preserve them.",
        },
        ("field_guide", "lifecycle") => FieldSemantics {
            ownership: "system",
            operations: &["schema.explain"],
            has_default: false,
            default: Value::Null,
            example: json!({"base_values":["open","in_progress"],"allowed_transitions":["open->in_progress"]}),
            invariants: &[
                "the stored state vocabulary and the transitions the store applies",
                "never empty: every catalog identity publishes populated lifecycle content",
            ],
            common_mistake: "Reading blocked or ready as base values; they are derived views.",
        },
        ("field_guide", "derived_state") => FieldSemantics {
            ownership: "system",
            operations: &["schema.explain"],
            has_default: false,
            default: Value::Null,
            example: json!({"ready":{"ownership":"derived","rules":["base status open, not manually blocked, unassigned, and no unfinished blocks blocker"]}}),
            invariants: &[
                "status, ready, blocked_by, and blocking with their ownership and derivation rules",
            ],
            common_mistake: "Storing derived state; none of it is a stored field.",
        },
        ("field_guide", "events") => FieldSemantics {
            ownership: "system",
            operations: &["schema.explain"],
            has_default: false,
            default: Value::Null,
            example: json!({"envelope_member":"event","schema_ref_member":"$schema","identity":["origin_store_uuid","origin_event_sequence"]}),
            invariants: &[
                "the audit-event envelope: which member wraps an event, which carries its schema identity, and what identifies and orders events",
            ],
            common_mistake: "Ordering events by time; the identity pair is the ordering.",
        },
        ("field_guide", "operations") => FieldSemantics {
            ownership: "system",
            operations: &["schema.explain"],
            has_default: false,
            default: Value::Null,
            example: json!([{"name":"close","ownership_effect":"terminal caller-visible transition","success_exit":0,"failure_exits":[2,3,4]}]),
            invariants: &[
                "documented operations with ownership effects, exit codes, affected fields, and rules, sorted lexicographically by name",
            ],
            common_mistake: "Assuming exit codes are uniform across operations; each entry lists its own failures.",
        },
        ("field_guide", "rehydration") => FieldSemantics {
            ownership: "system",
            operations: &["schema.explain"],
            has_default: false,
            default: Value::Null,
            example: json!({"source_mode":"read-only","allowed_writes":["public bead commands in a separate destination"],"forbidden_writes":["foreign SQLite","native SQLite","synthetic checkpoint JSON"]}),
            invariants: &[
                "the rehydration posture: source mode, allowed and forbidden writes, and the verification expectations",
            ],
            common_mistake: "Writing a source tracker's data directly into a native store during rehydration.",
        },
        ("field_guide", "known_implementation_deviations") => FieldSemantics {
            ownership: "system",
            operations: &["schema.explain"],
            has_default: false,
            default: Value::Null,
            example: json!([{"id":"manual-blocked-cli-projection","severity":"known","behavior":"v0.1 CLI projections expose base_status without the manual_blocked overlay","required_disposition":"Consumers must not infer readiness from status alone."}]),
            invariants: &[
                "recorded places this implementation deviates from the profile, each with a required consumer disposition",
            ],
            common_mistake: "Treating a listed deviation as a bug to work around; the disposition is the contract.",
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

/// Curated guide entry for one member of a registry schema — the concise-path
/// counterpart of `guide_field`. The published property shape (typed JSON
/// type, nullability, presence from `required_for`) merges with the semantics
/// table so every concise-path field carries the same curated depth the
/// native guide publishes; a member without a semantics-table entry falls
/// through to `unspecified_semantics`, which the conformance tests fail the
/// build on rather than publish.
fn guide_field_for_schema(kind: &str, name: &str) -> Value {
    let schema = property_schema(kind, name);
    let json_type = schema
        .get("type")
        .and_then(|value| {
            value.as_str().or_else(|| {
                value.as_array()?.iter().find_map(|candidate| {
                    let candidate = candidate.as_str()?;
                    (candidate != "null").then_some(candidate)
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
    let presence = if required_for(kind).iter().any(|member| member == name) {
        "required"
    } else {
        "optional"
    };
    let semantics = field_semantics(kind, name);
    let mut operations: Vec<&str> = semantics.operations.to_vec();
    operations.sort_unstable();
    json!({
        "document":kind,"name":name,"json_type":json_type,"nullable":nullable,
        "presence":presence,"has_default":semantics.has_default,
        "default":semantics.default,"ownership":semantics.ownership,
        "operations":operations,"invariants":semantics.invariants,
        "example":semantics.example,"common_mistake":semantics.common_mistake
    })
}

/// Lifecycle section for a concise explanation. Every catalog identity
/// publishes populated lifecycle content — the stored-state machine for the
/// documents that capture issue states, the publication states for redaction
/// records, the pointer/manifest relationship for checkpoint documents, and
/// the compiled/read-only posture for the registry's own documents. The
/// catchall stays an explicit tripwire: a registry kind added without curated
/// lifecycle content fails the conformance tests instead of silently
/// publishing an empty section.
fn concise_lifecycle(kind: &str) -> Value {
    match kind {
        // Documents that record or cause issue-state transitions carry the
        // stored issue machine they act on: the four transitions the resolve
        // actions apply (close from any non-closed base status, release from
        // in_progress), not the full derived machine.
        "attempt_outcome" | "resolve_receipt" | "resolve_request" => json!({
            "base_values":["closed","deferred","in_progress","open"],
            "allowed_transitions":[
                "deferred->closed by a close action",
                "in_progress->closed by a close action",
                "in_progress->open by a release action",
                "open->closed by a close action"
            ]
        }),
        // Checkpoint pointer roles: current.json names the active generation
        // and previous.json the outgoing one; every publication moves the
        // outgoing root to previous.json, and a redaction publication rewrites
        // both pointers without retaining the pre-redaction generation.
        "checkpoint_pointer" => json!({
            "base_values":["current.json","previous.json"],
            "allowed_transitions":[
                "current.json->previous.json when the next publication supersedes the active generation",
                "previous.json->overwritten when the publication after that supersedes it; a redaction publication rewrites both pointers and does not retain the pre-redaction generation"
            ]
        }),
        // Manifest lifecycle is the retention question: a manifest stays
        // reachable through whichever pointer names it, and once neither does
        // the publication's tombstone declarations retire it. An
        // already-absent object counts as resolved, so an interrupted cleanup
        // is safely reapplied.
        "checkpoint_manifest" => json!({
            "base_values":["referenced by current.json or previous.json","tombstoned"],
            "allowed_transitions":[
                "referenced by current.json or previous.json->tombstoned once a publication leaves the manifest named by neither pointer"
            ]
        }),
        // Redaction records commit in the same transaction that replaces the
        // bytes, then publish as one generation set; discarded is recorded,
        // never deleted.
        "redaction_receipt" | "redaction_epoch" => json!({
            "base_values":["committed","discarded","published"],
            "allowed_transitions":[
                "committed->published when the sanitized generation set is published",
                "committed->discarded when the redaction is rolled back"
            ]
        }),
        // A finding is written only by the redact commit that consumes it,
        // and its severity is fixed by the scan that produced it.
        "redaction_finding" => json!({
            "base_values":["advisory","blocking"],
            "allowed_transitions":[
                "detected->redacted when the redact commit persists the finding alongside its receipt"
            ]
        }),
        // Acknowledgment records enter through checkpoint import activation;
        // the invocation-level admission path appends a secret_acknowledged
        // audit event instead of a durable row.
        "redaction_acknowledgment" => json!({
            "base_values":["admitted"],
            "allowed_transitions":[
                "reported->admitted when the exact fingerprint is acknowledged, by import activation for the durable row or invocation-level acknowledgment for the audit event"
            ]
        }),
        // The selector addresses bytes; its subject moves from intact to
        // replaced inside the redaction transaction, after the prior-record
        // hash revalidates.
        "redaction_field_selector" => json!({
            "base_values":["intact","replaced"],
            "allowed_transitions":[
                "intact->replaced when the redaction transaction revalidates prior_record_hash and writes the sanitized record"
            ]
        }),
        // Tombstones guard forever once written; their transition describes
        // what they do to pre-redaction content, not a state they move to.
        "redaction_tombstone" => json!({
            "base_values":["guarding"],
            "allowed_transitions":[
                "guarding->refuses pre-redaction content on recovery whenever incoming content hashes to prior_record_hash"
            ]
        }),
        // The registry's own documents are compiled artifacts with no stored
        // state: published on demand, never mutated.
        "capabilities" => json!({
            "base_values":["compiled into the binary"],
            "allowed_transitions":[
                "compiled->advertised by each bead capabilities invocation; read-only, the document has no stored state"
            ]
        }),
        "field_guide" => json!({
            "base_values":["compiled at the pinned guide version"],
            "allowed_transitions":[
                "compiled->published by each bead schema explain invocation; read-only, and a semantic change bumps FIELD_GUIDE_VERSION rather than mutating a stored document"
            ]
        }),
        _ => json!({
            "base_values":[],
            "allowed_transitions":[]
        }),
    }
}

/// Additional-properties section for a concise explanation, replicating the
/// published schema's own rule: issue documents and redaction record
/// documents preserve unknown members, every other document rejects them.
/// Deriving this from the same expression `schema_document` publishes keeps
/// the guide from misciting the closure.
fn concise_additional_properties(kind: &str) -> Value {
    if kind == "issue" || kind.starts_with("redaction_") {
        json!({
            "allowed": true,
            "ownership": "preserved",
            "rules": ["Unknown members are allowed by the published schema and preserved with their exact JSON name, type, value, and null-versus-absence presence."]
        })
    } else {
        json!({
            "allowed": false,
            "ownership": "rejected",
            "rules": ["Unknown members are rejected: the published schema is closed and names every valid member."]
        })
    }
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
            rules: &[
                "workspace-independent; exact catalog identities only",
                "worked example (typed JSON): bead schema explain urn:bead-rs:schema:issue:native-v1 --format json",
                "worked example (Markdown): bead schema explain urn:bead-rs:schema:issue:native-v1 --format markdown",
                "a request for any other catalog identity, for example bead schema explain urn:bead-rs:schema:checkpoint-pointer:native-v1, returns the concise explanation of that schema alone",
                "common mistake: passing a shortened, padded, or wrong-case identity such as urn:bead-rs:schema:issue — resolution is byte-exact, so anything that is not a catalog identity is a usage error (exit 2); copy the identity verbatim from bead schema list",
                "common mistake: expecting a concise explanation to carry the issue lifecycle, the derived-state rules, or the full operations table — the issue, audit-event, and provenance-receipt identities share the full native guide, while every other identity publishes only its own members with schema.show as the sole listed operation",
            ],
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
            failure_exits: &[2, 3, 4, 5],
            affected_fields: &[],
            rules: &[
                "writes the durable checkpoint from the live store",
                "idempotent",
                "refuses to publish over a remote-advanced checkpoint (exit 4) or a covered-ahead integrity failure (exit 5)",
            ],
        },
        OperationSemantics {
            name: "sync.import-only",
            ownership_effect: "restores or merges checkpoint state",
            failure_exits: &[2, 3, 4, 5],
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
        .map(|name| guide_field_for_schema(descriptor.document_kind, name))
        .collect();
    // Audit F1 (beadrs-f8805045): the two checkpoint documents are the only
    // registry kinds whose on-disk documents carry no in-document schema
    // URN — the manifest self-identifies as format checkpoint-set-v1 and the
    // pointer only as schema_version 1 — while issue, audit_event,
    // provenance_receipt, attempt_outcome, and redaction documents stamp
    // `$schema`. Publish that binding difference for these kinds instead of
    // leaving it implicit.
    let known_deviations = if matches!(
        descriptor.document_kind,
        "checkpoint_manifest" | "checkpoint_pointer"
    ) {
        json!([{
            "id": "checkpoint-documents-carry-no-schema-urn",
            "severity": "known",
            "behavior": "checkpoint_manifest and checkpoint_pointer documents stamp no in-document schema URN: the manifest self-identifies as format checkpoint-set-v1 and the pointer only as schema_version 1, unlike issue, audit_event, provenance_receipt, attempt_outcome, and redaction documents, which stamp $schema",
            "required_disposition": "Resolve checkpoint document identity through the checkpoint set (the pointer active_root names the manifest) or bead schema list, never by requiring a $schema member on a checkpoint document."
        }])
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
        "additional_properties": concise_additional_properties(descriptor.document_kind),
        "lifecycle": concise_lifecycle(descriptor.document_kind),
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
        "known_implementation_deviations": known_deviations
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

//! Immutable public schema registry.
//!
//! Both capability negotiation and `bead schema list` project this registry,
//! preventing those discovery surfaces from drifting apart.

use crate::error::{Error, Result};
use crate::service::capabilities::SchemaEntry;
use serde_json::{json, Map, Value};
use std::collections::HashSet;

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
        schema_ref: "urn:bead-rs:schema:field-guide:native-v1",
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
            "secret_scan",
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
            "attempt_outcome_shards",
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
        | ("checkpoint_manifest", "attempt_outcome_shards") => json!({"type":"array"}),
        ("checkpoint_manifest", "attempt_outcome_count") => json!({"type":"integer", "minimum":0}),
        ("checkpoint_manifest", "partition_thresholds") => json!({"type":"object"}),
        ("capabilities", "store_layout") => json!({"type":"integer", "minimum":1}),
        ("capabilities", "atomic_claim")
        | ("capabilities", "logical_revision")
        | ("capabilities", "auto_flush") => {
            json!({"type":"boolean"})
        }
        ("capabilities", "statuses")
        | ("capabilities", "checkpoint_modes")
        | ("capabilities", "checkpoint_formats")
        | ("capabilities", "schemas")
        | ("capabilities", "commands") => json!({"type":"array"}),
        ("capabilities", "priorities") => json!({"type":"object"}),
        ("capabilities", "secret_scan") => json!({"type":"object"}),
        ("field_guide", "guide_version") => json!({"const":1}),
        ("attempt_outcome", "attempt_id") => json!({"type":"string", "minLength":1, "maxLength":255}),
        ("attempt_outcome", "issue_id") => json!({"type":"string", "minLength":1, "maxLength":255}),
        ("attempt_outcome", "outcome") => json!({"type":"string", "enum":["verified_success","work_failure","infrastructure_failure","cancelled","indeterminate"]}),
        ("attempt_outcome", "action") => json!({"type":"string", "enum":["close","release","quarantine","block","none"]}),
        ("attempt_outcome", "reason") => json!({"type":["string","null"], "maxLength":4194304}),
        ("attempt_outcome", "canonical_request_hash") => json!({"type":"string", "minLength":64, "maxLength":64}),
        ("attempt_outcome", "resulting_state") => json!({"type":"string", "enum":["open","in_progress","deferred","closed"]}),
        ("attempt_outcome", "resulting_issue_revision") => json!({"type":"integer", "minimum":1}),
        ("attempt_outcome", "resulting_attempt_tier") => json!({"type":"integer", "minimum":0, "maximum":3}),
        ("attempt_outcome", "receipt_id") => json!({"type":"string", "minLength":1}),
        ("attempt_outcome", "actor") => json!({"type":"string", "minLength":1, "maxLength":255}),
        ("attempt_outcome", "created_at") => timestamp(),
        ("attempt_outcome", "evidence_refs") => json!({"type":"array", "items":{"type":"string"}}),
        ("attempt_outcome", "model")
        | ("attempt_outcome", "harness")
        | ("attempt_outcome", "harness_version") => json!({"type":["string","null"]}),
        ("resolve_receipt", "receipt_id") => json!({"type":"string", "minLength":1}),
        ("resolve_receipt", "canonical_request_hash") => json!({"type":"string", "minLength":64, "maxLength":64}),
        ("resolve_receipt", "issue_id") => json!({"type":"string", "minLength":1, "maxLength":255}),
        ("resolve_receipt", "attempt_id") => json!({"type":"string", "minLength":1, "maxLength":255}),
        ("resolve_receipt", "resulting_issue_revision") => json!({"type":"integer", "minimum":1}),
        ("resolve_receipt", "resulting_state") => json!({"type":"string"}),
        ("resolve_receipt", "resulting_attempt_tier") => json!({"type":"integer", "minimum":0, "maximum":3}),
        ("resolve_receipt", "created_at") => timestamp(),
        ("resolve_receipt", "is_replay") => json!({"type":"boolean"}),
        ("resolve_request", "attempt_id") => json!({"type":"string", "minLength":1, "maxLength":255}),
        ("resolve_request", "issue_id") => json!({"type":"string", "minLength":1, "maxLength":255}),
        ("resolve_request", "outcome") => json!({"type":"string", "enum":["verified_success","work_failure","infrastructure_failure","cancelled","indeterminate"]}),
        ("resolve_request", "action") => json!({"type":["string","null"], "enum":["close","release","quarantine","block","none"]}),
        ("resolve_request", "reason") => json!({"type":["string","null"], "maxLength":4194304}),
        ("resolve_request", "if_revision") => json!({"type":["integer","null"], "minimum":1}),
        ("resolve_request", "fencing_token") => json!({"type":["string","null"]}),
        ("resolve_request", "evidence_refs") => json!({"type":"array", "items":{"type":"string"}}),
        ("resolve_request", "actor") => json!({"type":"string", "minLength":1, "maxLength":255}),
        ("resolve_request", "model")
        | ("resolve_request", "harness")
        | ("resolve_request", "harness_version") => json!({"type":["string","null"]}),
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
        // validate against the same additive identity (plan section 11)
        "capabilities" => &["auto_flush", "secret_scan"],
        "attempt_outcome" => &["reason", "model", "harness", "harness_version"],
        "checkpoint_pointer" => &["attempt_outcome_count"],
        "checkpoint_manifest" => &["attempt_outcome_count", "attempt_outcome_shards"],
        "resolve_receipt" => &[],
        "resolve_request" => &["action", "reason", "if_revision", "fencing_token", "evidence_refs", "model", "harness", "harness_version"],
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
                "created_at",
                "dependencies",
                "description",
                "id",
                "labels",
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
            &["assignee", "bead_id", "lease"],
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

fn guide_field(document: &str, name: &str) -> Value {
    let checkpoint_kind = match document {
        "checkpoint_event" => "audit_event",
        "checkpoint_provenance_receipt" => "provenance_receipt",
        _ => "issue",
    };
    let schema = if document == "cli_issue" && name == "status" {
        json!({"type":"string"})
    } else if document == "claim_result" {
        match name {
            "lease" => json!({"type":["object","null"]}),
            _ => json!({"type":["string","null"]}),
        }
    } else {
        property_schema(checkpoint_kind, name)
    };
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
    let required = match document {
        "cli_issue" => true,
        "claim_result" => name != "lease",
        _ => required_for(checkpoint_kind)
            .iter()
            .any(|required| required == name),
    };
    let (ownership, operations, default, has_default, example, mistake): (
        &str,
        Vec<&str>,
        Value,
        bool,
        Value,
        &str,
    ) = match name {
        "id" => (
            "system",
            vec!["create", "sync.import-only"],
            Value::Null,
            false,
            json!("bead-18409c0e"),
            "Manufacturing an ID or inferring chronology from its spelling.",
        ),
        "title" => (
            "caller",
            vec!["create"],
            Value::Null,
            false,
            json!("Verify restore invariants"),
            "Attempting update --title, which is a usage error.",
        ),
        "revision" => (
            "system",
            vec!["close", "release", "reopen", "update"],
            json!(1),
            true,
            json!(4),
            "Choosing the next revision or treating it as time.",
        ),
        "description" => (
            "caller",
            vec!["create"],
            if document == "cli_issue" {
                json!("")
            } else {
                Value::Null
            },
            document == "cli_issue",
            json!("Rehearse flush and restore."),
            "Treating absence, null, and projected empty text as interchangeable.",
        ),
        "notes" => (
            "caller",
            vec!["update"],
            json!(""),
            true,
            json!("Reproduction captured."),
            "Expecting create --notes or assuming notes are private.",
        ),
        "priority" => (
            "caller",
            vec!["create"],
            json!(2),
            true,
            json!(2),
            "Treating P2 as normal or reversing priority order.",
        ),
        "base_status" => (
            "system",
            vec!["claim", "close", "release", "reopen", "update"],
            json!("open"),
            true,
            json!("open"),
            "Storing blocked or ready as a base value.",
        ),
        "status" => (
            "derived",
            vec!["list", "show"],
            Value::Null,
            false,
            json!("open"),
            "Assuming status open proves readiness.",
        ),
        "manual_blocked" => (
            "caller",
            vec!["close", "reopen", "update"],
            json!(false),
            true,
            json!(false),
            "Encoding graph blocking in this flag.",
        ),
        "assignee" => (
            "caller",
            vec!["claim", "create", "release", "update"],
            Value::Null,
            document == "cli_issue",
            Value::Null,
            "Treating assignment as authorization.",
        ),
        "labels" => (
            "caller",
            vec!["create", "label.add", "label.remove"],
            json!([]),
            true,
            json!([]),
            "Treating a label as lifecycle state.",
        ),
        "dependencies" => (
            "caller",
            vec!["dep.add", "dep.remove"],
            json!([]),
            true,
            json!([]),
            "Reversing the blocked-first direction.",
        ),
        "$schema" | "schema_ref" => (
            "system",
            vec![],
            Value::Null,
            false,
            json!("urn:bead-rs:schema:issue:native-v1"),
            "Confusing a public schema identity with private storage layout.",
        ),
        _ => (
            "system",
            vec![],
            Value::Null,
            false,
            Value::Null,
            "Inferring semantics from the member name instead of the governing schema.",
        ),
    };
    json!({
        "document":document,"name":name,"json_type":json_type,"nullable":nullable,
        "presence":if required {"required"} else {"optional"},"has_default":has_default,
        "default":default,"ownership":ownership,"operations":operations,"invariants":[],
        "example":example,"common_mistake":mistake
    })
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
        "schema_ref":"urn:bead-rs:schema:field-guide:native-v1","guide_version":1,
        "describes_schema_refs":["urn:bead-rs:schema:event:native-v1","urn:bead-rs:schema:issue:native-v1","urn:bead-rs:schema:provenance-receipt:native-v1"],
        "documents":documents,"fields":fields,
        "additional_properties":{"allowed":true,"ownership":"preserved","rules":["Unknown checkpoint issue members retain exact JSON name, type, value, and null-versus-absence presence."]},
        "lifecycle":{"base_values":["closed","deferred","in_progress","open"],"allowed_transitions":["closed->open","deferred->closed","deferred->open","in_progress->closed","in_progress->deferred","in_progress->open","open->closed","open->deferred","open->in_progress"]},
        "derived_state":{"status":{"ownership":"derived","rules":["manual_blocked overlays non-closed base status as blocked"]},"ready":{"ownership":"derived","rules":["base status is open, not manually blocked, unassigned, and has no unfinished blocks blocker"]},"blocked_by":{"ownership":"derived","rules":["derived from incoming blocks edges"]},"blocking":{"ownership":"derived","rules":["derived from outgoing blocks edges"]}},
        "events":{"envelope_member":"event","schema_ref_member":"$schema","identity":["origin_store_uuid","origin_event_sequence"],"ordering":["origin_store_uuid","origin_event_sequence"]},
        "operations":[],
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
        "schema_ref": "urn:bead-rs:schema:field-guide:native-v1",
        "guide_version": 1,
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
            "### {}.{}\n\n- Type: `{}`{}\n- Presence: `{}`\n- Ownership: `{}`\n- Common mistake: {}\n\n",
            field["document"].as_str().unwrap_or("document"),
            field["name"].as_str().unwrap_or("member"),
            field["json_type"].as_str().unwrap_or("json"),
            if field["nullable"].as_bool().unwrap_or(false) {
                " (nullable)"
            } else {
                ""
            },
            field["presence"].as_str().unwrap_or("unknown"),
            field["ownership"].as_str().unwrap_or("unknown"),
            field["common_mistake"].as_str().unwrap_or("")
        ));
    }
    output
}

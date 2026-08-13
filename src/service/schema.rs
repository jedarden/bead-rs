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
        validate: false,
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

fn properties_for(kind: &str) -> Map<String, Value> {
    let names: &[&str] = match kind {
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
        ],
        "checkpoint_pointer" => &[
            "$schema",
            "mode",
            "store_uuid",
            "snapshot_sequence",
            "active_root",
        ],
        "checkpoint_manifest" => &[
            "$schema",
            "mode",
            "store_uuid",
            "snapshot_sequence",
            "shards",
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
    };
    names
        .iter()
        .map(|name| ((*name).to_string(), json!({})))
        .collect()
}

pub fn schema_document(schema_ref: &str) -> Result<Value> {
    let descriptor = descriptor(schema_ref)?;
    let properties = properties_for(descriptor.document_kind);
    let required: Vec<String> = properties.keys().cloned().collect();
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

pub fn schema_explanation(schema_ref: &str) -> Result<Value> {
    let descriptor = descriptor(schema_ref)?;
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
    let document = &explanation["documents"][0];
    let mut output = format!(
        "# {}\n\nSchema: `{}`\n\n## Members\n\n",
        document["document_kind"].as_str().unwrap_or("document"),
        document["schema_ref"].as_str().unwrap_or("")
    );
    for member in document["members"].as_array().into_iter().flatten() {
        output.push_str(&format!("- `{}`\n", member.as_str().unwrap_or("")));
    }
    output
}

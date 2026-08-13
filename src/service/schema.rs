//! Immutable public schema registry.
//!
//! Both capability negotiation and `bead schema list` project this registry,
//! preventing those discovery surfaces from drifting apart.

use crate::error::{Error, Result};
use crate::service::capabilities::SchemaEntry;
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

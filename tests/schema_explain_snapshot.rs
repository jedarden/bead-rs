//! Snapshot and version-conformance tests for `bead schema explain`.
//!
//! The field guide is a published contract: `FIELD_GUIDE_VERSION` exists so a
//! consumer can tell guide shapes apart, and its doc comment in
//! `src/service/schema.rs` promises that snapshot and conformance tests pin the
//! rendered content to that version. This file is that promise.
//!
//! Covered here:
//!
//! * byte digests of both renderings (JSON, Markdown) of the canonical native
//!   guide and of a concise-path explanation, keyed to the pinned guide version
//! * the version tripwire: any change to rendered guide content fails the
//!   snapshot tests with instructions to bump `FIELD_GUIDE_VERSION` and
//!   refresh the digests in the same commit
//! * a version-conformance sweep over every catalog identity: each
//!   explanation carries the compiled guide identity, describes the schema it
//!   was asked about, and every field it publishes is fully curated
//! * field/document parity: the fields array is exactly the flattened members
//!   of the documents array, for every identity
//! * renderer parity: the Markdown the CLI emits is byte-for-byte
//!   `schema_explanation_markdown` applied to the very JSON explanation the
//!   CLI emits — one typed source, two renderings, no second code path
//! * repeat-invocation determinism: every snapshot rendering reproduces its
//!   pinned digest on every repeated run, so determinism is pinned by digest
//!   rather than asserted between two runs

use assert_cmd::Command;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashSet;

use bead_rs::service::schema::{
    schema_explanation_markdown, FIELD_GUIDE_SCHEMA_REF, FIELD_GUIDE_VERSION,
};

/// Canonical native-guide identity: the richest explanation the command emits
/// (all five documents, the full semantics table, and every section rendered).
const NATIVE_GUIDE_SCHEMA_REF: &str = "urn:bead-rs:schema:issue:native-v1";

/// Concise-path identity: a closed published schema explained through the
/// per-schema branch rather than the native guide.
const CONCISE_SCHEMA_REF: &str = "urn:bead-rs:schema:checkpoint-pointer:native-v1";

/// Guide version the snapshots below were taken at. Bumping
/// `FIELD_GUIDE_VERSION` in `src/service/schema.rs` without refreshing the
/// digests is exactly the drift this constant exists to catch.
const SNAPSHOT_GUIDE_VERSION: i64 = 3;

/// `bead schema explain <native-guide identity> --format json`, guide v3.
const NATIVE_GUIDE_JSON_SHA256: &str =
    "9e90be650926a08266104d1c5b9a51915fe561ed5efc5e8ef9ba548e3c2b989d";

/// `bead schema explain <native-guide identity> --format markdown`, guide v3.
const NATIVE_GUIDE_MARKDOWN_SHA256: &str =
    "0178eab46c8b8ca8a1a28862b2bd8690405bf5d4dab5aa53aa45f0acd80d4108";

/// `bead schema explain <concise identity> --format json`, guide v3.
const CONCISE_JSON_SHA256: &str =
    "3879c438ee5d2a520ee693b0abbd81d55c8b9dab94215fcec543343d62b7d33e";

/// `bead schema explain <concise identity> --format markdown`, guide v3.
const CONCISE_MARKDOWN_SHA256: &str =
    "afdb5b5ce2901185b39dcd7214b46d5699df19ead9f73f57d46266ef475807cf";

const GUIDE_FIELD_MEMBERS: [&str; 10] = [
    "json_type",
    "nullable",
    "presence",
    "has_default",
    "default",
    "ownership",
    "operations",
    "invariants",
    "example",
    "common_mistake",
];

fn explain(schema_ref: &str, format: &str) -> Vec<u8> {
    Command::cargo_bin("bead")
        .unwrap()
        .args(["schema", "explain", schema_ref, "--format", format])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone()
}

fn explain_json(schema_ref: &str) -> Value {
    serde_json::from_slice(&explain(schema_ref, "json")).unwrap()
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// One snapshot: an identity, both renderings, and the digests they must
/// reproduce byte-for-byte at `SNAPSHOT_GUIDE_VERSION`.
struct Snapshot {
    schema_ref: &'static str,
    json_sha256: &'static str,
    markdown_sha256: &'static str,
}

const SNAPSHOTS: [Snapshot; 2] = [
    Snapshot {
        schema_ref: NATIVE_GUIDE_SCHEMA_REF,
        json_sha256: NATIVE_GUIDE_JSON_SHA256,
        markdown_sha256: NATIVE_GUIDE_MARKDOWN_SHA256,
    },
    Snapshot {
        schema_ref: CONCISE_SCHEMA_REF,
        json_sha256: CONCISE_JSON_SHA256,
        markdown_sha256: CONCISE_MARKDOWN_SHA256,
    },
];

/// The version tripwire. If this fails, the guide's rendered content changed:
/// decide whether the change is an incompatible shape or semantic change, bump
/// `FIELD_GUIDE_VERSION` in `src/service/schema.rs` if so, and refresh the
/// `*_SHA256` constants in this file — all in the same commit.
#[test]
fn snapshots_pin_field_guide_v3_renders() {
    assert_eq!(
        FIELD_GUIDE_VERSION, SNAPSHOT_GUIDE_VERSION,
        "FIELD_GUIDE_VERSION moved without refreshing the snapshot digests in \
         tests/schema_explain_snapshot.rs — bump SNAPSHOT_GUIDE_VERSION and all \
         *_SHA256 constants in the same commit as the version bump"
    );
    for snapshot in SNAPSHOTS {
        for (format, expected) in [
            ("json", snapshot.json_sha256),
            ("markdown", snapshot.markdown_sha256),
        ] {
            let output = explain(snapshot.schema_ref, format);
            assert!(
                !output.is_empty(),
                "{} {format} output must not be empty",
                snapshot.schema_ref
            );
            assert_eq!(
                sha256(&output),
                expected,
                "{format} rendering of {} drifted from the guide v3 snapshot; \
                 if this change is intentional, bump FIELD_GUIDE_VERSION and \
                 refresh the digest in tests/schema_explain_snapshot.rs",
                snapshot.schema_ref
            );
        }
    }
}

/// Every explanation carries the published guide identity, names the schema it
/// was asked about, and publishes only fully curated fields — no
/// `unspecified` ownership and no documentation-gap invariants.
#[test]
fn every_catalog_identity_conforms_to_the_published_guide_version() {
    let catalog: Vec<Value> = serde_json::from_slice(
        &Command::cargo_bin("bead")
            .unwrap()
            .args(["schema", "list", "--format", "json"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone(),
    )
    .unwrap();
    assert!(!catalog.is_empty());

    for entry in &catalog {
        let schema_ref = entry["schema_ref"].as_str().unwrap();
        let explanation = explain_json(schema_ref);
        assert_eq!(
            explanation["guide_version"], FIELD_GUIDE_VERSION,
            "{schema_ref} explanation must carry the compiled guide version"
        );
        assert_eq!(
            explanation["schema_ref"], FIELD_GUIDE_SCHEMA_REF,
            "{schema_ref} explanation must carry the published guide identity"
        );
        assert!(
            explanation["describes_schema_refs"]
                .as_array()
                .unwrap()
                .iter()
                .any(|value| value == schema_ref),
            "{schema_ref} explanation must describe the identity asked about"
        );

        let fields = explanation["fields"].as_array().unwrap();
        assert!(
            !fields.is_empty(),
            "{schema_ref} must publish field entries"
        );
        for field in fields {
            let key = format!(
                "{}.{}",
                field["document"].as_str().unwrap(),
                field["name"].as_str().unwrap()
            );
            for member in GUIDE_FIELD_MEMBERS {
                assert!(
                    field.get(member).is_some(),
                    "field {key} of {schema_ref} is missing guide member {member}"
                );
            }
            assert_ne!(
                field["ownership"], "unspecified",
                "{key} of {schema_ref} lacks curated semantics — extend the \
                 field_semantics table in src/service/schema.rs instead of \
                 publishing a documentation gap"
            );
            assert!(
                !field["invariants"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|rule| rule.as_str().unwrap().contains("documentation gap")),
                "{key} of {schema_ref} publishes the documentation-gap fallback \
                 invariant — extend the field_semantics table instead"
            );
        }
    }
}

/// The fields array is exactly the flattened, attributed members of the
/// documents array — one curated field per published member, no orphans on
/// either side — for both explanation paths.
#[test]
fn fields_match_published_document_members_exactly() {
    for schema_ref in [NATIVE_GUIDE_SCHEMA_REF, CONCISE_SCHEMA_REF] {
        let explanation = explain_json(schema_ref);
        let mut expected: Vec<String> = Vec::new();
        for document in explanation["documents"].as_array().unwrap() {
            let name = document["name"].as_str().unwrap();
            for member in document["members"].as_array().unwrap() {
                expected.push(format!("{}.{}", name, member.as_str().unwrap()));
            }
        }
        expected.sort();
        expected.dedup();

        let mut actual: Vec<String> = explanation["fields"]
            .as_array()
            .unwrap()
            .iter()
            .map(|field| {
                format!(
                    "{}.{}",
                    field["document"].as_str().unwrap(),
                    field["name"].as_str().unwrap()
                )
            })
            .collect();
        actual.sort();
        actual.dedup();

        assert_eq!(
            actual, expected,
            "{schema_ref} fields must be exactly the flattened document members"
        );
        let keys: HashSet<&String> = actual.iter().collect();
        assert_eq!(
            keys.len(),
            actual.len(),
            "{schema_ref} has duplicate fields"
        );
    }
}

/// JSON and Markdown are two renderings of one typed explanation value. The
/// Markdown the CLI emits must be byte-for-byte what `schema_explanation_markdown`
/// produces when handed the very explanation JSON the CLI emits — if the
/// Markdown path ever grew its own source, this catches it on both identities.
#[test]
fn markdown_is_exactly_the_rendering_of_the_emitted_json_explanation() {
    for snapshot in SNAPSHOTS {
        let explanation: Value = serde_json::from_slice(&explain(snapshot.schema_ref, "json"))
            .unwrap_or_else(|error| {
                panic!("{} emitted unparseable JSON: {error}", snapshot.schema_ref)
            });
        let cli_markdown = String::from_utf8(explain(snapshot.schema_ref, "markdown"))
            .expect("markdown output is UTF-8");
        let rendered = schema_explanation_markdown(&explanation);
        // The renderer is a pure function of the typed value: rendering the
        // same value twice never differs.
        assert_eq!(
            rendered,
            schema_explanation_markdown(&explanation),
            "{} renderer must be deterministic for a fixed explanation value",
            snapshot.schema_ref
        );
        assert_eq!(
            rendered, cli_markdown,
            "{} markdown must be exactly the rendering of the emitted JSON \
             explanation — same typed source, no second code path",
            snapshot.schema_ref
        );
    }
}

/// Repeated invocations are byte-identical, pinned by digest: every run of
/// every snapshot rendering must reproduce the exact pinned sha256, so
/// determinism holds per-run against a fixed value rather than merely
/// between two adjacent runs.
#[test]
fn repeated_invocations_reproduce_the_pinned_digests() {
    const RUNS: usize = 3;
    for snapshot in SNAPSHOTS {
        for (format, expected) in [
            ("json", snapshot.json_sha256),
            ("markdown", snapshot.markdown_sha256),
        ] {
            let mut digests: HashSet<String> = HashSet::new();
            for _ in 0..RUNS {
                let output = explain(snapshot.schema_ref, format);
                assert!(
                    !output.is_empty(),
                    "{} {format} output must not be empty",
                    snapshot.schema_ref
                );
                let digest = sha256(&output);
                digests.insert(digest.clone());
                assert_eq!(
                    digest, expected,
                    "{format} rendering of {} drifted from the pinned guide v3 \
                     digest across repeated invocations",
                    snapshot.schema_ref
                );
            }
            assert_eq!(
                digests.len(),
                1,
                "{} {format} varied across {RUNS} repeated invocations: {digests:?}",
                snapshot.schema_ref
            );
        }
    }
}

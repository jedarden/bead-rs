//! Semantic conformance tests for `bead schema explain` against the accepted
//! field-guide contract (`research/specs/native-field-guide-v1.md`).
//!
//! `tests/schema_explain_snapshot.rs` pins the rendered bytes and the guide
//! version tripwire; this file pins the *semantics* the contract requires of
//! every explanation, so a drift that keeps the shape but changes meaning
//! still fails here:
//!
//! * required-field semantics — the presence vocabulary, agreement between the
//!   guide's `presence` classification and the published schema's `required`
//!   array for every registry-backed document, the pinned projection presence
//!   table, and the default pairing rule (`has_default == false` means the
//!   published `default` is exactly null, never a phantom value)
//! * stored versus derived/read-only classification — the contract §2
//!   ownership vocabulary is closed, derived fields live only in the
//!   interactive CLI projection and carry only read-only projection
//!   operations, `derived_state` publishes exactly the four documented
//!   projections, and additional-property ownership tracks `allowed`
//! * minimal examples — every field publishes an example consistent with its
//!   published `json_type` (null exactly when the member is nullable) and a
//!   non-empty common mistake
//! * formats — both renderings carry the classification, and rendering is
//!   workspace-independent
//! * rejection — an unsupported schema reference is a usage failure (exit 2)
//!   that echoes the requested identity and writes nothing to stdout

use assert_cmd::Command;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use tempfile::TempDir;

use bead_rs::service::schema::FIELD_GUIDE_VERSION;

/// Canonical native-guide identity: the richest explanation, covering every
/// document kind (both stored registry documents and interactive projections).
const NATIVE_GUIDE_SCHEMA_REF: &str = "urn:bead-rs:schema:issue:native-v1";

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

fn schema_show(schema_ref: &str) -> Value {
    serde_json::from_slice(
        &Command::cargo_bin("bead")
            .unwrap()
            .args(["schema", "show", schema_ref])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone(),
    )
    .unwrap()
}

fn catalog_identities() -> Vec<String> {
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
    catalog
        .iter()
        .map(|entry| entry["schema_ref"].as_str().unwrap().to_string())
        .collect()
}

fn field_key(field: &Value) -> String {
    format!(
        "{}.{}",
        field["document"].as_str().unwrap(),
        field["name"].as_str().unwrap()
    )
}

/// Every explanation the command emits: the native guide plus one concise
/// explanation per catalog identity.
fn all_identities() -> Vec<String> {
    let mut identities = vec![NATIVE_GUIDE_SCHEMA_REF.to_string()];
    identities.extend(catalog_identities());
    identities
}

/// Guide fields whose document is a stored registry document — the
/// checkpoint envelopes of the native guide (the `checkpoint_*` documents;
/// the two interactive projections are computed views, not stored records)
/// and every concise-path document (`member_source` marks the registry
/// provenance). Their members and presence must agree with the published
/// schema, not merely with the guide's own tables.
fn registry_backed_documents(explanation: &Value) -> Vec<&Value> {
    explanation["documents"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|document| {
            let native_stored = document["name"]
                .as_str()
                .unwrap()
                .starts_with("checkpoint_");
            let concise = document["member_source"].as_str().unwrap() == "typed schema registry";
            native_stored || concise
        })
        .collect()
}

/// The presence classification a field of a stored registry document carries
/// must be exactly the published schema's requiredness: `required` if and only
/// if the member is in the published `required` array, `optional` otherwise —
/// and never `conditional`, which is a projection-only concept (a stored
/// document member is either always present or genuinely optional).
fn assert_presence_matches_published_schema(explanation: &Value) {
    for document in registry_backed_documents(explanation) {
        let schema_ref = document["schema_ref"].as_str().unwrap();
        let schema = schema_show(schema_ref);
        let properties: HashSet<&str> = schema["properties"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        let required: HashSet<&str> = schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap())
            .collect();
        assert!(
            required.is_subset(&properties),
            "{schema_ref} publishes a required member with no property schema"
        );

        let members: Vec<&str> = document["members"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap())
            .collect();
        let member_set: HashSet<&str> = members.iter().copied().collect();
        assert_eq!(
            member_set, properties,
            "{schema_ref} guide members must be exactly the published properties"
        );

        for field in explanation["fields"].as_array().unwrap() {
            if field["document"].as_str().unwrap() != document["name"].as_str().unwrap() {
                continue;
            }
            let name = field["name"].as_str().unwrap();
            let presence = field["presence"].as_str().unwrap();
            let expected = if required.contains(name) {
                "required"
            } else {
                "optional"
            };
            assert_eq!(
                presence,
                expected,
                "{}.{name} presence must match the published schema's requiredness",
                document["name"].as_str().unwrap()
            );
        }
    }
}

/// Every field of every catalog identity (concise path) plus the native guide
/// publishes a presence classification from the documented vocabulary, and its
/// default pairing is honest: a field without a default publishes exactly
/// null, never a phantom value a consumer might read as materialized state.
#[test]
fn presence_vocabulary_and_default_pairing_conform() {
    const PRESENCE_VOCABULARY: [&str; 3] = ["required", "optional", "conditional"];

    for schema_ref in all_identities() {
        let explanation = explain_json(&schema_ref);
        let mut seen: usize = 0;
        for field in explanation["fields"].as_array().unwrap() {
            let key = field_key(field);
            let presence = field["presence"].as_str().unwrap();
            assert!(
                PRESENCE_VOCABULARY.contains(&presence),
                "{key} publishes presence `{presence}` outside the documented \
                 vocabulary {PRESENCE_VOCABULARY:?}"
            );
            if !field["has_default"].as_bool().unwrap() {
                assert!(
                    field["default"].is_null(),
                    "{key} publishes default {} while has_default is false — \
                     a phantom default reads as materialized state",
                    field["default"]
                );
            }
            seen += 1;
        }
        assert!(seen > 0, "{schema_ref} publishes no fields");
    }
}

/// Registry-backed documents — every concise-path identity plus the native
/// guide's stored checkpoint envelopes — classify presence exactly as their
/// published JSON Schema `required` arrays do. The guide and `schema show`
/// are two projections of one registry; this is the tie that must not drift.
#[test]
fn guide_presence_matches_published_required_arrays() {
    let native = explain_json(NATIVE_GUIDE_SCHEMA_REF);
    assert!(
        !registry_backed_documents(&native).is_empty(),
        "native guide must cover stored registry documents"
    );
    assert_presence_matches_published_schema(&native);

    for schema_ref in catalog_identities() {
        let explanation = explain_json(&schema_ref);
        assert_presence_matches_published_schema(&explanation);
    }
}

/// The two interactive projections of the native guide publish presence from
/// their own documented tables (contract §3.1/§3.4): the CLI projection always
/// materializes every member except the claim epoch, which appears only when
/// a claim holds; the claim result carries the empty-queue shape where
/// `bead_id` and `lease` are emitted null and `claim_epoch` is skipped.
#[test]
fn projection_presence_tables_are_pinned() {
    let explanation = explain_json(NATIVE_GUIDE_SCHEMA_REF);
    let expected: HashMap<&str, HashMap<&str, &str>> = HashMap::from([
        (
            "cli_issue",
            HashMap::from([
                ("claim_epoch", "conditional"),
                ("assignee", "required"),
                ("attempts", "required"),
                ("comments", "required"),
                ("created_at", "required"),
                ("dependencies", "required"),
                ("description", "required"),
                ("effective_status", "required"),
                ("id", "required"),
                ("labels", "required"),
                ("manual_blocked", "required"),
                ("notes", "required"),
                ("priority", "required"),
                ("revision", "required"),
                ("status", "required"),
                ("title", "required"),
                ("updated_at", "required"),
            ]),
        ),
        (
            "claim_result",
            HashMap::from([
                ("bead_id", "optional"),
                ("lease", "optional"),
                ("claim_epoch", "conditional"),
                ("assignee", "required"),
            ]),
        ),
    ]);
    for (document_name, table) in &expected {
        let document = explanation["documents"]
            .as_array()
            .unwrap()
            .iter()
            .find(|document| document["name"].as_str().unwrap() == *document_name)
            .unwrap_or_else(|| panic!("native guide publishes no {document_name} document"));
        let members: HashSet<&str> = document["members"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap())
            .collect();
        assert_eq!(
            members,
            table.keys().copied().collect(),
            "{document_name} members drifted from the pinned projection table"
        );
        for field in explanation["fields"].as_array().unwrap() {
            if field["document"].as_str().unwrap() != *document_name {
                continue;
            }
            let name = field["name"].as_str().unwrap();
            assert_eq!(
                field["presence"].as_str().unwrap(),
                table[name],
                "{document_name}.{name} presence drifted from the pinned table"
            );
        }
    }
}

/// The ownership vocabulary is closed (contract §2): caller, system, derived,
/// preserved. Derived fields are computed views, never stored state, so they
/// may exist only in the interactive CLI projection document and may carry
/// only the read-only projection operations. `derived_state` publishes exactly
/// the four documented projections, each derived with documented rules in the
/// native guide. Additional-property ownership tracks `allowed` exactly.
#[test]
fn ownership_classification_matches_the_contract_vocabulary() {
    const OWNERSHIP_VOCABULARY: [&str; 4] = ["caller", "system", "derived", "preserved"];
    /// Read-only projection commands: the only operations a derived field may
    /// name, because a computed view is never written by any command.
    const READ_ONLY_PROJECTION_OPERATIONS: [&str; 2] = ["list", "show"];
    const DERIVED_STATE_PROJECTIONS: [&str; 4] = ["status", "ready", "blocked_by", "blocking"];

    let native = explain_json(NATIVE_GUIDE_SCHEMA_REF);
    let concise = explain_json("urn:bead-rs:schema:checkpoint-pointer:native-v1");
    for explanation in [&native, &concise] {
        for field in explanation["fields"].as_array().unwrap() {
            let key = field_key(field);
            let ownership = field["ownership"].as_str().unwrap();
            assert!(
                OWNERSHIP_VOCABULARY.contains(&ownership),
                "{key} publishes ownership `{ownership}` outside the contract §2 \
                 vocabulary {OWNERSHIP_VOCABULARY:?}"
            );
            if ownership == "derived" {
                assert_eq!(
                    field["document"].as_str().unwrap(),
                    "cli_issue",
                    "{key} is derived yet lives outside the interactive CLI \
                     projection — derived state is never stored, and a stored \
                     document must not publish it as a member"
                );
                for operation in field["operations"].as_array().unwrap() {
                    let operation = operation.as_str().unwrap();
                    assert!(
                        READ_ONLY_PROJECTION_OPERATIONS.contains(&operation),
                        "{key} is derived yet names operation `{operation}` — a \
                         computed view must carry only the read-only projection \
                         operations {READ_ONLY_PROJECTION_OPERATIONS:?}"
                    );
                }
            }
        }

        let derived_state = explanation["derived_state"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<HashSet<_>>();
        assert_eq!(
            derived_state,
            DERIVED_STATE_PROJECTIONS.into_iter().collect(),
            "derived_state must publish exactly the documented projections"
        );
    }

    let derived_rules: HashMap<String, &Value> = native["derived_state"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(name, entry)| (name.clone(), &entry["ownership"]))
        .collect();
    for name in DERIVED_STATE_PROJECTIONS {
        assert_eq!(
            derived_rules[name], "derived",
            "native guide derived_state.{name} must be classified derived"
        );
    }
    for (name, entry) in native["derived_state"].as_object().unwrap() {
        assert!(
            !entry["rules"].as_array().unwrap().is_empty(),
            "native guide derived_state.{name} publishes no derivation rules"
        );
    }

    for schema_ref in all_identities() {
        let explanation = explain_json(&schema_ref);
        let additional = &explanation["additional_properties"];
        let ownership = additional["ownership"].as_str().unwrap();
        assert!(
            matches!(ownership, "preserved" | "rejected"),
            "additional-properties ownership `{ownership}` is outside the \
             documented classification"
        );
        assert_eq!(
            additional["allowed"].as_bool().unwrap(),
            ownership == "preserved",
            "additional-properties allowed must track its ownership exactly"
        );
        assert!(
            !additional["rules"].as_array().unwrap().is_empty(),
            "additional-properties publishes no rules"
        );
    }
}

/// Every field publishes a minimal worked example — a value whose JSON type is
/// the published `json_type`, exactly null when (and only when) the member is
/// nullable, or, for array-typed members, a single entry object (contract §4
/// publishes entry examples for `dependencies` and `external_references`) —
/// plus a non-empty common mistake (contract §4: every field supplies example,
/// common mistake, type, presence, default, ownership, operations, and
/// invariants).
#[test]
fn field_examples_are_minimal_and_type_consistent() {
    fn example_matches_type(example: &Value, json_type: &str, nullable: bool) -> bool {
        if example.is_null() {
            return nullable;
        }
        match json_type {
            "string" => example.is_string(),
            "integer" => example.is_i64() || example.is_u64(),
            "number" => example.is_number(),
            "boolean" => example.is_boolean(),
            // A collection member may demonstrate one entry instead of a
            // wrapping array — the contract's own examples do.
            "array" => example.is_array() || example.is_object(),
            "object" => example.is_object(),
            // The fallback type for members whose published shape is any JSON
            // value: no constraint beyond nullability.
            "json" => true,
            other => panic!("field publishes unrecognized json_type `{other}`"),
        }
    }

    for schema_ref in all_identities() {
        let explanation = explain_json(&schema_ref);
        for field in explanation["fields"].as_array().unwrap() {
            let key = field_key(field);
            let example = &field["example"];
            assert!(
                example_matches_type(
                    example,
                    field["json_type"].as_str().unwrap(),
                    field["nullable"].as_bool().unwrap()
                ),
                "{key} example {example} does not match its published type `{}` \
                 (nullable: {})",
                field["json_type"],
                field["nullable"]
            );
            assert!(
                !field["common_mistake"].as_str().unwrap().trim().is_empty(),
                "{key} publishes an empty common mistake"
            );
        }
    }
}

/// Both renderings carry the classification, and rendering does not depend on
/// the working directory: the command is a pure projection of the compiled
/// registry, so it succeeds with byte-identical output from a bare directory
/// that is not a workspace at all.
#[test]
fn explain_is_workspace_independent_and_both_formats_carry_the_classification() {
    let first = TempDir::new().unwrap();
    let second = TempDir::new().unwrap();

    for format in ["json", "markdown"] {
        let mut outputs = Vec::new();
        for dir in [&first, &second] {
            let output = Command::cargo_bin("bead")
                .unwrap()
                .args([
                    "schema",
                    "explain",
                    NATIVE_GUIDE_SCHEMA_REF,
                    "--format",
                    format,
                ])
                .current_dir(dir.path())
                .assert()
                .success()
                .get_output()
                .clone();
            assert!(
                output.stderr.is_empty(),
                "{format} rendering must not warn outside a workspace"
            );
            assert!(!output.stdout.is_empty());
            outputs.push(output.stdout);
        }
        assert_eq!(
            outputs[0], outputs[1],
            "{format} rendering must not depend on the working directory"
        );
    }

    let explanation: Value =
        serde_json::from_slice(&explain(NATIVE_GUIDE_SCHEMA_REF, "json")).unwrap();
    assert_eq!(explanation["guide_version"], FIELD_GUIDE_VERSION);
    let markdown = String::from_utf8(explain(NATIVE_GUIDE_SCHEMA_REF, "markdown")).unwrap();

    // The Fields section renders one Presence and one Ownership line per
    // field. Ownership is additionally rendered once per derived_state
    // projection and once for the additional-properties classification, so
    // the expected markdown count per value is the typed count of all three
    // sections combined — a field (or projection) whose classification fails
    // to render, or renders twice, breaks the count instead of silently
    // diverging between formats.
    let mut expected_counts: HashMap<&str, HashMap<String, usize>> =
        HashMap::from([("presence", HashMap::new()), ("ownership", HashMap::new())]);
    for field in explanation["fields"].as_array().unwrap() {
        for member in ["presence", "ownership"] {
            *expected_counts
                .get_mut(member)
                .unwrap()
                .entry(field[member].as_str().unwrap().to_string())
                .or_default() += 1;
        }
    }
    for entry in explanation["derived_state"].as_object().unwrap().values() {
        *expected_counts
            .get_mut("ownership")
            .unwrap()
            .entry(entry["ownership"].as_str().unwrap().to_string())
            .or_default() += 1;
    }
    *expected_counts
        .get_mut("ownership")
        .unwrap()
        .entry(
            explanation["additional_properties"]["ownership"]
                .as_str()
                .unwrap()
                .to_string(),
        )
        .or_default() += 1;

    for (member, counts) in &expected_counts {
        let label = match *member {
            "presence" => "Presence",
            "ownership" => "Ownership",
            other => unreachable!("{other}"),
        };
        for (value, expected) in counts {
            let line = format!("- {label}: `{value}`");
            let rendered = markdown.matches(&line).count();
            assert_eq!(
                rendered, *expected,
                "markdown renders the {member} classification `{value}` {rendered} \
                 times but the typed JSON publishes it {expected} times"
            );
        }
    }
}

/// An unsupported schema reference is a usage failure (contract §1): exit 2,
/// the diagnostic names the exact identity that was requested, and nothing is
/// written to stdout — a consumer piping stdout must never receive a partial
/// document for a ref the registry does not publish.
#[test]
fn unsupported_schema_reference_is_a_usage_failure_that_echoes_the_identity() {
    let unsupported = "urn:bead-rs:schema:issue:native-v2";
    let assertion = Command::cargo_bin("bead")
        .unwrap()
        .args(["schema", "explain", unsupported, "--format", "json"])
        .assert()
        .code(2);
    let output = assertion.get_output();
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Unsupported schema identity"),
        "diagnostic must classify the failure: {stderr}"
    );
    assert!(
        stderr.contains(unsupported),
        "diagnostic must echo the requested identity: {stderr}"
    );
}

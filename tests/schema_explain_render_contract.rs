//! Rendering-contract tests for `bead schema explain`, completing the ADR-002
//! pin that `tests/schema_explain_snapshot.rs` and
//! `tests/schema_explain_conformance.rs` leave open.
//!
//! The snapshot file pins the rendered *bytes* of two identities and the guide
//! version tripwire; the conformance file pins the *semantics* of the JSON
//! contract plus the rendered Presence/Ownership *counts*. Three properties of
//! the machine-contract↔render pairing are pinned nowhere:
//!
//! * per-field completeness — every member of every field entry in the JSON
//!   contract (type, nullability, presence, ownership, default, operations,
//!   invariants, example, common mistake) must appear, on that field's own
//!   rendered entry, in the Markdown. The renderer falls back to `unknown` /
//!   empty strings on a malformed member, and renderer-parity plus byte
//!   snapshots hold even when the renderer silently drops a line, so only a
//!   contract-to-render check per field catches the omission — on every
//!   catalog identity, not just the two snapshot identities.
//! * structural completeness — every document, lifecycle value, derived-state
//!   projection, event member, operation, rehydration member, and known
//!   deviation the JSON publishes must be rendered in its section, and the
//!   Markdown header must bind the same guide version and schema identity the
//!   JSON contract carries (stable versioning, per identity, in both formats).
//! * whole-catalog determinism — repeated invocations render byte-identically
//!   for *every* identity in both formats. The snapshot digests prove this for
//!   two identities; a registry-ordering flake on any other identity would
//!   flap unconstrained without this sweep.

use assert_cmd::Command;
use serde_json::Value;

use bead_rs::service::schema::FIELD_GUIDE_VERSION;

/// Canonical native-guide identity: the richest explanation, carrying every
/// section the renderer knows how to render.
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

fn explain_markdown(schema_ref: &str) -> String {
    String::from_utf8(explain(schema_ref, "markdown")).expect("markdown output is UTF-8")
}

/// Every explanation the command emits: the native guide plus one concise
/// explanation per catalog identity.
fn all_identities() -> Vec<String> {
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
    assert!(
        !catalog.is_empty(),
        "schema catalog must publish identities to explain"
    );
    let mut identities = vec![NATIVE_GUIDE_SCHEMA_REF.to_string()];
    identities.extend(
        catalog
            .iter()
            .map(|entry| entry["schema_ref"].as_str().unwrap().to_string()),
    );
    identities
}

/// The body of one `## {header}` section: everything between that section's
/// heading and the next `## ` heading (or the end of the document).
fn section<'a>(markdown: &'a str, header: &str) -> &'a str {
    let marker = format!("\n## {header}\n\n");
    let start = markdown
        .find(&marker)
        .unwrap_or_else(|| panic!("markdown must carry a `## {header}` section"));
    let body_start = start + marker.len();
    let end = markdown[body_start..]
        .find("\n## ")
        .map_or(markdown.len(), |offset| body_start + offset);
    &markdown[body_start..end]
}

/// The `### `-delimited blocks of a section body: block i runs from its
/// heading to the next heading, and the first block starts at the body's
/// first character. Guide prose (invariants, common mistakes, rules) is
/// single-line, so a `### ` inside a block body is not a thing.
fn heading_blocks(body: &str) -> Vec<&str> {
    let mut starts: Vec<usize> = vec![0];
    let mut search = 0;
    while let Some(offset) = body[search..].find("\n### ") {
        starts.push(search + offset + 1);
        search += offset + 1;
    }
    starts
        .iter()
        .enumerate()
        .map(|(index, start)| {
            let end = starts.get(index + 1).copied().unwrap_or(body.len());
            &body[*start..end]
        })
        .collect()
}

fn field_key(field: &Value) -> String {
    format!(
        "{}.{}",
        field["document"].as_str().unwrap(),
        field["name"].as_str().unwrap()
    )
}

/// The exact Markdown lines the renderer must emit for one field entry,
/// derived from that field's own JSON contract members. The renderer falls
/// back to `unknown`/empty on a malformed member, so these are constructed
/// from the JSON — a fallback rendering cannot satisfy them.
fn expected_field_lines(field: &Value) -> Vec<String> {
    let mut lines = vec![format!(
        "- Type: `{}`{}",
        field["json_type"].as_str().unwrap(),
        if field["nullable"].as_bool().unwrap() {
            " (nullable)"
        } else {
            ""
        }
    )];
    lines.push(format!(
        "- Presence: `{}`",
        field["presence"].as_str().unwrap()
    ));
    lines.push(format!(
        "- Ownership: `{}`",
        field["ownership"].as_str().unwrap()
    ));
    if field["has_default"].as_bool().unwrap() {
        lines.push(format!("- Default: `{}`", field["default"]));
    } else {
        lines.push("- Default: none".to_string());
    }
    let operations: Vec<String> = field["operations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|operation| format!("`{}`", operation.as_str().unwrap()))
        .collect();
    lines.push(format!("- Operations: {}", operations.join(", ")));
    for invariant in field["invariants"].as_array().unwrap() {
        lines.push(format!("- Invariant: {}", invariant.as_str().unwrap()));
    }
    lines.push(format!("- Example: `{}`", field["example"]));
    lines.push(format!(
        "- Common mistake: {}",
        field["common_mistake"].as_str().unwrap()
    ));
    lines
}

/// Every field of every identity renders its complete metadata on its own
/// entry: the heading, then every contract member as the exact line the
/// renderer documents. No field may render on a block the JSON does not
/// publish, and no field may render twice.
#[test]
fn every_field_renders_its_full_contract_metadata_on_every_identity() {
    for schema_ref in all_identities() {
        let explanation = explain_json(&schema_ref);
        let markdown = explain_markdown(&schema_ref);
        let fields_section = section(&markdown, "Fields");
        let blocks = heading_blocks(fields_section);
        let fields = explanation["fields"].as_array().unwrap();

        assert_eq!(
            blocks.len(),
            fields.len(),
            "{schema_ref} renders {} field entries for {} published fields — \
             the rendering must carry exactly the machine contract's fields",
            blocks.len(),
            fields.len()
        );

        for field in fields {
            let key = field_key(field);
            let heading = format!("### {key}\n\n");
            let block = blocks
                .iter()
                .find(|block| block.starts_with(&heading))
                .unwrap_or_else(|| panic!("{schema_ref} renders no field entry headed `{key}`"));
            for line in expected_field_lines(field) {
                assert!(
                    block.contains(&line),
                    "{schema_ref} field {key} renders no line `{line}` — the \
                     Markdown must carry every member the JSON contract \
                     publishes for the field"
                );
            }
        }
    }
}

/// Every section the JSON contract publishes is rendered in full: documents
/// with their members, the header binding of guide version and schema
/// identity, additional properties, lifecycle values, derived-state
/// projections, event members, operations with their exit/field/rule detail,
/// rehydration members, and known implementation deviations.
#[test]
fn every_published_section_renders_in_full_on_every_identity() {
    for schema_ref in all_identities() {
        let explanation = explain_json(&schema_ref);
        let markdown = explain_markdown(&schema_ref);

        // Stable versioning, bound across formats: the Markdown header names
        // the same guide version and schema identity the JSON contract carries
        // (the published guide identity; the requested ref rides in
        // `describes_schema_refs`, pinned by the snapshot file).
        let header = format!(
            "# Native field guide v{FIELD_GUIDE_VERSION}\n\nSchema: `{}`\n\n",
            explanation["schema_ref"].as_str().unwrap()
        );
        assert!(
            markdown.starts_with(&header),
            "{schema_ref} markdown must open by binding guide v{FIELD_GUIDE_VERSION} \
             to the identity the JSON contract carries"
        );
        assert_eq!(
            explanation["guide_version"].as_i64().unwrap(),
            FIELD_GUIDE_VERSION,
            "{schema_ref} JSON contract must carry the compiled guide version"
        );

        // Documents: every document renders its heading, schema identity, and
        // exactly its member list.
        let documents_section = section(&markdown, "Documents");
        for document in explanation["documents"].as_array().unwrap() {
            let name = document["name"].as_str().unwrap();
            let heading = format!(
                "### {name}\n\nSchema: `{}`\n\nMembers:\n\n",
                document["schema_ref"].as_str().unwrap()
            );
            let block = documents_section
                .split(&heading)
                .nth(1)
                .unwrap_or_else(|| panic!("{schema_ref} renders no `{name}` document"));
            let members: Vec<String> = document["members"]
                .as_array()
                .unwrap()
                .iter()
                .map(|member| format!("- `{}`\n", member.as_str().unwrap()))
                .collect();
            let rendered_members = block
                .split("\n\n")
                .next()
                .unwrap()
                .lines()
                .filter(|line| line.starts_with("- `"))
                .count();
            assert_eq!(
                rendered_members,
                members.len(),
                "{schema_ref} document {name} must render exactly its published \
                 member list"
            );
            for member in members {
                assert!(
                    block.contains(&member),
                    "{schema_ref} document {name} renders no member line {member}"
                );
            }
        }

        // Additional properties: allowed flag, ownership, and every rule.
        let additional = &explanation["additional_properties"];
        let additional_section = section(&markdown, "Additional properties");
        let expected_additional = format!(
            "- Allowed: {}\n- Ownership: `{}`\n",
            additional["allowed"].as_bool().unwrap(),
            additional["ownership"].as_str().unwrap()
        );
        assert!(
            additional_section.starts_with(&expected_additional),
            "{schema_ref} additional-properties rendering must open with the \
             published allowed/ownership classification"
        );
        for rule in additional["rules"].as_array().unwrap() {
            assert!(
                additional_section.contains(&format!("- {}\n", rule.as_str().unwrap())),
                "{schema_ref} renders no additional-properties rule `{}`",
                rule.as_str().unwrap()
            );
        }

        // Lifecycle: every base value and every allowed transition, backticked.
        let lifecycle = &explanation["lifecycle"];
        let lifecycle_section = section(&markdown, "Lifecycle");
        for (label, member) in [
            ("- Base values:", "base_values"),
            ("- Allowed transitions:", "allowed_transitions"),
        ] {
            let mut expected = label.to_string();
            for value in lifecycle[member].as_array().unwrap() {
                expected.push_str(&format!(" `{}`", value.as_str().unwrap()));
            }
            assert!(
                lifecycle_section.contains(&expected),
                "{schema_ref} lifecycle must render {label} exactly as the JSON \
                 publishes it"
            );
        }

        // Derived state: every projection renders its ownership and every rule.
        let derived_section = section(&markdown, "Derived state");
        for (name, entry) in explanation["derived_state"].as_object().unwrap() {
            let block = derived_section
                .split(&format!("### {name}\n\n"))
                .nth(1)
                .unwrap_or_else(|| panic!("{schema_ref} renders no `{name}` projection"));
            assert!(
                block.contains(&format!(
                    "- Ownership: `{}`\n",
                    entry["ownership"].as_str().unwrap()
                )),
                "{schema_ref} projection {name} must render its ownership"
            );
            for rule in entry["rules"].as_array().unwrap() {
                assert!(
                    block.contains(&format!("- {}\n", rule.as_str().unwrap())),
                    "{schema_ref} projection {name} renders no rule `{}`",
                    rule.as_str().unwrap()
                );
            }
        }

        // Events: envelope member, schema-reference member, identity, ordering.
        let events = &explanation["events"];
        let mut expected_events = format!(
            "- Envelope member: `{}`\n- Schema reference member: `{}`\n- Identity:",
            events["envelope_member"].as_str().unwrap(),
            events["schema_ref_member"].as_str().unwrap()
        );
        for member in events["identity"].as_array().unwrap() {
            expected_events.push_str(&format!(" `{}`", member.as_str().unwrap()));
        }
        expected_events.push_str("\n- Ordering:");
        for member in events["ordering"].as_array().unwrap() {
            expected_events.push_str(&format!(" `{}`", member.as_str().unwrap()));
        }
        assert!(
            section(&markdown, "Events").starts_with(&expected_events),
            "{schema_ref} events rendering must carry the published envelope, \
             schema-reference, identity, and ordering members"
        );

        // Operations: every operation renders, with its ownership effect,
        // success exit, failure exits, affected fields, and every rule.
        let operations_section = section(&markdown, "Operations");
        let operations = explanation["operations"].as_array().unwrap();
        let operation_blocks = heading_blocks(operations_section);
        assert_eq!(
            operation_blocks.len(),
            operations.len(),
            "{schema_ref} must render exactly its published operations"
        );
        for operation in operations {
            let name = operation["name"].as_str().unwrap();
            let mut expected = format!(
                "### {name}\n\n- Ownership effect: {}\n- Success exit: `{}`\n- Failure exits:",
                operation["ownership_effect"].as_str().unwrap(),
                operation["success_exit"].as_i64().unwrap()
            );
            for exit in operation["failure_exits"].as_array().unwrap() {
                expected.push_str(&format!(" `{}`", exit));
            }
            expected.push_str("\n- Affected fields:");
            for field in operation["affected_fields"].as_array().unwrap() {
                expected.push_str(&format!(" `{}`", field.as_str().unwrap()));
            }
            expected.push_str("\n\nRules:\n\n");
            for rule in operation["rules"].as_array().unwrap() {
                expected.push_str(&format!("- {}\n", rule.as_str().unwrap()));
            }
            assert!(
                operation_blocks
                    .iter()
                    .any(|block| block.starts_with(&expected)),
                "{schema_ref} renders no `{name}` operation with its complete \
                 published detail (effect, exits, affected fields, rules)"
            );
        }

        // Rehydration: source mode plus every allowed/forbidden/verification
        // member, byte-for-byte.
        let rehydration = &explanation["rehydration"];
        let mut expected_rehydration = format!(
            "- Source mode: `{}`\n",
            rehydration["source_mode"].as_str().unwrap()
        );
        for (label, member) in [
            ("Allowed writes", "allowed_writes"),
            ("Forbidden writes", "forbidden_writes"),
            ("Verification", "verification"),
        ] {
            expected_rehydration.push_str(&format!("- {label}:"));
            for value in rehydration[member].as_array().unwrap() {
                expected_rehydration.push_str(&format!(" `{}`", value.as_str().unwrap()));
            }
            expected_rehydration.push('\n');
        }
        assert_eq!(
            section(&markdown, "Rehydration"),
            expected_rehydration,
            "{schema_ref} rehydration rendering must carry exactly the published \
             source mode, allowed writes, forbidden writes, and verification"
        );

        // Known deviations: every deviation renders id, severity, behavior,
        // and required disposition.
        let deviations_section = section(&markdown, "Known implementation deviations");
        for deviation in explanation["known_implementation_deviations"]
            .as_array()
            .unwrap()
        {
            let expected = format!(
                "### {}\n\n- Severity: `{}`\n- Behavior: {}\n- Required disposition: {}\n\n",
                deviation["id"].as_str().unwrap(),
                deviation["severity"].as_str().unwrap(),
                deviation["behavior"].as_str().unwrap(),
                deviation["required_disposition"].as_str().unwrap()
            );
            assert!(
                deviations_section.contains(&expected),
                "{schema_ref} renders no deviation `{}` with its severity, \
                 behavior, and required disposition",
                deviation["id"].as_str().unwrap()
            );
        }
    }
}

/// Repeated invocations render byte-identically for every identity in both
/// formats — the whole-catalog complement of the snapshot file's
/// digest-pinned two-identity determinism.
#[test]
fn every_identity_renders_deterministically_across_repeated_invocations() {
    const RUNS: usize = 2;
    for schema_ref in all_identities() {
        for format in ["json", "markdown"] {
            let first = explain(&schema_ref, format);
            assert!(!first.is_empty(), "{schema_ref} {format} must not be empty");
            for _ in 1..RUNS {
                let again = explain(&schema_ref, format);
                assert_eq!(
                    first, again,
                    "{schema_ref} {format} rendering varied across repeated \
                     invocations — the explanation is a pure projection of the \
                     compiled registry and must be deterministic"
                );
            }
        }
    }
}

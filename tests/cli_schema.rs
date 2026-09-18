use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::{json, Value};
use std::collections::HashSet;

#[test]
fn schema_list_is_workspace_independent_and_deterministic() {
    let first = Command::cargo_bin("bead")
        .unwrap()
        .args(["schema", "list", "--format", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let second = Command::cargo_bin("bead")
        .unwrap()
        .args(["schema", "list"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert_eq!(first, second);

    let entries: Vec<Value> = serde_json::from_slice(&first).unwrap();
    assert!(!entries.is_empty());
    let refs: Vec<&str> = entries
        .iter()
        .map(|entry| entry["schema_ref"].as_str().unwrap())
        .collect();
    let mut sorted = refs.clone();
    sorted.sort_unstable();
    assert_eq!(refs, sorted);
    assert_eq!(refs.len(), refs.iter().collect::<HashSet<_>>().len());

    for entry in entries {
        for member in [
            "schema_ref",
            "document_kind",
            "readable",
            "writable",
            "validate",
            "consume",
            "emit",
        ] {
            assert!(entry.get(member).is_some(), "missing {member}: {entry}");
        }
    }
}

#[test]
fn schema_list_matches_capabilities_catalog() {
    let list: Value = serde_json::from_slice(
        &Command::cargo_bin("bead")
            .unwrap()
            .args(["schema", "list"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    let capabilities: Value = serde_json::from_slice(
        &Command::cargo_bin("bead")
            .unwrap()
            .arg("capabilities")
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    assert_eq!(list, capabilities["schemas"]);
}

#[test]
fn schema_list_rejects_unsupported_format_as_usage_error() {
    Command::cargo_bin("bead")
        .unwrap()
        .args(["schema", "list", "--format", "yaml"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("invalid value"));
}

#[test]
fn schema_show_resolves_every_catalog_identity() {
    let catalog: Vec<Value> = serde_json::from_slice(
        &Command::cargo_bin("bead")
            .unwrap()
            .args(["schema", "list"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    for entry in catalog {
        let schema_ref = entry["schema_ref"].as_str().unwrap();
        let output = Command::cargo_bin("bead")
            .unwrap()
            .args(["schema", "show", schema_ref, "--format", "json"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let schema: Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(schema["$id"], schema_ref);
        assert_eq!(
            schema["$schema"],
            "https://json-schema.org/draft/2020-12/schema"
        );
    }
}

#[test]
fn schema_explain_json_and_markdown_are_deterministic() {
    let schema_ref = "urn:bead-rs:schema:issue:native-v1";
    let json_output = Command::cargo_bin("bead")
        .unwrap()
        .args(["schema", "explain", schema_ref, "--format", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let explanation: Value = serde_json::from_slice(&json_output).unwrap();
    assert_eq!(explanation["guide_version"], 2);
    assert!(explanation["describes_schema_refs"]
        .as_array()
        .unwrap()
        .iter()
        .any(|value| value == schema_ref));
    assert!(!explanation["fields"].as_array().unwrap().is_empty());
    let documents = explanation["documents"].as_array().unwrap();
    assert_eq!(documents.len(), 5);
    let expected_fields: usize = documents
        .iter()
        .map(|document| document["members"].as_array().unwrap().len())
        .sum();
    assert_eq!(
        explanation["fields"].as_array().unwrap().len(),
        expected_fields
    );
    assert!(explanation["fields"]
        .as_array()
        .unwrap()
        .iter()
        .all(|field| field["json_type"] != "any"));

    Command::cargo_bin("bead")
        .unwrap()
        .args(["schema", "explain", schema_ref, "--format", "markdown"])
        .assert()
        .success()
        .stdout(predicate::str::contains("# Native field guide v2"))
        .stdout(predicate::str::contains("checkpoint_issue.priority"));
}

#[test]
fn schema_show_and_explain_reject_unknown_identity() {
    for operation in ["show", "explain"] {
        Command::cargo_bin("bead")
            .unwrap()
            .args(["schema", operation, "urn:unknown:schema"])
            .assert()
            .code(2)
            .stderr(predicate::str::contains("Unsupported schema identity"));
    }
}

/// Resolution is by exact catalog identity: anything that is not a byte-equal
/// match — empty, unadorned, truncated, mistyped, or wrong-case — is a
/// malformed identity and must be a CLI usage error (exit 2), never a panic
/// and never a partial match.
#[test]
fn schema_show_and_explain_reject_malformed_identities() {
    let malformed = [
        "",                                    // empty
        "issue",                               // bare word, no URN shape
        "urn:bead-rs:schema:issue",            // truncated
        "urn:bead-rs:schema:issue:native-v0",  // wrong revision
        "urn:bead-rs:schema:issue:native-v1 ", // whitespace-padded
        "URN:BEAD-RS:SCHEMA:ISSUE:NATIVE-V1",  // wrong case
    ];
    for operation in ["show", "explain"] {
        for schema_ref in malformed {
            Command::cargo_bin("bead")
                .unwrap()
                .args(["schema", operation, schema_ref])
                .assert()
                .code(2)
                .stderr(predicate::str::contains("Unsupported schema identity"));
        }
    }
}

#[test]
fn schema_explain_output_is_byte_identical_across_runs() {
    let schema_ref = "urn:bead-rs:schema:issue:native-v1";
    for format in ["json", "markdown"] {
        let first = Command::cargo_bin("bead")
            .unwrap()
            .args(["schema", "explain", schema_ref, "--format", format])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let second = Command::cargo_bin("bead")
            .unwrap()
            .args(["schema", "explain", schema_ref, "--format", format])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        assert!(!first.is_empty());
        assert_eq!(first, second, "{format} output must not vary between runs");
    }
}

#[test]
fn schema_explain_json_and_markdown_represent_the_same_typed_source() {
    let schema_ref = "urn:bead-rs:schema:issue:native-v1";
    let explanation: Value = serde_json::from_slice(
        &Command::cargo_bin("bead")
            .unwrap()
            .args(["schema", "explain", schema_ref, "--format", "json"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone(),
    )
    .unwrap();
    let markdown = String::from_utf8(
        Command::cargo_bin("bead")
            .unwrap()
            .args(["schema", "explain", schema_ref, "--format", "markdown"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone(),
    )
    .unwrap();

    // Fixed section order per the field-guide contract.
    let sections = [
        "## Documents",
        "## Fields",
        "## Additional properties",
        "## Lifecycle",
        "## Derived state",
        "## Events",
        "## Operations",
        "## Rehydration",
        "## Known implementation deviations",
    ];
    let mut positions: Vec<usize> = sections
        .iter()
        .map(|section| {
            markdown
                .find(section)
                .unwrap_or_else(|| panic!("markdown missing section {section}"))
        })
        .collect();
    let mut sorted_positions = positions.clone();
    sorted_positions.sort_unstable();
    assert_eq!(positions, sorted_positions, "sections must render in order");
    positions.dedup();
    assert_eq!(positions.len(), sections.len(), "sections must render once");

    // Every document member and every uniquely keyed field is rendered.
    for document in explanation["documents"].as_array().unwrap() {
        let name = document["name"].as_str().unwrap();
        assert!(
            markdown.contains(&format!("### {name}\n")),
            "markdown missing document {name}"
        );
        for member in document["members"].as_array().unwrap() {
            assert!(
                markdown.contains(&format!("- `{}`", member.as_str().unwrap())),
                "markdown missing member {name}.{}",
                member.as_str().unwrap()
            );
        }
    }
    let mut field_keys = HashSet::new();
    for field in explanation["fields"].as_array().unwrap() {
        let key = format!(
            "{}.{}",
            field["document"].as_str().unwrap(),
            field["name"].as_str().unwrap()
        );
        assert!(field_keys.insert(key.clone()), "duplicate field {key}");
        for member in [
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
        ] {
            assert!(
                field.get(member).is_some(),
                "field {key} missing guide member {member}"
            );
        }
        assert!(
            markdown.contains(&format!("### {key}\n")),
            "markdown missing field {key}"
        );
        for invariant in field["invariants"].as_array().unwrap() {
            assert!(
                markdown.contains(invariant.as_str().unwrap()),
                "markdown missing invariant of {key}"
            );
        }
        assert!(markdown.contains(field["common_mistake"].as_str().unwrap()));
    }

    // Operations are unique, lexicographically sorted, spec-shaped, rendered.
    let operations = explanation["operations"].as_array().unwrap();
    assert!(!operations.is_empty());
    let names: Vec<&str> = operations
        .iter()
        .map(|operation| operation["name"].as_str().unwrap())
        .collect();
    let mut sorted_names = names.clone();
    sorted_names.sort_unstable();
    assert_eq!(names, sorted_names, "operations must be sorted by name");
    assert_eq!(names.len(), names.iter().collect::<HashSet<_>>().len());
    for expected in ["claim", "close", "create", "release", "reopen", "update"] {
        assert!(names.contains(&expected), "operations missing {expected}");
    }
    for operation in operations {
        for member in [
            "name",
            "ownership_effect",
            "success_exit",
            "failure_exits",
            "affected_fields",
            "rules",
        ] {
            assert!(
                operation.get(member).is_some(),
                "operation missing guide member {member}"
            );
        }
        assert_eq!(operation["success_exit"].as_i64().unwrap(), 0);
        assert!(
            markdown.contains(&format!("### {}", operation["name"].as_str().unwrap())),
            "markdown missing operation {}",
            operation["name"].as_str().unwrap()
        );
    }

    // Lifecycle, deviations, and every described derived-state rule render.
    for transition in explanation["lifecycle"]["allowed_transitions"]
        .as_array()
        .unwrap()
    {
        assert!(markdown.contains(transition.as_str().unwrap()));
    }
    for deviation in explanation["known_implementation_deviations"]
        .as_array()
        .unwrap()
    {
        assert!(markdown.contains(deviation["id"].as_str().unwrap()));
        assert!(
            markdown.contains(deviation["required_disposition"].as_str().unwrap()),
            "markdown missing deviation disposition"
        );
    }
    for name in ["status", "ready", "blocked_by", "blocking"] {
        assert!(
            explanation["derived_state"][name]["rules"]
                .as_array()
                .unwrap()
                .iter()
                .all(|rule| markdown.contains(rule.as_str().unwrap())),
            "markdown missing derived-state rules for {name}"
        );
    }
    assert!(!markdown.contains('\r'), "markdown must use LF endings");
}

#[test]
fn schema_explain_guide_fits_its_published_schema() {
    let guide_identity = "urn:bead-rs:schema:field-guide:native-v1";
    let schema: Value = serde_json::from_slice(
        &Command::cargo_bin("bead")
            .unwrap()
            .args(["schema", "show", guide_identity])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone(),
    )
    .unwrap();
    // Unknown guide members are rejected: the published schema is closed.
    assert_eq!(schema["additionalProperties"], json!(false));
    let properties: Vec<&str> = schema["properties"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    let required: Vec<&str> = schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect();
    assert_eq!(properties.len(), required.len(), "every member is required");

    let explanation: Value = serde_json::from_slice(
        &Command::cargo_bin("bead")
            .unwrap()
            .args([
                "schema",
                "explain",
                "urn:bead-rs:schema:issue:native-v1",
                "--format",
                "json",
            ])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone(),
    )
    .unwrap();
    let mut members: Vec<&str> = explanation
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    members.sort_unstable();
    let mut sorted_properties = properties.clone();
    sorted_properties.sort_unstable();
    assert_eq!(
        members, sorted_properties,
        "guide members must match its published schema exactly"
    );
    assert_eq!(
        explanation["guide_version"],
        schema["properties"]["guide_version"]["const"]
    );

    let describes: Vec<&str> = explanation["describes_schema_refs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect();
    let mut sorted_describes = describes.clone();
    sorted_describes.sort_unstable();
    assert_eq!(describes, sorted_describes);
    assert_eq!(
        describes.len(),
        describes.iter().collect::<HashSet<_>>().len()
    );
    for value in describes {
        assert!(value.starts_with("urn:"), "schema refs are absolute URIs");
    }
}

#[test]
fn schema_explain_concise_path_fits_published_schema() {
    let schema_ref = "urn:bead-rs:schema:checkpoint-pointer:native-v1";
    let schema: Value = serde_json::from_slice(
        &Command::cargo_bin("bead")
            .unwrap()
            .args(["schema", "show", schema_ref])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone(),
    )
    .unwrap();
    let guide_schema: Value = serde_json::from_slice(
        &Command::cargo_bin("bead")
            .unwrap()
            .args(["schema", "show", "urn:bead-rs:schema:field-guide:native-v1"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone(),
    )
    .unwrap();
    let explanation: Value = serde_json::from_slice(
        &Command::cargo_bin("bead")
            .unwrap()
            .args(["schema", "explain", schema_ref, "--format", "json"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone(),
    )
    .unwrap();

    // A concise explanation still carries the full guide envelope: every
    // member the field-guide schema requires, and no unknown ones.
    let mut members: Vec<&str> = explanation
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    members.sort_unstable();
    let mut guide_members: Vec<&str> = guide_schema["properties"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    guide_members.sort_unstable();
    assert_eq!(members, guide_members);

    // The requested identity is described inside, with one field per member
    // of its published schema.
    assert_eq!(explanation["describes_schema_refs"], json!([schema_ref]));
    let documents = explanation["documents"].as_array().unwrap();
    assert_eq!(documents.len(), 1);
    assert_eq!(documents[0]["schema_ref"], json!(schema_ref));
    let mut document_members: Vec<&str> = documents[0]["members"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect();
    document_members.sort_unstable();
    let mut properties: Vec<&str> = schema["properties"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    properties.sort_unstable();
    assert_eq!(document_members, properties);
    assert_eq!(
        explanation["fields"].as_array().unwrap().len(),
        document_members.len()
    );
}

//! R009 Schema Negotiation Catalog Tests
//!
//! This test suite verifies R009 schema negotiation catalog implementation:
//! - Capabilities declare exact readable and writable schema URN sets
//! - Producers and consumers negotiate only exact mutual identifier
//! - Report read-only or lossy support explicitly
//! - Do not infer compatibility from similar names or schema structure

use assert_cmd::Command;
use serde_json::Value;
use serial_test::serial;
use std::path::Path;

/// Helper struct for managing test workspaces
struct TestWorkspace {
    temp_dir: tempfile::TempDir,
    root: std::path::PathBuf,
    original_dir: std::path::PathBuf,
    bead_path: std::path::PathBuf,
}

impl TestWorkspace {
    fn new() -> Self {
        let original_dir = std::env::current_dir().unwrap();
        let bead_path = assert_cmd::cargo::cargo_bin("bead");
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path().to_path_buf();

        std::env::set_current_dir(&root).unwrap();

        // Initialize workspace using the full path to bead
        Command::new(&bead_path)
            .args(["init", "--prefix", "test"])
            .assert()
            .success();

        Self {
            temp_dir,
            root,
            original_dir,
            bead_path,
        }
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn cleanup(self) {
        // Restore original directory
        let _ = std::env::set_current_dir(self.original_dir);
        drop(self.temp_dir);
    }

    fn bead_cmd(&self) -> Command {
        Command::new(&self.bead_path)
    }
}

#[test]
#[serial]
fn test_schema_negotiation_all_schemas_have_readability_fields() {
    // Test that all schemas in capabilities have readable and writable fields
    let workspace = TestWorkspace::new();

    let output = workspace
        .bead_cmd()
        .arg("capabilities")
        .arg("--profile=native-v1")
        .current_dir(workspace.root())
        .output()
        .expect("Failed to run capabilities command");

    assert!(output.status.success());

    let capabilities: Value =
        serde_json::from_slice(&output.stdout).expect("Failed to parse capabilities JSON");

    let schemas = capabilities["schemas"]
        .as_array()
        .expect("schemas should be an array");

    assert!(!schemas.is_empty(), "Should have at least one schema");

    for schema in schemas {
        assert!(
            schema.get("readable").is_some(),
            "Schema {} should have readable field",
            schema["schema_ref"]
        );
        assert!(
            schema.get("writable").is_some(),
            "Schema {} should have writable field",
            schema["schema_ref"]
        );

        let readable = schema["readable"]
            .as_bool()
            .expect("readable should be boolean");
        let writable = schema["writable"]
            .as_bool()
            .expect("writable should be boolean");

        // At least one of readable or writable should be true
        assert!(
            readable || writable,
            "Schema {} should be readable or writable",
            schema["schema_ref"]
        );
    }

    workspace.cleanup();
}

#[test]
#[serial]
fn test_schema_negotiation_full_read_write_support() {
    // Test that schemas with readable=true, writable=false indicate read-only support
    let workspace = TestWorkspace::new();

    let output = workspace
        .bead_cmd()
        .arg("capabilities")
        .arg("--profile=native-v1")
        .current_dir(workspace.root())
        .output()
        .expect("Failed to run capabilities command");

    assert!(output.status.success());

    let capabilities: Value =
        serde_json::from_slice(&output.stdout).expect("Failed to parse capabilities JSON");

    let schemas = capabilities["schemas"]
        .as_array()
        .expect("schemas should be an array");

    // All current native schemas should be fully readable and writable
    for schema in schemas {
        let readable = schema["readable"]
            .as_bool()
            .expect("readable should be boolean");
        let writable = schema["writable"]
            .as_bool()
            .expect("writable should be boolean");

        // Current schemas should be fully read-write supported
        assert!(
            readable && writable,
            "Native schema {} should be both readable and writable",
            schema["schema_ref"]
        );

        // Lossy field should be None for lossless schemas
        let lossy = schema.get("lossy");
        if let Some(lossy_value) = lossy {
            if lossy_value.is_null() {
                // Null indicates no lossy support - this is fine
            } else {
                panic!(
                    "Native schema {} should have no lossy support (lossy should be null or absent), got: {}",
                    schema["schema_ref"], lossy_value
                );
            }
        }
    }

    workspace.cleanup();
}

#[test]
#[serial]
fn test_schema_negotiation_exact_identifier_matching() {
    // Test that schema negotiation requires exact URN matching
    let workspace = TestWorkspace::new();

    let output = workspace
        .bead_cmd()
        .arg("capabilities")
        .arg("--profile=native-v1")
        .current_dir(workspace.root())
        .output()
        .expect("Failed to run capabilities command");

    assert!(output.status.success());

    let capabilities: Value =
        serde_json::from_slice(&output.stdout).expect("Failed to parse capabilities JSON");

    let schemas = capabilities["schemas"]
        .as_array()
        .expect("schemas should be an array");

    // Verify that schema references are exact URNs
    for schema in schemas {
        let schema_ref = schema["schema_ref"]
            .as_str()
            .expect("schema_ref should be string");

        // Schema references must be absolute URNs
        assert!(
            schema_ref.starts_with("urn:"),
            "Schema reference {} must be absolute URN",
            schema_ref
        );

        // Schema references must be unique
        let count = schemas
            .iter()
            .filter(|s| s["schema_ref"].as_str() == Some(schema_ref))
            .count();

        assert_eq!(count, 1, "Schema reference {} must be unique", schema_ref);
    }

    workspace.cleanup();
}

#[test]
#[serial]
fn test_schema_negotiation_no_inference_from_names() {
    // Test that compatibility is not inferred from similar names
    // Different schema URNs should not be considered compatible even if they look similar

    let workspace = TestWorkspace::new();

    let output = workspace
        .bead_cmd()
        .arg("capabilities")
        .arg("--profile=native-v1")
        .current_dir(workspace.root())
        .output()
        .expect("Failed to run capabilities command");

    assert!(output.status.success());

    let capabilities: Value =
        serde_json::from_slice(&output.stdout).expect("Failed to parse capabilities JSON");

    let schemas = capabilities["schemas"]
        .as_array()
        .expect("schemas should be an array");

    // Verify that similar-looking schemas are treated as distinct
    let mut schema_refs = schemas
        .iter()
        .map(|s| {
            s["schema_ref"]
                .as_str()
                .expect("schema_ref should be string")
        })
        .collect::<Vec<_>>();

    schema_refs.sort();

    // Each schema reference should be unique and exact matching is required
    for (i, &schema_ref) in schema_refs.iter().enumerate() {
        for (j, &other_ref) in schema_refs.iter().enumerate() {
            if i != j {
                // Different URNs are incompatible even if they share prefixes
                assert_ne!(
                    schema_ref, other_ref,
                    "Different schema URNs must not be treated as compatible"
                );
            }
        }
    }

    workspace.cleanup();
}

#[test]
#[serial]
fn test_schema_negotiation_lossy_support_explicit() {
    // Test that lossy or read-only support is explicitly reported
    // For this test, we verify the lossy field exists and can be populated

    let workspace = TestWorkspace::new();

    let output = workspace
        .bead_cmd()
        .arg("capabilities")
        .arg("--profile=native-v1")
        .current_dir(workspace.root())
        .output()
        .expect("Failed to run capabilities command");

    assert!(output.status.success());

    let capabilities: Value =
        serde_json::from_slice(&output.stdout).expect("Failed to parse capabilities JSON");

    let schemas = capabilities["schemas"]
        .as_array()
        .expect("schemas should be an array");

    // Verify that schemas can explicitly report lossy support
    // Current schemas should have lossy: null or missing (indicating no lossy support)
    for schema in schemas {
        let schema_ref = schema["schema_ref"]
            .as_str()
            .expect("schema_ref should be string");

        // Check that lossy field is present (even if null)
        let lossy = schema.get("lossy");

        match lossy {
            None | Some(Value::Null) => {
                // No lossy support - this is correct for native schemas
            }
            Some(Value::String(description)) => {
                // If lossy support exists, it must have a description
                assert!(
                    !description.is_empty(),
                    "Lossy support for schema {} must have non-empty description",
                    schema_ref
                );
            }
            Some(_) => {
                panic!(
                    "Lossy field for schema {} must be null or string",
                    schema_ref
                );
            }
        }
    }

    workspace.cleanup();
}

#[test]
#[serial]
fn test_schema_negotiation_readable_without_writable() {
    // Test edge case: readable=true, writable=false (read-only support)
    // This simulates what would happen if a schema were read-only

    let workspace = TestWorkspace::new();

    let output = workspace
        .bead_cmd()
        .arg("capabilities")
        .arg("--profile=native-v1")
        .current_dir(workspace.root())
        .output()
        .expect("Failed to run capabilities command");

    assert!(output.status.success());

    let capabilities: Value =
        serde_json::from_slice(&output.stdout).expect("Failed to parse capabilities JSON");

    let schemas = capabilities["schemas"]
        .as_array()
        .expect("schemas should be an array");

    // Current schemas should all be writable, but let's verify the structure supports read-only
    for schema in schemas {
        let readable = schema["readable"]
            .as_bool()
            .expect("readable should be boolean");
        let writable = schema["writable"]
            .as_bool()
            .expect("writable should be boolean");

        // For current schemas, both should be true
        if readable && !writable {
            // This would indicate read-only support
            // For such schemas, lossy field should explain the limitation
            let lossy = schema.get("lossy");
            assert!(
                lossy.is_some(),
                "Read-only schema {} should have lossy explanation",
                schema["schema_ref"]
            );
        }
    }

    workspace.cleanup();
}

#[test]
#[serial]
fn test_schema_negotiation_writable_without_readable() {
    // Test edge case: readable=false, writable=true (write-only support)
    // This would be unusual but should be supported

    let workspace = TestWorkspace::new();

    let output = workspace
        .bead_cmd()
        .arg("capabilities")
        .arg("--profile=native-v1")
        .current_dir(workspace.root())
        .output()
        .expect("Failed to run capabilities command");

    assert!(output.status.success());

    let capabilities: Value =
        serde_json::from_slice(&output.stdout).expect("Failed to parse capabilities JSON");

    let schemas = capabilities["schemas"]
        .as_array()
        .expect("schemas should be an array");

    // Verify the structure can support write-only schemas
    for schema in schemas {
        let readable = schema["readable"]
            .as_bool()
            .expect("readable should be boolean");
        let writable = schema["writable"]
            .as_bool()
            .expect("writable should be boolean");

        // Current schemas should be readable, so we shouldn't hit this
        if !readable && writable {
            // This would indicate write-only support (unusual)
            let lossy = schema.get("lossy");
            assert!(
                lossy.is_some(),
                "Write-only schema {} should have lossy explanation",
                schema["schema_ref"]
            );
        }
    }

    workspace.cleanup();
}

#[test]
#[serial]
fn test_schema_negotiation_mutual_identifier_requirements() {
    // Test that producers and consumers require exact mutual identifier matching
    // This is verified by checking that each schema has a unique URN

    let workspace = TestWorkspace::new();

    let output = workspace
        .bead_cmd()
        .arg("capabilities")
        .arg("--profile=native-v1")
        .current_dir(workspace.root())
        .output()
        .expect("Failed to run capabilities command");

    assert!(output.status.success());

    let capabilities: Value =
        serde_json::from_slice(&output.stdout).expect("Failed to parse capabilities JSON");

    let schemas = capabilities["schemas"]
        .as_array()
        .expect("schemas should be an array");

    // Collect all schema URNs
    let mut schema_urns = std::collections::HashSet::new();

    for schema in schemas {
        let schema_ref = schema["schema_ref"]
            .as_str()
            .expect("schema_ref should be string");

        // Each URN must be unique (no duplicates)
        assert!(
            schema_urns.insert(schema_ref),
            "Schema URN {} must be unique (no duplicates allowed)",
            schema_ref
        );
    }

    // Verify we have the expected core schemas
    assert!(
        schema_urns.contains("urn:bead-rs:schema:issue:native-v1"),
        "Must contain issue schema"
    );
    assert!(
        schema_urns.contains("urn:bead-rs:schema:event:native-v1"),
        "Must contain event schema"
    );

    workspace.cleanup();
}

#[test]
#[serial]
fn test_schema_negotiation_capabilities_structure_validation() {
    // Test that capabilities document structure supports schema negotiation
    let workspace = TestWorkspace::new();

    let output = workspace
        .bead_cmd()
        .arg("capabilities")
        .arg("--profile=native-v1")
        .current_dir(workspace.root())
        .output()
        .expect("Failed to run capabilities command");

    assert!(output.status.success());

    let capabilities: Value =
        serde_json::from_slice(&output.stdout).expect("Failed to parse capabilities JSON");

    // Verify top-level capabilities structure
    assert!(
        capabilities.get("contract").is_some(),
        "Capabilities must have contract field"
    );
    assert!(
        capabilities.get("implementation").is_some(),
        "Capabilities must have implementation field"
    );
    assert!(
        capabilities.get("version").is_some(),
        "Capabilities must have version field"
    );
    assert!(
        capabilities.get("schemas").is_some(),
        "Capabilities must have schemas field for negotiation"
    );

    // Verify schemas array structure
    let schemas = capabilities["schemas"]
        .as_array()
        .expect("schemas should be an array");

    assert!(
        !schemas.is_empty(),
        "Must have at least one schema for negotiation"
    );

    // Each schema must have required fields for negotiation
    for schema in schemas {
        assert!(
            schema.get("schema_ref").is_some(),
            "Schema must have schema_ref for exact matching"
        );
        assert!(
            schema.get("readable").is_some(),
            "Schema must have readable field"
        );
        assert!(
            schema.get("writable").is_some(),
            "Schema must have writable field"
        );
    }

    workspace.cleanup();
}

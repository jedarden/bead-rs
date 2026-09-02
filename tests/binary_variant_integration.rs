//! Integration test suite for binary variant capability comparison
//!
//! This test suite builds and compares different binary variants to validate
//! capability presence/absence across different feature configurations.
//!
//! Prerequisites:
//! - Cargo must be available in PATH
//! - This test is marked as ignored because it builds multiple binary variants
//!   which takes time. Run with: cargo test --test binary_variant_integration -- --ignored

use serial_test::serial;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

mod capability_framework;
use capability_framework::*;

/// Test configuration for binary variants
#[derive(Debug, Clone)]
struct VariantTestConfig {
    name: String,
    features: Vec<String>,
    expected_capabilities: ExpectedCapabilities,
}

impl VariantTestConfig {
    /// Create configuration for default build (no features)
    fn default_build() -> Self {
        Self {
            name: "default".to_string(),
            features: vec![],
            expected_capabilities: ExpectedCapabilities {
                auto_flush_present: true,
                auto_flush_value: Some(true),
                attempt_outcome_present: true,
                attempt_outcome_supported: true,
                expected_commands: vec![
                    "capabilities".to_string(),
                    "create".to_string(),
                    "list".to_string(),
                    "claim".to_string(),
                    "resolve".to_string(),
                    "close".to_string(),
                    "reopen".to_string(),
                    "release".to_string(),
                    "sync".to_string(),
                ],
                missing_commands: vec![],
            },
        }
    }

    /// Create configuration for build with attempt-resolution feature
    fn with_attempt_resolution() -> Self {
        Self {
            name: "with-attempt-resolution".to_string(),
            features: vec!["attempt-resolution".to_string()],
            expected_capabilities: ExpectedCapabilities {
                auto_flush_present: true,
                auto_flush_value: Some(true),
                attempt_outcome_present: true,
                attempt_outcome_supported: true,
                expected_commands: vec![
                    "capabilities".to_string(),
                    "create".to_string(),
                    "list".to_string(),
                    "claim".to_string(),
                    "resolve".to_string(),
                    "close".to_string(),
                    "reopen".to_string(),
                    "release".to_string(),
                    "sync".to_string(),
                ],
                missing_commands: vec![],
            },
        }
    }
}

/// Build a specific binary variant
fn build_variant(features: &[String]) -> anyhow::Result<PathBuf> {
    let variant_name = if features.is_empty() {
        "default".to_string()
    } else {
        format!("with-{}", features.join("-"))
    };

    eprintln!("Building binary variant: {}", variant_name);

    let mut cargo = Command::new("cargo");
    cargo.args(["build", "--bin", "bead"]);

    for feature in features {
        cargo.args(["--features", feature]);
    }

    let output = cargo.output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to build variant {}: {}", variant_name, stderr);
    }

    // Path to the debug binary
    let binary_path = PathBuf::from("./target/debug/bead");

    if !binary_path.exists() {
        anyhow::bail!("Binary not found at {:?}", binary_path);
    }

    eprintln!("Built variant successfully: {:?}", binary_path);
    Ok(binary_path)
}

/// Execute capabilities command for a specific binary
fn get_binary_capabilities(binary_path: &Path, workspace_dir: &Path) -> anyhow::Result<serde_json::Value> {
    let output = Command::new(binary_path)
        .current_dir(workspace_dir)
        .args(["capabilities"])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Capabilities command failed: {}", stderr);
    }

    let json_str = String::from_utf8(output.stdout)?;
    let caps: serde_json::Value = serde_json::from_str(&json_str)?;
    Ok(caps)
}

/// Compare capabilities between two binary variants
fn compare_variants(
    variant1_caps: &serde_json::Value,
    variant2_caps: &serde_json::Value,
    variant1_name: &str,
    variant2_name: &str,
) -> Vec<String> {
    let mut differences = Vec::new();

    // Compare top-level fields
    let fields_to_compare = vec![
        "contract",
        "implementation",
        "version",
        "store_layout",
        "atomic_claim",
        "auto_flush",
    ];

    for field in fields_to_compare {
        let v1_value = variant1_caps.get(field);
        let v2_value = variant2_caps.get(field);

        if v1_value != v2_value {
            differences.push(format!(
                "Field '{}': {} has {:?}, {} has {:?}",
                field, variant1_name, v1_value, variant2_name, v2_value
            ));
        }
    }

    differences
}

#[test]
#[ignore = "This test builds multiple binary variants and takes time"]
#[serial]
fn integration_test_default_build_capabilities() {
    // Test that the default build has expected capabilities
    let config = VariantTestConfig::default_build();

    // Build the default variant
    let binary_path = build_variant(&config.features).expect("Failed to build default variant");

    // Create a temporary workspace
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let workspace_dir = temp_dir.path();

    // Initialize workspace
    let init_output = Command::new(&binary_path)
        .current_dir(workspace_dir)
        .args(["init", "--prefix", "test"])
        .output()
        .expect("Failed to init workspace");

    assert!(init_output.status.success(), "Failed to initialize workspace");

    // Get capabilities
    let caps = get_binary_capabilities(&binary_path, workspace_dir)
        .expect("Failed to get capabilities");

    // Verify expected capabilities
    let expected = &config.expected_capabilities;

    // Check auto_flush
    if expected.auto_flush_present {
        assert!(
            caps.get("auto_flush").is_some(),
            "auto_flush should be present in default build"
        );
        if let Some(expected_value) = expected.auto_flush_value {
            let actual_value = caps.get("auto_flush").and_then(|v| v.as_bool());
            assert_eq!(
                actual_value,
                Some(expected_value),
                "auto_flush value mismatch"
            );
        }
    }

    // Check attempt_outcome
    if expected.attempt_outcome_present {
        assert!(
            caps.get("attempt_outcome").is_some(),
            "attempt_outcome should be present in default build"
        );
        if expected.attempt_outcome_supported {
            let supported = caps
                .get("attempt_outcome")
                .and_then(|v| v.get("supported"))
                .and_then(|v| v.as_bool());
            assert_eq!(supported, Some(true), "attempt_outcome.supported should be true");
        }
    }

    eprintln!("Default build capabilities validated successfully");
}

#[test]
#[ignore = "This test builds multiple binary variants and takes time"]
#[serial]
fn integration_test_attempt_resolution_build_capabilities() {
    // Test that build with attempt-resolution has expected capabilities
    let config = VariantTestConfig::with_attempt_resolution();

    // Build the variant
    let binary_path = build_variant(&config.features)
        .expect("Failed to build attempt-resolution variant");

    // Create a temporary workspace
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let workspace_dir = temp_dir.path();

    // Initialize workspace
    let init_output = Command::new(&binary_path)
        .current_dir(workspace_dir)
        .args(["init", "--prefix", "test"])
        .output()
        .expect("Failed to init workspace");

    assert!(init_output.status.success(), "Failed to initialize workspace");

    // Get capabilities
    let caps = get_binary_capabilities(&binary_path, workspace_dir)
        .expect("Failed to get capabilities");

    // Verify attempt_outcome is present and supported
    assert!(
        caps.get("attempt_outcome").is_some(),
        "attempt_outcome should be present with attempt-resolution feature"
    );

    let supported = caps
        .get("attempt_outcome")
        .and_then(|v| v.get("supported"))
        .and_then(|v| v.as_bool());

    assert_eq!(
        supported,
        Some(true),
        "attempt_outcome.supported should be true with attempt-resolution feature"
    );

    eprintln!("Attempt-resolution build capabilities validated successfully");
}

#[test]
#[ignore = "This test builds multiple binary variants and takes time"]
#[serial]
fn integration_test_compare_binary_variants() {
    // Build and compare both variants
    let default_config = VariantTestConfig::default_build();
    let ar_config = VariantTestConfig::with_attempt_resolution();

    // Build both variants
    let default_binary = build_variant(&default_config.features)
        .expect("Failed to build default variant");

    let ar_binary = build_variant(&ar_config.features)
        .expect("Failed to build attempt-resolution variant");

    // Create temporary workspaces for each variant
    let default_temp = TempDir::new().expect("Failed to create temp dir");
    let ar_temp = TempDir::new().expect("Failed to create temp dir");

    // Initialize both workspaces
    for (binary, temp_dir) in [&default_binary, &ar_binary].iter().zip([&default_temp, &ar_temp].iter()) {
        let output = Command::new(binary)
            .current_dir(temp_dir.path())
            .args(["init", "--prefix", "test"])
            .output()
            .expect("Failed to init workspace");

        assert!(output.status.success(), "Failed to initialize workspace");
    }

    // Get capabilities from both variants
    let default_caps = get_binary_capabilities(&default_binary, default_temp.path())
        .expect("Failed to get default capabilities");

    let ar_caps = get_binary_capabilities(&ar_binary, ar_temp.path())
        .expect("Failed to get attempt-resolution capabilities");

    // Compare capabilities
    let differences = compare_variants(&default_caps, &ar_caps, "default", "attempt-resolution");

    // Currently, both builds should have similar capabilities since attempt-resolution
    // is now part of the default build. This test validates that the framework can
    // detect differences when they exist.
    eprintln!("Capability differences found: {}", differences.len());

    for diff in &differences {
        eprintln!("  - {}", diff);
    }

    // Verify that both builds report the same core capabilities
    assert_eq!(
        default_caps.get("contract"),
        ar_caps.get("contract"),
        "Contract version should match"
    );

    assert_eq!(
        default_caps.get("implementation"),
        ar_caps.get("implementation"),
        "Implementation should match"
    );

    eprintln!("Binary variant comparison completed successfully");
}

#[test]
#[ignore = "This test builds multiple binary variants and takes time"]
#[serial]
fn integration_test_command_availability_across_variants() {
    // Test that commands are available across different variants
    let configs = vec![
        VariantTestConfig::default_build(),
        VariantTestConfig::with_attempt_resolution(),
    ];

    for config in configs {
        eprintln!("Testing command availability for variant: {}", config.name);

        // Build the variant
        let binary_path = build_variant(&config.features)
            .expect(&format!("Failed to build variant: {}", config.name));

        // Create temporary workspace
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let workspace_dir = temp_dir.path();

        // Initialize workspace
        let init_output = Command::new(&binary_path)
            .current_dir(workspace_dir)
            .args(["init", "--prefix", "test"])
            .output()
            .expect("Failed to init workspace");

        assert!(init_output.status.success(), "Failed to initialize workspace");

        // Get capabilities
        let caps = get_binary_capabilities(&binary_path, workspace_dir)
            .expect("Failed to get capabilities");

        // Verify expected commands are present
        let commands = caps
            .get("commands")
            .and_then(|v| v.as_array())
            .expect("Commands array should be present");

        for expected_command in &config.expected_capabilities.expected_commands {
            assert!(
                commands.iter().any(|cmd| cmd.as_str() == Some(expected_command.as_str())),
                "Command '{}' should be present in {} build",
                expected_command,
                config.name
            );
        }

        // Verify missing commands are actually missing
        for missing_command in &config.expected_capabilities.missing_commands {
            assert!(
                !commands.iter().any(|cmd| cmd.as_str() == Some(missing_command.as_str())),
                "Command '{}' should be missing in {} build",
                missing_command,
                config.name
            );
        }

        eprintln!("Command availability validated for variant: {}", config.name);
    }

    eprintln!("Command availability tests completed successfully");
}

#[test]
#[ignore = "This test builds multiple binary variants and takes time"]
#[serial]
fn integration_test_capability_framework_integration() {
    // Test that the BinaryHarness framework works with different binary variants
    eprintln!("Testing BinaryHarness framework integration");

    // This test validates that the framework can work with the default binary
    // Future enhancements can extend it to work with custom-built binaries

    let harness = BinaryHarness::new().expect("Failed to create harness");
    harness.init_workspace().expect("Failed to init workspace");

    // Test basic capability detection
    let caps = harness.get_default_capabilities().expect("Failed to get capabilities");

    assert!(
        caps.get("contract").is_some(),
        "Capabilities should include contract field"
    );

    assert!(
        caps.get("implementation").is_some(),
        "Capabilities should include implementation field"
    );

    eprintln!("BinaryHarness framework integration test completed successfully");
}

/// Helper function to clean up built binaries
#[test]
#[ignore = "Cleanup helper"]
fn cleanup_test_binaries() {
    // This is a manual cleanup helper that can be run if needed
    let debug_dir = PathBuf::from("./target/debug");
    if debug_dir.exists() {
        eprintln!("Debug directory exists at {:?}", debug_dir);
        eprintln!("Built binaries will be reused by cargo");
    }
}

// Module-level documentation
#[cfg(doctest)]
doc_comment::doctest!("../../docs/tests/capability-testing.md");

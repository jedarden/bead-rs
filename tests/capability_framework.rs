//! Test framework for capability detection and binary variant testing
//!
//! This module provides utilities for:
//! - Building and testing multiple binary variants (with/without features)
//! - Executing binaries and capturing their capabilities output
//! - Testing capability presence/absence across different builds
//! - Comparing capabilities between binary versions

// Included as a module by several test crates; not every consumer uses every
// helper, so unused items in any one crate are expected.
#![allow(dead_code)]

use assert_cmd::Command;
use serde_json::Value;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Binary variant configuration
#[derive(Debug, Clone)]
pub struct BinaryVariant {
    /// Name for this variant (e.g., "default", "with-attempt-resolution")
    pub name: String,
    /// Cargo features to enable (e.g., ["attempt-resolution"])
    pub features: Vec<String>,
    /// Expected capabilities for this variant
    pub expected_capabilities: ExpectedCapabilities,
}

/// Expected capabilities for a binary variant
#[derive(Debug, Clone)]
pub struct ExpectedCapabilities {
    /// Whether auto_flush field should be present in capabilities output
    pub auto_flush_present: bool,
    /// Whether auto_flush should be true when present
    pub auto_flush_value: Option<bool>,
    /// Whether attempt_outcome capability should be present
    pub attempt_outcome_present: bool,
    /// Whether attempt_outcome should report supported: true
    pub attempt_outcome_supported: bool,
    /// Commands that should be available
    pub expected_commands: Vec<String>,
    /// Commands that should NOT be available
    pub missing_commands: Vec<String>,
}

impl Default for ExpectedCapabilities {
    fn default() -> Self {
        Self {
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
            ],
            missing_commands: vec![],
        }
    }
}

/// Directory holding the pinned binary variants (the pin location of record)
pub fn pinned_binaries_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("pinned-binaries")
}

/// Resolve a pinned binary variant by its role key in `pinned-binaries/commits.json`
/// (e.g. `pre_feature`, `attempt_resolution_f25ab5c`)
///
/// The registry maps each role to the pin name currently holding it, so this
/// survives re-pins that rename the binary; hardcoding a shaslice here would not.
pub fn pinned_variant(role: &str) -> anyhow::Result<PathBuf> {
    let registry_path = pinned_binaries_dir().join("commits.json");
    let registry: Value = serde_json::from_slice(&std::fs::read(&registry_path)?)?;
    let name = registry
        .get(role)
        .and_then(|entry| entry.get("binary_name"))
        .and_then(|n| n.as_str())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "pin role '{}' has no binary_name in {}",
                role,
                registry_path.display()
            )
        })?;
    let binary = pinned_binaries_dir().join(name);
    anyhow::ensure!(
        binary.exists(),
        "pin role '{}' points at '{}' which is not on disk under {}",
        role,
        name,
        pinned_binaries_dir().display()
    );
    Ok(binary)
}

/// The binary's own `--version` string (used to check pin provenance)
pub fn version_of(binary: &Path) -> anyhow::Result<String> {
    let scratch = scratch_dir()?;
    let output = Command::new(binary)
        .current_dir(scratch.path())
        .args(["--version"])
        .output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("--version failed for {}: {}", binary.display(), stderr);
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

/// `capabilities` JSON emitted by an explicit binary, executed outside any workspace
pub fn capabilities_of(binary: &Path) -> anyhow::Result<Value> {
    let scratch = scratch_dir()?;
    let output = Command::new(binary)
        .current_dir(scratch.path())
        .args(["capabilities"])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Capabilities command failed: {}", stderr);
    }

    Ok(serde_json::from_slice(&output.stdout)?)
}

/// Verify a pinned binary's embedded `--version` matches the version recorded in
/// its metadata file, so the variant tests assert against the pinned bytes and
/// not whatever happens to sit at that name. Returns the binary path and metadata.
pub fn verified_pinned_variant(role: &str) -> anyhow::Result<(PathBuf, Value)> {
    let binary = pinned_variant(role)?;
    let name = binary
        .file_name()
        .expect("pin path has a file name")
        .to_string_lossy()
        .to_string();
    let meta_path = pinned_binaries_dir().join(format!("{}.metadata.json", name));
    let meta: Value = serde_json::from_slice(&std::fs::read(&meta_path)?)?;
    let recorded = meta
        .get("embedded_version_string")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("{} lacks embedded_version_string", meta_path.display()))?;
    let actual = version_of(&binary)?;
    anyhow::ensure!(
        actual == recorded,
        "pin provenance mismatch for {}: binary reports {:?}, metadata records {:?}",
        name,
        actual,
        recorded
    );
    Ok((binary, meta))
}

/// Disposable working directory for direct binary invocations
///
/// /var/tmp rather than /tmp to stay clear of foreign `.beads` ancestors
fn scratch_dir() -> anyhow::Result<TempDir> {
    Ok(tempfile::Builder::new()
        .prefix("bead-caps-")
        .tempdir_in("/var/tmp")?)
}

/// Binary test harness
pub struct BinaryHarness {
    /// Temporary directory for test workspaces
    _temp_dir: TempDir,
    /// Path to test workspace
    workspace_path: PathBuf,
    /// Binary under test; `None` means the cargo-built `bead` test binary
    binary: Option<PathBuf>,
}

impl BinaryHarness {
    /// Create a new test harness with a temporary workspace
    ///
    /// Uses /var/tmp instead of /tmp to avoid conflicts with /tmp/.beads
    /// which can interfere with workspace discovery
    pub fn new() -> anyhow::Result<Self> {
        Self::with_binary_path(None)
    }

    /// Create a harness that executes an explicit binary instead of the
    /// cargo-built `bead` — the entry point for running the same test body
    /// against the pinned pre-feature and feature-enabled variants
    pub fn with_binary(binary: impl Into<PathBuf>) -> anyhow::Result<Self> {
        let binary = binary.into();
        anyhow::ensure!(
            binary.exists(),
            "binary under test not found: {}",
            binary.display()
        );
        Self::with_binary_path(Some(binary))
    }

    fn with_binary_path(binary: Option<PathBuf>) -> anyhow::Result<Self> {
        let temp_dir = tempfile::Builder::new()
            .prefix("bead-test-")
            .tempdir_in("/var/tmp")?;
        let workspace_path = temp_dir.path().to_path_buf();

        Ok(Self {
            _temp_dir: temp_dir,
            workspace_path,
            binary,
        })
    }

    /// The binary this harness executes
    pub fn binary_path(&self) -> anyhow::Result<PathBuf> {
        match &self.binary {
            Some(p) => Ok(p.clone()),
            // Set by cargo for every integration test target of this package
            None => Ok(PathBuf::from(env!("CARGO_BIN_EXE_bead"))),
        }
    }

    /// A command builder for the binary under test
    fn command(&self) -> anyhow::Result<Command> {
        Ok(Command::new(self.binary_path()?))
    }

    /// Run an arbitrary subcommand of the binary under test, returning raw output
    pub fn run(&self, args: &[&str]) -> anyhow::Result<std::process::Output> {
        Ok(self
            .command()?
            .current_dir(&self.workspace_path)
            .args(args)
            .output()?)
    }

    /// Detect a missing subcommand: returns `Some(stderr)` when the binary exits
    /// non-zero with clap's "unrecognized subcommand" error — the degradation
    /// signal a consumer sees when a capability is absent
    pub fn unrecognized_subcommand(&self, subcommand: &str) -> anyhow::Result<Option<String>> {
        let output = self.run(&[subcommand, "--help"])?;
        if output.status.success() {
            return Ok(None);
        }
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        if stderr.contains("unrecognized subcommand") {
            Ok(Some(stderr))
        } else {
            Ok(None)
        }
    }

    /// Initialize a bead workspace in the test directory
    pub fn init_workspace(&self) -> anyhow::Result<()> {
        let output = self
            .command()?
            .current_dir(&self.workspace_path)
            .args(["init", "--prefix", "test"])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to init workspace: {}", stderr);
        }

        Ok(())
    }

    /// Get the workspace path
    pub fn workspace_path(&self) -> &Path {
        &self.workspace_path
    }

    /// Execute `bead capabilities` and parse JSON output
    pub fn get_capabilities(&self, args: &[&str]) -> anyhow::Result<Value> {
        let output = self
            .command()?
            .current_dir(&self.workspace_path)
            .args(args)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Capabilities command failed: {}", stderr);
        }

        let json_str = String::from_utf8(output.stdout)?;
        let caps: Value = serde_json::from_str(&json_str)?;
        Ok(caps)
    }

    /// Execute `bead capabilities` with default args
    pub fn get_default_capabilities(&self) -> anyhow::Result<Value> {
        self.get_capabilities(&["capabilities"])
    }

    /// Check if a specific capability field exists
    pub fn has_capability_field(&self, field_path: &str) -> anyhow::Result<bool> {
        let caps = self.get_default_capabilities()?;
        Ok(self.navigate_json_path(&caps, field_path).is_some())
    }

    /// Navigate a dot-separated path in JSON (e.g., "attempt_outcome.supported")
    fn navigate_json_path<'a>(&self, value: &'a Value, path: &str) -> Option<&'a Value> {
        let parts: Vec<&str> = path.split('.').collect();
        let mut current = value;

        for part in parts {
            match current {
                Value::Object(map) => {
                    current = map.get(part)?;
                }
                Value::Array(arr) => {
                    let index = part.parse::<usize>().ok()?;
                    current = arr.get(index)?;
                }
                _ => return None,
            }
        }

        Some(current)
    }

    /// Test if a command exists in the capabilities command list
    pub fn command_exists(&self, command_name: &str) -> anyhow::Result<bool> {
        let caps = self.get_default_capabilities()?;
        let commands = caps
            .get("commands")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("Missing commands array"))?;

        Ok(commands
            .iter()
            .any(|cmd| cmd.as_str().map(|s| s == command_name).unwrap_or(false)))
    }

    /// Compare capabilities against expected values
    pub fn verify_capabilities(
        &self,
        expected: &ExpectedCapabilities,
    ) -> anyhow::Result<Vec<String>> {
        let mut failures = Vec::new();
        let caps = self.get_default_capabilities()?;

        // Check auto_flush presence
        if expected.auto_flush_present {
            if caps.get("auto_flush").is_none() {
                failures.push("auto_flush field is missing but should be present".to_string());
            } else if let Some(expected_value) = expected.auto_flush_value {
                let actual_value = caps.get("auto_flush").and_then(|v| v.as_bool());
                if actual_value != Some(expected_value) {
                    failures.push(format!(
                        "auto_flush value mismatch: expected {:?}, got {:?}",
                        expected_value, actual_value
                    ));
                }
            }
        } else if caps.get("auto_flush").is_some() {
            failures.push("auto_flush field is present but should be absent".to_string());
        }

        // Check attempt_outcome presence
        if expected.attempt_outcome_present {
            if caps.get("attempt_outcome").is_none() {
                failures.push("attempt_outcome field is missing but should be present".to_string());
            } else if expected.attempt_outcome_supported {
                let supported = caps
                    .get("attempt_outcome")
                    .and_then(|v| v.get("supported"))
                    .and_then(|v| v.as_bool());
                if supported != Some(true) {
                    failures.push(format!(
                        "attempt_outcome.supported mismatch: expected true, got {:?}",
                        supported
                    ));
                }
            }
        } else if caps.get("attempt_outcome").is_some() {
            failures.push("attempt_outcome field is present but should be absent".to_string());
        }

        // Check expected commands
        for command in &expected.expected_commands {
            if !self.command_exists(command)? {
                failures.push(format!(
                    "Command '{}' is missing but should be present",
                    command
                ));
            }
        }

        // Check that missing commands are actually missing
        for command in &expected.missing_commands {
            if self.command_exists(command)? {
                failures.push(format!(
                    "Command '{}' is present but should be missing",
                    command
                ));
            }
        }

        Ok(failures)
    }

    /// Execute resolve command (should fail if attempt-resolution feature disabled)
    pub fn test_resolve_command(&self) -> anyhow::Result<Result<(), String>> {
        let output = self
            .command()?
            .current_dir(&self.workspace_path)
            .args(["resolve", "--help"])
            .output();

        match output {
            Ok(output) => {
                if output.status.success() {
                    Ok(Ok(()))
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    if stderr.contains("unrecognized subcommand")
                        || stderr.contains("unknown argument")
                    {
                        Ok(Err(stderr.to_string()))
                    } else {
                        Ok(Ok(())) // Command exists but failed for other reasons
                    }
                }
            }
            Err(e) => Ok(Err(format!("Failed to execute resolve command: {}", e))),
        }
    }
}

/// Test macro for capability assertions
#[macro_export]
macro_rules! assert_capability_present {
    ($harness:expr, $field_path:expr) => {
        match $harness.has_capability_field($field_path) {
            Ok(true) => (),
            Ok(false) => panic!(
                "Capability field '{}' should be present but is absent",
                $field_path
            ),
            Err(e) => panic!("Failed to check capability field '{}': {}", $field_path, e),
        }
    };
}

#[macro_export]
macro_rules! assert_capability_absent {
    ($harness:expr, $field_path:expr) => {
        match $harness.has_capability_field($field_path) {
            Ok(false) => (),
            Ok(true) => panic!(
                "Capability field '{}' should be absent but is present",
                $field_path
            ),
            Err(e) => panic!("Failed to check capability field '{}': {}", $field_path, e),
        }
    };
}

#[macro_export]
macro_rules! assert_command_exists {
    ($harness:expr, $command:expr) => {
        match $harness.command_exists($command) {
            Ok(true) => (),
            Ok(false) => panic!("Command '{}' should be present but is absent", $command),
            Err(e) => panic!("Failed to check command '{}': {}", $command, e),
        }
    };
}

#[macro_export]
macro_rules! assert_command_missing {
    ($harness:expr, $command:expr) => {
        match $harness.command_exists($command) {
            Ok(false) => (),
            Ok(true) => panic!("Command '{}' should be absent but is present", $command),
            Err(e) => panic!("Failed to check command '{}': {}", $command, e),
        }
    };
}

/// Build a specific binary variant for testing
///
/// This function is meant to be called from build scripts or test setup
/// to compile different feature combinations of the binary.
///
/// # Arguments
/// * `features` - Slice of feature flags to enable
///
/// # Returns
/// * `Ok(PathBuf)` - Path to the built binary
/// * `Err(String)` - Error message if build fails
pub fn build_binary_variant(features: &[&str]) -> anyhow::Result<PathBuf> {
    use std::process::Command;

    let mut cargo = Command::new("cargo");
    cargo.args(["build", "--bins"]);

    for feature in features {
        cargo.args(["--features", feature]);
    }

    let output = cargo.output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to build binary variant: {}", stderr);
    }

    // Return path to debug binary
    Ok(PathBuf::from("./target/debug/bead"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    #[serial]
    fn test_harness_creation() {
        let harness = BinaryHarness::new().unwrap();
        assert!(harness.workspace_path().exists());
    }

    #[test]
    #[serial]
    fn test_workspace_init() {
        let harness = BinaryHarness::new().unwrap();
        harness.init_workspace().unwrap();
        assert!(harness.workspace_path().join(".beads").exists());
    }

    #[test]
    #[serial]
    fn test_get_capabilities() {
        let harness = BinaryHarness::new().unwrap();
        let caps = harness.get_default_capabilities().unwrap();

        // Basic capability structure validation
        assert_eq!(caps["contract"], "native-v1");
        assert_eq!(caps["implementation"], "bead-rs");
        assert!(caps.get("version").is_some());
    }

    #[test]
    #[serial]
    fn test_has_capability_field() {
        let harness = BinaryHarness::new().unwrap();

        // Test existing field
        assert!(harness.has_capability_field("contract").unwrap());

        // Test nested field
        assert!(harness.has_capability_field("priorities.min").unwrap());

        // Test non-existing field
        assert!(!harness.has_capability_field("nonexistent").unwrap());
    }

    #[test]
    #[serial]
    fn test_command_exists() {
        let harness = BinaryHarness::new().unwrap();

        assert!(harness.command_exists("capabilities").unwrap());
        assert!(harness.command_exists("create").unwrap());
        assert!(!harness.command_exists("nonexistent").unwrap());
    }
}

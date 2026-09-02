//! Test harness and helper functions for attempt-resolution fixtures
//!
//! This module provides utilities for:
//! - Loading old-format (pre-attempt-resolution) fixtures
//! - Loading new-format (with attempt-resolution) fixtures
//! - Validating schema compliance
//! - Detecting binary version and capability support
//! - Probing attempt-resolution feature availability
//!
//! See: tests/fixtures/attempts/README.md for fixture structure

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

/// Fixture format variant
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixtureFormat {
    /// Pre-attempt-resolution format (no attempt_outcome records)
    Old,
    /// With attempt-resolution feature (includes attempt_outcome records)
    New,
}

/// Attempt-resolution capability detection result
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityInfo {
    /// Whether attempt-resolution is supported
    pub supports_attempt_resolution: bool,
    /// Whether the `bead resolve` command is available
    pub has_resolve_command: bool,
    /// Whether attempt_outcome_count field is supported in manifests
    pub has_attempt_outcome_count: bool,
}

/// Schema validation result
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// Whether validation passed
    pub is_valid: bool,
    /// Validation errors (if any)
    pub errors: Vec<String>,
    /// Validation warnings (if any)
    pub warnings: Vec<String>,
}

impl ValidationResult {
    /// Create a successful validation result
    pub fn success() -> Self {
        ValidationResult {
            is_valid: true,
            errors: vec![],
            warnings: vec![],
        }
    }

    /// Create a failed validation result with errors
    pub fn failure(errors: Vec<String>) -> Self {
        ValidationResult {
            is_valid: false,
            errors,
            warnings: vec![],
        }
    }

    /// Add a warning to an existing result
    pub fn with_warning(mut self, warning: String) -> Self {
        self.warnings.push(warning);
        self
    }
}

/// Get the path to the attempts fixtures directory
pub fn fixtures_path() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests/fixtures/attempts");
    path
}

/// Get the path to old-format fixtures
pub fn old_format_path() -> PathBuf {
    fixtures_path().join("old")
}

/// Get the path to new-format fixtures
pub fn new_format_path() -> PathBuf {
    fixtures_path().join("new")
}

/// Load old-format checkpoint (pre-attempt-resolution)
pub fn load_old_checkpoint() -> Result<Vec<Value>, String> {
    let checkpoint_path = old_format_path().join("checkpoint.jsonl");
    load_checkpoint_jsonl(&checkpoint_path)
}

/// Load new-format checkpoint (with attempt-resolution)
pub fn load_new_checkpoint() -> Result<Vec<Value>, String> {
    let checkpoint_path = new_format_path().join("checkpoint.jsonl");
    load_checkpoint_jsonl(&checkpoint_path)
}

/// Load checkpoint records from a JSONL file
pub fn load_checkpoint_jsonl(path: &Path) -> Result<Vec<Value>, String> {
    if !path.exists() {
        return Err(format!("Checkpoint file not found: {:?}", path));
    }

    let content =
        fs::read_to_string(path).map_err(|e| format!("Failed to read checkpoint file: {}", e))?;

    let mut records = Vec::new();
    for (line_num, line) in content.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        serde_json::from_str::<Value>(line)
            .map_err(|e| format!("Invalid JSON on line {}: {}", line_num + 1, e))
            .and_then(|value| {
                // Verify record_type field exists
                if value.get("record_type").is_none() {
                    return Err(format!("Missing record_type on line {}", line_num + 1));
                }
                Ok(value)
            })
            .map(|value| records.push(value))?;
    }

    Ok(records)
}

/// Load old-format manifest (current.json)
pub fn load_old_manifest() -> Result<Value, String> {
    let manifest_path = old_format_path().join("current.json");
    load_manifest(&manifest_path)
}

/// Load new-format manifest (current.json)
pub fn load_new_manifest() -> Result<Value, String> {
    let manifest_path = new_format_path().join("current.json");
    load_manifest(&manifest_path)
}

/// Load manifest from a JSON file
pub fn load_manifest(path: &Path) -> Result<Value, String> {
    if !path.exists() {
        return Err(format!("Manifest file not found: {:?}", path));
    }

    let content =
        fs::read_to_string(path).map_err(|e| format!("Failed to read manifest file: {}", e))?;

    serde_json::from_str::<Value>(&content).map_err(|e| format!("Invalid manifest JSON: {}", e))
}

/// Detect fixture format from manifest
pub fn detect_fixture_format(manifest: &Value) -> Result<FixtureFormat, String> {
    // New format has attempt_outcome_count field
    if manifest.get("attempt_outcome_count").is_some() {
        Ok(FixtureFormat::New)
    } else if manifest.get("issue_count").is_some() {
        // Old format has issue_count but not attempt_outcome_count
        Ok(FixtureFormat::Old)
    } else {
        Err("Cannot determine fixture format: missing required fields".to_string())
    }
}

/// Validate old-format fixture schema
pub fn validate_old_format() -> ValidationResult {
    let mut errors = Vec::new();

    // Load and validate manifest
    let manifest = match load_old_manifest() {
        Ok(m) => m,
        Err(e) => return ValidationResult::failure(vec![e]),
    };

    // Check for attempt_outcome_count absence (should NOT be present)
    if manifest.get("attempt_outcome_count").is_some() {
        errors
            .push("Old format should not have attempt_outcome_count field in manifest".to_string());
    }

    // Validate required fields
    let required_fields = ["issue_count", "total_record_count", "store_uuid"];
    for field in &required_fields {
        if manifest.get(*field).is_none() {
            errors.push(format!("Missing required field in manifest: {}", field));
        }
    }

    // Load and validate checkpoint
    let checkpoint = match load_old_checkpoint() {
        Ok(c) => c,
        Err(e) => {
            errors.push(e);
            return ValidationResult::failure(errors);
        }
    };

    // All records should be issues (no attempt_outcome records)
    for (i, record) in checkpoint.iter().enumerate() {
        let record_type = match record.get("record_type") {
            Some(t) => t,
            None => {
                errors.push(format!("Record {} missing record_type", i));
                continue;
            }
        };

        match record_type.as_str() {
            Some("issue") => { /* expected */ }
            Some(other) => {
                errors.push(format!(
                    "Old format should only contain issue records, found: {} at record {}",
                    other, i
                ));
            }
            None => {
                errors.push(format!("record_type is not a string at record {}", i));
            }
        }
    }

    // Verify issue count matches manifest
    let manifest_issue_count = manifest.get("issue_count").and_then(|v| v.as_u64());
    let actual_issue_count = checkpoint
        .iter()
        .filter(|r| r.get("record_type").and_then(|t| t.as_str()) == Some("issue"))
        .count();

    if manifest_issue_count != Some(actual_issue_count as u64) {
        errors.push(format!(
            "Manifest issue_count ({:?}) does not match actual count ({})",
            manifest_issue_count, actual_issue_count
        ));
    }

    if errors.is_empty() {
        ValidationResult::success()
    } else {
        ValidationResult::failure(errors)
    }
}

/// Validate new-format fixture schema
pub fn validate_new_format() -> ValidationResult {
    let mut errors = Vec::new();

    // Load and validate manifest
    let manifest = match load_new_manifest() {
        Ok(m) => m,
        Err(e) => return ValidationResult::failure(vec![e]),
    };

    // Check for attempt_outcome_count presence (should be present)
    if manifest.get("attempt_outcome_count").is_none() {
        errors.push("New format must have attempt_outcome_count field in manifest".to_string());
    }

    // Validate required fields
    let required_fields = [
        "issue_count",
        "attempt_outcome_count",
        "total_record_count",
        "store_uuid",
    ];
    for field in &required_fields {
        if manifest.get(*field).is_none() {
            errors.push(format!("Missing required field in manifest: {}", field));
        }
    }

    // Load and validate checkpoint
    let checkpoint = match load_new_checkpoint() {
        Ok(c) => c,
        Err(e) => {
            errors.push(e);
            return ValidationResult::failure(errors);
        }
    };

    // Should have both issue and attempt_outcome records
    let mut issue_count = 0;
    let mut attempt_outcome_count = 0;

    for (i, record) in checkpoint.iter().enumerate() {
        let record_type = match record.get("record_type") {
            Some(t) => t,
            None => {
                errors.push(format!("Record {} missing record_type", i));
                continue;
            }
        };

        match record_type.as_str() {
            Some("issue") => {
                issue_count += 1;
                // Validate issue record structure
                validate_issue_record(record, i, &mut errors);
            }
            Some("attempt_outcome") => {
                attempt_outcome_count += 1;
                // Validate attempt_outcome record structure
                validate_attempt_outcome_record(record, i, &mut errors);
            }
            Some(other) => {
                errors.push(format!("Unknown record type: {} at record {}", other, i));
            }
            None => {
                errors.push(format!("record_type is not a string at record {}", i));
            }
        }
    }

    // Verify counts match manifest
    let manifest_issue_count = manifest.get("issue_count").and_then(|v| v.as_u64());
    let manifest_attempt_count = manifest
        .get("attempt_outcome_count")
        .and_then(|v| v.as_u64());

    if manifest_issue_count != Some(issue_count as u64) {
        errors.push(format!(
            "Manifest issue_count ({:?}) does not match actual count ({})",
            manifest_issue_count, issue_count
        ));
    }

    if manifest_attempt_count != Some(attempt_outcome_count as u64) {
        errors.push(format!(
            "Manifest attempt_outcome_count ({:?}) does not match actual count ({})",
            manifest_attempt_count, attempt_outcome_count
        ));
    }

    if errors.is_empty() {
        ValidationResult::success()
    } else {
        ValidationResult::failure(errors)
    }
}

/// Validate issue record structure
fn validate_issue_record(record: &Value, index: usize, errors: &mut Vec<String>) {
    let issue = match record.get("issue") {
        Some(i) => i,
        None => {
            errors.push(format!("Issue record {} missing 'issue' field", index));
            return;
        }
    };

    // Required fields for issue records
    let required_fields = ["id", "title", "base_status", "created_at", "schema_ref"];
    for field in &required_fields {
        if issue.get(*field).is_none() {
            errors.push(format!(
                "Issue record {} missing required field: {}",
                index, field
            ));
        }
    }
}

/// Validate attempt_outcome record structure
fn validate_attempt_outcome_record(record: &Value, index: usize, errors: &mut Vec<String>) {
    let outcome = match record.get("attempt_outcome") {
        Some(o) => o,
        None => {
            errors.push(format!(
                "Attempt outcome record {} missing 'attempt_outcome' field",
                index
            ));
            return;
        }
    };

    // Required fields for attempt_outcome records
    let required_fields = [
        "schema_ref",
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
    ];

    for field in &required_fields {
        if outcome.get(*field).is_none() {
            errors.push(format!(
                "Attempt outcome record {} missing required field: {}",
                index, field
            ));
        }
    }

    // Validate schema_ref
    if let Some(schema) = outcome.get("schema_ref").and_then(|s| s.as_str()) {
        if schema != "urn:bead-rs:schema:attempt-outcome:native-v1" {
            errors.push(format!(
                "Invalid schema_ref at record {}: expected 'urn:bead-rs:schema:attempt-outcome:native-v1', got '{}'",
                index, schema
            ));
        }
    }

    // Validate outcome enum value
    if let Some(outcome_val) = outcome.get("outcome").and_then(|o| o.as_str()) {
        let valid_outcomes = [
            "verified_success",
            "work_failure",
            "infrastructure_failure",
            "cancelled",
            "indeterminate",
        ];
        if !valid_outcomes.contains(&outcome_val) {
            errors.push(format!(
                "Invalid outcome value at record {}: '{}'",
                index, outcome_val
            ));
        }
    }

    // Validate action enum value
    if let Some(action_val) = outcome.get("action").and_then(|a| a.as_str()) {
        let valid_actions = ["close", "release", "quarantine", "block", "none"];
        if !valid_actions.contains(&action_val) {
            errors.push(format!(
                "Invalid action value at record {}: '{}'",
                index, action_val
            ));
        }
    }
}

/// Probe attempt-resolution support from checkpoint records
pub fn probe_attempt_resolution_support(records: &[Value]) -> CapabilityInfo {
    let has_attempt_outcomes = records
        .iter()
        .any(|r| r.get("record_type").and_then(|t| t.as_str()) == Some("attempt_outcome"));

    let supports_attempt_resolution = has_attempt_outcomes;

    // If we have attempt outcomes, the binary must support resolve command
    let has_resolve_command = supports_attempt_resolution;

    // If we have attempt outcomes, we support attempt_outcome_count
    let has_attempt_outcome_count = supports_attempt_resolution;

    CapabilityInfo {
        supports_attempt_resolution,
        has_resolve_command,
        has_attempt_outcome_count,
    }
}

/// Detect attempt-resolution support from manifest
pub fn detect_capability_from_manifest(manifest: &Value) -> CapabilityInfo {
    let has_attempt_outcome_count = manifest.get("attempt_outcome_count").is_some();

    // If we have attempt_outcome_count, we support attempt-resolution
    let supports_attempt_resolution = has_attempt_outcome_count;

    // If we support attempt-resolution, we have the resolve command
    let has_resolve_command = supports_attempt_resolution;

    CapabilityInfo {
        supports_attempt_resolution,
        has_resolve_command,
        has_attempt_outcome_count,
    }
}

/// Check if a binary version string indicates attempt-resolution support
pub fn check_binary_version_support(version: &str) -> Result<bool, String> {
    // Parse version string to check for attempt-resolution support
    // This is a simplified check - in real usage you'd query the binary

    // For testing purposes, we'll use commit hash pattern matching
    // Pre-attempt-resolution: beads before 2026-08-31
    // With attempt-resolution: beads from 2026-08-31 onward

    if version.contains("pre-attempt-resolution") || version.contains("bead-pre-attempt-resolution")
    {
        Ok(false)
    } else if version.contains("attempt-resolution")
        || version.contains("0.2.0")
        || version.contains("0.3.0")
    {
        Ok(true)
    } else {
        Err(format!(
            "Unable to determine attempt-resolution support for version: {}",
            version
        ))
    }
}

/// Get attempt outcome records from checkpoint
pub fn get_attempt_outcome_records(records: &[Value]) -> Vec<&Value> {
    records
        .iter()
        .filter(|r| r.get("record_type").and_then(|t| t.as_str()) == Some("attempt_outcome"))
        .collect()
}

/// Get issue records from checkpoint
pub fn get_issue_records(records: &[Value]) -> Vec<&Value> {
    records
        .iter()
        .filter(|r| r.get("record_type").and_then(|t| t.as_str()) == Some("issue"))
        .collect()
}

/// Validate outcome-action combination per spec
pub fn validate_outcome_action_combo(outcome: &str, action: &str) -> Result<(), String> {
    use std::collections::HashSet;

    let valid_combos: HashSet<(&str, &str)> = [
        ("verified_success", "close"),
        ("verified_success", "none"),
        ("verified_success", "release"),
        ("work_failure", "close"),
        ("work_failure", "quarantine"),
        ("work_failure", "release"),
        ("work_failure", "none"),
        ("infrastructure_failure", "none"),
        ("infrastructure_failure", "release"),
        ("cancelled", "close"),
        ("cancelled", "release"),
        ("cancelled", "none"),
        ("indeterminate", "block"),
        ("indeterminate", "release"),
        ("indeterminate", "none"),
    ]
    .iter()
    .cloned()
    .collect();

    if valid_combos.contains(&(outcome, action)) {
        Ok(())
    } else {
        Err(format!(
            "Invalid outcome-action combination: {} + {}",
            outcome, action
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fixture_paths_exist() {
        assert!(fixtures_path().exists());
        assert!(old_format_path().exists());
        assert!(new_format_path().exists());
    }

    #[test]
    fn test_load_old_checkpoint() {
        let checkpoint = load_old_checkpoint().unwrap();
        assert_eq!(
            checkpoint.len(),
            3,
            "Old format should have 3 issue records"
        );

        // All records should be issues
        for record in &checkpoint {
            assert_eq!(
                record.get("record_type").and_then(|t| t.as_str()),
                Some("issue")
            );
        }
    }

    #[test]
    fn test_load_new_checkpoint() {
        let checkpoint = load_new_checkpoint().unwrap();
        assert_eq!(
            checkpoint.len(),
            5,
            "New format should have 5 total records"
        );

        // Should have 3 issues and 2 attempt outcomes
        let issue_count = checkpoint
            .iter()
            .filter(|r| r.get("record_type").and_then(|t| t.as_str()) == Some("issue"))
            .count();
        let attempt_count = checkpoint
            .iter()
            .filter(|r| r.get("record_type").and_then(|t| t.as_str()) == Some("attempt_outcome"))
            .count();

        assert_eq!(issue_count, 3, "Should have 3 issue records");
        assert_eq!(attempt_count, 2, "Should have 2 attempt outcome records");
    }

    #[test]
    fn test_load_old_manifest() {
        let manifest = load_old_manifest().unwrap();

        // Should NOT have attempt_outcome_count
        assert!(manifest.get("attempt_outcome_count").is_none());

        // Should have issue_count
        assert_eq!(
            manifest.get("issue_count").and_then(|v| v.as_u64()),
            Some(3)
        );
    }

    #[test]
    fn test_load_new_manifest() {
        let manifest = load_new_manifest().unwrap();

        // Should have attempt_outcome_count
        assert_eq!(
            manifest
                .get("attempt_outcome_count")
                .and_then(|v| v.as_u64()),
            Some(2)
        );

        // Should have issue_count
        assert_eq!(
            manifest.get("issue_count").and_then(|v| v.as_u64()),
            Some(3)
        );
    }

    #[test]
    fn test_detect_old_format() {
        let manifest = load_old_manifest().unwrap();
        let format = detect_fixture_format(&manifest).unwrap();
        assert_eq!(format, FixtureFormat::Old);
    }

    #[test]
    fn test_detect_new_format() {
        let manifest = load_new_manifest().unwrap();
        let format = detect_fixture_format(&manifest).unwrap();
        assert_eq!(format, FixtureFormat::New);
    }

    #[test]
    fn test_validate_old_format() {
        let result = validate_old_format();
        assert!(
            result.is_valid,
            "Old format validation failed: {:?}",
            result.errors
        );
    }

    #[test]
    fn test_validate_new_format() {
        let result = validate_new_format();
        assert!(
            result.is_valid,
            "New format validation failed: {:?}",
            result.errors
        );
    }

    #[test]
    fn test_probe_old_format_capabilities() {
        let checkpoint = load_old_checkpoint().unwrap();
        let info = probe_attempt_resolution_support(&checkpoint);

        assert!(!info.supports_attempt_resolution);
        assert!(!info.has_resolve_command);
        assert!(!info.has_attempt_outcome_count);
    }

    #[test]
    fn test_probe_new_format_capabilities() {
        let checkpoint = load_new_checkpoint().unwrap();
        let info = probe_attempt_resolution_support(&checkpoint);

        assert!(info.supports_attempt_resolution);
        assert!(info.has_resolve_command);
        assert!(info.has_attempt_outcome_count);
    }

    #[test]
    fn test_detect_capability_from_old_manifest() {
        let manifest = load_old_manifest().unwrap();
        let info = detect_capability_from_manifest(&manifest);

        assert!(!info.supports_attempt_resolution);
        assert!(!info.has_resolve_command);
        assert!(!info.has_attempt_outcome_count);
    }

    #[test]
    fn test_detect_capability_from_new_manifest() {
        let manifest = load_new_manifest().unwrap();
        let info = detect_capability_from_manifest(&manifest);

        assert!(info.supports_attempt_resolution);
        assert!(info.has_resolve_command);
        assert!(info.has_attempt_outcome_count);
    }

    #[test]
    fn test_get_attempt_outcome_records() {
        let checkpoint = load_new_checkpoint().unwrap();
        let outcomes = get_attempt_outcome_records(&checkpoint);

        assert_eq!(outcomes.len(), 2);

        // Verify they're attempt outcomes
        for outcome in &outcomes {
            assert_eq!(
                outcome.get("record_type").and_then(|t| t.as_str()),
                Some("attempt_outcome")
            );
        }
    }

    #[test]
    fn test_get_issue_records() {
        let checkpoint = load_new_checkpoint().unwrap();
        let issues = get_issue_records(&checkpoint);

        assert_eq!(issues.len(), 3);

        // Verify they're issues
        for issue in &issues {
            assert_eq!(
                issue.get("record_type").and_then(|t| t.as_str()),
                Some("issue")
            );
        }
    }

    #[test]
    fn test_validate_outcome_action_combinations() {
        // Valid combinations
        assert!(validate_outcome_action_combo("verified_success", "close").is_ok());
        assert!(validate_outcome_action_combo("verified_success", "none").is_ok());
        assert!(validate_outcome_action_combo("work_failure", "quarantine").is_ok());
        assert!(validate_outcome_action_combo("indeterminate", "block").is_ok());

        // Invalid combinations
        assert!(validate_outcome_action_combo("verified_success", "quarantine").is_err());
        assert!(validate_outcome_action_combo("work_failure", "block").is_err());
    }

    #[test]
    fn test_check_binary_version_support() {
        // Pre-attempt-resolution versions
        assert!(!check_binary_version_support("bead-pre-attempt-resolution").unwrap());
        assert!(!check_binary_version_support("pre-attempt-resolution-0.1.0").unwrap());

        // With attempt-resolution support
        assert!(check_binary_version_support("attempt-resolution-0.2.0").unwrap());
        assert!(check_binary_version_support("0.3.0").unwrap());

        // Unknown version
        assert!(check_binary_version_support("unknown-1.0.0").is_err());
    }

    #[test]
    fn test_checkpoint_jsonl_with_empty_lines() {
        // Test that empty lines are ignored
        let temp_dir = tempfile::tempdir().unwrap();
        let test_file = temp_dir.path().join("test.jsonl");

        fs::write(
            &test_file,
            r#"{"record_type":"issue","issue":{"id":"test1"}}

{"record_type":"issue","issue":{"id":"test2"}}
"#,
        )
        .unwrap();

        let records = load_checkpoint_jsonl(&test_file).unwrap();
        assert_eq!(records.len(), 2);
    }

    #[test]
    fn test_checkpoint_jsonl_missing_record_type() {
        // Test that missing record_type is rejected
        let temp_dir = tempfile::tempdir().unwrap();
        let test_file = temp_dir.path().join("test.jsonl");

        fs::write(&test_file, r#"{"issue":{"id":"test1"}}"#).unwrap();

        let result = load_checkpoint_jsonl(&test_file);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing record_type"));
    }
}

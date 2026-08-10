// comparison.rs - Cross-profile semantic comparison service
//
// This module implements read-only comparison that renders selected native records
// through two explicit installed profiles and reports preserved, transformed, omitted,
// and unsupported semantic fields by canonical field path.

use anyhow::{Context, Result};
use rusqlite::Connection;
use serde_json::Value;
use std::collections::BTreeSet;

/// Represents the comparison result between two profiles
#[derive(Debug, Clone, serde::Serialize)]
pub struct ComparisonResult {
    /// Issue ID being compared
    pub issue_id: String,
    /// Source profile name
    pub source_profile: String,
    /// Target profile name
    pub target_profile: String,
    /// Field-by-field comparison
    pub field_comparisons: Vec<FieldComparison>,
    /// Overall comparison summary
    pub summary: ComparisonSummary,
}

/// Comparison of a single field between two profiles
#[derive(Debug, Clone, serde::Serialize)]
pub struct FieldComparison {
    /// Canonical field path (e.g., "title", "priority", "extensions.custom_field")
    pub field_path: String,
    /// Status of this field in the comparison
    pub status: FieldStatus,
    /// Value from source profile (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_value: Option<Value>,
    /// Value from target profile (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_value: Option<Value>,
}

/// Status of a field in cross-profile comparison
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldStatus {
    /// Field is preserved as-is in both profiles
    Preserved,
    /// Field is transformed between profiles (meaning preserved, representation changed)
    Transformed,
    /// Field is present in source but omitted in target
    Omitted,
    /// Field is present in target but not in source
    Added,
    /// Field is unsupported in one or both profiles
    Unsupported,
}

/// Summary of comparison results
#[derive(Debug, Clone, serde::Serialize)]
pub struct ComparisonSummary {
    /// Total fields compared
    pub total_fields: usize,
    /// Count of preserved fields
    pub preserved_count: usize,
    /// Count of transformed fields
    pub transformed_count: usize,
    /// Count of omitted fields
    pub omitted_count: usize,
    /// Count of added fields
    pub added_count: usize,
    /// Count of unsupported fields
    pub unsupported_count: usize,
}

/// Compare a single issue through two different profiles
pub fn compare_issue_profiles(
    conn: &Connection,
    issue_id: &str,
    source_profile: &str,
    target_profile: &str,
) -> Result<ComparisonResult> {
    // Import the get_issue_by_id function from the issues service
    use crate::profile::ProfileRegistry;
    use crate::service::issues::get_issue_by_id;

    // Get the issue from the database
    let issue = get_issue_by_id(conn, issue_id)
        .with_context(|| format!("Failed to retrieve issue {}", issue_id))?
        .ok_or_else(|| anyhow::anyhow!("Issue {} not found", issue_id))?;

    // Get both profile adapters
    let registry = ProfileRegistry::new();
    let source_adapter = registry
        .get_adapter(source_profile)
        .map_err(|e| anyhow::anyhow!("Source profile '{}' error: {}", source_profile, e))?;
    let target_adapter = registry
        .get_adapter(target_profile)
        .map_err(|e| anyhow::anyhow!("Target profile '{}' error: {}", target_profile, e))?;

    // Render the issue through both profiles
    let source_result = source_adapter.native_to_profile(&issue)?;
    let target_result = target_adapter.native_to_profile(&issue)?;

    // Extract the actual JSON values from TransformResult
    let source_value = source_result.data;
    let target_value = target_result.data;

    // Compare the two rendered representations
    let field_comparisons = compare_values(&source_value, &target_value);

    // Calculate summary statistics
    let summary = calculate_summary(&field_comparisons);

    Ok(ComparisonResult {
        issue_id: issue_id.to_string(),
        source_profile: source_profile.to_string(),
        target_profile: target_profile.to_string(),
        field_comparisons,
        summary,
    })
}

/// Compare two JSON values and return field-by-field comparisons
fn compare_values(source: &Value, target: &Value) -> Vec<FieldComparison> {
    let mut comparisons = Vec::new();

    // Collect all field paths from both values
    let mut all_fields = BTreeSet::new();
    collect_field_paths(source, "", &mut all_fields);
    collect_field_paths(target, "", &mut all_fields);

    // Compare each field
    for field_path in all_fields {
        let source_value = get_value_at_path(source, &field_path);
        let target_value = get_value_at_path(target, &field_path);

        let status = determine_field_status(&source_value, &target_value);

        comparisons.push(FieldComparison {
            field_path,
            status,
            source_value,
            target_value,
        });
    }

    comparisons
}

/// Collect all field paths from a JSON value
fn collect_field_paths(value: &Value, prefix: &str, paths: &mut BTreeSet<String>) {
    match value {
        Value::Object(map) => {
            for (key, val) in map {
                let path = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{}.{}", prefix, key)
                };

                // Add this path
                paths.insert(path.clone());

                // Recursively collect nested paths
                collect_field_paths(val, &path, paths);
            }
        }
        Value::Array(arr) => {
            for (idx, val) in arr.iter().enumerate() {
                let path = format!("{}[{}]", prefix, idx);
                collect_field_paths(val, &path, paths);
            }
        }
        _ => {
            // Primitive value, already added by parent
        }
    }
}

/// Get the value at a specific path
fn get_value_at_path(value: &Value, path: &str) -> Option<Value> {
    if path.is_empty() {
        return Some(value.clone());
    }

    let parts: Vec<&str> = path.split('.').collect();
    let mut current = value;

    for part in parts {
        match current {
            Value::Object(map) => {
                current = map.get(part)?;
            }
            Value::Array(arr) => {
                // Handle array indexing if needed
                if let Ok(idx) = part.parse::<usize>() {
                    current = arr.get(idx)?;
                } else {
                    return None;
                }
            }
            _ => return None,
        }
    }

    Some(current.clone())
}

/// Determine the status of a field based on source and target values
fn determine_field_status(source: &Option<Value>, target: &Option<Value>) -> FieldStatus {
    match (source, target) {
        (Some(src), Some(tgt)) => {
            // Both present - check if they're semantically the same
            if values_semantically_equal(src, tgt) {
                FieldStatus::Preserved
            } else {
                FieldStatus::Transformed
            }
        }
        (Some(_), None) => FieldStatus::Omitted,
        (None, Some(_)) => FieldStatus::Added,
        (None, None) => FieldStatus::Unsupported,
    }
}

/// Check if two JSON values are semantically equal (ignoring formatting)
fn values_semantically_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Number(x), Value::Number(y)) => {
            // Compare numbers semantically (allowing for different representations)
            x.as_f64()
                .and_then(|xf| y.as_f64().map(|yf| (xf - yf).abs() < f64::EPSILON))
                .unwrap_or(false)
        }
        (Value::String(x), Value::String(y)) => x == y,
        (Value::Null, Value::Null) => true,
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len()
                && x.iter()
                    .zip(y.iter())
                    .all(|(a, b)| values_semantically_equal(a, b))
        }
        (Value::Object(x), Value::Object(y)) => {
            // Compare objects by key-value pairs, ignoring order
            let x_keys: BTreeSet<_> = x.keys().collect();
            let y_keys: BTreeSet<_> = y.keys().collect();

            x_keys == y_keys
                && x_keys.iter().all(|key| match (x.get(*key), y.get(*key)) {
                    (Some(xv), Some(yv)) => values_semantically_equal(xv, yv),
                    _ => false,
                })
        }
        _ => false,
    }
}

/// Calculate summary statistics from field comparisons
fn calculate_summary(comparisons: &[FieldComparison]) -> ComparisonSummary {
    let mut preserved_count = 0;
    let mut transformed_count = 0;
    let mut omitted_count = 0;
    let mut added_count = 0;
    let mut unsupported_count = 0;

    for comparison in comparisons {
        match comparison.status {
            FieldStatus::Preserved => preserved_count += 1,
            FieldStatus::Transformed => transformed_count += 1,
            FieldStatus::Omitted => omitted_count += 1,
            FieldStatus::Added => added_count += 1,
            FieldStatus::Unsupported => unsupported_count += 1,
        }
    }

    ComparisonSummary {
        total_fields: comparisons.len(),
        preserved_count,
        transformed_count,
        omitted_count,
        added_count,
        unsupported_count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_semantic_equality_primitive() {
        let a = serde_json::json!(42);
        let b = serde_json::json!(42);
        assert!(values_semantically_equal(&a, &b));
    }

    #[test]
    fn test_semantic_equality_string() {
        let a = serde_json::json!("hello");
        let b = serde_json::json!("hello");
        assert!(values_semantically_equal(&a, &b));
    }

    #[test]
    fn test_semantic_inequality() {
        let a = serde_json::json!("hello");
        let b = serde_json::json!("world");
        assert!(!values_semantically_equal(&a, &b));
    }

    #[test]
    fn test_semantic_equality_objects() {
        let a = serde_json::json!({"a": 1, "b": 2});
        let b = serde_json::json!({"b": 2, "a": 1}); // Different order
        assert!(values_semantically_equal(&a, &b));
    }

    #[test]
    fn test_calculate_summary() {
        let comparisons = vec![
            FieldComparison {
                field_path: "title".to_string(),
                status: FieldStatus::Preserved,
                source_value: Some(serde_json::json!("test")),
                target_value: Some(serde_json::json!("test")),
            },
            FieldComparison {
                field_path: "priority".to_string(),
                status: FieldStatus::Transformed,
                source_value: Some(serde_json::json!(1)),
                target_value: Some(serde_json::json!("P1")),
            },
        ];

        let summary = calculate_summary(&comparisons);
        assert_eq!(summary.total_fields, 2);
        assert_eq!(summary.preserved_count, 1);
        assert_eq!(summary.transformed_count, 1);
    }
}

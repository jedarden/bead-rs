//! Conditional dependency expressions and evaluation.
//!
//! This module implements R017's conditional dependencies:
//! - Bounded declarative predicates over stored fields, labels, issue type, priority, assignee presence, and schema-bound data
//! - Typed all/any/not composition and comparison/set operators
//! - Safe evaluation without scripts, SQL, wall-clock, environment, network, comments, or recursively derived readiness

use crate::error::Error;
use crate::model::BaseStatus;
use crate::store::SqliteStore;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Conditional dependency expression
///
/// Represents a declarative predicate that can be evaluated against an issue's state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum ConditionExpr {
    /// Field equality comparison
    #[serde(rename = "equals")]
    Equals {
        field: String,
        value: serde_json::Value,
    },

    /// Field inequality comparison
    #[serde(rename = "not_equals")]
    NotEquals {
        field: String,
        value: serde_json::Value,
    },

    /// Less than comparison
    #[serde(rename = "less_than")]
    LessThan {
        field: String,
        value: serde_json::Value,
    },

    /// Greater than comparison
    #[serde(rename = "greater_than")]
    GreaterThan {
        field: String,
        value: serde_json::Value,
    },

    /// Less than or equal comparison
    #[serde(rename = "less_than_or_equal")]
    LessThanOrEqual {
        field: String,
        value: serde_json::Value,
    },

    /// Greater than or equal comparison
    #[serde(rename = "greater_than_or_equal")]
    GreaterThanOrEqual {
        field: String,
        value: serde_json::Value,
    },

    /// String contains operator
    #[serde(rename = "contains")]
    Contains { field: String, substring: String },

    /// String starts with operator
    #[serde(rename = "starts_with")]
    StartsWith { field: String, prefix: String },

    /// String ends with operator
    #[serde(rename = "ends_with")]
    EndsWith { field: String, suffix: String },

    /// Field is null operator
    #[serde(rename = "is_null")]
    IsNull { field: String },

    /// Field is not null operator
    #[serde(rename = "is_not_null")]
    IsNotNull { field: String },

    /// Value in set operator
    #[serde(rename = "in")]
    InSet {
        field: String,
        values: Vec<serde_json::Value>,
    },

    /// Value not in set operator
    #[serde(rename = "not_in")]
    NotInSet {
        field: String,
        values: Vec<serde_json::Value>,
    },

    /// Logical AND (all conditions must be true)
    #[serde(rename = "all")]
    All(Vec<ConditionExpr>),

    /// Logical OR (any condition must be true)
    #[serde(rename = "any")]
    Any(Vec<ConditionExpr>),

    /// Logical NOT (condition must be false)
    #[serde(rename = "not")]
    Not(Box<ConditionExpr>),
}

impl ConditionExpr {
    /// Parse a condition expression from JSON
    pub fn from_json(json: &str) -> Result<Self, Error> {
        serde_json::from_str(json)
            .map_err(|e| Error::validation(format!("Invalid condition JSON: {}", e)))
    }

    /// Serialize the condition expression to JSON
    pub fn to_json(&self) -> Result<String, Error> {
        serde_json::to_string(self)
            .map_err(|e| Error::validation(format!("Failed to serialize condition: {}", e)))
    }

    /// Validate that the condition expression only uses supported fields
    pub fn validate_fields(&self) -> Result<(), Error> {
        self.validate_fields_impl(&mut HashSet::new())
    }

    #[allow(clippy::only_used_in_recursion)]
    fn validate_fields_impl(&self, seen: &mut HashSet<String>) -> Result<(), Error> {
        match self {
            // Comparison operators - validate field names
            ConditionExpr::Equals { field, .. }
            | ConditionExpr::NotEquals { field, .. }
            | ConditionExpr::LessThan { field, .. }
            | ConditionExpr::GreaterThan { field, .. }
            | ConditionExpr::LessThanOrEqual { field, .. }
            | ConditionExpr::GreaterThanOrEqual { field, .. }
            | ConditionExpr::Contains { field, .. }
            | ConditionExpr::StartsWith { field, .. }
            | ConditionExpr::EndsWith { field, .. }
            | ConditionExpr::IsNull { field, .. }
            | ConditionExpr::IsNotNull { field, .. }
            | ConditionExpr::InSet { field, .. }
            | ConditionExpr::NotInSet { field, .. } => {
                if !is_supported_field(field) {
                    return Err(Error::validation(format!(
                        "Unsupported field in condition: '{}'. Supported fields are: priority, base_status, issue_type, assignee, manual_blocked, labels, data.*",
                        field
                    )));
                }
            }

            // Logical operators - recurse into sub-conditions
            ConditionExpr::All(conditions) | ConditionExpr::Any(conditions) => {
                for condition in conditions {
                    condition.validate_fields_impl(seen)?;
                }
            }

            ConditionExpr::Not(condition) => {
                condition.validate_fields_impl(seen)?;
            }
        }
        Ok(())
    }
}

/// Check if a field name is supported for conditional dependencies
fn is_supported_field(field: &str) -> bool {
    match field {
        // Core issue fields
        "priority" | "base_status" | "issue_type" | "assignee" | "manual_blocked" => true,

        // Special field accessors
        "labels" => true,

        // Schema-bound data (namespaced)
        f if f.starts_with("data.") => true,

        _ => false,
    }
}

/// Issue state context for condition evaluation
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct IssueContext {
    pub id: String,
    pub priority: i64,
    pub base_status: BaseStatus,
    pub issue_type: String,
    pub assignee: Option<String>,
    pub manual_blocked: bool,
    pub labels: Vec<String>,
    pub data_fields: std::collections::HashMap<String, serde_json::Value>,
}

impl IssueContext {
    /// Create an issue context by querying the store
    pub fn from_store(store: &mut SqliteStore, issue_id: &str) -> Result<Self, Error> {
        let conn = store.conn();

        // Query basic issue fields
        let (priority, base_status_str, issue_type, assignee, manual_blocked): (
            i64,
            String,
            String,
            Option<String>,
            bool,
        ) = conn
            .query_row(
                "SELECT priority, base_status, issue_type, assignee, manual_blocked
                 FROM issues WHERE id = ?",
                [issue_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .map_err(|_| Error::not_found(format!("Issue {}", issue_id)))?;

        let base_status = crate::model::BaseStatus::parse(&base_status_str)
            .map_err(|e| Error::validation(format!("Invalid base_status: {}", e)))?;

        // Query labels
        let mut labels_stmt =
            conn.prepare("SELECT label FROM labels WHERE issue_id = ? ORDER BY label")?;
        let labels: Vec<String> = labels_stmt
            .query_map([issue_id], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;

        // Query schema-bound data
        let mut data_fields = std::collections::HashMap::new();
        let mut data_stmt =
            conn.prepare("SELECT namespace, value FROM issue_data WHERE issue_id = ?")?;
        let mut data_rows = data_stmt.query([issue_id])?;
        while let Some(row) = data_rows.next()? {
            let namespace: String = row.get(0)?;
            let value_str: String = row.get(1)?;
            if let Ok(value) = serde_json::from_str(&value_str) {
                data_fields.insert(namespace, value);
            }
        }

        Ok(IssueContext {
            id: issue_id.to_string(),
            priority,
            base_status,
            issue_type,
            assignee,
            manual_blocked,
            labels,
            data_fields,
        })
    }
}

/// Evaluate a condition expression against an issue context
#[allow(dead_code)]
pub fn evaluate_condition(
    condition: &ConditionExpr,
    context: &IssueContext,
) -> Result<bool, Error> {
    match condition {
        // Comparison operators
        ConditionExpr::Equals { field, value } => {
            evaluate_comparison(field, value, context, |a, b| a == b)
        }

        ConditionExpr::NotEquals { field, value } => {
            evaluate_comparison(field, value, context, |a, b| a != b)
        }

        ConditionExpr::LessThan { field, value } => {
            evaluate_numeric_comparison(field, value, context, |a, b| a < b)
        }

        ConditionExpr::GreaterThan { field, value } => {
            evaluate_numeric_comparison(field, value, context, |a, b| a > b)
        }

        ConditionExpr::LessThanOrEqual { field, value } => {
            evaluate_numeric_comparison(field, value, context, |a, b| a <= b)
        }

        ConditionExpr::GreaterThanOrEqual { field, value } => {
            evaluate_numeric_comparison(field, value, context, |a, b| a >= b)
        }

        ConditionExpr::Contains { field, substring } => {
            // Special handling for labels field which is stored as an array
            if field == "labels" {
                let field_value = get_field_value(field, context);
                if let Some(ref actual) = field_value {
                    if let Some(array) = actual.as_array() {
                        // Check if substring is in any label string
                        return Ok(array.iter().any(|v| {
                            if let Some(s) = v.as_str() {
                                s.contains(substring)
                            } else {
                                false
                            }
                        }));
                    }
                }
                return Ok(false);
            }
            evaluate_string_op(field, context, |s| s.contains(substring))
        }

        ConditionExpr::StartsWith { field, prefix } => {
            evaluate_string_op(field, context, |s| s.starts_with(prefix))
        }

        ConditionExpr::EndsWith { field, suffix } => {
            evaluate_string_op(field, context, |s| s.ends_with(suffix))
        }

        ConditionExpr::IsNull { field } => Ok(get_field_value(field, context).is_none()),

        ConditionExpr::IsNotNull { field } => Ok(get_field_value(field, context).is_some()),

        ConditionExpr::InSet { field, values } => {
            let field_value = get_field_value(field, context);
            Ok(values.iter().any(|v| Some(v.clone()) == field_value))
        }

        ConditionExpr::NotInSet { field, values } => {
            let field_value = get_field_value(field, context);
            Ok(values.iter().all(|v| Some(v.clone()) != field_value))
        }

        // Logical operators
        ConditionExpr::All(conditions) => {
            for condition in conditions {
                if !evaluate_condition(condition, context)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }

        ConditionExpr::Any(conditions) => {
            for condition in conditions {
                if evaluate_condition(condition, context)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }

        ConditionExpr::Not(condition) => Ok(!evaluate_condition(condition, context)?),
    }
}

/// Get the value of a field from the issue context
#[allow(dead_code)]
fn get_field_value(field: &str, context: &IssueContext) -> Option<serde_json::Value> {
    match field {
        "priority" => Some(serde_json::json!(context.priority)),
        "base_status" => {
            let status_str = match context.base_status {
                BaseStatus::Open => "open",
                BaseStatus::InProgress => "in_progress",
                BaseStatus::Deferred => "deferred",
                BaseStatus::Closed => "closed",
            };
            Some(serde_json::json!(status_str))
        }
        "issue_type" => Some(serde_json::json!(context.issue_type)),
        "assignee" => context.assignee.as_ref().map(|a| serde_json::json!(a)),
        "manual_blocked" => Some(serde_json::json!(context.manual_blocked)),
        "labels" => Some(serde_json::json!(context.labels)),
        f if f.starts_with("data.") => {
            let namespace = &f[5..]; // Remove "data." prefix
            context.data_fields.get(namespace).cloned()
        }
        _ => None,
    }
}

/// Evaluate a comparison operation
#[allow(dead_code)]
fn evaluate_comparison<F>(
    field: &str,
    expected: &serde_json::Value,
    context: &IssueContext,
    compare_fn: F,
) -> Result<bool, Error>
where
    F: Fn(&serde_json::Value, &serde_json::Value) -> bool,
{
    let field_value = get_field_value(field, context);
    match field_value {
        Some(ref actual) => Ok(compare_fn(actual, expected)),
        None => Ok(false), // Treat missing field as comparison failure
    }
}

/// Evaluate a numeric comparison operation
#[allow(dead_code)]
fn evaluate_numeric_comparison<F>(
    field: &str,
    expected: &serde_json::Value,
    context: &IssueContext,
    compare_fn: F,
) -> Result<bool, Error>
where
    F: Fn(f64, f64) -> bool,
{
    let field_value = get_field_value(field, context);
    match field_value {
        Some(ref actual) => {
            let actual_num = actual.as_f64().ok_or_else(|| {
                Error::validation(format!("Field '{}' is not numeric in comparison", field))
            })?;
            let expected_num = expected.as_f64().ok_or_else(|| {
                Error::validation(format!(
                    "Expected value is not numeric for field '{}'",
                    field
                ))
            })?;
            Ok(compare_fn(actual_num, expected_num))
        }
        None => Ok(false),
    }
}

/// Evaluate a string operation
fn evaluate_string_op<F>(field: &str, context: &IssueContext, string_fn: F) -> Result<bool, Error>
where
    F: Fn(&str) -> bool,
{
    let field_value = get_field_value(field, context);
    match field_value {
        Some(ref actual) => {
            let actual_str = actual
                .as_str()
                .ok_or_else(|| Error::validation(format!("Field '{}' is not a string", field)))?;
            Ok(string_fn(actual_str))
        }
        None => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::SqliteStore;
    use tempfile::TempDir;

    fn test_store() -> (SqliteStore, TempDir) {
        let temp_dir = tempfile::tempdir().unwrap();
        let beads_path = temp_dir.path().join(".beads");
        std::fs::create_dir(&beads_path).unwrap();

        let db_path = beads_path.join("beads.db");
        let mut store = SqliteStore::with_path(&db_path).unwrap();
        store.apply_migrations().unwrap();

        // Create a test issue
        let conn = store.conn();
        conn.execute(
            "INSERT INTO issues (id, title, priority, base_status, issue_type, manual_blocked, assignee, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            [
                "test-1",
                "Test Issue",
                "2",
                "open",
                "bug",
                "0",
                "alice",
                "2026-08-09T12:00:00Z",
                "2026-08-09T12:00:00Z",
            ],
        ).unwrap();

        // Add labels
        conn.execute(
            "INSERT INTO labels (issue_id, label) VALUES (?1, ?2)",
            ["test-1", "urgent"],
        )
        .unwrap();

        conn.execute(
            "INSERT INTO labels (issue_id, label) VALUES (?1, ?2)",
            ["test-1", "backend"],
        )
        .unwrap();

        // Add schema-bound data
        conn.execute(
            "INSERT INTO issue_data (issue_id, namespace, schema_ref, value) VALUES (?1, ?2, ?3, ?4)",
            ["test-1", "tracker", "urn:bead-rs:schema:data:test", r#"{"ticket_id": "ABC-123"}"#],
        ).unwrap();

        (store, temp_dir)
    }

    #[test]
    fn test_condition_serialization() {
        let condition = ConditionExpr::Equals {
            field: "priority".to_string(),
            value: serde_json::json!(2),
        };

        let json = condition.to_json().unwrap();
        let parsed = ConditionExpr::from_json(&json).unwrap();

        assert_eq!(condition, parsed);
    }

    #[test]
    fn test_validate_supported_fields() {
        let valid_condition = ConditionExpr::Equals {
            field: "priority".to_string(),
            value: serde_json::json!(2),
        };
        assert!(valid_condition.validate_fields().is_ok());

        let invalid_condition = ConditionExpr::Equals {
            field: "unsupported_field".to_string(),
            value: serde_json::json!(2),
        };
        assert!(invalid_condition.validate_fields().is_err());
    }

    #[test]
    fn test_evaluate_equals_condition() {
        let (mut store, _temp) = test_store();
        let context = IssueContext::from_store(&mut store, "test-1").unwrap();

        let condition = ConditionExpr::Equals {
            field: "priority".to_string(),
            value: serde_json::json!(2),
        };

        assert!(evaluate_condition(&condition, &context).unwrap());

        let condition = ConditionExpr::Equals {
            field: "priority".to_string(),
            value: serde_json::json!(3),
        };

        assert!(!evaluate_condition(&condition, &context).unwrap());
    }

    #[test]
    fn test_evaluate_string_condition() {
        let (mut store, _temp) = test_store();
        let context = IssueContext::from_store(&mut store, "test-1").unwrap();

        let condition = ConditionExpr::Equals {
            field: "base_status".to_string(),
            value: serde_json::json!("open"),
        };

        assert!(evaluate_condition(&condition, &context).unwrap());

        let condition = ConditionExpr::StartsWith {
            field: "issue_type".to_string(),
            prefix: "bug".to_string(),
        };

        assert!(evaluate_condition(&condition, &context).unwrap());
    }

    #[test]
    fn test_evaluate_logical_operators() {
        let (mut store, _temp) = test_store();
        let context = IssueContext::from_store(&mut store, "test-1").unwrap();

        // ALL operator
        let condition = ConditionExpr::All(vec![
            ConditionExpr::Equals {
                field: "priority".to_string(),
                value: serde_json::json!(2),
            },
            ConditionExpr::Equals {
                field: "base_status".to_string(),
                value: serde_json::json!("open"),
            },
        ]);

        assert!(evaluate_condition(&condition, &context).unwrap());

        // ANY operator
        let condition = ConditionExpr::Any(vec![
            ConditionExpr::Equals {
                field: "priority".to_string(),
                value: serde_json::json!(3),
            },
            ConditionExpr::Equals {
                field: "priority".to_string(),
                value: serde_json::json!(2),
            },
        ]);

        assert!(evaluate_condition(&condition, &context).unwrap());

        // NOT operator
        let condition = ConditionExpr::Not(Box::new(ConditionExpr::Equals {
            field: "priority".to_string(),
            value: serde_json::json!(3),
        }));

        assert!(evaluate_condition(&condition, &context).unwrap());
    }

    #[test]
    fn test_evaluate_labels_condition() {
        let (mut store, _temp) = test_store();
        let context = IssueContext::from_store(&mut store, "test-1").unwrap();

        // Check if labels contains "urgent"
        let condition = ConditionExpr::Contains {
            field: "labels".to_string(),
            substring: "urgent".to_string(),
        };

        // This tests if "urgent" is in the labels array when serialized
        assert!(evaluate_condition(&condition, &context).unwrap());
    }

    #[test]
    fn test_evaluate_data_field_condition() {
        let (mut store, _temp) = test_store();
        let context = IssueContext::from_store(&mut store, "test-1").unwrap();

        // Check if data.tracker exists
        let condition = ConditionExpr::IsNotNull {
            field: "data.tracker".to_string(),
        };

        assert!(evaluate_condition(&condition, &context).unwrap());

        // Check if data.missing exists
        let condition = ConditionExpr::IsNotNull {
            field: "data.missing".to_string(),
        };

        assert!(!evaluate_condition(&condition, &context).unwrap());
    }

    #[test]
    fn test_evaluate_in_set_condition() {
        let (mut store, _temp) = test_store();
        let context = IssueContext::from_store(&mut store, "test-1").unwrap();

        let condition = ConditionExpr::InSet {
            field: "priority".to_string(),
            values: vec![
                serde_json::json!(1),
                serde_json::json!(2),
                serde_json::json!(3),
            ],
        };

        assert!(evaluate_condition(&condition, &context).unwrap());

        let condition = ConditionExpr::InSet {
            field: "priority".to_string(),
            values: vec![serde_json::json!(1), serde_json::json!(3)],
        };

        assert!(!evaluate_condition(&condition, &context).unwrap());
    }

    #[test]
    fn test_evaluate_numeric_comparison() {
        let (mut store, _temp) = test_store();
        let context = IssueContext::from_store(&mut store, "test-1").unwrap();

        let condition = ConditionExpr::GreaterThan {
            field: "priority".to_string(),
            value: serde_json::json!(1),
        };

        assert!(evaluate_condition(&condition, &context).unwrap());

        let condition = ConditionExpr::LessThan {
            field: "priority".to_string(),
            value: serde_json::json!(3),
        };

        assert!(evaluate_condition(&condition, &context).unwrap());
    }
}

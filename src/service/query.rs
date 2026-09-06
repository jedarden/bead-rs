//! Safe query language and saved views for R004
//!
//! This module implements a versioned, typed query grammar for filtering and
//! projecting issues without exposing raw SQL or private schema details.

use crate::error::{Error, Result};
use crate::model::{BaseStatus, Issue};
use rusqlite::{params_from_iter, Connection};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Parse base status string from SQL into BaseStatus enum
fn parse_base_status(s: String) -> BaseStatus {
    match s.to_lowercase().as_str() {
        "open" => BaseStatus::Open,
        "in_progress" => BaseStatus::InProgress,
        "deferred" => BaseStatus::Deferred,
        "closed" => BaseStatus::Closed,
        _ => BaseStatus::Open, // Default fallback
    }
}

/// Query language version
pub const QUERY_LANGUAGE_VERSION: &str = "v1";

/// Supported query fields - deliberately limited public schema subset
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryField {
    /// Issue identifier
    Id,
    /// Human-readable title
    Title,
    /// Priority level (0-4)
    Priority,
    /// Base status (open, in_progress, closed)
    BaseStatus,
    /// Manual block flag
    ManualBlocked,
    /// Assigned worker
    Assignee,
    /// Issue type
    IssueType,
    /// Creation timestamp
    CreatedAt,
    /// Last update timestamp
    UpdatedAt,
    /// Close timestamp
    ClosedAt,
    /// Close reason
    CloseReason,
    /// Issue description
    Description,
    /// Issue notes
    Notes,
}

impl QueryField {
    /// Convert field to database column name
    fn to_column(&self) -> &'static str {
        match self {
            QueryField::Id => "id",
            QueryField::Title => "title",
            QueryField::Priority => "priority",
            QueryField::BaseStatus => "base_status",
            QueryField::ManualBlocked => "manual_blocked",
            QueryField::Assignee => "assignee",
            QueryField::IssueType => "issue_type",
            QueryField::CreatedAt => "created_at",
            QueryField::UpdatedAt => "updated_at",
            QueryField::ClosedAt => "closed_at",
            QueryField::CloseReason => "close_reason",
            QueryField::Description => "description",
            QueryField::Notes => "notes",
        }
    }

    /// Validate that a field can be used in predicates
    fn is_predicate_valid(&self) -> bool {
        matches!(
            self,
            QueryField::Id
                | QueryField::Title
                | QueryField::Priority
                | QueryField::BaseStatus
                | QueryField::ManualBlocked
                | QueryField::Assignee
                | QueryField::IssueType
                | QueryField::CreatedAt
                | QueryField::UpdatedAt
                | QueryField::ClosedAt
        )
    }

    /// Validate that a field can be used in sorting
    fn is_sortable(&self) -> bool {
        matches!(
            self,
            QueryField::Id
                | QueryField::Title
                | QueryField::Priority
                | QueryField::CreatedAt
                | QueryField::UpdatedAt
        )
    }
}

/// Query operators
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryOperator {
    /// Equality
    Equals,
    /// Inequality
    NotEquals,
    /// Greater than
    GreaterThan,
    /// Less than
    LessThan,
    /// Greater than or equal
    GreaterThanOrEqual,
    /// Less than or equal
    LessThanOrEqual,
    /// String contains substring
    Contains,
    /// String starts with prefix
    StartsWith,
    /// String ends with suffix
    EndsWith,
    /// Is null
    IsNull,
    /// Is not null
    IsNotNull,
}

/// Sort direction
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortDirection {
    Asc,
    Desc,
}

/// Query predicate
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryPredicate {
    pub field: QueryField,
    pub operator: QueryOperator,
    pub value: Option<QueryValue>,
}

/// Query value (supports multiple types)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum QueryValue {
    String(String),
    Integer(i64),
    Boolean(bool),
}

/// Sort specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuerySort {
    pub field: QueryField,
    pub direction: SortDirection,
}

/// Projection specification (which fields to return)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryProjection {
    pub fields: Vec<QueryField>,
}

/// Main query structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Query {
    pub version: String,
    pub predicates: Vec<QueryPredicate>,
    pub sort: Vec<QuerySort>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub projection: Option<QueryProjection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

/// Saved view structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedView {
    pub id: String,
    pub name: String,
    pub description: String,
    pub query_json: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Parse query from JSON string
pub fn parse_query(json: &str) -> Result<Query> {
    let query: Query = serde_json::from_str(json)
        .map_err(|e| Error::validation(format!("Invalid query JSON: {}", e)))?;

    // Validate version
    if query.version != QUERY_LANGUAGE_VERSION {
        return Err(Error::validation(format!(
            "Unsupported query version '{}'. Only '{}' is supported.",
            query.version, QUERY_LANGUAGE_VERSION
        )));
    }

    // Validate predicates
    for predicate in &query.predicates {
        if !predicate.field.is_predicate_valid() {
            return Err(Error::validation(format!(
                "Field '{:?}' cannot be used in predicates",
                predicate.field
            )));
        }

        // Validate operator/value compatibility
        validate_operator_value(&predicate.operator, &predicate.value)?;
    }

    // Validate sort fields
    for sort in &query.sort {
        if !sort.field.is_sortable() {
            return Err(Error::validation(format!(
                "Field '{:?}' cannot be used for sorting",
                sort.field
            )));
        }
    }

    Ok(query)
}

/// Validate operator and value compatibility
fn validate_operator_value(operator: &QueryOperator, value: &Option<QueryValue>) -> Result<()> {
    match operator {
        QueryOperator::IsNull | QueryOperator::IsNotNull => {
            if value.is_some() {
                return Err(Error::validation("NULL operators must not have a value"));
            }
        }
        _ => {
            if value.is_none() {
                return Err(Error::validation("Operator requires a value"));
            }
        }
    }
    Ok(())
}

/// Build SQL WHERE clause from predicates
fn build_where_clause(predicates: &[QueryPredicate]) -> (String, Vec<String>) {
    if predicates.is_empty() {
        return (String::from("1=1"), Vec::new());
    }

    let mut conditions = Vec::new();
    let mut params = Vec::new();

    for predicate in predicates {
        let column = predicate.field.to_column();
        let (condition, param) = build_condition(predicate, column);
        conditions.push(condition);
        params.extend(param);
    }

    let where_clause = conditions.join(" AND ");
    (where_clause, params)
}

/// Build individual condition
fn build_condition(predicate: &QueryPredicate, column: &str) -> (String, Vec<String>) {
    let operator_sql = match &predicate.operator {
        QueryOperator::Equals => "=",
        QueryOperator::NotEquals => "!=",
        QueryOperator::GreaterThan => ">",
        QueryOperator::LessThan => "<",
        QueryOperator::GreaterThanOrEqual => ">=",
        QueryOperator::LessThanOrEqual => "<=",
        QueryOperator::Contains => "LIKE",
        QueryOperator::StartsWith => "LIKE",
        QueryOperator::EndsWith => "LIKE",
        QueryOperator::IsNull => "IS NULL",
        QueryOperator::IsNotNull => "IS NOT NULL",
    };

    let mut params = Vec::new();

    let condition = if let QueryOperator::Contains
    | QueryOperator::StartsWith
    | QueryOperator::EndsWith = &predicate.operator
    {
        // String matching operators
        if let Some(QueryValue::String(ref s)) = predicate.value {
            let pattern = match &predicate.operator {
                QueryOperator::Contains => format!("%{}%", s),
                QueryOperator::StartsWith => format!("{}%", s),
                QueryOperator::EndsWith => format!("%{}", s),
                _ => unreachable!(),
            };
            params.push(pattern);
            format!("{} {} ?", column, operator_sql)
        } else {
            // Should never happen due to validation
            format!("{} = 0", column) // Invalid comparison
        }
    } else if let QueryOperator::IsNull | QueryOperator::IsNotNull = &predicate.operator {
        // NULL operators don't use values
        format!("{} {}", column, operator_sql)
    } else {
        // Standard operators with parameters
        if let Some(ref value) = predicate.value {
            let param = match value {
                QueryValue::String(s) => s.clone(),
                QueryValue::Integer(i) => i.to_string(),
                QueryValue::Boolean(b) => {
                    if *b {
                        "1".to_string()
                    } else {
                        "0".to_string()
                    }
                }
            };
            params.push(param);
        }
        format!("{} {} ?", column, operator_sql)
    };

    (condition, params)
}

/// Build ORDER BY clause
fn build_order_clause(sort: &[QuerySort]) -> String {
    if sort.is_empty() {
        return String::from("priority ASC, created_at ASC, id ASC"); // Default FIFO ordering
    }

    let orders: Vec<String> = sort
        .iter()
        .map(|s| {
            let dir = match s.direction {
                SortDirection::Asc => "ASC",
                SortDirection::Desc => "DESC",
            };
            format!("{} {}", s.field.to_column(), dir)
        })
        .collect();

    orders.join(", ")
}

/// Execute query and return matching issues
pub fn execute_query(conn: &Connection, query: &Query) -> Result<Vec<Issue>> {
    let (where_clause, params) = build_where_clause(&query.predicates);
    let order_clause = build_order_clause(&query.sort);
    let limit_clause = query
        .limit
        .map(|l| format!("LIMIT {}", l))
        .unwrap_or_default();

    let sql = format!(
        "SELECT id, title, priority, base_status, created_at, updated_at,
                description, notes, assignee, issue_type, manual_blocked,
                closed_at, close_reason, source_repo, profile, schema_ref,
                NULLIF(claim_epoch, 0)
         FROM issues
         WHERE {}
         ORDER BY {}
         {}",
        where_clause, order_clause, limit_clause
    );

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to prepare query: {}", e)))?;

    let issue_iter = stmt
        .query_map(params_from_iter(params.iter()), |row| {
            Ok(Issue {
                id: row.get(0)?,
                title: row.get(1)?,
                priority: row.get(2)?,
                base_status: parse_base_status(row.get(3)?),
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
                description: row.get(6)?,
                notes: row.get(7)?,
                assignee: row.get(8)?,
                issue_type: row.get(9)?,
                manual_blocked: {
                    let blocked_val: Option<i64> = row.get(10)?;
                    blocked_val.map(|v| v == 1)
                },
                closed_at: row.get(11)?,
                close_reason: row.get(12)?,
                source_repo: row.get(13)?,
                profile: row.get(14)?,
                schema_ref: row.get(15)?,
                claim_epoch: row.get(16)?,
                data: None,                 // Data loaded separately if needed
                extensions: HashMap::new(), // Extensions loaded separately if needed
                revision: None,             // Revision loaded separately if needed
            })
        })
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to execute query: {}", e)))?;

    let mut issues = Vec::new();
    for issue in issue_iter {
        issues.push(
            issue.map_err(|e| Error::Internal(anyhow::anyhow!("Failed to parse issue: {}", e)))?,
        );
    }

    Ok(issues)
}

/// Project issue to include only specified fields
pub fn project_issue(issue: &Issue, projection: &QueryProjection) -> Result<serde_json::Value> {
    let mut result = serde_json::Map::new();

    for field in &projection.fields {
        let value = match field {
            QueryField::Id => serde_json::to_value(&issue.id)?,
            QueryField::Title => serde_json::to_value(&issue.title)?,
            QueryField::Priority => serde_json::to_value(issue.priority)?,
            QueryField::BaseStatus => serde_json::to_value(issue.base_status)?,
            QueryField::ManualBlocked => serde_json::to_value(issue.manual_blocked)?,
            QueryField::Assignee => serde_json::to_value(&issue.assignee)?,
            QueryField::IssueType => serde_json::to_value(&issue.issue_type)?,
            QueryField::CreatedAt => serde_json::to_value(&issue.created_at)?,
            QueryField::UpdatedAt => serde_json::to_value(&issue.updated_at)?,
            QueryField::ClosedAt => serde_json::to_value(&issue.closed_at)?,
            QueryField::CloseReason => serde_json::to_value(&issue.close_reason)?,
            QueryField::Description => serde_json::to_value(&issue.description)?,
            QueryField::Notes => serde_json::to_value(&issue.notes)?,
        };
        result.insert(format!("{:?}", field), value);
    }

    Ok(serde_json::Value::Object(result))
}

/// Save a query as a named view
pub fn save_view(conn: &Connection, name: &str, description: &str, query_json: &str) -> Result<()> {
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string());

    // Generate view ID using timestamp and random bytes
    let view_id = format!("view-{}", now.replace(":", "").replace("-", ""));

    conn.execute(
        "INSERT INTO saved_views (id, name, description, query_json, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(name) DO UPDATE SET
            query_json = ?4,
            updated_at = ?6",
        [&view_id, name, description, query_json, &now, &now],
    )
    .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to save view: {}", e)))?;

    Ok(())
}

/// List all saved views
pub fn list_views(conn: &Connection) -> Result<Vec<SavedView>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, name, description, query_json, created_at, updated_at
         FROM saved_views
         ORDER BY name",
    )?;

    let views = stmt
        .query_map([], |row| {
            Ok(SavedView {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                query_json: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        })
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to list views: {}", e)))?;

    let mut result = Vec::new();
    for view in views {
        result.push(
            view.map_err(|e| Error::Internal(anyhow::anyhow!("Failed to parse view: {}", e)))?,
        );
    }

    Ok(result)
}

/// Delete a saved view
pub fn delete_view(conn: &Connection, name: &str) -> Result<()> {
    let rows_affected = conn
        .execute("DELETE FROM saved_views WHERE name = ?1", [name])
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to delete view: {}", e)))?;

    if rows_affected == 0 {
        return Err(Error::validation(format!("View '{}' not found", name)));
    }

    Ok(())
}

/// Get a saved view by name
pub fn get_view(conn: &Connection, name: &str) -> Result<SavedView> {
    conn.query_row(
        "SELECT id, name, description, query_json, created_at, updated_at
         FROM saved_views
         WHERE name = ?1",
        [name],
        |row| {
            Ok(SavedView {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                query_json: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        },
    )
    .map_err(|e| {
        use rusqlite::Error;
        match e {
            Error::QueryReturnedNoRows => {
                crate::Error::validation(format!("View '{}' not found", name))
            }
            _ => crate::Error::Internal(anyhow::anyhow!("Failed to get view: {}", e)),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_field_validation() {
        assert!(QueryField::Id.is_predicate_valid());
        assert!(QueryField::Title.is_predicate_valid());
        assert!(QueryField::Priority.is_predicate_valid());
        assert!(!QueryField::Description.is_predicate_valid()); // Cannot be used in predicates
    }

    #[test]
    fn test_sort_field_validation() {
        assert!(QueryField::Id.is_sortable());
        assert!(QueryField::Priority.is_sortable());
        assert!(!QueryField::BaseStatus.is_sortable()); // Cannot be used for sorting
    }

    #[test]
    fn test_parse_simple_query() {
        let json = r#"{
            "version": "v1",
            "predicates": [
                {"field": "priority", "operator": "greater_than_or_equal", "value": 2}
            ],
            "sort": [
                {"field": "priority", "direction": "asc"}
            ]
        }"#;

        let query = parse_query(json).unwrap();
        assert_eq!(query.version, "v1");
        assert_eq!(query.predicates.len(), 1);
        assert_eq!(query.sort.len(), 1);
    }

    #[test]
    fn test_parse_invalid_version() {
        let json = r#"{
            "version": "v2",
            "predicates": [],
            "sort": []
        }"#;

        assert!(parse_query(json).is_err());
    }

    #[test]
    fn test_build_where_clause() {
        let predicates = vec![
            QueryPredicate {
                field: QueryField::Priority,
                operator: QueryOperator::GreaterThan,
                value: Some(QueryValue::Integer(1)),
            },
            QueryPredicate {
                field: QueryField::BaseStatus,
                operator: QueryOperator::Equals,
                value: Some(QueryValue::String("open".to_string())),
            },
        ];

        let (where_clause, params) = build_where_clause(&predicates);
        assert!(where_clause.contains("priority >"));
        assert!(where_clause.contains("base_status ="));
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn test_build_order_clause() {
        let sort = vec![
            QuerySort {
                field: QueryField::Priority,
                direction: SortDirection::Desc,
            },
            QuerySort {
                field: QueryField::CreatedAt,
                direction: SortDirection::Asc,
            },
        ];

        let order_clause = build_order_clause(&sort);
        assert!(order_clause.contains("priority DESC"));
        assert!(order_clause.contains("created_at ASC"));
    }
}

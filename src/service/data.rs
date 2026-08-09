//! Structured bead data operations
//!
//! This module implements atomic CRUD operations for namespaced JSON values
//! as specified in R018. Each namespace is governed by its own immutable
//! schema reference, and unknown schemas are preserved for interchange but
//! fail closed for native mutation.

use crate::error::{Error, Result};
use crate::store::SqliteStore;

/// Set a structured data value for an issue
///
/// Sets or replaces the JSON value for a specific namespace with schema governance.
/// The operation is atomic and validates that the issue exists.
pub fn set_data(
    store: &mut SqliteStore,
    issue_id: &str,
    namespace: &str,
    schema_ref: &str,
    value: &serde_json::Value,
) -> Result<()> {
    let conn = store.conn();
    let tx = conn.unchecked_transaction()?;

    // Verify issue exists
    let issue_exists: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM issues WHERE id = ?)",
        [issue_id],
        |row| row.get(0),
    )?;

    if !issue_exists {
        return Err(Error::not_found(format!(
            "Cannot set data on nonexistent issue: {}",
            issue_id
        )));
    }

    // Validate namespace
    validate_namespace(namespace)?;

    // Validate schema_ref
    validate_schema_ref(schema_ref)?;

    // Serialize JSON value to string
    let value_str = serde_json::to_string(value)
        .map_err(|e| Error::validation(format!("Invalid JSON value: {}", e)))?;

    // Insert or replace the data value
    tx.execute(
        "INSERT OR REPLACE INTO issue_data (issue_id, namespace, schema_ref, value) VALUES (?1, ?2, ?3, ?4)",
        [issue_id, namespace, schema_ref, &value_str],
    )?;

    tx.commit()?;
    Ok(())
}

/// Get a structured data value from an issue
///
/// Retrieves the JSON value for a specific namespace if it exists.
pub fn get_data(
    store: &mut SqliteStore,
    issue_id: &str,
    namespace: &str,
) -> Result<Option<(String, serde_json::Value)>> {
    let conn = store.conn();

    let mut stmt = conn
        .prepare("SELECT schema_ref, value FROM issue_data WHERE issue_id = ? AND namespace = ?")?;

    let result = stmt.query_row([issue_id, namespace], |row| {
        let schema_ref: String = row.get(0)?;
        let value_str: String = row.get(1)?;
        let value: serde_json::Value = serde_json::from_str(&value_str)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        Ok((schema_ref, value))
    });

    match result {
        Ok(data) => Ok(Some(data)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// List all structured data namespaces for an issue
///
/// Returns all namespaces with their schema references for an issue.
pub fn list_data(store: &mut SqliteStore, issue_id: &str) -> Result<Vec<(String, String)>> {
    let conn = store.conn();

    // Verify issue exists
    let issue_exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM issues WHERE id = ?)",
        [issue_id],
        |row| row.get(0),
    )?;

    if !issue_exists {
        return Err(Error::not_found(format!(
            "Cannot list data for nonexistent issue: {}",
            issue_id
        )));
    }

    let mut stmt = conn.prepare(
        "SELECT namespace, schema_ref FROM issue_data WHERE issue_id = ? ORDER BY namespace",
    )?;

    let namespaces = stmt
        .query_map([issue_id], |row| {
            let namespace: String = row.get(0)?;
            let schema_ref: String = row.get(1)?;
            Ok((namespace, schema_ref))
        })?
        .collect::<std::result::Result<Vec<_>, rusqlite::Error>>()?;

    Ok(namespaces)
}

/// Remove a structured data value from an issue
///
/// Removes the JSON value for a specific namespace if it exists.
/// Idempotent - succeeds whether or not the namespace exists.
pub fn remove_data(store: &mut SqliteStore, issue_id: &str, namespace: &str) -> Result<()> {
    let conn = store.conn();
    let tx = conn.unchecked_transaction()?;

    // Verify issue exists
    let issue_exists: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM issues WHERE id = ?)",
        [issue_id],
        |row| row.get(0),
    )?;

    if !issue_exists {
        return Err(Error::not_found(format!(
            "Cannot remove data from nonexistent issue: {}",
            issue_id
        )));
    }

    // Validate namespace
    validate_namespace(namespace)?;

    // Remove the data value (idempotent - no error if not found)
    tx.execute(
        "DELETE FROM issue_data WHERE issue_id = ? AND namespace = ?",
        [issue_id, namespace],
    )?;

    tx.commit()?;
    Ok(())
}

/// Validate a namespace string
///
/// Namespaces must be nonempty, lowercase alphanumeric with hyphens/underscores,
/// 1-64 bytes, must start with a letter.
fn validate_namespace(namespace: &str) -> Result<()> {
    if namespace.is_empty() {
        return Err(Error::validation("Namespace cannot be empty"));
    }

    if namespace.len() > 64 {
        return Err(Error::validation("Namespace cannot exceed 64 bytes"));
    }

    if !namespace.chars().next().unwrap().is_ascii_lowercase() {
        return Err(Error::validation(
            "Namespace must start with a lowercase letter",
        ));
    }

    if !namespace
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
    {
        return Err(Error::validation(
            "Namespace must contain only lowercase letters, digits, hyphens, and underscores",
        ));
    }

    Ok(())
}

/// Validate a schema reference
///
/// Schema references must be nonempty and reasonable length.
fn validate_schema_ref(schema_ref: &str) -> Result<()> {
    if schema_ref.is_empty() {
        return Err(Error::validation("Schema reference cannot be empty"));
    }

    if schema_ref.len() > 512 {
        return Err(Error::validation(
            "Schema reference cannot exceed 512 bytes",
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::SqliteStore;
    use serial_test::serial;
    use tempfile::TempDir;

    fn test_store() -> (TempDir, SqliteStore) {
        let temp_dir = TempDir::new().unwrap();
        let beads_path = temp_dir.path().join(".beads");
        std::fs::create_dir(&beads_path).unwrap();

        let db_path = beads_path.join("beads.db");
        let mut store = SqliteStore::with_path(&db_path).unwrap();
        store.apply_migrations().unwrap();

        (temp_dir, store)
    }

    fn create_test_issue(store: &mut SqliteStore) -> String {
        let issue_id = "test-issue-123";
        let conn = store.conn();
        conn.execute(
            "INSERT INTO issues (id, title, priority, base_status, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            [issue_id, "Test Issue", "0", "open", "2026-08-09T00:00:00Z", "2026-08-09T00:00:00Z"],
        ).unwrap();
        issue_id.to_string()
    }

    #[test]
    #[serial]
    fn test_set_and_get_data() {
        let (_temp, mut store) = test_store();
        let issue_id = create_test_issue(&mut store);

        let value = serde_json::json!({"key": "value"});
        set_data(&mut store, &issue_id, "test", "schema:1", &value).unwrap();

        let result = get_data(&mut store, &issue_id, "test").unwrap();
        assert!(result.is_some());
        let (schema_ref, retrieved_value) = result.unwrap();
        assert_eq!(schema_ref, "schema:1");
        assert_eq!(retrieved_value, value);
    }

    #[test]
    #[serial]
    #[serial]
    fn test_get_nonexistent_namespace() {
        let (_temp, mut store) = test_store();
        let issue_id = create_test_issue(&mut store);

        let result = get_data(&mut store, &issue_id, "nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    #[serial]
    fn test_set_replaces_existing() {
        let (_temp, mut store) = test_store();
        let issue_id = create_test_issue(&mut store);

        let value1 = serde_json::json!({"old": "data"});
        set_data(&mut store, &issue_id, "test", "schema:1", &value1).unwrap();

        let value2 = serde_json::json!({"new": "data"});
        set_data(&mut store, &issue_id, "test", "schema:2", &value2).unwrap();

        let result = get_data(&mut store, &issue_id, "test").unwrap();
        assert!(result.is_some());
        let (schema_ref, retrieved_value) = result.unwrap();
        assert_eq!(schema_ref, "schema:2");
        assert_eq!(retrieved_value, value2);
    }

    #[test]
    #[serial]
    fn test_list_data() {
        let (_temp, mut store) = test_store();
        let issue_id = create_test_issue(&mut store);

        set_data(
            &mut store,
            &issue_id,
            "alpha",
            "schema:1",
            &serde_json::json!({"a": 1}),
        )
        .unwrap();
        set_data(
            &mut store,
            &issue_id,
            "beta",
            "schema:2",
            &serde_json::json!({"b": 2}),
        )
        .unwrap();
        set_data(
            &mut store,
            &issue_id,
            "gamma",
            "schema:3",
            &serde_json::json!({"c": 3}),
        )
        .unwrap();

        let namespaces = list_data(&mut store, &issue_id).unwrap();
        assert_eq!(namespaces.len(), 3);
        assert_eq!(namespaces[0], ("alpha".to_string(), "schema:1".to_string()));
        assert_eq!(namespaces[1], ("beta".to_string(), "schema:2".to_string()));
        assert_eq!(namespaces[2], ("gamma".to_string(), "schema:3".to_string()));
    }

    #[test]
    #[serial]
    fn test_list_empty_issue() {
        let (_temp, mut store) = test_store();
        let issue_id = create_test_issue(&mut store);

        let namespaces = list_data(&mut store, &issue_id).unwrap();
        assert_eq!(namespaces.len(), 0);
    }

    #[test]
    #[serial]
    fn test_remove_data() {
        let (_temp, mut store) = test_store();
        let issue_id = create_test_issue(&mut store);

        set_data(
            &mut store,
            &issue_id,
            "test",
            "schema:1",
            &serde_json::json!({"key": "value"}),
        )
        .unwrap();
        remove_data(&mut store, &issue_id, "test").unwrap();

        let result = get_data(&mut store, &issue_id, "test").unwrap();
        assert!(result.is_none());
    }

    #[test]
    #[serial]
    fn test_remove_data_idempotent() {
        let (_temp, mut store) = test_store();
        let issue_id = create_test_issue(&mut store);

        // Remove should succeed even if namespace doesn't exist
        remove_data(&mut store, &issue_id, "nonexistent").unwrap();
    }

    #[test]
    #[serial]
    fn test_set_data_on_nonexistent_issue() {
        let (_temp, mut store) = test_store();

        let result = set_data(
            &mut store,
            "nonexistent",
            "test",
            "schema:1",
            &serde_json::json!({"key": "value"}),
        );
        assert!(result.is_err());
    }

    #[test]
    #[serial]
    fn test_validate_namespace() {
        assert!(validate_namespace("valid").is_ok());
        assert!(validate_namespace("valid-name").is_ok());
        assert!(validate_namespace("valid_name").is_ok());
        assert!(validate_namespace("valid-name_123").is_ok());

        assert!(validate_namespace("").is_err());
        assert!(validate_namespace("Invalid").is_err());
        assert!(validate_namespace("1invalid").is_err());
        assert!(validate_namespace("invalid@name").is_err());
        assert!(validate_namespace(&"a".repeat(65)).is_err());
    }

    #[test]
    #[serial]
    fn test_validate_schema_ref() {
        assert!(validate_schema_ref("schema:1").is_ok());
        assert!(validate_schema_ref("http://example.com/schema").is_ok());

        assert!(validate_schema_ref("").is_err());
        assert!(validate_schema_ref(&"a".repeat(513)).is_err());
    }

    #[test]
    #[serial]
    fn test_list_data_on_nonexistent_issue() {
        let (_temp, mut store) = test_store();

        let result = list_data(&mut store, "nonexistent");
        assert!(result.is_err());
    }

    #[test]
    #[serial]
    fn test_remove_data_on_nonexistent_issue() {
        let (_temp, mut store) = test_store();

        let result = remove_data(&mut store, "nonexistent", "test");
        assert!(result.is_err());
    }

    #[test]
    #[serial]
    fn test_complex_json_value() {
        let (_temp, mut store) = test_store();
        let issue_id = create_test_issue(&mut store);

        let complex_value = serde_json::json!({
            "string": "test",
            "number": 42,
            "boolean": true,
            "array": [1, 2, 3],
            "nested": {
                "key": "value"
            }
        });

        set_data(&mut store, &issue_id, "complex", "schema:1", &complex_value).unwrap();

        let result = get_data(&mut store, &issue_id, "complex").unwrap();
        assert!(result.is_some());
        let (_, retrieved_value) = result.unwrap();
        assert_eq!(retrieved_value, complex_value);
    }

    #[test]
    #[serial]
    fn test_multiple_namespaces_per_issue() {
        let (_temp, mut store) = test_store();
        let issue_id = create_test_issue(&mut store);

        set_data(
            &mut store,
            &issue_id,
            "config",
            "schema:1",
            &serde_json::json!({"setting": "value"}),
        )
        .unwrap();
        set_data(
            &mut store,
            &issue_id,
            "metrics",
            "schema:2",
            &serde_json::json!({"count": 100}),
        )
        .unwrap();
        set_data(
            &mut store,
            &issue_id,
            "state",
            "schema:3",
            &serde_json::json!({"active": true}),
        )
        .unwrap();

        // Each namespace should be retrievable independently
        let config = get_data(&mut store, &issue_id, "config").unwrap();
        assert!(config.is_some());

        let metrics = get_data(&mut store, &issue_id, "metrics").unwrap();
        assert!(metrics.is_some());

        let state = get_data(&mut store, &issue_id, "state").unwrap();
        assert!(state.is_some());

        // List should show all three
        let namespaces = list_data(&mut store, &issue_id).unwrap();
        assert_eq!(namespaces.len(), 3);
    }
}

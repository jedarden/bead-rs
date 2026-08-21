//! Structured bead data operations
//!
//! This module implements atomic CRUD operations for namespaced JSON values
//! as specified in R018. Each namespace is governed by its own immutable
//! schema reference, and unknown schemas are preserved for interchange but
//! fail closed for native mutation.

use crate::error::{Error, Result};
use crate::store::SqliteStore;
use rusqlite::OptionalExtension;

/// Set a structured data value for an issue
///
/// Sets or replaces the JSON value for a specific namespace with schema governance.
/// The operation is atomic and validates that the issue exists.
///
/// A committed set that stores a new pair or changes the schema reference or
/// value appends a `data_set` audit event in the same transaction; an
/// idempotent re-set of the identical row appends none.
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

    // The set is upsert-shaped: INSERT OR REPLACE reports one row changed
    // whether it creates the pair or rewrites an identical one, so the prior
    // row decides whether a semantic mutation is about to commit. A re-set of
    // the same (namespace, schema_ref, value) is a no-op and appends no event.
    let prior: Option<(String, String)> = tx
        .query_row(
            "SELECT schema_ref, value FROM issue_data WHERE issue_id = ?1 AND namespace = ?2",
            [issue_id, namespace],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;

    // Insert or replace the data value
    tx.execute(
        "INSERT OR REPLACE INTO issue_data (issue_id, namespace, schema_ref, value) VALUES (?1, ?2, ?3, ?4)",
        [issue_id, namespace, schema_ref, &value_str],
    )?;

    // Record the mutation as an audit event inside this transaction: the live
    // event sequence is the dirtiness signal (plan 6.2.1 P3), so an unrecorded
    // data set would silently read as no change. The document body is
    // schema-governed but unbounded, so the detail records only the namespace
    // and schema_ref - never the body itself.
    let is_semantic_change = prior
        .as_ref()
        .map(|(prior_schema, prior_value)| prior_schema != schema_ref || prior_value != &value_str)
        .unwrap_or(true);

    if is_semantic_change {
        append_data_event(
            &tx,
            issue_id,
            "data_set",
            &serde_json::json!({
                "actor": "system",
                "namespace": namespace,
                "schema_ref": schema_ref,
            }),
        )?;
    }

    tx.commit()?;
    Ok(())
}

/// Append the audit event for a structured-data mutation.
///
/// The event is inserted on the mutation's own transaction, so it commits (or
/// rolls back) with the row it describes. `issue_id` is the event's issue
/// subject. Callers append only after a real row change, which guarantees the
/// issue exists (both mutations verify it up front), so the events table's own
/// foreign key holds.
fn append_data_event(
    tx: &rusqlite::Transaction,
    issue_id: &str,
    kind: &str,
    detail: &serde_json::Value,
) -> Result<()> {
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string());

    let mut stmt = tx.prepare_cached(
        "INSERT INTO events (issue_id, kind, actor, time, detail) VALUES (?1, ?2, ?3, ?4, ?5)",
    )?;

    stmt.execute((issue_id, kind, "system", now, detail.to_string()))?;

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
/// A remove that deletes a row appends a `data_removed` audit event in the
/// same transaction; an idempotent no-op remove appends none.
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

    // Capture the governed schema reference before the row goes away: the
    // remove takes no schema_ref, but the audit detail still records it.
    let prior_schema_ref: Option<String> = tx
        .query_row(
            "SELECT schema_ref FROM issue_data WHERE issue_id = ?1 AND namespace = ?2",
            [issue_id, namespace],
            |row| row.get(0),
        )
        .optional()?;

    // Remove the data value (idempotent - no error if not found). Removing a
    // nonexistent namespace commits no semantic mutation, so it must append no
    // event.
    let removed = tx.execute(
        "DELETE FROM issue_data WHERE issue_id = ? AND namespace = ?",
        [issue_id, namespace],
    )?;

    // Record the mutation as an audit event inside this transaction: the live
    // event sequence is the dirtiness signal (plan 6.2.1 P3), so an unrecorded
    // data remove would silently read as no change. The detail records the
    // namespace and the removed row's schema_ref - never the document body.
    if removed > 0 {
        append_data_event(
            &tx,
            issue_id,
            "data_removed",
            &serde_json::json!({
                "actor": "system",
                "namespace": namespace,
                "schema_ref": prior_schema_ref,
            }),
        )?;
    }

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

    /// Reads every event row as (issue_id, kind, detail-as-JSON)
    fn read_events(store: &mut SqliteStore) -> Vec<(Option<String>, String, serde_json::Value)> {
        let conn = store.conn();
        conn.prepare("SELECT issue_id, kind, detail FROM events ORDER BY sequence")
            .unwrap()
            .query_map([], |row| {
                let issue_id: Option<String> = row.get(0)?;
                let kind: String = row.get(1)?;
                let detail: String = row.get(2)?;
                Ok((issue_id, kind, serde_json::from_str(&detail).unwrap()))
            })
            .unwrap()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap()
    }

    #[test]
    #[serial]
    fn test_set_data_appends_event() {
        let (_temp, mut store) = test_store();
        let issue_id = create_test_issue(&mut store);

        set_data(
            &mut store,
            &issue_id,
            "config",
            "schema:1",
            &serde_json::json!({"key": "value"}),
        )
        .unwrap();

        let events = read_events(&mut store);
        assert_eq!(events.len(), 1, "one event per committed set");

        let (event_issue, kind, detail) = &events[0];
        assert_eq!(event_issue.as_deref(), Some(issue_id.as_str()));
        assert_eq!(kind, "data_set");
        assert_eq!(detail["actor"], "system");
        assert_eq!(detail["namespace"], "config");
        assert_eq!(detail["schema_ref"], "schema:1");
    }

    #[test]
    #[serial]
    fn test_set_data_event_omits_document_body() {
        let (_temp, mut store) = test_store();
        let issue_id = create_test_issue(&mut store);

        // A credential-shaped body must never reach the event log verbatim
        let fixture_body = "bearer-sup3r-s3cret-fixture-value";
        set_data(
            &mut store,
            &issue_id,
            "config",
            "schema:1",
            &serde_json::json!({"token": fixture_body}),
        )
        .unwrap();

        let raw_detail = {
            let conn = store.conn();
            conn.query_row(
                "SELECT detail FROM events WHERE kind = 'data_set'",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap()
        };
        assert!(
            !raw_detail.contains(fixture_body),
            "event detail must not carry the document body"
        );
        assert!(!raw_detail.contains("token"), "no body key leaks either");
    }

    #[test]
    #[serial]
    fn test_set_data_identical_reset_appends_no_event() {
        let (_temp, mut store) = test_store();
        let issue_id = create_test_issue(&mut store);

        set_data(
            &mut store,
            &issue_id,
            "config",
            "schema:1",
            &serde_json::json!({"key": "value"}),
        )
        .unwrap();
        set_data(
            &mut store,
            &issue_id,
            "config",
            "schema:1",
            &serde_json::json!({"key": "value"}),
        )
        .unwrap(); // Identical re-set

        let events = read_events(&mut store);
        assert_eq!(events.len(), 1, "an identical re-set is not a mutation");
    }

    #[test]
    #[serial]
    fn test_set_data_changed_value_appends_event() {
        let (_temp, mut store) = test_store();
        let issue_id = create_test_issue(&mut store);

        set_data(
            &mut store,
            &issue_id,
            "config",
            "schema:1",
            &serde_json::json!({"key": "old"}),
        )
        .unwrap();
        // The set is upsert-shaped: replacing the stored value is a mutation
        set_data(
            &mut store,
            &issue_id,
            "config",
            "schema:1",
            &serde_json::json!({"key": "new"}),
        )
        .unwrap();

        let events = read_events(&mut store);
        assert_eq!(events.len(), 2, "a changed value is a semantic mutation");
        assert_eq!(events[1].2["schema_ref"], "schema:1");
    }

    #[test]
    #[serial]
    fn test_set_data_changed_schema_ref_appends_event() {
        let (_temp, mut store) = test_store();
        let issue_id = create_test_issue(&mut store);

        set_data(
            &mut store,
            &issue_id,
            "config",
            "schema:1",
            &serde_json::json!({"key": "value"}),
        )
        .unwrap();
        // Re-governing the same body under a new schema reference is a mutation
        set_data(
            &mut store,
            &issue_id,
            "config",
            "schema:2",
            &serde_json::json!({"key": "value"}),
        )
        .unwrap();

        let events = read_events(&mut store);
        assert_eq!(
            events.len(),
            2,
            "a changed schema_ref is a semantic mutation"
        );
        assert_eq!(events[1].2["schema_ref"], "schema:2");
    }

    #[test]
    #[serial]
    fn test_remove_data_appends_event() {
        let (_temp, mut store) = test_store();
        let issue_id = create_test_issue(&mut store);

        set_data(
            &mut store,
            &issue_id,
            "config",
            "schema:1",
            &serde_json::json!({"key": "value"}),
        )
        .unwrap();
        remove_data(&mut store, &issue_id, "config").unwrap();

        let events = read_events(&mut store);
        assert_eq!(events.len(), 2, "one event per committed mutation");

        let (event_issue, kind, detail) = &events[1];
        assert_eq!(event_issue.as_deref(), Some(issue_id.as_str()));
        assert_eq!(kind, "data_removed");
        assert_eq!(detail["actor"], "system");
        assert_eq!(detail["namespace"], "config");
        assert_eq!(detail["schema_ref"], "schema:1");
    }

    #[test]
    #[serial]
    fn test_remove_nonexistent_data_appends_no_event() {
        let (_temp, mut store) = test_store();
        let issue_id = create_test_issue(&mut store);

        remove_data(&mut store, &issue_id, "nonexistent").unwrap();

        let events = read_events(&mut store);
        assert_eq!(events.len(), 0, "a no-op remove is not a mutation");
    }

    #[test]
    #[serial]
    fn test_data_events_advance_sequence_per_mutation() {
        let (_temp, mut store) = test_store();
        let issue_id = create_test_issue(&mut store);

        let max_sequence = |store: &mut SqliteStore| -> i64 {
            let conn = store.conn();
            conn.query_row("SELECT COALESCE(MAX(sequence), 0) FROM events", [], |row| {
                row.get(0)
            })
            .unwrap()
        };

        let before = max_sequence(&mut store);

        set_data(
            &mut store,
            &issue_id,
            "config",
            "schema:1",
            &serde_json::json!({"key": "value"}),
        )
        .unwrap();
        assert_eq!(max_sequence(&mut store), before + 1);

        // Identical re-set does not advance the sequence
        set_data(
            &mut store,
            &issue_id,
            "config",
            "schema:1",
            &serde_json::json!({"key": "value"}),
        )
        .unwrap();
        assert_eq!(max_sequence(&mut store), before + 1);

        // Changed value does advance it
        set_data(
            &mut store,
            &issue_id,
            "config",
            "schema:1",
            &serde_json::json!({"key": "changed"}),
        )
        .unwrap();
        assert_eq!(max_sequence(&mut store), before + 2);

        remove_data(&mut store, &issue_id, "config").unwrap();
        assert_eq!(max_sequence(&mut store), before + 3);

        // No-op re-remove does not advance the sequence
        remove_data(&mut store, &issue_id, "config").unwrap();
        assert_eq!(max_sequence(&mut store), before + 3);
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

//! External references service for R011
//!
//! This module provides functions for managing namespaced external references
//! such as tracker IDs and commit identifiers.

use crate::error::Error;
use crate::model::{
    validate_reference_key, validate_reference_namespace, validate_reference_value,
    ExternalReference,
};
use crate::store::SqliteStore;

/// Add an external reference to an issue
///
/// This operation is idempotent - adding the same reference multiple times
/// will succeed without creating duplicates.
pub fn add_external_reference(
    store: &mut SqliteStore,
    issue_id: &str,
    namespace: &str,
    key: &str,
    value: &str,
) -> Result<(), Error> {
    // Validate inputs
    let reference = ExternalReference {
        issue_id: issue_id.to_string(),
        namespace: namespace.to_string(),
        key: key.to_string(),
        value: value.to_string(),
    };
    reference
        .validate()
        .map_err(|e| Error::validation(e.to_string()))?;

    let conn = store.conn();
    let tx = conn.unchecked_transaction()?;

    // Check if issue exists
    let issue_exists = tx
        .query_row("SELECT 1 FROM issues WHERE id = ?", [&issue_id], |_| Ok(()))
        .is_ok();

    if !issue_exists {
        return Err(Error::not_found(format!("Issue {}", issue_id)));
    }

    // Insert or replace the external reference
    tx.execute(
        "INSERT OR REPLACE INTO external_references (issue_id, namespace, key, value)
         VALUES (?1, ?2, ?3, ?4)",
        [&issue_id, &namespace, &key, &value],
    )?;

    tx.commit()?;
    Ok(())
}

/// Remove an external reference from an issue
///
/// This operation is idempotent - removing a non-existent reference will succeed.
pub fn remove_external_reference(
    store: &mut SqliteStore,
    issue_id: &str,
    namespace: &str,
    key: &str,
) -> Result<(), Error> {
    // Validate inputs
    validate_reference_namespace(namespace).map_err(|e| Error::validation(e.to_string()))?;
    validate_reference_key(key).map_err(|e| Error::validation(e.to_string()))?;

    let conn = store.conn();
    let tx = conn.unchecked_transaction()?;

    // Remove the reference if it exists
    tx.execute(
        "DELETE FROM external_references
         WHERE issue_id = ?1 AND namespace = ?2 AND key = ?3",
        [&issue_id, &namespace, &key],
    )?;

    tx.commit()?;
    Ok(())
}

/// List all external references for an issue
pub fn list_external_references(
    store: &mut SqliteStore,
    issue_id: &str,
) -> Result<Vec<ExternalReference>, Error> {
    let conn = store.conn();

    // Check if issue exists
    let issue_exists = conn
        .query_row("SELECT 1 FROM issues WHERE id = ?", [&issue_id], |_| Ok(()))
        .is_ok();

    if !issue_exists {
        return Err(Error::not_found(format!("Issue {}", issue_id)));
    }

    // Query all external references for the issue
    let mut stmt = conn.prepare(
        "SELECT issue_id, namespace, key, value
         FROM external_references
         WHERE issue_id = ?1
         ORDER BY namespace, key",
    )?;

    let mut rows = stmt.query([&issue_id])?;
    let mut results = Vec::new();

    while let Some(row) = rows.next()? {
        results.push(ExternalReference {
            issue_id: row.get(0)?,
            namespace: row.get(1)?,
            key: row.get(2)?,
            value: row.get(3)?,
        });
    }

    Ok(results)
}

/// Find issues by external reference (namespace-scoped lookup)
///
/// This supports cross-tool recognition by finding all issues that have
/// a reference with the given namespace and value.
pub fn find_issues_by_reference(
    store: &mut SqliteStore,
    namespace: &str,
    value: &str,
) -> Result<Vec<String>, Error> {
    // Validate inputs
    validate_reference_namespace(namespace).map_err(|e| Error::validation(e.to_string()))?;
    validate_reference_value(value).map_err(|e| Error::validation(e.to_string()))?;

    let conn = store.conn();

    // Query all issues with the given reference
    let mut stmt = conn.prepare(
        "SELECT DISTINCT issue_id
         FROM external_references
         WHERE namespace = ?1 AND value = ?2
         ORDER BY issue_id",
    )?;

    let mut rows = stmt.query([&namespace, &value])?;
    let mut results = Vec::new();

    while let Some(row) = rows.next()? {
        results.push(row.get(0)?);
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store() -> (SqliteStore, tempfile::TempDir) {
        let temp_dir = tempfile::tempdir().unwrap();
        let beads_path = temp_dir.path().join(".beads");
        std::fs::create_dir(&beads_path).unwrap();

        let db_path = beads_path.join("beads.db");
        let mut store = SqliteStore::with_path(&db_path).unwrap();
        store.apply_migrations().unwrap();

        (store, temp_dir)
    }

    fn create_test_issue(store: &mut SqliteStore, id: &str, title: &str) {
        let conn = store.conn();
        conn.execute(
            "INSERT INTO issues (id, title, priority, base_status, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            [
                id,
                title,
                "2",
                "open",
                "2026-08-09T12:00:00Z",
                "2026-08-09T12:00:00Z",
            ],
        )
        .unwrap();
    }

    #[test]
    fn test_add_and_list_external_references() {
        let (mut store, _temp) = test_store();
        create_test_issue(&mut store, "bead-test001", "Test Issue");

        // Add external references
        add_external_reference(
            &mut store,
            "bead-test001",
            "github",
            "issue-number",
            "12345",
        )
        .unwrap();
        add_external_reference(&mut store, "bead-test001", "gitlab", "mr-id", "42").unwrap();

        // List references
        let refs = list_external_references(&mut store, "bead-test001").unwrap();
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].namespace, "github");
        assert_eq!(refs[0].key, "issue-number");
        assert_eq!(refs[0].value, "12345");
    }

    #[test]
    fn test_add_duplicate_reference_idempotent() {
        let (mut store, _temp) = test_store();
        create_test_issue(&mut store, "bead-test002", "Test Issue");

        // Add the same reference twice
        add_external_reference(
            &mut store,
            "bead-test002",
            "github",
            "issue-number",
            "12345",
        )
        .unwrap();
        add_external_reference(
            &mut store,
            "bead-test002",
            "github",
            "issue-number",
            "12345",
        )
        .unwrap();

        // Should only have one reference
        let refs = list_external_references(&mut store, "bead-test002").unwrap();
        assert_eq!(refs.len(), 1);
    }

    #[test]
    fn test_remove_external_reference() {
        let (mut store, _temp) = test_store();
        create_test_issue(&mut store, "bead-test003", "Test Issue");

        // Add a reference
        add_external_reference(
            &mut store,
            "bead-test003",
            "github",
            "issue-number",
            "12345",
        )
        .unwrap();

        // Remove it
        remove_external_reference(&mut store, "bead-test003", "github", "issue-number").unwrap();

        // Should be gone
        let refs = list_external_references(&mut store, "bead-test003").unwrap();
        assert_eq!(refs.len(), 0);
    }

    #[test]
    fn test_remove_nonexistent_reference_idempotent() {
        let (mut store, _temp) = test_store();
        create_test_issue(&mut store, "bead-test004", "Test Issue");

        // Remove a non-existent reference - should succeed
        remove_external_reference(&mut store, "bead-test004", "github", "issue-number").unwrap();
    }

    #[test]
    fn test_find_issues_by_reference() {
        let (mut store, _temp) = test_store();
        create_test_issue(&mut store, "bead-test005", "Test Issue 1");
        create_test_issue(&mut store, "bead-test006", "Test Issue 2");

        // Add references with the same namespace and value to different issues
        add_external_reference(
            &mut store,
            "bead-test005",
            "github",
            "issue-number",
            "12345",
        )
        .unwrap();
        add_external_reference(
            &mut store,
            "bead-test006",
            "github",
            "issue-number",
            "12345",
        )
        .unwrap();

        // Find issues by reference
        let issues = find_issues_by_reference(&mut store, "github", "12345").unwrap();
        assert_eq!(issues.len(), 2);
        assert!(issues.contains(&"bead-test005".to_string()));
        assert!(issues.contains(&"bead-test006".to_string()));
    }

    #[test]
    fn test_external_reference_validation() {
        let (mut store, _temp) = test_store();
        create_test_issue(&mut store, "bead-test008", "Test Issue");

        // Test invalid namespace (must start with letter)
        assert!(
            add_external_reference(&mut store, "bead-test008", "123invalid", "key", "value",)
                .is_err()
        );

        // Test invalid namespace (contains uppercase)
        assert!(
            add_external_reference(&mut store, "bead-test008", "GitHub", "key", "value",).is_err()
        );

        // Test empty key
        assert!(add_external_reference(&mut store, "bead-test008", "github", "", "value").is_err());

        // Test empty value
        assert!(add_external_reference(&mut store, "bead-test008", "github", "key", "").is_err());
    }

    #[test]
    fn test_nonexistent_issue() {
        let (mut store, _temp) = test_store();

        // Try to add reference to non-existent issue
        assert!(
            add_external_reference(&mut store, "bead-nonexistent", "github", "key", "value",)
                .is_err()
        );

        // Try to list references for non-existent issue
        assert!(list_external_references(&mut store, "bead-nonexistent").is_err());
    }
}

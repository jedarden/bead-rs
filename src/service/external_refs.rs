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
use rusqlite::OptionalExtension;

/// Add an external reference to an issue
///
/// This operation is idempotent - adding the same reference multiple times
/// will succeed without creating duplicates. Adding a reference whose value
/// differs from the stored one replaces it.
///
/// A committed add that stores a new pair or a changed value appends an
/// `external_ref_added` audit event in the same transaction; an idempotent
/// re-add of the identical reference appends none.
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

    // The add is upsert-shaped: INSERT OR REPLACE reports one row changed
    // whether it creates the pair or rewrites an identical one, so the prior
    // value decides whether a semantic mutation is about to commit. A re-add
    // of the same (namespace, key, value) is a no-op and must append no event.
    let prior_value: Option<String> = tx
        .query_row(
            "SELECT value FROM external_references
             WHERE issue_id = ?1 AND namespace = ?2 AND key = ?3",
            [&issue_id, &namespace, &key],
            |row| row.get(0),
        )
        .optional()?;

    // Insert or replace the external reference
    tx.execute(
        "INSERT OR REPLACE INTO external_references (issue_id, namespace, key, value)
         VALUES (?1, ?2, ?3, ?4)",
        [&issue_id, &namespace, &key, &value],
    )?;

    // Record the mutation as an audit event inside this transaction: the live
    // event sequence is the dirtiness signal (plan 6.2.1 P3), so an unrecorded
    // reference add would silently read as no change. Reference values are
    // free-form and can carry a credential-shaped string, so the detail never
    // carries the value verbatim - only its SHA-256 fingerprint.
    if prior_value.as_deref() != Some(value) {
        append_external_ref_event(
            &tx,
            issue_id,
            "external_ref_added",
            &serde_json::json!({
                "actor": "system",
                "namespace": namespace,
                "key": key,
                "value_sha256": value_fingerprint(value),
            }),
        )?;
    }

    tx.commit()?;
    Ok(())
}

/// Append the audit event for an external-reference mutation.
///
/// The event is inserted on the mutation's own transaction, so it commits (or
/// rolls back) with the row it describes. `issue_id` is the event's issue
/// subject. Callers append only after a real row change, which guarantees the
/// issue exists (add verifies it up front; a deleted reference row cannot
/// outlive its cascading foreign key), so the events table's own foreign key
/// holds.
fn append_external_ref_event(
    tx: &rusqlite::Transaction,
    issue_id: &str,
    kind: &str,
    detail: &serde_json::Value,
) -> Result<(), Error> {
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string());

    let mut stmt = tx.prepare_cached(
        "INSERT INTO events (issue_id, kind, actor, time, detail) VALUES (?1, ?2, ?3, ?4, ?5)",
    )?;

    stmt.execute((issue_id, kind, "system", now, detail.to_string()))?;

    Ok(())
}

/// Non-reversible fingerprint of a reference value for audit detail.
///
/// R011 reference values are free-form, so an event must never record one
/// verbatim. The digest still lets an audit consumer distinguish an identical
/// re-write from a changed value without the value itself landing in the
/// event log.
fn value_fingerprint(value: &str) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(value.as_bytes()))
}

/// Remove an external reference from an issue
///
/// This operation is idempotent - removing a non-existent reference will succeed.
/// A remove that deletes a row appends an `external_ref_removed` audit event in
/// the same transaction; an idempotent no-op remove appends none.
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

    // Idempotent delete: removing a non-existent reference commits no semantic
    // mutation, so it must append no event.
    let removed = tx.execute(
        "DELETE FROM external_references
         WHERE issue_id = ?1 AND namespace = ?2 AND key = ?3",
        [&issue_id, &namespace, &key],
    )?;

    // Record the mutation as an audit event inside this transaction: the live
    // event sequence is the dirtiness signal (plan 6.2.1 P3), so an unrecorded
    // reference remove would silently read as no change.
    if removed > 0 {
        append_external_ref_event(
            &tx,
            issue_id,
            "external_ref_removed",
            &serde_json::json!({
                "actor": "system",
                "namespace": namespace,
                "key": key,
            }),
        )?;
    }

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
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    }

    #[test]
    fn test_add_external_reference_appends_event() {
        let (mut store, _temp) = test_store();
        create_test_issue(&mut store, "bead-test010", "Test Issue");

        add_external_reference(
            &mut store,
            "bead-test010",
            "github",
            "issue-number",
            "12345",
        )
        .unwrap();

        let events = read_events(&mut store);
        assert_eq!(events.len(), 1, "one event per committed add");

        let (issue_id, kind, detail) = &events[0];
        assert_eq!(issue_id.as_deref(), Some("bead-test010"));
        assert_eq!(kind, "external_ref_added");
        assert_eq!(detail["namespace"], "github");
        assert_eq!(detail["key"], "issue-number");
        assert_eq!(detail["value_sha256"], value_fingerprint("12345"));
    }

    #[test]
    fn test_add_external_reference_event_omits_value_verbatim() {
        let (mut store, _temp) = test_store();
        create_test_issue(&mut store, "bead-test011", "Test Issue");

        // A credential-shaped value must never reach the event log verbatim
        let fixture_value = "bearer-sup3r-s3cret-fixture-value";
        add_external_reference(&mut store, "bead-test011", "github", "token", fixture_value)
            .unwrap();

        let events = read_events(&mut store);
        assert_eq!(events.len(), 1);

        let raw_detail = {
            let conn = store.conn();
            conn.query_row(
                "SELECT detail FROM events WHERE kind = 'external_ref_added'",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap()
        };
        assert!(
            !raw_detail.contains(fixture_value),
            "event detail must not carry the value verbatim"
        );
        assert_eq!(
            events[0].2["value_sha256"],
            value_fingerprint(fixture_value)
        );
    }

    #[test]
    fn test_add_duplicate_reference_idempotent_appends_no_event() {
        let (mut store, _temp) = test_store();
        create_test_issue(&mut store, "bead-test012", "Test Issue");

        add_external_reference(
            &mut store,
            "bead-test012",
            "github",
            "issue-number",
            "12345",
        )
        .unwrap();
        add_external_reference(
            &mut store,
            "bead-test012",
            "github",
            "issue-number",
            "12345",
        )
        .unwrap(); // Identical re-add

        let events = read_events(&mut store);
        assert_eq!(events.len(), 1, "an identical re-add is not a mutation");
    }

    #[test]
    fn test_add_reference_with_changed_value_appends_event() {
        let (mut store, _temp) = test_store();
        create_test_issue(&mut store, "bead-test013", "Test Issue");

        add_external_reference(
            &mut store,
            "bead-test013",
            "github",
            "issue-number",
            "12345",
        )
        .unwrap();
        // The add is upsert-shaped: replacing the stored value is a mutation
        add_external_reference(
            &mut store,
            "bead-test013",
            "github",
            "issue-number",
            "67890",
        )
        .unwrap();

        let events = read_events(&mut store);
        assert_eq!(events.len(), 2, "a changed value is a semantic mutation");
        assert_eq!(events[1].2["value_sha256"], value_fingerprint("67890"));
    }

    #[test]
    fn test_remove_external_reference_appends_event() {
        let (mut store, _temp) = test_store();
        create_test_issue(&mut store, "bead-test014", "Test Issue");

        add_external_reference(
            &mut store,
            "bead-test014",
            "github",
            "issue-number",
            "12345",
        )
        .unwrap();
        remove_external_reference(&mut store, "bead-test014", "github", "issue-number").unwrap();

        let events = read_events(&mut store);
        assert_eq!(events.len(), 2, "one event per committed mutation");

        let (issue_id, kind, detail) = &events[1];
        assert_eq!(issue_id.as_deref(), Some("bead-test014"));
        assert_eq!(kind, "external_ref_removed");
        assert_eq!(detail["namespace"], "github");
        assert_eq!(detail["key"], "issue-number");
    }

    #[test]
    fn test_remove_nonexistent_reference_appends_no_event() {
        let (mut store, _temp) = test_store();
        create_test_issue(&mut store, "bead-test015", "Test Issue");

        remove_external_reference(&mut store, "bead-test015", "github", "issue-number").unwrap();

        let events = read_events(&mut store);
        assert_eq!(events.len(), 0, "a no-op remove is not a mutation");
    }

    #[test]
    fn test_external_reference_events_advance_sequence_per_mutation() {
        let (mut store, _temp) = test_store();
        create_test_issue(&mut store, "bead-test016", "Test Issue");

        let max_sequence = |store: &mut SqliteStore| -> i64 {
            let conn = store.conn();
            conn.query_row("SELECT COALESCE(MAX(sequence), 0) FROM events", [], |row| {
                row.get(0)
            })
            .unwrap()
        };

        let before = max_sequence(&mut store);

        add_external_reference(
            &mut store,
            "bead-test016",
            "github",
            "issue-number",
            "12345",
        )
        .unwrap();
        assert_eq!(max_sequence(&mut store), before + 1);

        // Identical re-add does not advance the sequence
        add_external_reference(
            &mut store,
            "bead-test016",
            "github",
            "issue-number",
            "12345",
        )
        .unwrap();
        assert_eq!(max_sequence(&mut store), before + 1);

        // Changed value does advance it
        add_external_reference(
            &mut store,
            "bead-test016",
            "github",
            "issue-number",
            "67890",
        )
        .unwrap();
        assert_eq!(max_sequence(&mut store), before + 2);

        remove_external_reference(&mut store, "bead-test016", "github", "issue-number").unwrap();
        assert_eq!(max_sequence(&mut store), before + 3);

        // No-op re-remove does not advance the sequence
        remove_external_reference(&mut store, "bead-test016", "github", "issue-number").unwrap();
        assert_eq!(max_sequence(&mut store), before + 3);
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

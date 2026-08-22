//! Labels and dependency graph operations.

use crate::error::{Error, ValidationError};
use crate::service::conditions::{evaluate_condition, ConditionExpr, IssueContext};
use crate::store::SqliteStore;
use rusqlite::{Connection, Transaction, TransactionBehavior};

/// Adds a label to an issue.
///
/// This operation is idempotent: adding an existing label succeeds without changes.
/// A committed add appends a `label_added` audit event in the same transaction;
/// an idempotent no-op appends none.
pub fn add_label(store: &mut SqliteStore, issue_id: &str, label: &str) -> Result<(), Error> {
    let conn = store.conn();
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;

    // Verify issue exists
    let issue_exists = tx
        .query_row("SELECT 1 FROM issues WHERE id = ?", [&issue_id], |_| Ok(()))
        .is_ok();

    if !issue_exists {
        return Err(Error::not_found(format!("Issue {issue_id}")));
    }

    // Idempotent insert: a re-add of an existing label commits no semantic
    // mutation, so it must append no event.
    let inserted = tx.execute(
        "INSERT OR IGNORE INTO labels (issue_id, label) VALUES (?1, ?2)",
        [issue_id, label],
    )?;

    // Record the mutation as an audit event inside this transaction: the live
    // event sequence is the dirtiness signal (plan 6.2.1 P3), so an unrecorded
    // label add would silently read as no change.
    if inserted > 0 {
        append_label_event(
            &tx,
            issue_id,
            "label_added",
            &serde_json::json!({
                "actor": "system",
                "label": label,
            }),
        )?;
    }

    tx.commit()?;
    Ok(())
}

/// Removes a label from an issue.
///
/// This operation is idempotent: removing a non-existent label succeeds without changes.
/// A remove that deletes the label appends a `label_removed` audit event in the
/// same transaction; an idempotent no-op appends none.
pub fn remove_label(store: &mut SqliteStore, issue_id: &str, label: &str) -> Result<(), Error> {
    let conn = store.conn();
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;

    // Verify issue exists
    let issue_exists = tx
        .query_row("SELECT 1 FROM issues WHERE id = ?", [&issue_id], |_| Ok(()))
        .is_ok();

    if !issue_exists {
        return Err(Error::not_found(format!("Issue {issue_id}")));
    }

    // Idempotent delete: removing a non-existent label commits no semantic
    // mutation, so it must append no event.
    let removed = tx.execute(
        "DELETE FROM labels WHERE issue_id = ?1 AND label = ?2",
        [issue_id, label],
    )?;

    // Record the mutation as an audit event inside this transaction: the live
    // event sequence is the dirtiness signal (plan 6.2.1 P3), so an unrecorded
    // label remove would silently read as no change.
    if removed > 0 {
        append_label_event(
            &tx,
            issue_id,
            "label_removed",
            &serde_json::json!({
                "actor": "system",
                "label": label,
            }),
        )?;
    }

    tx.commit()?;
    Ok(())
}

/// Append the audit event for a label mutation.
///
/// The event is inserted on the mutation's own transaction, so it commits (or
/// rolls back) with the row it describes. `issue_id` is the event's issue
/// subject. Callers append only after a real row change, which guarantees the
/// issue exists (both mutations verify it up front), so the events table's own
/// foreign key holds.
fn append_label_event(
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

/// Adds a dependency edge between two issues.
///
/// Rejects self-edges and cycles in `blocks` dependencies.
/// This operation is idempotent: adding an existing edge succeeds without changes.
/// A committed add appends a `dependency_added` audit event in the same
/// transaction; an idempotent no-op appends none.
///
/// # Arguments
/// * `blocked_id` - The issue that is blocked
/// * `blocker_id` - The issue that does the blocking
/// * `kind` - The dependency kind (e.g., "blocks", "relates_to")
/// * `condition` - Optional conditional dependency expression
pub fn add_dependency(
    store: &mut SqliteStore,
    blocked_id: &str,
    blocker_id: &str,
    kind: &str,
    condition: Option<&ConditionExpr>,
) -> Result<(), Error> {
    if !is_valid_kind(kind) {
        return Err(ValidationError::InvalidKind {
            kind: kind.to_string(),
        }
        .into());
    }

    let conn = store.conn();
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;

    // Verify both issues exist
    let blocked_exists = tx
        .query_row("SELECT 1 FROM issues WHERE id = ?", [&blocked_id], |_| {
            Ok(())
        })
        .is_ok();

    if !blocked_exists {
        return Err(Error::not_found(format!("Issue {blocked_id}")));
    }

    let blocker_exists = tx
        .query_row("SELECT 1 FROM issues WHERE id = ?", [&blocker_id], |_| {
            Ok(())
        })
        .is_ok();

    if !blocker_exists {
        return Err(Error::not_found(format!("Issue {blocker_id}")));
    }

    // Reject self-edges
    if blocked_id == blocker_id {
        return Err(Error::Conflict(
            "Self-edge: blocked and blocker cannot be the same issue".to_string(),
        ));
    }

    // Validate condition fields if provided
    if let Some(cond) = condition {
        cond.validate_fields()?;
    }

    // For `blocks` dependencies, detect and reject cycles
    // IMPORTANT: For conditional dependencies, we must treat them as potentially active
    // during cycle detection. This means if there's ANY condition (even conditional),
    // we must check for cycles.
    if kind == "blocks" && creates_cycle(&tx, blocked_id, blocker_id)? {
        return Err(Error::Conflict(
            "Adding this dependency would create a cycle".to_string(),
        ));
    }

    // Serialize condition if present
    let condition_json = condition.map(|c| c.to_json()).transpose()?;

    // Idempotent insert: ignore if already exists. A re-add of an existing
    // edge commits no semantic mutation, so it must append no event.
    let inserted = if let Some(ref cond_json) = condition_json {
        tx.execute(
            "INSERT OR IGNORE INTO dependencies (blocked_issue_id, blocker_issue_id, kind, condition) VALUES (?1, ?2, ?3, ?4)",
            [blocked_id, blocker_id, kind, cond_json.as_str()],
        )?
    } else {
        tx.execute(
            "INSERT OR IGNORE INTO dependencies (blocked_issue_id, blocker_issue_id, kind, condition) VALUES (?1, ?2, ?3, NULL)",
            [blocked_id, blocker_id, kind],
        )?
    };

    // Record the mutation as an audit event inside this transaction: the live
    // event sequence is the dirtiness signal (plan 6.2.1 P3), so an unrecorded
    // dependency add would silently read as no change.
    if inserted > 0 {
        let condition_value = match condition {
            Some(c) => serde_json::to_value(c)
                .map_err(|e| Error::validation(format!("Failed to serialize condition: {}", e)))?,
            None => serde_json::Value::Null,
        };

        append_dependency_event(
            &tx,
            blocked_id,
            "dependency_added",
            &serde_json::json!({
                "actor": "system",
                "blocked": blocked_id,
                "blocker": blocker_id,
                "kind": kind,
                "condition": condition_value,
            }),
        )?;
    }

    tx.commit()?;
    Ok(())
}

/// Append the audit event for a dependency-graph mutation.
///
/// The event is inserted on the mutation's own transaction, so it commits (or
/// rolls back) with the edge it describes. `blocked_id` is the event's issue
/// subject: it is the issue whose readiness the edge changes. Callers append
/// only after a real edge change, which guarantees the blocked issue exists
/// (add verifies both endpoints; a deleted edge row cannot outlive its
/// cascading foreign keys), so the events table's own foreign key holds.
fn append_dependency_event(
    tx: &rusqlite::Transaction,
    blocked_id: &str,
    kind: &str,
    detail: &serde_json::Value,
) -> Result<(), Error> {
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string());

    let mut stmt = tx.prepare_cached(
        "INSERT INTO events (issue_id, kind, actor, time, detail) VALUES (?1, ?2, ?3, ?4, ?5)",
    )?;

    stmt.execute((blocked_id, kind, "system", now, detail.to_string()))?;

    Ok(())
}

/// Returns whether `kind` is supported by the dependency graph.
fn is_valid_kind(kind: &str) -> bool {
    matches!(kind, "blocks" | "relates_to")
}

/// Removes a dependency edge.
///
/// This operation is idempotent: removing a non-existent edge succeeds without changes.
/// A remove that deletes at least one edge appends a `dependency_removed` audit
/// event in the same transaction; an idempotent no-op appends none.
///
/// # Arguments
/// * `blocked_id` - The blocked issue
/// * `blocker_id` - The blocker issue
/// * `kind` - Optional dependency kind filter; if None, removes all edges between the issues
pub fn remove_dependency(
    store: &mut SqliteStore,
    blocked_id: &str,
    blocker_id: &str,
    kind: Option<&str>,
) -> Result<(), Error> {
    let conn = store.conn();
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;

    // Idempotent delete: removing a non-existent edge commits no semantic
    // mutation, so it must append no event.
    let removed = if let Some(k) = kind {
        tx.execute(
            "DELETE FROM dependencies WHERE blocked_issue_id = ?1 AND blocker_issue_id = ?2 AND kind = ?3",
            [blocked_id, blocker_id, k],
        )?
    } else {
        tx.execute(
            "DELETE FROM dependencies WHERE blocked_issue_id = ?1 AND blocker_issue_id = ?2",
            [blocked_id, blocker_id],
        )?
    };

    // Record the mutation as an audit event inside this transaction: the live
    // event sequence is the dirtiness signal (plan 6.2.1 P3), so an unrecorded
    // dependency remove would silently read as no change. A null kind records
    // that the remove was not filtered by kind.
    if removed > 0 {
        append_dependency_event(
            &tx,
            blocked_id,
            "dependency_removed",
            &serde_json::json!({
                "actor": "system",
                "blocked": blocked_id,
                "blocker": blocker_id,
                "kind": kind,
            }),
        )?;
    }

    tx.commit()?;
    Ok(())
}

/// Detects if adding an edge from `blocked_id` to `blocker_id` would create a cycle.
///
/// Uses a DFS traversal to detect cycles in the dependency graph.
fn creates_cycle(
    tx: &rusqlite::Transaction,
    blocked_id: &str,
    blocker_id: &str,
) -> Result<bool, Error> {
    // Check if there's already a path from blocker to blocked
    // This would mean adding blocked->blocker creates a cycle
    let mut visited = std::collections::HashSet::new();
    dfs_has_path(tx, blocker_id, blocked_id, &mut visited)
}

/// DFS helper to check if there's a path from `current` to `target` following `blocks` edges.
fn dfs_has_path(
    tx: &rusqlite::Transaction,
    current: &str,
    target: &str,
    visited: &mut std::collections::HashSet<String>,
) -> Result<bool, Error> {
    if current == target {
        return Ok(true);
    }

    if visited.contains(current) {
        return Ok(false);
    }
    visited.insert(current.to_string());

    // Follow all outgoing `blocks` edges from current
    let mut stmt = tx.prepare(
        "SELECT blocker_issue_id FROM dependencies
         WHERE blocked_issue_id = ? AND kind = 'blocks'",
    )?;

    let mut rows = stmt.query([current])?;
    while let Some(row) = rows.next()? {
        let blocker_id: String = row.get(0)?;
        if dfs_has_path(tx, &blocker_id, target, visited)? {
            return Ok(true);
        }
    }

    Ok(false)
}

/// Check if adding a dependency would create a cycle
///
/// This is used by dry-run operations to validate potential dependencies before committing them.
pub fn would_create_cycle(conn: &Connection, blocked: &str, blocker: &str) -> Result<bool, Error> {
    // Use a read transaction for validation
    let tx = conn.unchecked_transaction()?;
    let mut visited = std::collections::HashSet::new();
    let result = dfs_has_path(&tx, blocker, blocked, &mut visited);
    // Don't commit the read transaction, just drop it
    drop(tx);
    result
}

/// Gets conditional dependencies for a blocked issue
///
/// Returns all dependencies that have a condition expression for the given blocked issue.
#[allow(dead_code)]
pub fn get_conditional_dependencies(
    store: &mut SqliteStore,
    blocked_id: &str,
) -> Result<Vec<(String, String, ConditionExpr)>, Error> {
    let conn = store.conn();

    let mut stmt = conn.prepare(
        "SELECT blocker_issue_id, kind, condition
         FROM dependencies
         WHERE blocked_issue_id = ? AND condition IS NOT NULL",
    )?;

    let mut results = Vec::new();
    let mut rows = stmt.query([blocked_id])?;

    while let Some(row) = rows.next()? {
        let blocker_id: String = row.get(0)?;
        let kind: String = row.get(1)?;
        let condition_json: String = row.get(2)?;

        let condition = ConditionExpr::from_json(&condition_json)?;
        results.push((blocker_id, kind, condition));
    }

    Ok(results)
}

/// Evaluates whether a conditional dependency is active
///
/// Checks if the condition expression evaluates to true for the blocker issue.
#[allow(dead_code)]
pub fn is_conditional_dependency_active(
    store: &mut SqliteStore,
    blocker_id: &str,
    condition: &ConditionExpr,
) -> Result<bool, Error> {
    let context = IssueContext::from_store(store, blocker_id)?;
    evaluate_condition(condition, &context)
}

/// Checks if a blocked issue has any active conditional blockers
///
/// Evaluates all conditional dependencies for the blocked issue and returns
/// true if any of them are currently active.
#[allow(dead_code)]
pub fn has_active_conditional_blockers(
    store: &mut SqliteStore,
    blocked_id: &str,
) -> Result<bool, Error> {
    let conditional_deps = get_conditional_dependencies(store, blocked_id)?;

    for (blocker_id, _kind, condition) in conditional_deps {
        if is_conditional_dependency_active(store, &blocker_id, &condition)? {
            return Ok(true);
        }
    }

    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::SqliteStore;

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
                "2026-08-08T12:00:00Z",
                "2026-08-08T12:00:00Z",
            ],
        )
        .unwrap();
    }

    #[test]
    fn test_add_label() {
        let (mut store, _temp) = test_store();
        create_test_issue(&mut store, "issue-1", "Test Issue");

        add_label(&mut store, "issue-1", "bug").unwrap();
        add_label(&mut store, "issue-1", "urgent").unwrap();

        // Verify labels were added
        let conn = store.conn();
        let labels: Vec<String> = conn
            .prepare("SELECT label FROM labels WHERE issue_id = ? ORDER BY label")
            .unwrap()
            .query_map(["issue-1"], |row| row.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert_eq!(labels, vec!["bug".to_string(), "urgent".to_string()]);
    }

    #[test]
    fn test_add_label_idempotent() {
        let (mut store, _temp) = test_store();
        create_test_issue(&mut store, "issue-1", "Test Issue");

        add_label(&mut store, "issue-1", "bug").unwrap();
        add_label(&mut store, "issue-1", "bug").unwrap(); // Duplicate

        // Verify only one label exists
        let conn = store.conn();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM labels WHERE issue_id = ?1",
                ["issue-1"],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(count, 1);
    }

    #[test]
    fn test_add_label_nonexistent_issue() {
        let (mut store, _temp) = test_store();

        let result = add_label(&mut store, "nonexistent", "bug");
        assert!(matches!(result, Err(Error::Workspace(_))));
    }

    #[test]
    fn test_add_label_appends_event() {
        let (mut store, _temp) = test_store();
        create_test_issue(&mut store, "issue-1", "Test Issue");

        add_label(&mut store, "issue-1", "bug").unwrap();

        let events = read_events(&mut store);
        assert_eq!(events.len(), 1, "one event per committed add");

        let (issue_id, kind, detail) = &events[0];
        assert_eq!(issue_id.as_deref(), Some("issue-1"));
        assert_eq!(kind, "label_added");
        assert_eq!(detail["label"], "bug");
    }

    #[test]
    fn test_add_label_idempotent_appends_no_event() {
        let (mut store, _temp) = test_store();
        create_test_issue(&mut store, "issue-1", "Test Issue");

        add_label(&mut store, "issue-1", "bug").unwrap();
        add_label(&mut store, "issue-1", "bug").unwrap(); // Duplicate

        let events = read_events(&mut store);
        assert_eq!(events.len(), 1, "a no-op re-add is not a semantic mutation");
    }

    #[test]
    fn test_remove_label_appends_event() {
        let (mut store, _temp) = test_store();
        create_test_issue(&mut store, "issue-1", "Test Issue");

        add_label(&mut store, "issue-1", "bug").unwrap();
        remove_label(&mut store, "issue-1", "bug").unwrap();

        let events = read_events(&mut store);
        assert_eq!(events.len(), 2, "one event per committed mutation");

        let (issue_id, kind, detail) = &events[1];
        assert_eq!(issue_id.as_deref(), Some("issue-1"));
        assert_eq!(kind, "label_removed");
        assert_eq!(detail["label"], "bug");
    }

    #[test]
    fn test_remove_label_idempotent_appends_no_event() {
        let (mut store, _temp) = test_store();
        create_test_issue(&mut store, "issue-1", "Test Issue");

        add_label(&mut store, "issue-1", "bug").unwrap();
        remove_label(&mut store, "issue-1", "bug").unwrap();
        remove_label(&mut store, "issue-1", "bug").unwrap(); // Duplicate remove

        let events = read_events(&mut store);
        assert_eq!(events.len(), 2, "a no-op remove is not a semantic mutation");
    }

    #[test]
    fn test_remove_label() {
        let (mut store, _temp) = test_store();
        create_test_issue(&mut store, "issue-1", "Test Issue");

        add_label(&mut store, "issue-1", "bug").unwrap();
        add_label(&mut store, "issue-1", "urgent").unwrap();

        remove_label(&mut store, "issue-1", "bug").unwrap();

        // Verify only one label remains
        let conn = store.conn();
        let labels: Vec<String> = conn
            .prepare("SELECT label FROM labels WHERE issue_id = ? ORDER BY label")
            .unwrap()
            .query_map(["issue-1"], |row| row.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert_eq!(labels, vec!["urgent".to_string()]);
    }

    #[test]
    fn test_remove_label_idempotent() {
        let (mut store, _temp) = test_store();
        create_test_issue(&mut store, "issue-1", "Test Issue");

        add_label(&mut store, "issue-1", "bug").unwrap();
        remove_label(&mut store, "issue-1", "bug").unwrap();
        remove_label(&mut store, "issue-1", "bug").unwrap(); // Duplicate remove

        // Verify no error on second remove
        let conn = store.conn();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM labels WHERE issue_id = ?1",
                ["issue-1"],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(count, 0);
    }

    #[test]
    fn test_add_dependency() {
        let (mut store, _temp) = test_store();
        create_test_issue(&mut store, "issue-1", "Blocked Issue");
        create_test_issue(&mut store, "issue-2", "Blocker Issue");

        add_dependency(&mut store, "issue-1", "issue-2", "blocks", None).unwrap();

        // Verify dependency was added
        let conn = store.conn();
        let (blocked, blocker, kind): (String, String, String) = conn
            .query_row(
                "SELECT blocked_issue_id, blocker_issue_id, kind FROM dependencies",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();

        assert_eq!(blocked, "issue-1");
        assert_eq!(blocker, "issue-2");
        assert_eq!(kind, "blocks");
    }

    #[test]
    fn test_add_dependency_idempotent() {
        let (mut store, _temp) = test_store();
        create_test_issue(&mut store, "issue-1", "Blocked Issue");
        create_test_issue(&mut store, "issue-2", "Blocker Issue");

        add_dependency(&mut store, "issue-1", "issue-2", "blocks", None).unwrap();
        add_dependency(&mut store, "issue-1", "issue-2", "blocks", None).unwrap(); // Duplicate

        // Verify only one dependency exists
        let conn = store.conn();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM dependencies", [], |row| row.get(0))
            .unwrap();

        assert_eq!(count, 1);
    }

    #[test]
    fn test_add_dependency_self_edge() {
        let (mut store, _temp) = test_store();
        create_test_issue(&mut store, "issue-1", "Self Issue");

        let result = add_dependency(&mut store, "issue-1", "issue-1", "blocks", None);
        assert!(matches!(result, Err(Error::Conflict(_))));
    }

    #[test]
    fn test_add_dependency_creates_cycle() {
        let (mut store, _temp) = test_store();
        create_test_issue(&mut store, "issue-1", "Issue 1");
        create_test_issue(&mut store, "issue-2", "Issue 2");
        create_test_issue(&mut store, "issue-3", "Issue 3");

        // Create chain: issue-1 -> issue-2 -> issue-3
        add_dependency(&mut store, "issue-2", "issue-3", "blocks", None).unwrap();
        add_dependency(&mut store, "issue-1", "issue-2", "blocks", None).unwrap();

        // Try to create cycle: issue-3 -> issue-1
        let result = add_dependency(&mut store, "issue-3", "issue-1", "blocks", None);
        assert!(matches!(result, Err(Error::Conflict(_))));
    }

    #[test]
    fn test_add_dependency_nonexistent_blocked() {
        let (mut store, _temp) = test_store();
        create_test_issue(&mut store, "issue-2", "Blocker Issue");

        let result = add_dependency(&mut store, "nonexistent", "issue-2", "blocks", None);
        assert!(matches!(result, Err(Error::Workspace(_))));
    }

    #[test]
    fn test_add_dependency_nonexistent_blocker() {
        let (mut store, _temp) = test_store();
        create_test_issue(&mut store, "issue-1", "Blocked Issue");

        let result = add_dependency(&mut store, "issue-1", "nonexistent", "blocks", None);
        assert!(matches!(result, Err(Error::Workspace(_))));
    }

    #[test]
    fn test_add_dependency_invalid_kind() {
        let (mut store, _temp) = test_store();
        create_test_issue(&mut store, "issue-1", "Blocked Issue");
        create_test_issue(&mut store, "issue-2", "Blocker Issue");

        let result = add_dependency(&mut store, "issue-1", "issue-2", "parent-child", None);
        assert!(matches!(
            result,
            Err(Error::Validation(ValidationError::InvalidKind { kind }))
                if kind == "parent-child"
        ));

        assert_eq!(
            Error::Validation(ValidationError::InvalidKind {
                kind: "parent-child".to_string(),
            })
            .exit_code(),
            4
        );

        // Verify no dependency was added
        let conn = store.conn();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM dependencies", [], |row| row.get(0))
            .unwrap();

        assert_eq!(count, 0);
    }

    #[test]
    fn test_is_valid_kind() {
        assert!(is_valid_kind("blocks"));
        assert!(is_valid_kind("relates_to"));
        assert!(!is_valid_kind("parent-child"));
        assert!(!is_valid_kind("depends-on"));
    }

    #[test]
    fn test_remove_dependency() {
        let (mut store, _temp) = test_store();
        create_test_issue(&mut store, "issue-1", "Blocked Issue");
        create_test_issue(&mut store, "issue-2", "Blocker Issue");

        add_dependency(&mut store, "issue-1", "issue-2", "blocks", None).unwrap();
        remove_dependency(&mut store, "issue-1", "issue-2", Some("blocks")).unwrap();

        // Verify dependency was removed
        let conn = store.conn();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM dependencies", [], |row| row.get(0))
            .unwrap();

        assert_eq!(count, 0);
    }

    #[test]
    fn test_remove_dependency_idempotent() {
        let (mut store, _temp) = test_store();
        create_test_issue(&mut store, "issue-1", "Blocked Issue");
        create_test_issue(&mut store, "issue-2", "Blocker Issue");

        add_dependency(&mut store, "issue-1", "issue-2", "blocks", None).unwrap();
        remove_dependency(&mut store, "issue-1", "issue-2", Some("blocks")).unwrap();
        remove_dependency(&mut store, "issue-1", "issue-2", Some("blocks")).unwrap(); // Duplicate

        // Verify no error on second remove
        let conn = store.conn();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM dependencies", [], |row| row.get(0))
            .unwrap();

        assert_eq!(count, 0);
    }

    #[test]
    fn test_remove_dependency_without_kind() {
        let (mut store, _temp) = test_store();
        create_test_issue(&mut store, "issue-1", "Blocked Issue");
        create_test_issue(&mut store, "issue-2", "Blocker Issue");

        add_dependency(&mut store, "issue-1", "issue-2", "blocks", None).unwrap();
        add_dependency(&mut store, "issue-1", "issue-2", "relates_to", None).unwrap();

        // Remove all edges between these issues
        remove_dependency(&mut store, "issue-1", "issue-2", None).unwrap();

        // Verify both dependencies were removed
        let conn = store.conn();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM dependencies", [], |row| row.get(0))
            .unwrap();

        assert_eq!(count, 0);
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
    fn test_add_dependency_appends_event() {
        let (mut store, _temp) = test_store();
        create_test_issue(&mut store, "issue-1", "Blocked Issue");
        create_test_issue(&mut store, "issue-2", "Blocker Issue");

        add_dependency(&mut store, "issue-1", "issue-2", "blocks", None).unwrap();

        let events = read_events(&mut store);
        assert_eq!(events.len(), 1, "one event per committed add");

        let (issue_id, kind, detail) = &events[0];
        assert_eq!(issue_id.as_deref(), Some("issue-1"));
        assert_eq!(kind, "dependency_added");
        assert_eq!(detail["blocked"], "issue-1");
        assert_eq!(detail["blocker"], "issue-2");
        assert_eq!(detail["kind"], "blocks");
        assert_eq!(detail["condition"], serde_json::Value::Null);
    }

    #[test]
    fn test_add_dependency_with_condition_records_condition() {
        let (mut store, _temp) = test_store();
        create_test_issue(&mut store, "issue-1", "Blocked Issue");
        create_test_issue(&mut store, "issue-2", "Blocker Issue");

        let condition = ConditionExpr::Equals {
            field: "priority".to_string(),
            value: serde_json::json!(2),
        };
        add_dependency(&mut store, "issue-1", "issue-2", "blocks", Some(&condition)).unwrap();

        let events = read_events(&mut store);
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].2["condition"],
            serde_json::json!({"type": "equals", "value": {"field": "priority", "value": 2}})
        );
    }

    #[test]
    fn test_add_dependency_idempotent_appends_no_event() {
        let (mut store, _temp) = test_store();
        create_test_issue(&mut store, "issue-1", "Blocked Issue");
        create_test_issue(&mut store, "issue-2", "Blocker Issue");

        add_dependency(&mut store, "issue-1", "issue-2", "blocks", None).unwrap();
        add_dependency(&mut store, "issue-1", "issue-2", "blocks", None).unwrap(); // Duplicate

        let events = read_events(&mut store);
        assert_eq!(events.len(), 1, "a no-op re-add is not a semantic mutation");
    }

    #[test]
    fn test_remove_dependency_appends_event() {
        let (mut store, _temp) = test_store();
        create_test_issue(&mut store, "issue-1", "Blocked Issue");
        create_test_issue(&mut store, "issue-2", "Blocker Issue");

        add_dependency(&mut store, "issue-1", "issue-2", "blocks", None).unwrap();
        remove_dependency(&mut store, "issue-1", "issue-2", Some("blocks")).unwrap();

        let events = read_events(&mut store);
        assert_eq!(events.len(), 2, "one event per committed mutation");

        let (issue_id, kind, detail) = &events[1];
        assert_eq!(issue_id.as_deref(), Some("issue-1"));
        assert_eq!(kind, "dependency_removed");
        assert_eq!(detail["blocked"], "issue-1");
        assert_eq!(detail["blocker"], "issue-2");
        assert_eq!(detail["kind"], "blocks");
    }

    #[test]
    fn test_remove_dependency_without_kind_records_null_kind() {
        let (mut store, _temp) = test_store();
        create_test_issue(&mut store, "issue-1", "Blocked Issue");
        create_test_issue(&mut store, "issue-2", "Blocker Issue");

        add_dependency(&mut store, "issue-1", "issue-2", "blocks", None).unwrap();
        add_dependency(&mut store, "issue-1", "issue-2", "relates_to", None).unwrap();

        // Removing every edge between the pair is one mutation, one event
        remove_dependency(&mut store, "issue-1", "issue-2", None).unwrap();

        let events = read_events(&mut store);
        assert_eq!(events.len(), 3);
        let (_, kind, detail) = &events[2];
        assert_eq!(kind, "dependency_removed");
        assert_eq!(
            detail["kind"],
            serde_json::Value::Null,
            "unfiltered remove records a null kind"
        );
    }

    #[test]
    fn test_remove_dependency_idempotent_appends_no_event() {
        let (mut store, _temp) = test_store();
        create_test_issue(&mut store, "issue-1", "Blocked Issue");
        create_test_issue(&mut store, "issue-2", "Blocker Issue");

        add_dependency(&mut store, "issue-1", "issue-2", "blocks", None).unwrap();
        remove_dependency(&mut store, "issue-1", "issue-2", Some("blocks")).unwrap();
        remove_dependency(&mut store, "issue-1", "issue-2", Some("blocks")).unwrap(); // Duplicate

        let events = read_events(&mut store);
        assert_eq!(events.len(), 2, "a no-op remove is not a semantic mutation");
    }

    #[test]
    fn test_dependency_events_advance_sequence_per_mutation() {
        let (mut store, _temp) = test_store();
        create_test_issue(&mut store, "issue-1", "Blocked Issue");
        create_test_issue(&mut store, "issue-2", "Blocker Issue");

        let max_sequence = |store: &mut SqliteStore| -> i64 {
            let conn = store.conn();
            conn.query_row("SELECT COALESCE(MAX(sequence), 0) FROM events", [], |row| {
                row.get(0)
            })
            .unwrap()
        };

        let before = max_sequence(&mut store);

        add_dependency(&mut store, "issue-1", "issue-2", "blocks", None).unwrap();
        assert_eq!(max_sequence(&mut store), before + 1);

        // No-op re-add does not advance the sequence
        add_dependency(&mut store, "issue-1", "issue-2", "blocks", None).unwrap();
        assert_eq!(max_sequence(&mut store), before + 1);

        remove_dependency(&mut store, "issue-1", "issue-2", Some("blocks")).unwrap();
        assert_eq!(max_sequence(&mut store), before + 2);

        // No-op re-remove does not advance the sequence
        remove_dependency(&mut store, "issue-1", "issue-2", Some("blocks")).unwrap();
        assert_eq!(max_sequence(&mut store), before + 2);
    }

    #[test]
    fn test_relates_to_allows_cycles() {
        let (mut store, _temp) = test_store();
        create_test_issue(&mut store, "issue-1", "Issue 1");
        create_test_issue(&mut store, "issue-2", "Issue 2");

        // Create cycle with relates_to: issue-1 -> issue-2 -> issue-1
        add_dependency(&mut store, "issue-1", "issue-2", "relates_to", None).unwrap();
        add_dependency(&mut store, "issue-2", "issue-1", "relates_to", None).unwrap();

        // Verify both dependencies exist
        let conn = store.conn();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM dependencies WHERE kind = 'relates_to'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(count, 2);
    }

    #[test]
    fn test_dfs_has_path_direct() {
        let (mut store, _temp) = test_store();
        create_test_issue(&mut store, "issue-1", "Issue 1");
        create_test_issue(&mut store, "issue-2", "Issue 2");

        let conn = store.conn();
        conn.execute(
            "INSERT INTO dependencies (blocked_issue_id, blocker_issue_id, kind)
             VALUES (?1, ?2, ?3)",
            ["issue-1", "issue-2", "blocks"],
        )
        .unwrap();

        let tx = conn.unchecked_transaction().unwrap();
        let has_path = dfs_has_path(
            &tx,
            "issue-1",
            "issue-2",
            &mut std::collections::HashSet::new(),
        )
        .unwrap();
        assert!(has_path);
    }

    #[test]
    fn test_dfs_has_path_indirect() {
        let (mut store, _temp) = test_store();
        create_test_issue(&mut store, "issue-1", "Issue 1");
        create_test_issue(&mut store, "issue-2", "Issue 2");
        create_test_issue(&mut store, "issue-3", "Issue 3");

        let conn = store.conn();
        conn.execute(
            "INSERT INTO dependencies (blocked_issue_id, blocker_issue_id, kind)
             VALUES (?1, ?2, ?3)",
            ["issue-2", "issue-3", "blocks"],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO dependencies (blocked_issue_id, blocker_issue_id, kind)
             VALUES (?1, ?2, ?3)",
            ["issue-1", "issue-2", "blocks"],
        )
        .unwrap();

        let tx = conn.unchecked_transaction().unwrap();
        let has_path = dfs_has_path(
            &tx,
            "issue-1",
            "issue-3",
            &mut std::collections::HashSet::new(),
        )
        .unwrap();
        assert!(has_path);
    }

    #[test]
    fn test_dfs_has_path_none() {
        let (mut store, _temp) = test_store();
        create_test_issue(&mut store, "issue-1", "Issue 1");
        create_test_issue(&mut store, "issue-2", "Issue 2");

        let conn = store.conn();
        let tx = conn.unchecked_transaction().unwrap();
        let has_path = dfs_has_path(
            &tx,
            "issue-1",
            "issue-2",
            &mut std::collections::HashSet::new(),
        )
        .unwrap();
        assert!(!has_path);
    }

    #[test]
    fn test_conditional_dependency_basic() {
        let (mut store, _temp) = test_store();
        create_test_issue(&mut store, "issue-1", "Blocked Issue");
        create_test_issue(&mut store, "issue-2", "Blocker Issue");

        // Create a conditional dependency: issue-1 is blocked by issue-2 only when issue-2 has priority 2
        let condition = ConditionExpr::Equals {
            field: "priority".to_string(),
            value: serde_json::json!(2),
        };

        add_dependency(&mut store, "issue-1", "issue-2", "blocks", Some(&condition)).unwrap();

        // Verify dependency was stored with condition
        let conn = store.conn();
        let stored_condition: Option<String> = conn
            .query_row(
                "SELECT condition FROM dependencies WHERE blocked_issue_id = ? AND blocker_issue_id = ?",
                ["issue-1", "issue-2"],
                |row| row.get(0),
            )
            .unwrap();

        assert!(stored_condition.is_some());

        // Parse and verify the condition
        let parsed_condition = ConditionExpr::from_json(&stored_condition.unwrap()).unwrap();
        assert_eq!(parsed_condition, condition);
    }

    #[test]
    fn test_conditional_dependency_validation() {
        let (mut store, _temp) = test_store();
        create_test_issue(&mut store, "issue-1", "Blocked Issue");
        create_test_issue(&mut store, "issue-2", "Blocker Issue");

        // Try to create a conditional dependency with an unsupported field
        let invalid_condition = ConditionExpr::Equals {
            field: "unsupported_field".to_string(),
            value: serde_json::json!(2),
        };

        let result = add_dependency(
            &mut store,
            "issue-1",
            "issue-2",
            "blocks",
            Some(&invalid_condition),
        );
        assert!(matches!(result, Err(Error::CliUsage(_))));
    }

    #[test]
    fn test_conditional_dependency_evaluation() {
        let (mut store, _temp) = test_store();
        // Create issues with different priorities
        create_test_issue(&mut store, "issue-1", "Blocked Issue");
        create_test_issue(&mut store, "issue-2", "Blocker Issue");

        // Update issue-2 to have priority 2
        {
            let conn = store.conn();
            conn.execute("UPDATE issues SET priority = 2 WHERE id = 'issue-2'", [])
                .unwrap();
        } // conn is dropped here, releasing the mutable borrow

        // Create a conditional dependency: issue-1 is blocked by issue-2 when issue-2 has priority 2
        let condition = ConditionExpr::Equals {
            field: "priority".to_string(),
            value: serde_json::json!(2),
        };

        add_dependency(&mut store, "issue-1", "issue-2", "blocks", Some(&condition)).unwrap();

        // Check if the conditional dependency is active
        let conditional_deps = get_conditional_dependencies(&mut store, "issue-1").unwrap();
        assert_eq!(conditional_deps.len(), 1);

        let (blocker_id, _kind, stored_condition) = &conditional_deps[0];
        assert_eq!(blocker_id, "issue-2");

        // Evaluate the condition - should be true since issue-2 has priority 2
        let is_active =
            is_conditional_dependency_active(&mut store, "issue-2", stored_condition).unwrap();
        assert!(is_active);

        // Check if issue-1 has active conditional blockers
        let has_active = has_active_conditional_blockers(&mut store, "issue-1").unwrap();
        assert!(has_active);
    }

    #[test]
    fn test_conditional_dependency_inactive() {
        let (mut store, _temp) = test_store();
        create_test_issue(&mut store, "issue-1", "Blocked Issue");
        create_test_issue(&mut store, "issue-2", "Blocker Issue");

        // Create a conditional dependency: issue-1 is blocked by issue-2 when issue-2 has priority 3
        let condition = ConditionExpr::Equals {
            field: "priority".to_string(),
            value: serde_json::json!(3),
        };

        add_dependency(&mut store, "issue-1", "issue-2", "blocks", Some(&condition)).unwrap();

        // Since issue-2 has priority 2 (default), the condition should be false
        let conditional_deps = get_conditional_dependencies(&mut store, "issue-1").unwrap();
        assert_eq!(conditional_deps.len(), 1);

        let (blocker_id, _kind, stored_condition) = &conditional_deps[0];
        let is_active =
            is_conditional_dependency_active(&mut store, blocker_id, stored_condition).unwrap();
        assert!(!is_active);

        // Check if issue-1 has active conditional blockers - should be false
        let has_active = has_active_conditional_blockers(&mut store, "issue-1").unwrap();
        assert!(!has_active);
    }

    #[test]
    fn test_conditional_dependency_with_logical_operators() {
        let (mut store, _temp) = test_store();
        create_test_issue(&mut store, "issue-1", "Blocked Issue");
        create_test_issue(&mut store, "issue-2", "Blocker Issue");

        // Create a conditional dependency with logical AND
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

        add_dependency(&mut store, "issue-1", "issue-2", "blocks", Some(&condition)).unwrap();

        // Both conditions should be true (issue-2 has priority 2 and status "open")
        let conditional_deps = get_conditional_dependencies(&mut store, "issue-1").unwrap();
        let (blocker_id, _kind, stored_condition) = &conditional_deps[0];
        let is_active =
            is_conditional_dependency_active(&mut store, blocker_id, stored_condition).unwrap();
        assert!(is_active);
    }

    #[test]
    fn test_conditional_dependency_prevents_cycles() {
        let (mut store, _temp) = test_store();
        create_test_issue(&mut store, "issue-1", "Issue 1");
        create_test_issue(&mut store, "issue-2", "Issue 2");
        create_test_issue(&mut store, "issue-3", "Issue 3");

        // Create chain: issue-1 -> issue-2 -> issue-3
        add_dependency(&mut store, "issue-2", "issue-3", "blocks", None).unwrap();
        add_dependency(&mut store, "issue-1", "issue-2", "blocks", None).unwrap();

        // Try to create conditional cycle: issue-3 -> issue-1 (even with a condition)
        let condition = ConditionExpr::Equals {
            field: "priority".to_string(),
            value: serde_json::json!(2),
        };

        // Should still reject cycle creation even with conditional dependencies
        let result = add_dependency(&mut store, "issue-3", "issue-1", "blocks", Some(&condition));
        assert!(matches!(result, Err(Error::Conflict(_))));
    }

    #[test]
    fn test_conditional_dependency_with_issue_type() {
        let (mut store, _temp) = test_store();
        create_test_issue(&mut store, "issue-1", "Blocked Issue");
        create_test_issue(&mut store, "issue-2", "Blocker Issue");

        // Update issue-2 to have a specific issue type
        let conn = store.conn();
        conn.execute(
            "UPDATE issues SET issue_type = ? WHERE id = ?",
            ["bug", "issue-2"],
        )
        .unwrap();

        // Create a conditional dependency based on issue type
        let condition = ConditionExpr::Equals {
            field: "issue_type".to_string(),
            value: serde_json::json!("bug"),
        };

        add_dependency(&mut store, "issue-1", "issue-2", "blocks", Some(&condition)).unwrap();

        // The condition should be active since issue-2 is of type "bug"
        let conditional_deps = get_conditional_dependencies(&mut store, "issue-1").unwrap();
        let (blocker_id, _kind, stored_condition) = &conditional_deps[0];
        let is_active =
            is_conditional_dependency_active(&mut store, blocker_id, stored_condition).unwrap();
        assert!(is_active);
    }
}

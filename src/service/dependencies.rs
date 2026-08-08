//! Labels and dependency graph operations.

use crate::error::Error;
use crate::store::SqliteStore;

/// Adds a label to an issue.
///
/// This operation is idempotent: adding an existing label succeeds without changes.
pub fn add_label(store: &mut SqliteStore, issue_id: &str, label: &str) -> Result<(), Error> {
    let conn = store.conn();
    let tx = conn.unchecked_transaction()?;

    // Verify issue exists
    let issue_exists = tx
        .query_row("SELECT 1 FROM issues WHERE id = ?", [&issue_id], |_| Ok(()))
        .is_ok();

    if !issue_exists {
        return Err(Error::not_found(format!("Issue {issue_id}")));
    }

    // Idempotent insert: ignore if already exists
    tx.execute(
        "INSERT OR IGNORE INTO labels (issue_id, label) VALUES (?1, ?2)",
        [issue_id, label],
    )?;

    tx.commit()?;
    Ok(())
}

/// Removes a label from an issue.
///
/// This operation is idempotent: removing a non-existent label succeeds without changes.
pub fn remove_label(store: &mut SqliteStore, issue_id: &str, label: &str) -> Result<(), Error> {
    let conn = store.conn();
    let tx = conn.unchecked_transaction()?;

    // Verify issue exists
    let issue_exists = tx
        .query_row("SELECT 1 FROM issues WHERE id = ?", [&issue_id], |_| Ok(()))
        .is_ok();

    if !issue_exists {
        return Err(Error::not_found(format!("Issue {issue_id}")));
    }

    // Idempotent delete: ignore if not exists
    tx.execute(
        "DELETE FROM labels WHERE issue_id = ?1 AND label = ?2",
        [issue_id, label],
    )?;

    tx.commit()?;
    Ok(())
}

/// Adds a dependency edge between two issues.
///
/// Rejects self-edges and cycles in `blocks` dependencies.
/// This operation is idempotent: adding an existing edge succeeds without changes.
///
/// # Arguments
/// * `blocked_id` - The issue that is blocked
/// * `blocker_id` - The issue that does the blocking
/// * `kind` - The dependency kind (e.g., "blocks", "relates_to")
pub fn add_dependency(
    store: &mut SqliteStore,
    blocked_id: &str,
    blocker_id: &str,
    kind: &str,
) -> Result<(), Error> {
    let conn = store.conn();
    let tx = conn.unchecked_transaction()?;

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

    // For `blocks` dependencies, detect and reject cycles
    if kind == "blocks" && creates_cycle(&tx, blocked_id, blocker_id)? {
        return Err(Error::Conflict(
            "Adding this dependency would create a cycle".to_string(),
        ));
    }

    // Idempotent insert: ignore if already exists
    tx.execute(
        "INSERT OR IGNORE INTO dependencies (blocked_issue_id, blocker_issue_id, kind) VALUES (?1, ?2, ?3)",
        [blocked_id, blocker_id, kind],
    )?;

    tx.commit()?;
    Ok(())
}

/// Removes a dependency edge.
///
/// This operation is idempotent: removing a non-existent edge succeeds without changes.
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
    let tx = conn.unchecked_transaction()?;

    if let Some(k) = kind {
        tx.execute(
            "DELETE FROM dependencies WHERE blocked_issue_id = ?1 AND blocker_issue_id = ?2 AND kind = ?3",
            [blocked_id, blocker_id, k],
        )?;
    } else {
        tx.execute(
            "DELETE FROM dependencies WHERE blocked_issue_id = ?1 AND blocker_issue_id = ?2",
            [blocked_id, blocker_id],
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

        add_dependency(&mut store, "issue-1", "issue-2", "blocks").unwrap();

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

        add_dependency(&mut store, "issue-1", "issue-2", "blocks").unwrap();
        add_dependency(&mut store, "issue-1", "issue-2", "blocks").unwrap(); // Duplicate

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

        let result = add_dependency(&mut store, "issue-1", "issue-1", "blocks");
        assert!(matches!(result, Err(Error::Conflict(_))));
    }

    #[test]
    fn test_add_dependency_creates_cycle() {
        let (mut store, _temp) = test_store();
        create_test_issue(&mut store, "issue-1", "Issue 1");
        create_test_issue(&mut store, "issue-2", "Issue 2");
        create_test_issue(&mut store, "issue-3", "Issue 3");

        // Create chain: issue-1 -> issue-2 -> issue-3
        add_dependency(&mut store, "issue-2", "issue-3", "blocks").unwrap();
        add_dependency(&mut store, "issue-1", "issue-2", "blocks").unwrap();

        // Try to create cycle: issue-3 -> issue-1
        let result = add_dependency(&mut store, "issue-3", "issue-1", "blocks");
        assert!(matches!(result, Err(Error::Conflict(_))));
    }

    #[test]
    fn test_add_dependency_nonexistent_blocked() {
        let (mut store, _temp) = test_store();
        create_test_issue(&mut store, "issue-2", "Blocker Issue");

        let result = add_dependency(&mut store, "nonexistent", "issue-2", "blocks");
        assert!(matches!(result, Err(Error::Workspace(_))));
    }

    #[test]
    fn test_add_dependency_nonexistent_blocker() {
        let (mut store, _temp) = test_store();
        create_test_issue(&mut store, "issue-1", "Blocked Issue");

        let result = add_dependency(&mut store, "issue-1", "nonexistent", "blocks");
        assert!(matches!(result, Err(Error::Workspace(_))));
    }

    #[test]
    fn test_remove_dependency() {
        let (mut store, _temp) = test_store();
        create_test_issue(&mut store, "issue-1", "Blocked Issue");
        create_test_issue(&mut store, "issue-2", "Blocker Issue");

        add_dependency(&mut store, "issue-1", "issue-2", "blocks").unwrap();
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

        add_dependency(&mut store, "issue-1", "issue-2", "blocks").unwrap();
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

        add_dependency(&mut store, "issue-1", "issue-2", "blocks").unwrap();
        add_dependency(&mut store, "issue-1", "issue-2", "relates_to").unwrap();

        // Remove all edges between these issues
        remove_dependency(&mut store, "issue-1", "issue-2", None).unwrap();

        // Verify both dependencies were removed
        let conn = store.conn();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM dependencies", [], |row| row.get(0))
            .unwrap();

        assert_eq!(count, 0);
    }

    #[test]
    fn test_relates_to_allows_cycles() {
        let (mut store, _temp) = test_store();
        create_test_issue(&mut store, "issue-1", "Issue 1");
        create_test_issue(&mut store, "issue-2", "Issue 2");

        // Create cycle with relates_to: issue-1 -> issue-2 -> issue-1
        add_dependency(&mut store, "issue-1", "issue-2", "relates_to").unwrap();
        add_dependency(&mut store, "issue-2", "issue-1", "relates_to").unwrap();

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
}

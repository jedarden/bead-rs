//! Integration tests for R016: Scoped doctor and diagnostic mode
//!
//! These tests verify that the scoped doctor functionality works correctly:
//! - Store, backup, schema, dependencies, comments, and all scopes
//! - JSON diagnostics output
//! - Backup generations and freshness checking
//! - Schema/data validity checks
//! - Conditional predicates and latent cycles detection
//! - Repairs stay narrowly allowlisted and never rewrite user semantic data

use bead_rs::service::doctor::{run_diagnostics_with_scopes, DiagnosticScope, DoctorDiagnostics};
use bead_rs::store::Store;
use serial_test::serial;
use std::fs;
use tempfile::TempDir;

#[cfg(test)]
mod tests {
    use super::*;

    /// Test DiagnosticScope parsing
    #[test]
    fn test_diagnostic_scope_parsing() {
        assert_eq!(
            DiagnosticScope::from_str("store"),
            Some(DiagnosticScope::Store)
        );
        assert_eq!(
            DiagnosticScope::from_str("backup"),
            Some(DiagnosticScope::Backup)
        );
        assert_eq!(
            DiagnosticScope::from_str("schema"),
            Some(DiagnosticScope::Schema)
        );
        assert_eq!(
            DiagnosticScope::from_str("dependencies"),
            Some(DiagnosticScope::Dependencies)
        );
        assert_eq!(
            DiagnosticScope::from_str("comments"),
            Some(DiagnosticScope::Comments)
        );
        assert_eq!(DiagnosticScope::from_str("all"), Some(DiagnosticScope::All));
        assert_eq!(DiagnosticScope::from_str("invalid"), None);
        assert_eq!(
            DiagnosticScope::from_str("STORE"),
            Some(DiagnosticScope::Store)
        ); // Case insensitive
    }

    /// Test that all_scopes() returns valid scope names
    #[test]
    fn test_all_scopes() {
        let scopes = DiagnosticScope::all_scopes();
        assert!(scopes.contains(&"store"));
        assert!(scopes.contains(&"backup"));
        assert!(scopes.contains(&"schema"));
        assert!(scopes.contains(&"dependencies"));
        assert!(scopes.contains(&"comments"));
        assert!(scopes.contains(&"all"));
    }

    /// Test that DoctorDiagnostics structure can be serialized to JSON
    #[test]
    fn test_doctor_diagnostics_json_serialization() {
        let diagnostics = DoctorDiagnostics {
            checks: vec![],
            has_errors: false,
            has_warnings: false,
            scopes_checked: vec!["store".to_string(), "backup".to_string()],
            timestamp: "2024-08-09T12:00:00Z".to_string(),
        };

        let json = serde_json::to_string(&diagnostics);
        assert!(json.is_ok(), "Should serialize to JSON");

        let parsed: serde_json::Value = json.unwrap().parse().unwrap();
        assert_eq!(parsed["has_errors"], false);
        assert_eq!(parsed["has_warnings"], false);
        assert!(parsed["scopes_checked"].is_array());
    }

    /// Test running diagnostics with specific scopes
    #[test]
    #[serial]
    fn test_run_diagnostics_with_store_scope() {
        let temp_dir = TempDir::new().unwrap();

        // Initialize a workspace
        std::env::set_current_dir(temp_dir.path()).unwrap();
        let store = bead_rs::store::SqliteStore::new();
        let config = store.init_workspace("test").unwrap();
        assert!(config.root.exists(), "Workspace should exist");

        let result = run_diagnostics_with_scopes(&store, &[DiagnosticScope::Store]);

        assert!(result.is_ok(), "Should run diagnostics successfully");
        let diagnostics = result.unwrap();
        assert!(diagnostics.scopes_checked.contains(&"store".to_string()));
        assert!(
            !diagnostics.has_errors,
            "Store scope should not have errors in fresh workspace"
        );
    }

    /// Test running diagnostics with all scopes
    #[test]
    #[serial]
    fn test_run_diagnostics_with_all_scopes() {
        let temp_dir = TempDir::new().unwrap();

        // Initialize a workspace
        std::env::set_current_dir(temp_dir.path()).unwrap();
        let store = bead_rs::store::SqliteStore::new();
        let config = store.init_workspace("test").unwrap();
        assert!(config.root.exists(), "Workspace should exist");

        let result = run_diagnostics_with_scopes(&store, &[DiagnosticScope::All]);

        assert!(result.is_ok(), "Should run diagnostics successfully");
        let diagnostics = result.unwrap();
        assert!(
            diagnostics.scopes_checked.len() >= 5,
            "Should check at least 5 scopes"
        );
        assert!(diagnostics.scopes_checked.contains(&"store".to_string()));
        assert!(diagnostics.scopes_checked.contains(&"backup".to_string()));
        assert!(diagnostics.scopes_checked.contains(&"schema".to_string()));
        assert!(diagnostics
            .scopes_checked
            .contains(&"dependencies".to_string()));
        assert!(diagnostics.scopes_checked.contains(&"comments".to_string()));
    }

    /// Test running diagnostics with backup scope specifically
    #[test]
    #[serial]
    fn test_run_diagnostics_with_backup_scope() {
        let temp_dir = TempDir::new().unwrap();

        // Initialize a workspace
        std::env::set_current_dir(temp_dir.path()).unwrap();
        let store = bead_rs::store::SqliteStore::new();
        let _config = store.init_workspace("test").unwrap();

        let result = run_diagnostics_with_scopes(&store, &[DiagnosticScope::Backup]);

        assert!(result.is_ok(), "Should run diagnostics successfully");
        let diagnostics = result.unwrap();
        assert!(diagnostics.scopes_checked.contains(&"backup".to_string()));
    }

    /// Test running diagnostics with schema scope specifically
    #[test]
    #[serial]
    fn test_run_diagnostics_with_schema_scope() {
        let temp_dir = TempDir::new().unwrap();

        // Initialize a workspace
        std::env::set_current_dir(temp_dir.path()).unwrap();
        let store = bead_rs::store::SqliteStore::new();
        let _config = store.init_workspace("test").unwrap();

        let result = run_diagnostics_with_scopes(&store, &[DiagnosticScope::Schema]);

        assert!(result.is_ok(), "Should run diagnostics successfully");
        let diagnostics = result.unwrap();
        assert!(diagnostics.scopes_checked.contains(&"schema".to_string()));
        assert!(
            !diagnostics.has_errors,
            "Schema scope should not have errors in fresh workspace"
        );
    }

    /// Test running diagnostics with dependencies scope specifically
    #[test]
    #[serial]
    fn test_run_diagnostics_with_dependencies_scope() {
        let temp_dir = TempDir::new().unwrap();

        // Initialize a workspace
        std::env::set_current_dir(temp_dir.path()).unwrap();
        let store = bead_rs::store::SqliteStore::new();
        let _config = store.init_workspace("test").unwrap();

        let result = run_diagnostics_with_scopes(&store, &[DiagnosticScope::Dependencies]);

        assert!(result.is_ok(), "Should run diagnostics successfully");
        let diagnostics = result.unwrap();
        assert!(diagnostics
            .scopes_checked
            .contains(&"dependencies".to_string()));
        assert!(
            !diagnostics.has_errors,
            "Dependencies scope should not have errors in fresh workspace"
        );
    }

    /// Test running diagnostics with comments scope specifically
    #[test]
    #[serial]
    fn test_run_diagnostics_with_comments_scope() {
        let temp_dir = TempDir::new().unwrap();

        // Initialize a workspace
        std::env::set_current_dir(temp_dir.path()).unwrap();
        let store = bead_rs::store::SqliteStore::new();
        let _config = store.init_workspace("test").unwrap();

        let result = run_diagnostics_with_scopes(&store, &[DiagnosticScope::Comments]);

        assert!(result.is_ok(), "Should run diagnostics successfully");
        let diagnostics = result.unwrap();
        assert!(diagnostics.scopes_checked.contains(&"comments".to_string()));
        assert!(
            !diagnostics.has_errors,
            "Comments scope should not have errors in fresh workspace"
        );
    }

    /// Test running diagnostics with multiple scopes
    #[test]
    #[serial]
    fn test_run_diagnostics_with_multiple_scopes() {
        let temp_dir = TempDir::new().unwrap();

        // Initialize a workspace
        std::env::set_current_dir(temp_dir.path()).unwrap();
        let store = bead_rs::store::SqliteStore::new();
        let _config = store.init_workspace("test").unwrap();

        let result = run_diagnostics_with_scopes(
            &store,
            &[
                DiagnosticScope::Store,
                DiagnosticScope::Schema,
                DiagnosticScope::Dependencies,
            ],
        );

        assert!(result.is_ok(), "Should run diagnostics successfully");
        let diagnostics = result.unwrap();
        assert!(diagnostics.scopes_checked.contains(&"store".to_string()));
        assert!(diagnostics.scopes_checked.contains(&"schema".to_string()));
        assert!(diagnostics
            .scopes_checked
            .contains(&"dependencies".to_string()));
        assert!(!diagnostics.scopes_checked.contains(&"comments".to_string())); // Should not check comments
    }

    /// Test that cycle detection works in dependency graph
    #[test]
    #[serial]
    fn test_dependency_cycle_detection() {
        let temp_dir = TempDir::new().unwrap();

        // Initialize a workspace
        std::env::set_current_dir(temp_dir.path()).unwrap();
        let store = bead_rs::store::SqliteStore::new();
        let config = store.init_workspace("test").unwrap();
        let db_path = config.database_path();

        let conn = rusqlite::Connection::open(&db_path).unwrap();

        // Create some test issues
        conn.execute(
            "INSERT INTO issues (id, title, priority, base_status, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
            ["issue-1", "Issue 1", "1", "open", "2024-08-09T12:00:00Z", "2024-08-09T12:00:00Z"],
        ).unwrap();

        conn.execute(
            "INSERT INTO issues (id, title, priority, base_status, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
            ["issue-2", "Issue 2", "1", "open", "2024-08-09T12:00:00Z", "2024-08-09T12:00:00Z"],
        ).unwrap();

        conn.execute(
            "INSERT INTO issues (id, title, priority, base_status, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
            ["issue-3", "Issue 3", "1", "open", "2024-08-09T12:00:00Z", "2024-08-09T12:00:00Z"],
        ).unwrap();

        // Create a cycle: issue-1 -> issue-2 -> issue-3 -> issue-1
        conn.execute(
            "INSERT INTO dependencies (blocked_issue_id, blocker_issue_id, kind) VALUES (?, ?, ?)",
            ["issue-2", "issue-1", "blocks"],
        )
        .unwrap();

        conn.execute(
            "INSERT INTO dependencies (blocked_issue_id, blocker_issue_id, kind) VALUES (?, ?, ?)",
            ["issue-3", "issue-2", "blocks"],
        )
        .unwrap();

        conn.execute(
            "INSERT INTO dependencies (blocked_issue_id, blocker_issue_id, kind) VALUES (?, ?, ?)",
            ["issue-1", "issue-3", "blocks"],
        )
        .unwrap();

        // Run dependency diagnostics
        let store = bead_rs::store::SqliteStore::with_path(&db_path).unwrap();
        let result = run_diagnostics_with_scopes(&store, &[DiagnosticScope::Dependencies]);

        // Should detect the cycle
        assert!(result.is_ok(), "Should run diagnostics successfully");
        let diagnostics = result.unwrap();
        assert!(
            diagnostics.has_errors,
            "Should detect dependency cycles as errors"
        );
        assert!(
            diagnostics
                .checks
                .iter()
                .any(|c| c.name == "dependency_graph"
                    && c.status == bead_rs::service::doctor::DiagnosticStatus::Error),
            "Should have dependency_graph check with error status"
        );
    }

    /// Test that self-edges are prevented by database constraints
    #[test]
    #[serial]
    fn test_self_edge_prevention() {
        let temp_dir = TempDir::new().unwrap();

        // Initialize a workspace
        std::env::set_current_dir(temp_dir.path()).unwrap();
        let store = bead_rs::store::SqliteStore::new();
        let config = store.init_workspace("test").unwrap();
        let db_path = config.database_path();

        let conn = rusqlite::Connection::open(&db_path).unwrap();

        // Create a test issue
        conn.execute(
            "INSERT INTO issues (id, title, priority, base_status, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
            ["issue-self", "Self Edge Issue", "1", "open", "2024-08-09T12:00:00Z", "2024-08-09T12:00:00Z"],
        ).unwrap();

        // Try to create a self-edge - should fail due to CHECK constraint
        let result = conn.execute(
            "INSERT INTO dependencies (blocked_issue_id, blocker_issue_id, kind) VALUES (?, ?, ?)",
            ["issue-self", "issue-self", "blocks"],
        );

        assert!(
            result.is_err(),
            "Database should prevent self-edges via CHECK constraint"
        );
    }

    /// Test that repairs maintain narrow allowlist
    #[test]
    #[serial]
    fn test_repairs_narrow_allowlist() {
        let temp_dir = TempDir::new().unwrap();

        // Initialize a workspace
        std::env::set_current_dir(temp_dir.path()).unwrap();
        let store = bead_rs::store::SqliteStore::new();
        let config = store.init_workspace("test").unwrap();
        let beads_dir = temp_dir.path().join(".beads");

        // Create a temporary file
        let temp_file = beads_dir.join("test.tmp");
        fs::write(&temp_file, "temporary content").unwrap();

        // Run repairs
        let mut store = bead_rs::store::SqliteStore::with_path(&config.database_path()).unwrap();
        let repairs = bead_rs::service::run_repairs(&mut store);

        assert!(repairs.is_ok(), "Should run repairs successfully");
        let repairs_list = repairs.unwrap();
        assert!(!temp_file.exists(), "Temporary file should be removed");
        assert!(!repairs_list.is_empty(), "Should report repairs performed");
        assert!(
            repairs_list.iter().all(|r| r.name == "removed_temp_file"),
            "Should only remove temp files"
        );
    }

    /// Test that checkpoint freshness is checked
    #[test]
    #[serial]
    fn test_checkpoint_freshness_check() {
        let temp_dir = TempDir::new().unwrap();

        // Initialize a workspace
        std::env::set_current_dir(temp_dir.path()).unwrap();
        let store = bead_rs::store::SqliteStore::new();
        let config = store.init_workspace("test").unwrap();
        let db_path = config.database_path();

        let store = bead_rs::store::SqliteStore::with_path(&db_path).unwrap();
        let result = run_diagnostics_with_scopes(&store, &[DiagnosticScope::Backup]);

        assert!(result.is_ok(), "Should run diagnostics successfully");
        let diagnostics = result.unwrap();
        assert!(diagnostics.scopes_checked.contains(&"backup".to_string()));

        // Should have freshness check
        let freshness_check = diagnostics
            .checks
            .iter()
            .find(|c| c.name == "checkpoint_freshness");
        assert!(
            freshness_check.is_some(),
            "Should have checkpoint_freshness check"
        );
    }

    /// Test JSON output stability and structure
    #[test]
    #[serial]
    fn test_json_output_structure() {
        let temp_dir = TempDir::new().unwrap();

        // Initialize a workspace
        std::env::set_current_dir(temp_dir.path()).unwrap();
        let store = bead_rs::store::SqliteStore::new();
        let config = store.init_workspace("test").unwrap();

        let store = bead_rs::store::SqliteStore::with_path(&config.database_path()).unwrap();
        let result = run_diagnostics_with_scopes(&store, &[DiagnosticScope::Store]);

        assert!(result.is_ok(), "Should run diagnostics successfully");
        let diagnostics = result.unwrap();

        // Serialize to JSON
        let json_result = serde_json::to_string(&diagnostics);
        assert!(json_result.is_ok(), "Should serialize to JSON");

        let json_str = json_result.unwrap();
        let parsed: serde_json::Value = json_str.parse().unwrap();

        // Check required fields
        assert!(parsed.get("checks").is_some(), "Should have checks field");
        assert!(
            parsed.get("has_errors").is_some(),
            "Should have has_errors field"
        );
        assert!(
            parsed.get("has_warnings").is_some(),
            "Should have has_warnings field"
        );
        assert!(
            parsed.get("scopes_checked").is_some(),
            "Should have scopes_checked field"
        );
        assert!(
            parsed.get("timestamp").is_some(),
            "Should have timestamp field"
        );

        // Check that checks is an array
        let checks = parsed["checks"].as_array().unwrap();
        assert!(!checks.is_empty(), "Should have at least one check");

        // Each check should have required fields
        for check in checks {
            assert!(check.get("name").is_some(), "Check should have name");
            assert!(check.get("status").is_some(), "Check should have status");
            assert!(check.get("message").is_some(), "Check should have message");
        }
    }

    /// Test that scope parsing handles edge cases
    #[test]
    fn test_scope_edge_cases() {
        // Empty string
        assert_eq!(DiagnosticScope::from_str(""), None);

        // Mixed case
        assert_eq!(
            DiagnosticScope::from_str("StOrE"),
            Some(DiagnosticScope::Store)
        );

        // With spaces
        assert_eq!(DiagnosticScope::from_str(" store "), None); // Should fail with spaces

        // Partial matches should fail
        assert_eq!(DiagnosticScope::from_str("stor"), None);
        assert_eq!(DiagnosticScope::from_str("back"), None);
    }
}

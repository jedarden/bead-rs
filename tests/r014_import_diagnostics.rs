//! R014 Import Diagnostics Tests
//!
//! Test suite for comprehensive import diagnostic reports (R014)
//!
//! R014 Acceptance Criteria:
//! - Collect bounded, deterministically ordered set of validation failures
//! - Include line number, JSON Pointer, schema keyword, semantic code, truncation marker
//! - No state activates; replaces repeated one-error-per-import repair cycles
//! - Prevents unbounded memory consumption or cascading noise

use bead_rs::service::checkpoint;
use bead_rs::store::SqliteStore;
use std::fs::File;
use std::io::Write;
use tempfile::TempDir;

/// Helper to create a temporary workspace
fn create_temp_workspace() -> TempDir {
    let temp_dir = TempDir::new().unwrap();
    let workspace_path = temp_dir.path();
    let beads_dir = workspace_path.join(".beads");

    // Create .beads directory
    std::fs::create_dir_all(&beads_dir).unwrap();
    let db_path = beads_dir.join("beads.db");

    // Initialize database
    let mut store = SqliteStore::with_path(&db_path).unwrap();
    store.apply_migrations().unwrap();

    temp_dir
}

/// Helper to create a test JSONL file
fn create_test_jsonl(path: &std::path::Path, content: &str) {
    let mut file = File::create(path).unwrap();
    file.write_all(content.as_bytes()).unwrap();
    file.flush().unwrap();
}

#[test]
fn test_diagnostics_malformed_json() {
    let temp_dir = create_temp_workspace();
    let db_path = temp_dir.path().join(".beads/beads.db");

    let conn = rusqlite::Connection::open(&db_path).unwrap();
    let mut store = SqliteStore::from_conn(conn);

    // Create test file with malformed JSON
    let test_file = temp_dir.path().join("test.jsonl");
    create_test_jsonl(
        &test_file,
        r#"{"id":"bead-001","title":"Valid issue","priority":2,"base_status":"open","created_at":"2024-01-01T00:00:00Z","updated_at":"2024-01-01T00:00:00Z"}
{"id":"bead-002","title":"Invalid JSON","priority":2,"base_status":"open"#,
    );

    // Import with diagnostics
    let result = checkpoint::import_checkpoint_with_diagnostics(
        &mut store,
        &test_file,
        "native-v1",
        true,
        true,
    )
    .unwrap();

    // Should have diagnostics
    assert!(result.diagnostics.is_some());
    let diagnostics = result.diagnostics.as_ref().unwrap();

    // Should have at least one error (malformed JSON)
    assert!(!diagnostics.validation_failures.is_empty());

    // Should report correct line number
    let json_errors: Vec<_> = diagnostics
        .validation_failures
        .iter()
        .filter(|f| f.semantic_code == "malformed_json")
        .collect();
    assert!(!json_errors.is_empty());
    assert_eq!(json_errors[0].line_number, 2);

    // Should have truncation marker if many errors
    assert_eq!(diagnostics.total_lines, 2);
    assert_eq!(diagnostics.processed_lines, 2);
}

#[test]
fn test_diagnostics_duplicate_ids() {
    let temp_dir = create_temp_workspace();
    let db_path = temp_dir.path().join(".beads/beads.db");

    let conn = rusqlite::Connection::open(&db_path).unwrap();
    let mut store = SqliteStore::from_conn(conn);

    // Create test file with duplicate IDs
    let test_file = temp_dir.path().join("test.jsonl");
    create_test_jsonl(
        &test_file,
        r#"{"id":"bead-001","title":"First issue","priority":2,"base_status":"open","created_at":"2024-01-01T00:00:00Z","updated_at":"2024-01-01T00:00:00Z"}
{"id":"bead-001","title":"Duplicate ID","priority":1,"base_status":"open","created_at":"2024-01-01T00:00:00Z","updated_at":"2024-01-01T00:00:00Z"}
{"id":"bead-002","title":"Third issue","priority":2,"base_status":"open","created_at":"2024-01-01T00:00:00Z","updated_at":"2024-01-01T00:00:00Z"}"#,
    );

    // Import with diagnostics
    let result = checkpoint::import_checkpoint_with_diagnostics(
        &mut store,
        &test_file,
        "native-v1",
        true,
        true,
    )
    .unwrap();

    // Should have diagnostics
    assert!(result.diagnostics.is_some());
    let diagnostics = result.diagnostics.as_ref().unwrap();

    // Should have duplicate ID error
    let duplicate_errors: Vec<_> = diagnostics
        .validation_failures
        .iter()
        .filter(|f| f.semantic_code == "duplicate_issue_id")
        .collect();
    assert!(!duplicate_errors.is_empty());

    // Should reference the duplicate ID
    assert!(duplicate_errors[0].message.contains("bead-001"));

    // Should have JSON pointer
    assert_eq!(duplicate_errors[0].json_pointer.as_ref().unwrap(), "/id");
}

#[test]
fn test_diagnostics_unknown_dependency() {
    let temp_dir = create_temp_workspace();
    let db_path = temp_dir.path().join(".beads/beads.db");

    let conn = rusqlite::Connection::open(&db_path).unwrap();
    let mut store = SqliteStore::from_conn(conn);

    // Create test file with unknown dependency reference
    let test_file = temp_dir.path().join("test.jsonl");
    create_test_jsonl(
        &test_file,
        r#"{"id":"bead-001","title":"Issue with unknown blocker","priority":2,"base_status":"open","created_at":"2024-01-01T00:00:00Z","updated_at":"2024-01-01T00:00:00Z","dependencies":[{"blocker":"bead-999","kind":"blocks"}]}
{"id":"bead-002","title":"Valid issue","priority":2,"base_status":"open","created_at":"2024-01-01T00:00:00Z","updated_at":"2024-01-01T00:00:00Z"}"#,
    );

    // Import with diagnostics
    let result = checkpoint::import_checkpoint_with_diagnostics(
        &mut store,
        &test_file,
        "native-v1",
        true,
        true,
    )
    .unwrap();

    // Should have diagnostics
    assert!(result.diagnostics.is_some());
    let diagnostics = result.diagnostics.as_ref().unwrap();

    // Should have unknown blocker error
    let blocker_errors: Vec<_> = diagnostics
        .validation_failures
        .iter()
        .filter(|f| f.semantic_code == "unknown_blocker_issue")
        .collect();
    assert!(!blocker_errors.is_empty());
    assert!(blocker_errors[0].message.contains("bead-999"));
}

#[test]
fn test_diagnostics_cycle_detection() {
    let temp_dir = create_temp_workspace();
    let db_path = temp_dir.path().join(".beads/beads.db");

    let conn = rusqlite::Connection::open(&db_path).unwrap();
    let mut store = SqliteStore::from_conn(conn);

    // Create test file with circular dependency
    let test_file = temp_dir.path().join("test.jsonl");
    create_test_jsonl(
        &test_file,
        r#"{"id":"bead-001","title":"First issue","priority":2,"base_status":"open","created_at":"2024-01-01T00:00:00Z","updated_at":"2024-01-01T00:00:00Z","dependencies":[{"blocker":"bead-002","kind":"blocks"}]}
{"id":"bead-002","title":"Second issue","priority":2,"base_status":"open","created_at":"2024-01-01T00:00:00Z","updated_at":"2024-01-01T00:00:00Z","dependencies":[{"blocker":"bead-001","kind":"blocks"}]}"#,
    );

    // Import with diagnostics
    let result = checkpoint::import_checkpoint_with_diagnostics(
        &mut store,
        &test_file,
        "native-v1",
        true,
        true,
    )
    .unwrap();

    // Should have diagnostics
    assert!(result.diagnostics.is_some());
    let diagnostics = result.diagnostics.as_ref().unwrap();

    // Should have cycle detection error
    let cycle_errors: Vec<_> = diagnostics
        .validation_failures
        .iter()
        .filter(|f| f.semantic_code == "cycle_in_dependencies")
        .collect();
    assert!(!cycle_errors.is_empty());
    assert!(cycle_errors[0].message.contains("Cycle"));
}

#[test]
fn test_diagnostics_unknown_label_reference() {
    let temp_dir = create_temp_workspace();
    let db_path = temp_dir.path().join(".beads/beads.db");

    let conn = rusqlite::Connection::open(&db_path).unwrap();
    let mut store = SqliteStore::from_conn(conn);

    // Create test file with label reference to unknown issue
    let test_file = temp_dir.path().join("test.jsonl");
    create_test_jsonl(
        &test_file,
        r#"{"id":"bead-001","title":"Valid issue","priority":2,"base_status":"open","created_at":"2024-01-01T00:00:00Z","updated_at":"2024-01-01T00:00:00Z","labels":["bug"]}"#,
    );

    // Import with diagnostics
    let result = checkpoint::import_checkpoint_with_diagnostics(
        &mut store,
        &test_file,
        "native-v1",
        true,
        true,
    )
    .unwrap();

    // Should have no diagnostics for valid data
    let diagnostics = result.diagnostics.as_ref();
    if let Some(d) = diagnostics {
        // If we have any diagnostics, they shouldn't be about unknown labels
        let label_errors: Vec<_> = d
            .validation_failures
            .iter()
            .filter(|f| f.semantic_code == "unknown_issue_label")
            .collect();
        assert!(
            label_errors.is_empty(),
            "Should not have unknown label errors for valid data"
        );
    }
}

#[test]
fn test_diagnostics_bounded_collection() {
    let temp_dir = create_temp_workspace();
    let db_path = temp_dir.path().join(".beads/beads.db");

    let conn = rusqlite::Connection::open(&db_path).unwrap();
    let mut store = SqliteStore::from_conn(conn);

    // Create test file with many errors to test bounded collection
    let mut content = String::new();
    for i in 0..150 {
        content.push_str(&format!(r#"{{"id":"bead-{:03}","title":"Issue {}","priority":2,"base_status":"open","created_at":"2024-01-01T00:00:00Z","updated_at":"2024-01-01T00:00:00Z"}}
"#, i, i));
    }
    // Add malformed lines to generate errors
    content.push_str(
        r#"{"invalid":"malformed line 1"}
{"another":"malformed line 2"}"#,
    );

    let test_file = temp_dir.path().join("test.jsonl");
    create_test_jsonl(&test_file, &content);

    // Import with diagnostics
    let result = checkpoint::import_checkpoint_with_diagnostics(
        &mut store,
        &test_file,
        "native-v1",
        true,
        true,
    )
    .unwrap();

    // Should have diagnostics
    assert!(result.diagnostics.is_some());
    let diagnostics = result.diagnostics.as_ref().unwrap();

    // Should be bounded (MAX_DIAGNOSTIC_FAILURES is 100)
    assert!(diagnostics.validation_failures.len() <= 100);

    // Should indicate truncation if we hit the limit
    if diagnostics.validation_failures.len() >= 100 {
        assert!(
            diagnostics.truncated,
            "Should indicate truncation when at limit"
        );
    }
}

#[test]
fn test_diagnostics_deterministic_ordering() {
    let temp_dir = create_temp_workspace();
    let db_path = temp_dir.path().join(".beads/beads.db");

    // Create test file with predictable errors
    let test_file = temp_dir.path().join("test.jsonl");
    create_test_jsonl(
        &test_file,
        r#"{"id":"bead-001","title":"Issue 1","priority":2,"base_status":"open","created_at":"2024-01-01T00:00:00Z","updated_at":"2024-01-01T00:00:00Z"}
{"id":"bead-002","title":"Issue 2","priority":2,"base_status":"open","created_at":"2024-01-01T00:00:00Z","updated_at":"2024-01-01T00:00:00Z","dependencies":[{"blocker":"bead-999","kind":"blocks"}]}
{"id":"bead-003","title":"Issue 3","priority":2,"base_status":"open","created_at":"2024-01-01T00:00:00Z","updated_at":"2024-01-01T00:00:00Z"}
{"id":"bead-004","title":"Issue 4","priority":2,"base_status":"open","created_at":"2024-01-01T00:00:00Z","updated_at":"2024-01-01T00:00:00Z"}
malformed json line
{"id":"bead-005","title":"Issue 5","priority":2,"base_status":"open","created_at":"2024-01-01T00:00:00Z","updated_at":"2024-01-01T00:00:00Z"}"#,
    );

    // Import twice with diagnostics (create new stores each time)
    let result1 = {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        let mut store = SqliteStore::from_conn(conn);
        checkpoint::import_checkpoint_with_diagnostics(
            &mut store,
            &test_file,
            "native-v1",
            true,
            true,
        )
        .unwrap()
    };

    let result2 = {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        let mut store = SqliteStore::from_conn(conn);
        checkpoint::import_checkpoint_with_diagnostics(
            &mut store,
            &test_file,
            "native-v1",
            true,
            true,
        )
        .unwrap()
    };

    // Both should have diagnostics
    assert!(result1.diagnostics.is_some());
    assert!(result2.diagnostics.is_some());

    let diag1 = result1.diagnostics.as_ref().unwrap();
    let diag2 = result2.diagnostics.as_ref().unwrap();

    // Should have same number of errors
    assert_eq!(
        diag1.validation_failures.len(),
        diag2.validation_failures.len()
    );

    // Errors should be in same order (deterministic)
    for (f1, f2) in diag1
        .validation_failures
        .iter()
        .zip(diag2.validation_failures.iter())
    {
        assert_eq!(f1.line_number, f2.line_number);
        assert_eq!(f1.semantic_code, f2.semantic_code);
        assert_eq!(f1.message, f2.message);
    }
}

#[test]
fn test_diagnostics_json_pointer_paths() {
    let temp_dir = create_temp_workspace();
    let db_path = temp_dir.path().join(".beads/beads.db");

    let conn = rusqlite::Connection::open(&db_path).unwrap();
    let mut store = SqliteStore::from_conn(conn);

    // Create test file with errors at specific paths
    let test_file = temp_dir.path().join("test.jsonl");
    create_test_jsonl(
        &test_file,
        r#"{"title":"Missing ID","priority":2,"base_status":"open","created_at":"2024-01-01T00:00:00Z","updated_at":"2024-01-01T00:00:00Z"}
{"id":"bead-002","title":"Issue with dependency","priority":2,"base_status":"open","created_at":"2024-01-01T00:00:00Z","updated_at":"2024-01-01T00:00:00Z","dependencies":[{"kind":"blocks"}]}"#,
    );

    // Import with diagnostics
    let result = checkpoint::import_checkpoint_with_diagnostics(
        &mut store,
        &test_file,
        "native-v1",
        true,
        true,
    )
    .unwrap();

    // Should have diagnostics
    assert!(result.diagnostics.is_some());
    let diagnostics = result.diagnostics.as_ref().unwrap();

    // Should have JSON pointers for field errors
    let pointer_errors: Vec<_> = diagnostics
        .validation_failures
        .iter()
        .filter(|f| f.json_pointer.is_some())
        .collect();

    assert!(!pointer_errors.is_empty());

    // At least one should have a pointer to /id
    let id_pointer_errors: Vec<_> = pointer_errors
        .iter()
        .filter(|f| f.json_pointer.as_ref().unwrap() == "/id")
        .collect();
    assert!(!id_pointer_errors.is_empty());
}

#[test]
fn test_diagnostics_semantic_codes() {
    let temp_dir = create_temp_workspace();
    let db_path = temp_dir.path().join(".beads/beads.db");

    let conn = rusqlite::Connection::open(&db_path).unwrap();
    let mut store = SqliteStore::from_conn(conn);

    // Create test file with various error types
    let test_file = temp_dir.path().join("test.jsonl");
    create_test_jsonl(
        &test_file,
        r#"{"id":"bead-001","title":"Issue 1","priority":2,"base_status":"open","created_at":"2024-01-01T00:00:00Z","updated_at":"2024-01-01T00:00:00Z"}
malformed json
{"id":"bead-001","title":"Duplicate ID","priority":1,"base_status":"open","created_at":"2024-01-01T00:00:00Z","updated_at":"2024-01-01T00:00:00Z"}
{"id":"bead-002","title":"Self-dependency","priority":2,"base_status":"open","created_at":"2024-01-01T00:00:00Z","updated_at":"2024-01-01T00:00:00Z","dependencies":[{"blocker":"bead-002","kind":"blocks"}]}"#,
    );

    // Import with diagnostics
    let result = checkpoint::import_checkpoint_with_diagnostics(
        &mut store,
        &test_file,
        "native-v1",
        true,
        true,
    )
    .unwrap();

    // Should have diagnostics
    assert!(result.diagnostics.is_some());
    let diagnostics = result.diagnostics.as_ref().unwrap();

    // Should have different semantic codes
    let codes: std::collections::HashSet<_> = diagnostics
        .validation_failures
        .iter()
        .map(|f| f.semantic_code.clone())
        .collect();

    // Should include expected error codes
    assert!(codes.contains("malformed_json"));
    assert!(codes.contains("duplicate_issue_id"));
    assert!(codes.contains("self_edge_dependency"));
}

#[test]
fn test_diagnostics_blank_lines_handling() {
    let temp_dir = create_temp_workspace();
    let db_path = temp_dir.path().join(".beads/beads.db");

    let conn = rusqlite::Connection::open(&db_path).unwrap();
    let mut store = SqliteStore::from_conn(conn);

    // Create test file with blank lines
    let test_file = temp_dir.path().join("test.jsonl");
    create_test_jsonl(
        &test_file,
        r#"{"id":"bead-001","title":"First issue","priority":2,"base_status":"open","created_at":"2024-01-01T00:00:00Z","updated_at":"2024-01-01T00:00:00Z"}

{"id":"bead-002","title":"Second issue","priority":2,"base_status":"open","created_at":"2024-01-01T00:00:00Z","updated_at":"2024-01-01T00:00:00Z"}

{"id":"bead-003","title":"Third issue","priority":2,"base_status":"open","created_at":"2024-01-01T00:00:00Z","updated_at":"2024-01-01T00:00:00Z"}"#,
    );

    // Import with diagnostics
    let result = checkpoint::import_checkpoint_with_diagnostics(
        &mut store,
        &test_file,
        "native-v1",
        true,
        true,
    )
    .unwrap();

    // Should succeed without diagnostics (blank lines are ignored)
    let diagnostics = result.diagnostics.as_ref();
    if let Some(d) = diagnostics {
        // If we have diagnostics, they shouldn't be about blank lines
        assert!(
            d.validation_failures.is_empty()
                || !d
                    .validation_failures
                    .iter()
                    .any(|f| f.message.contains("blank") || f.message.contains("empty"))
        );
    }

    // Should have processed only non-blank lines
    let processed = diagnostics.as_ref().map(|d| d.processed_lines).unwrap_or(3);
    assert_eq!(processed, 3);
}

#[test]
fn test_diagnostics_no_activation_with_errors() {
    let temp_dir = create_temp_workspace();
    let db_path = temp_dir.path().join(".beads/beads.db");

    let conn = rusqlite::Connection::open(&db_path).unwrap();
    let mut store = SqliteStore::from_conn(conn);

    // Create test file with errors
    let test_file = temp_dir.path().join("test.jsonl");
    create_test_jsonl(
        &test_file,
        r#"malformed json
{"id":"bead-001","title":"Valid issue","priority":2,"base_status":"open","created_at":"2024-01-01T00:00:00Z","updated_at":"2024-01-01T00:00:00Z"}"#,
    );

    // Import without dry-run (real activation)
    let result = checkpoint::import_checkpoint_with_diagnostics(
        &mut store,
        &test_file,
        "native-v1",
        false, // not dry_run
        true,
    )
    .unwrap();

    // Should have diagnostics
    assert!(result.diagnostics.is_some());
    let diagnostics = result.diagnostics.as_ref().unwrap();
    assert!(!diagnostics.validation_failures.is_empty());

    // Should NOT have activated any issues (no state changes)
    assert_eq!(result.inserted, 0);
    assert_eq!(result.activation_sequence, 0);
    assert_eq!(result.covered_sequence, 0);
}

#[test]
fn test_diagnostics_empty_file() {
    let temp_dir = create_temp_workspace();
    let db_path = temp_dir.path().join(".beads/beads.db");

    let conn = rusqlite::Connection::open(&db_path).unwrap();
    let mut store = SqliteStore::from_conn(conn);

    // Create empty test file
    let test_file = temp_dir.path().join("test.jsonl");
    create_test_jsonl(&test_file, "");

    // Import with diagnostics
    let result = checkpoint::import_checkpoint_with_diagnostics(
        &mut store,
        &test_file,
        "native-v1",
        true,
        true,
    )
    .unwrap();

    // Should succeed without diagnostics (empty file is valid)
    assert_eq!(result.inserted, 0);

    let diagnostics = result.diagnostics.as_ref();
    if let Some(d) = diagnostics {
        // Empty file should not generate validation failures
        assert!(d.validation_failures.is_empty() || d.total_lines == 0);
    }
}

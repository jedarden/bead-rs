//! Integration tests for R018 structured bead data CLI commands
//!
//! This test suite validates the complete CLI integration for structured data operations
//! including set, get, list, and remove commands with proper JSON output and error handling.

use std::process::Command;
use tempfile::TempDir;

/// Helper function to run bead commands in a test workspace
fn run_bead_command(args: &[&str], workspace_dir: &std::path::Path) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_bead"))
        .args(args)
        .current_dir(workspace_dir)
        .output()
        .expect("Failed to execute bead command");

    // Combine stdout and stderr for output
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    format!("{}{}", stdout, stderr)
}

/// Helper function to run command and capture stderr as well
fn run_bead_command_full(args: &[&str], workspace_dir: &std::path::Path) -> (String, String, i32) {
    let output = Command::new(env!("CARGO_BIN_EXE_bead"))
        .args(args)
        .current_dir(workspace_dir)
        .output()
        .expect("Failed to execute bead command");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let exit_code = output.status.code().unwrap_or(1);
    (stdout, stderr, exit_code)
}

#[test]
fn test_data_set_and_get() {
    let temp_dir = TempDir::new().unwrap();
    let workspace_dir = temp_dir.path();

    // Initialize workspace
    run_bead_command(&["init"], workspace_dir);

    // Create a test issue
    let issue_id = run_bead_command(&["create", "--title", "Test issue"], workspace_dir);
    let issue_id = issue_id.trim().to_string();

    // Set structured data
    let stdout = run_bead_command(
        &[
            "data",
            "set",
            "--id",
            &issue_id,
            "--namespace",
            "config",
            "--schema-ref",
            "schema:config-v1",
            "--value",
            "{\"setting\": \"value\", \"enabled\": true}",
        ],
        workspace_dir,
    );

    assert!(stdout.contains("Set structured data:"));
    assert!(stdout.contains("Issue:"));
    assert!(stdout.contains(&issue_id));
    assert!(stdout.contains("Namespace: config"));
    assert!(stdout.contains("Schema: schema:config-v1"));

    // Get the data back
    let stdout = run_bead_command(
        &["data", "get", "--id", &issue_id, "--namespace", "config"],
        workspace_dir,
    );

    assert!(stdout.contains("Structured data:"));
    assert!(stdout.contains("config"));
    assert!(stdout.contains("schema:config-v1"));
    assert!(stdout.contains("\"setting\": \"value\""));
    assert!(stdout.contains("\"enabled\": true"));
}

#[test]
fn test_data_get_json_output() {
    let temp_dir = TempDir::new().unwrap();
    let workspace_dir = temp_dir.path();

    // Initialize workspace
    run_bead_command(&["init"], workspace_dir);

    // Create a test issue
    let issue_id = run_bead_command(&["create", "--title", "Test issue"], workspace_dir);
    let issue_id = issue_id.trim().to_string();

    // Set structured data
    run_bead_command(
        &[
            "data",
            "set",
            "--id",
            &issue_id,
            "--namespace",
            "test",
            "--schema-ref",
            "schema:test",
            "--value",
            "{\"key\": \"value\"}",
        ],
        workspace_dir,
    );

    // Get data in JSON format
    let stdout = run_bead_command(
        &[
            "data",
            "get",
            "--id",
            &issue_id,
            "--namespace",
            "test",
            "--json",
        ],
        workspace_dir,
    );

    // Parse and validate JSON output
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(json["issue_id"], issue_id);
    assert_eq!(json["namespace"], "test");
    assert_eq!(json["schema_ref"], "schema:test");
    assert_eq!(json["value"]["key"], "value");
}

#[test]
fn test_data_list_empty() {
    let temp_dir = TempDir::new().unwrap();
    let workspace_dir = temp_dir.path();

    // Initialize workspace
    run_bead_command(&["init"], workspace_dir);

    // Create a test issue
    let issue_id = run_bead_command(&["create", "--title", "Test issue"], workspace_dir);
    let issue_id = issue_id.trim().to_string();

    // List data (should be empty)
    let stdout = run_bead_command(&["data", "list", "--id", &issue_id], workspace_dir);

    assert!(stdout.contains("No structured data found"));
}

#[test]
fn test_data_list_multiple_namespaces() {
    let temp_dir = TempDir::new().unwrap();
    let workspace_dir = temp_dir.path();

    // Initialize workspace
    run_bead_command(&["init"], workspace_dir);

    // Create a test issue
    let issue_id = run_bead_command(&["create", "--title", "Test issue"], workspace_dir);
    let issue_id = issue_id.trim().to_string();

    // Set multiple data namespaces
    run_bead_command(
        &[
            "data",
            "set",
            "--id",
            &issue_id,
            "--namespace",
            "config",
            "--schema-ref",
            "schema:1",
            "--value",
            "{\"setting\": \"value\"}",
        ],
        workspace_dir,
    );

    run_bead_command(
        &[
            "data",
            "set",
            "--id",
            &issue_id,
            "--namespace",
            "metrics",
            "--schema-ref",
            "schema:2",
            "--value",
            "{\"count\": 100}",
        ],
        workspace_dir,
    );

    run_bead_command(
        &[
            "data",
            "set",
            "--id",
            &issue_id,
            "--namespace",
            "state",
            "--schema-ref",
            "schema:3",
            "--value",
            "{\"active\": true}",
        ],
        workspace_dir,
    );

    // List data
    let stdout = run_bead_command(&["data", "list", "--id", &issue_id], workspace_dir);

    assert!(stdout.contains("config"));
    assert!(stdout.contains("schema:1"));
    assert!(stdout.contains("metrics"));
    assert!(stdout.contains("schema:2"));
    assert!(stdout.contains("state"));
    assert!(stdout.contains("schema:3"));
}

#[test]
fn test_data_list_json_output() {
    let temp_dir = TempDir::new().unwrap();
    let workspace_dir = temp_dir.path();

    // Initialize workspace
    run_bead_command(&["init"], workspace_dir);

    // Create a test issue
    let issue_id = run_bead_command(&["create", "--title", "Test issue"], workspace_dir);
    let issue_id = issue_id.trim().to_string();

    // Set data
    run_bead_command(
        &[
            "data",
            "set",
            "--id",
            &issue_id,
            "--namespace",
            "test",
            "--schema-ref",
            "schema:test",
            "--value",
            "{\"data\": \"value\"}",
        ],
        workspace_dir,
    );

    // List in JSON format
    let stdout = run_bead_command(
        &["data", "list", "--id", &issue_id, "--json"],
        workspace_dir,
    );

    // Parse and validate JSON output
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let array = json.as_array().unwrap();
    assert_eq!(array.len(), 1);

    let entry = &array[0];
    assert_eq!(entry["namespace"], "test");
    assert_eq!(entry["schema_ref"], "schema:test");
}

#[test]
fn test_data_remove() {
    let temp_dir = TempDir::new().unwrap();
    let workspace_dir = temp_dir.path();

    // Initialize workspace
    run_bead_command(&["init"], workspace_dir);

    // Create a test issue
    let issue_id = run_bead_command(&["create", "--title", "Test issue"], workspace_dir);
    let issue_id = issue_id.trim().to_string();

    // Set data
    run_bead_command(
        &[
            "data",
            "set",
            "--id",
            &issue_id,
            "--namespace",
            "temp",
            "--schema-ref",
            "schema:temp",
            "--value",
            "{\"temporary\": true}",
        ],
        workspace_dir,
    );

    // Remove data
    let stdout = run_bead_command(
        &["data", "remove", "--id", &issue_id, "--namespace", "temp"],
        workspace_dir,
    );

    assert!(stdout.contains("Removed structured data:"));
    assert!(stdout.contains("temp"));

    // Verify it's gone
    let stdout = run_bead_command(
        &["data", "get", "--id", &issue_id, "--namespace", "temp"],
        workspace_dir,
    );

    assert!(stdout.contains("No structured data found") || stdout.contains("not found"));
}

#[test]
fn test_data_remove_idempotent() {
    let temp_dir = TempDir::new().unwrap();
    let workspace_dir = temp_dir.path();

    // Initialize workspace
    run_bead_command(&["init"], workspace_dir);

    // Create a test issue
    let issue_id = run_bead_command(&["create", "--title", "Test issue"], workspace_dir);
    let issue_id = issue_id.trim().to_string();

    // Remove non-existent data (should succeed)
    let stdout = run_bead_command(
        &[
            "data",
            "remove",
            "--id",
            &issue_id,
            "--namespace",
            "nonexistent",
        ],
        workspace_dir,
    );

    assert!(stdout.contains("Removed structured data:"));
}

#[test]
fn test_data_set_replaces_existing() {
    let temp_dir = TempDir::new().unwrap();
    let workspace_dir = temp_dir.path();

    // Initialize workspace
    run_bead_command(&["init"], workspace_dir);

    // Create a test issue
    let issue_id = run_bead_command(&["create", "--title", "Test issue"], workspace_dir);
    let issue_id = issue_id.trim().to_string();

    // Set initial data
    run_bead_command(
        &[
            "data",
            "set",
            "--id",
            &issue_id,
            "--namespace",
            "config",
            "--schema-ref",
            "schema:v1",
            "--value",
            "{\"version\": 1}",
        ],
        workspace_dir,
    );

    // Replace with new data
    run_bead_command(
        &[
            "data",
            "set",
            "--id",
            &issue_id,
            "--namespace",
            "config",
            "--schema-ref",
            "schema:v2",
            "--value",
            "{\"version\": 2}",
        ],
        workspace_dir,
    );

    // Get and verify the new value
    let stdout = run_bead_command(
        &[
            "data",
            "get",
            "--id",
            &issue_id,
            "--namespace",
            "config",
            "--json",
        ],
        workspace_dir,
    );

    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["schema_ref"], "schema:v2");
    assert_eq!(json["value"]["version"], 2);
}

#[test]
fn test_data_set_invalid_json() {
    let temp_dir = TempDir::new().unwrap();
    let workspace_dir = temp_dir.path();

    // Initialize workspace
    run_bead_command(&["init"], workspace_dir);

    // Create a test issue
    let issue_id = run_bead_command(&["create", "--title", "Test issue"], workspace_dir);
    let issue_id = issue_id.trim().to_string();

    // Try to set invalid JSON
    let (_stdout, stderr, exit_code) = run_bead_command_full(
        &[
            "data",
            "set",
            "--id",
            &issue_id,
            "--namespace",
            "test",
            "--schema-ref",
            "schema:test",
            "--value",
            "not valid json",
        ],
        workspace_dir,
    );

    // Should fail with validation error
    assert_ne!(exit_code, 0);
    assert!(stderr.contains("Invalid JSON") || stderr.contains("validation"));
}

#[test]
fn test_data_set_nonexistent_issue() {
    let temp_dir = TempDir::new().unwrap();
    let workspace_dir = temp_dir.path();

    // Initialize workspace
    run_bead_command(&["init"], workspace_dir);

    // Try to set data on non-existent issue
    let (_stdout, stderr, exit_code) = run_bead_command_full(
        &[
            "data",
            "set",
            "--id",
            "bead-nonexistent",
            "--namespace",
            "test",
            "--schema-ref",
            "schema:test",
            "--value",
            "{\"test\": true}",
        ],
        workspace_dir,
    );

    // Should fail with not found error
    assert_ne!(exit_code, 0);
    assert!(stderr.contains("not found") || stderr.contains("nonexistent"));
}

#[test]
fn test_data_get_nonexistent_namespace() {
    let temp_dir = TempDir::new().unwrap();
    let workspace_dir = temp_dir.path();

    // Initialize workspace
    run_bead_command(&["init"], workspace_dir);

    // Create a test issue
    let issue_id = run_bead_command(&["create", "--title", "Test issue"], workspace_dir);
    let issue_id = issue_id.trim().to_string();

    // Try to get non-existent namespace
    let (_stdout, stderr, exit_code) = run_bead_command_full(
        &[
            "data",
            "get",
            "--id",
            &issue_id,
            "--namespace",
            "nonexistent",
        ],
        workspace_dir,
    );

    // Should fail with not found error
    assert_ne!(exit_code, 0);
    assert!(stderr.contains("not found") || stderr.contains("No structured data"));
}

#[test]
fn test_data_invalid_namespace() {
    let temp_dir = TempDir::new().unwrap();
    let workspace_dir = temp_dir.path();

    // Initialize workspace
    run_bead_command(&["init"], workspace_dir);

    // Create a test issue
    let issue_id = run_bead_command(&["create", "--title", "Test issue"], workspace_dir);
    let issue_id = issue_id.trim().to_string();

    // Try to use invalid namespace (uppercase)
    let (_stdout, stderr, exit_code) = run_bead_command_full(
        &[
            "data",
            "set",
            "--id",
            &issue_id,
            "--namespace",
            "Invalid",
            "--schema-ref",
            "schema:test",
            "--value",
            "{\"test\": true}",
        ],
        workspace_dir,
    );

    // Should fail with validation error
    assert_ne!(exit_code, 0);
    assert!(stderr.contains("validation") || stderr.contains("Namespace"));
}

#[test]
fn test_data_complex_json_value() {
    let temp_dir = TempDir::new().unwrap();
    let workspace_dir = temp_dir.path();

    // Initialize workspace
    run_bead_command(&["init"], workspace_dir);

    // Create a test issue
    let issue_id = run_bead_command(&["create", "--title", "Test issue"], workspace_dir);
    let issue_id = issue_id.trim().to_string();

    // Set complex JSON value
    let complex_json = r#"{
        "string": "test",
        "number": 42,
        "boolean": true,
        "array": [1, 2, 3],
        "nested": {
            "key": "value"
        }
    }"#;

    run_bead_command(
        &[
            "data",
            "set",
            "--id",
            &issue_id,
            "--namespace",
            "complex",
            "--schema-ref",
            "schema:complex",
            "--value",
            complex_json,
        ],
        workspace_dir,
    );

    // Get and verify
    let stdout = run_bead_command(
        &[
            "data",
            "get",
            "--id",
            &issue_id,
            "--namespace",
            "complex",
            "--json",
        ],
        workspace_dir,
    );

    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["value"]["string"], "test");
    assert_eq!(json["value"]["number"], 42);
    assert_eq!(json["value"]["boolean"], true);
    assert_eq!(json["value"]["array"][0], 1);
    assert_eq!(json["value"]["nested"]["key"], "value");
}

#[test]
fn test_data_mutations_advance_change_feed() {
    let temp_dir = TempDir::new().unwrap();
    let workspace_dir = temp_dir.path();

    // Initialize workspace
    run_bead_command(&["init"], workspace_dir);

    // Create a test issue (the create itself appends one event)
    let issue_id = run_bead_command(&["create", "--title", "Data Issue"], workspace_dir);
    let issue_id = issue_id.trim().to_string();

    let latest_cursor = || -> i64 {
        let stdout = run_bead_command(&["changes", "--latest"], workspace_dir);
        stdout
            .lines()
            .find_map(|l| l.strip_prefix("Latest cursor: "))
            .unwrap()
            .trim()
            .parse()
            .unwrap()
    };

    // Baseline after one create
    let baseline = latest_cursor();

    // data set appends one event: the cursor advances by exactly one
    run_bead_command(
        &[
            "data",
            "set",
            "--id",
            &issue_id,
            "--namespace",
            "config",
            "--schema-ref",
            "schema:v1",
            "--value",
            r#"{"token": "fixture-sup3r-s3cret-value"}"#,
        ],
        workspace_dir,
    );
    assert_eq!(latest_cursor(), baseline + 1);

    // Idempotent identical re-set appends no event
    run_bead_command(
        &[
            "data",
            "set",
            "--id",
            &issue_id,
            "--namespace",
            "config",
            "--schema-ref",
            "schema:v1",
            "--value",
            r#"{"token": "fixture-sup3r-s3cret-value"}"#,
        ],
        workspace_dir,
    );
    assert_eq!(latest_cursor(), baseline + 1);

    // Re-set under a changed schema_ref and value appends one event
    run_bead_command(
        &[
            "data",
            "set",
            "--id",
            &issue_id,
            "--namespace",
            "config",
            "--schema-ref",
            "schema:v2",
            "--value",
            r#"{"token": "rotated-fixture-value"}"#,
        ],
        workspace_dir,
    );
    assert_eq!(latest_cursor(), baseline + 2);

    // data remove appends one event
    run_bead_command(
        &["data", "remove", "--id", &issue_id, "--namespace", "config"],
        workspace_dir,
    );
    assert_eq!(latest_cursor(), baseline + 3);

    // Idempotent no-op re-remove appends no event
    run_bead_command(
        &["data", "remove", "--id", &issue_id, "--namespace", "config"],
        workspace_dir,
    );
    assert_eq!(latest_cursor(), baseline + 3);

    // The recorded events carry namespace and schema_ref in their detail, and
    // never the document body
    let conn = rusqlite::Connection::open(workspace_dir.join(".beads/beads.db")).unwrap();
    let mut stmt = conn
        .prepare("SELECT detail FROM events WHERE kind LIKE 'data_%' ORDER BY sequence")
        .unwrap();
    let details: Vec<String> = stmt
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(details.len(), 3);

    let set: serde_json::Value = serde_json::from_str(&details[0]).unwrap();
    assert_eq!(set["namespace"], "config");
    assert_eq!(set["schema_ref"], "schema:v1");

    let replaced: serde_json::Value = serde_json::from_str(&details[1]).unwrap();
    assert_eq!(replaced["namespace"], "config");
    assert_eq!(replaced["schema_ref"], "schema:v2");

    let removed: serde_json::Value = serde_json::from_str(&details[2]).unwrap();
    assert_eq!(removed["namespace"], "config");
    assert_eq!(removed["schema_ref"], "schema:v2");

    for detail in &details {
        assert!(
            !detail.contains("fixture-sup3r-s3cret-value")
                && !detail.contains("rotated-fixture-value")
                && !detail.contains("token"),
            "event detail must not carry the document body: {detail}"
        );
    }
}

#[test]
fn test_data_help() {
    let temp_dir = TempDir::new().unwrap();
    let workspace_dir = temp_dir.path();

    // `-h` shows each command's summary.
    let summaries = [
        (vec!["data", "-h"], "Manage structured bead data"),
        (vec!["data", "set", "-h"], "Set a structured data value"),
        (vec!["data", "get", "-h"], "Get a structured data value"),
        (
            vec!["data", "list", "-h"],
            "List all structured data namespaces",
        ),
        (
            vec!["data", "remove", "-h"],
            "Remove a structured data value",
        ),
    ];
    for (args, expected) in summaries {
        let stdout = run_bead_command(&args, workspace_dir);
        assert!(
            stdout.contains(expected),
            "`bead {}` is missing its summary {expected:?}",
            args.join(" ")
        );
    }

    // `--help` shows the long description, which must be distinct from the
    // summary -- if the two are identical the long help has been shadowed.
    let long_help = [
        (
            vec!["data", "--help"],
            "Attach schema-governed JSON documents to an issue",
        ),
        (
            vec!["data", "set", "--help"],
            "Set or replace a JSON value for a specific namespace",
        ),
        (
            vec!["data", "get", "--help"],
            "Retrieve the JSON value and schema reference",
        ),
        (
            vec!["data", "list", "--help"],
            "List all namespaces and their schema references",
        ),
        (
            vec!["data", "remove", "--help"],
            "Remove a structured data value from an issue (idempotent)",
        ),
    ];
    for (args, expected) in long_help {
        let stdout = run_bead_command(&args, workspace_dir);
        assert!(
            stdout.contains(expected),
            "`bead {}` is missing its long help {expected:?}",
            args.join(" ")
        );
    }
}

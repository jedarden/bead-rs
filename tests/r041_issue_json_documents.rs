//! R041's issue-facing JSON transports have stable, single-document shapes.

use assert_cmd::Command;
use serde_json::{json, Value};
use std::path::Path;
use std::process::Output;

fn run(workspace: &Path, args: &[&str]) -> Output {
    Command::cargo_bin("bead")
        .unwrap()
        .current_dir(workspace)
        .args(args)
        .output()
        .unwrap()
}

fn workspace() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    let output = run(
        temp.path(),
        &["init", "--skip-foreign-workspace", "--prefix", "r041"],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    temp
}

fn json_output(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn list_json_is_one_array_for_empty_one_and_many_results() {
    let workspace = workspace();

    let empty = run(workspace.path(), &["list", "--json"]);
    assert!(empty.status.success());
    assert_eq!(empty.stdout, b"[]\n");
    assert_eq!(
        serde_json::from_slice::<Value>(&empty.stdout).unwrap(),
        json!([])
    );

    let empty_jsonl = run(workspace.path(), &["list", "--jsonl"]);
    assert!(empty_jsonl.status.success());
    assert!(empty_jsonl.stdout.is_empty());

    let first = json_output(run(
        workspace.path(),
        &[
            "create",
            "--title",
            "First issue",
            "--description",
            "Created as a JSON document",
            "--label",
            "r041",
            "--json",
        ],
    ));
    assert!(first.is_object());

    let one = json_output(run(workspace.path(), &["list", "--json"]));
    assert_eq!(one.as_array().unwrap().len(), 1);
    assert_eq!(one[0], first);

    let show = json_output(run(
        workspace.path(),
        &["show", first["id"].as_str().unwrap(), "--json"],
    ));
    assert!(show.is_object());
    assert_eq!(show, first);

    let plain = run(workspace.path(), &["create", "--title", "Plain ID output"]);
    assert!(plain.status.success());
    let plain_id = String::from_utf8(plain.stdout).unwrap();
    assert!(plain_id.ends_with('\n'));
    assert!(!plain_id.trim_end().contains(char::is_whitespace));

    let many = json_output(run(workspace.path(), &["list", "--json"]));
    assert_eq!(many.as_array().unwrap().len(), 2);

    let jsonl = run(workspace.path(), &["list", "--jsonl"]);
    assert!(jsonl.status.success());
    let lines = String::from_utf8(jsonl.stdout).unwrap();
    let documents = lines
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(documents.len(), 2);
    assert!(documents.iter().all(Value::is_object));
}

#[test]
fn list_json_and_jsonl_conflict_with_usage_exit_code() {
    let workspace = workspace();
    let output = run(workspace.path(), &["list", "--json", "--jsonl"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("--jsonl cannot be combined with --json")
    );
    assert!(output.stdout.is_empty());
}

#[test]
fn capabilities_advertise_issue_document_shapes() {
    let temp = tempfile::tempdir().unwrap();
    let capabilities = json_output(run(temp.path(), &["capabilities"]));
    assert_eq!(capabilities["agent_ergonomics"]["version"], 1);
    assert_eq!(
        capabilities["agent_ergonomics"]["issue_documents"],
        json!({
            "list_json": "array",
            "list_jsonl": "object-per-line",
            "show_json": "object",
            "create_json": "object",
            "history_json": "array",
            "dependency_list_json": "array",
            "count_json": "object"
        })
    );
    assert_eq!(
        capabilities["agent_ergonomics"]["json_selectors"],
        json!(["--json", "--format json"])
    );
    assert_eq!(
        capabilities["agent_ergonomics"]["claim_credential_environment"],
        json!(["NEEDLE_BEAD_ID", "NEEDLE_BEAD_FENCING_TOKEN"])
    );
    assert_eq!(
        capabilities["agent_ergonomics"]["text_sources"],
        json!(["inline", "file", "stdin"])
    );
    assert_eq!(
        capabilities["agent_ergonomics"]["shortcuts"],
        json!([
            "ready",
            "search",
            "list-all",
            "list-search",
            "list-count",
            "dependency-list",
            "bare-id",
            "history",
            "positional-labels"
        ])
    );
}

#[test]
fn output_selector_aliases_are_byte_identical_and_query_keeps_its_json_input() {
    let workspace = workspace();
    let created = run(
        workspace.path(),
        &["create", "--title", "Selector target", "--json"],
    );
    let created = json_output(created);
    let id = created["id"].as_str().unwrap();

    let list_json = run(workspace.path(), &["list", "--json"]);
    let list_format = run(workspace.path(), &["list", "--format", "json"]);
    assert!(list_json.status.success());
    assert!(list_format.status.success());
    assert_eq!(list_json.stdout, list_format.stdout);

    let show_json = run(workspace.path(), &["show", id, "--json"]);
    let show_format = run(workspace.path(), &["show", id, "--format=json"]);
    assert!(show_json.status.success());
    assert!(show_format.status.success());
    assert_eq!(show_json.stdout, show_format.stdout);

    let status_json = run(workspace.path(), &["sync", "status", "--format", "json"]);
    let status_alias = run(workspace.path(), &["sync", "status", "--json"]);
    assert!(status_json.status.success());
    assert!(status_alias.status.success());
    assert_eq!(status_json.stdout, status_alias.stdout);

    let capabilities_json = run(workspace.path(), &["capabilities", "--format", "json"]);
    let capabilities_alias = run(workspace.path(), &["capabilities", "--json"]);
    assert!(capabilities_json.status.success());
    assert!(capabilities_alias.status.success());
    assert_eq!(capabilities_json.stdout, capabilities_alias.stdout);

    let create_alias = json_output(run(
        workspace.path(),
        &["create", "--title", "Create alias", "--format", "json"],
    ));
    assert!(create_alias.is_object());

    let resource_add_json = run(
        workspace.path(),
        &["resource", "add", id, "--key", "selector:test", "--json"],
    );
    let resource_add_format = run(
        workspace.path(),
        &[
            "resource",
            "add",
            id,
            "--key",
            "selector:test",
            "--format",
            "json",
        ],
    );
    assert!(resource_add_json.status.success());
    assert!(resource_add_format.status.success());
    assert_eq!(resource_add_json.stdout, resource_add_format.stdout);

    let query = r#"{"version":"v1","predicates":[],"sort":[]}"#;
    let query_result = run(
        workspace.path(),
        &["query", "--json", query, "--format", "json"],
    );
    assert!(query_result.status.success());
    assert!(serde_json::from_slice::<Value>(&query_result.stdout).is_ok());
}

#[test]
fn unsupported_and_conflicting_output_selectors_have_stable_usage_errors() {
    let workspace = workspace();

    let unsupported = run(workspace.path(), &["list", "--format", "yaml"]);
    assert_eq!(unsupported.status.code(), Some(2));
    assert_eq!(
        String::from_utf8(unsupported.stderr).unwrap().trim(),
        "bead: CLI usage error: unsupported output format 'yaml'; use --format json"
    );

    let unsupported_jsonl = run(workspace.path(), &["list", "--format", "jsonl"]);
    assert_eq!(unsupported_jsonl.status.code(), Some(2));
    assert_eq!(
        String::from_utf8(unsupported_jsonl.stderr).unwrap().trim(),
        "bead: CLI usage error: unsupported output format 'jsonl'; use --format json"
    );

    let conflict = run(workspace.path(), &["list", "--json", "--format", "text"]);
    assert_eq!(conflict.status.code(), Some(2));
    assert_eq!(
        String::from_utf8(conflict.stderr).unwrap().trim(),
        "bead: CLI usage error: conflicting output format selectors"
    );

    let jsonl_conflict = run(workspace.path(), &["list", "--jsonl", "--format", "json"]);
    assert_eq!(jsonl_conflict.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&jsonl_conflict.stderr)
        .contains("--jsonl cannot be combined with --json"));
}

#[test]
fn help_describes_the_issue_json_shapes() {
    let temp = tempfile::tempdir().unwrap();
    let root = run(temp.path(), &["--help"]);
    let root = String::from_utf8(root.stdout).unwrap();
    assert!(root.contains("--json and --format json"));
    assert!(root.contains("query --json DOCUMENT"));

    let list = run(temp.path(), &["list", "--help"]);
    let list = String::from_utf8(list.stdout).unwrap();
    assert!(list.contains("one compact JSON array"));
    assert!(list.contains("--jsonl"));

    let show = run(temp.path(), &["show", "--help"]);
    let show = String::from_utf8(show.stdout).unwrap();
    assert!(show.contains("One JSON object for the issue"));
    assert!(!show.contains("one-element JSON array"));

    let create = run(temp.path(), &["create", "--help"]);
    let create = String::from_utf8(create.stdout).unwrap();
    assert!(create.contains("--json"));
    assert!(create.contains("created issue document"));
}

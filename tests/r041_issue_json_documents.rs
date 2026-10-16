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
}

#[test]
fn help_describes_the_issue_json_shapes() {
    let temp = tempfile::tempdir().unwrap();
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

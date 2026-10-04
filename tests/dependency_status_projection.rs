//! Regression coverage for resolved blocker status in issue JSON projections.

use std::path::Path;
use std::process::{Command, Output};

fn run(workspace: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_bead"))
        .current_dir(workspace)
        .arg("--skip-foreign-workspace")
        .args(args)
        .output()
        .expect("bead command should start")
}

fn setup() -> tempfile::TempDir {
    let workspace = tempfile::tempdir().unwrap();
    let output = run(workspace.path(), &["init", "--prefix", "dsp"]);
    assert!(output.status.success(), "init failed: {output:?}");
    workspace
}

fn create(workspace: &Path, title: &str) -> String {
    let output = run(
        workspace,
        &["create", "--title", title, "--issue-type", "task"],
    );
    assert!(output.status.success(), "create failed: {output:?}");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn show_record(workspace: &Path, id: &str) -> serde_json::Value {
    let output = run(workspace, &["show", id, "--json"]);
    assert!(output.status.success(), "show --json failed: {output:?}");
    let records: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    records[0].clone()
}

fn first_dependency(record: &serde_json::Value) -> &serde_json::Value {
    record["dependencies"]
        .as_array()
        .and_then(|dependencies| dependencies.first())
        .expect("record should contain a dependency")
}

#[test]
fn show_and_list_json_include_resolved_blocker_status() {
    let workspace = setup();
    let blocked = create(workspace.path(), "blocked");
    let blocker = create(workspace.path(), "blocker");

    let output = run(workspace.path(), &["dep", "add", &blocked, &blocker]);
    assert!(output.status.success(), "dep add failed: {output:?}");

    let initial_record = show_record(workspace.path(), &blocked);
    let show_edge = first_dependency(&initial_record);
    assert_eq!(show_edge["blocker"], blocker);
    assert_eq!(show_edge["kind"], "blocks");
    assert_eq!(show_edge["blocker_status"], "open");
    assert_eq!(show_edge["finished"], false);

    let output = run(workspace.path(), &["list", "--json", "--limit", "999999"]);
    assert!(output.status.success(), "list --json failed: {output:?}");
    let record = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find(|record| record["id"] == blocked)
        .expect("list should include the blocked issue");
    let list_edge = first_dependency(&record);
    assert_eq!(list_edge["blocker_status"], "open");
    assert_eq!(list_edge["finished"], false);

    let output = run(
        workspace.path(),
        &[
            "close",
            &blocker,
            "--reason",
            "completed for projection test",
        ],
    );
    assert!(output.status.success(), "close failed: {output:?}");

    let closed_record = show_record(workspace.path(), &blocked);
    let closed_edge = first_dependency(&closed_record);
    assert_eq!(closed_edge["blocker_status"], "closed");
    assert_eq!(closed_edge["finished"], true);
}

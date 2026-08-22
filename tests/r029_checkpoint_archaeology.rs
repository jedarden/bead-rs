//! R029 conformance: verified read-only checkpoint archaeology.
//!
//! The historical view is selected from retained pointers or the pointer-
//! selected root/manifest, uses the restore verifier before serving data, and
//! is permanently non-importable.

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn bead(dir: &Path) -> Command {
    let mut command = Command::cargo_bin("bead").unwrap();
    command.current_dir(dir);
    command.arg("--skip-foreign-workspace");
    command
}

fn run(dir: &Path, args: &[&str]) -> std::process::Output {
    bead(dir).args(args).assert().success().get_output().clone()
}

fn create_issue(dir: &Path, title: &str) -> String {
    String::from_utf8(run(dir, &["create", "--title", title]).stdout)
        .unwrap()
        .trim()
        .to_string()
}

fn suppress_auto_flush(workspace: &Path) {
    let config_path = workspace.join(".beads/config.json");
    let mut config: Value = serde_json::from_slice(&fs::read(&config_path).unwrap()).unwrap();
    config["checkpoint"] = serde_json::json!({ "auto_flush": false });
    fs::write(&config_path, serde_json::to_vec_pretty(&config).unwrap()).unwrap();
}

struct Source {
    _dir: TempDir,
    workspace: PathBuf,
    checkpoint: PathBuf,
    previous: Value,
    current: Value,
    first_id: String,
    later_id: String,
}

fn historical_source() -> Source {
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path().to_path_buf();
    run(&workspace, &["init", "--prefix", "history"]);
    suppress_auto_flush(&workspace);

    let first_id = create_issue(&workspace, "first historical title");
    run(&workspace, &["sync", "flush-only"]);

    run(
        &workspace,
        &["update", &first_id, "--notes", "revised for archaeology"],
    );
    let later_id = create_issue(&workspace, "later historical title");
    run(&workspace, &["sync", "flush-only"]);

    let checkpoint = workspace.join(".beads/checkpoint");
    let previous: Value =
        serde_json::from_slice(&fs::read(checkpoint.join("previous.json")).unwrap()).unwrap();
    let current: Value =
        serde_json::from_slice(&fs::read(checkpoint.join("current.json")).unwrap()).unwrap();
    assert_ne!(previous["generation_id"], current["generation_id"]);

    Source {
        _dir: dir,
        workspace,
        checkpoint,
        previous,
        current,
        first_id,
        later_id,
    }
}

fn title_query(title: &str) -> String {
    serde_json::json!({
        "version": "v1",
        "predicates": [{
            "field": "title",
            "operator": "contains",
            "value": title
        }],
        "sort": []
    })
    .to_string()
}

#[test]
fn query_uses_retained_pointer_read_only_without_a_workspace() {
    let source = historical_source();
    let outside = tempfile::tempdir().unwrap();
    let database_before = fs::read(source.workspace.join(".beads/beads.db")).unwrap();
    let previous = source.checkpoint.join("previous.json");
    let query = title_query("first");

    let output = run(
        outside.path(),
        &[
            "query",
            "--checkpoint",
            previous.to_str().unwrap(),
            "--json",
            &query,
        ],
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();

    assert_eq!(
        report["artifact_kind"],
        "bead-rs-checkpoint-archaeology-view-v1"
    );
    assert_eq!(report["importable"], false);
    assert_eq!(
        report["generation"]["generation_id"],
        source.previous["generation_id"]
    );
    assert_eq!(report["results"].as_array().unwrap().len(), 1);
    assert_eq!(report["results"][0]["id"], source.first_id);
    assert_eq!(report["results"][0]["title"], "first historical title");
    assert_eq!(
        fs::read(source.workspace.join(".beads/beads.db")).unwrap(),
        database_before
    );
}

#[test]
fn pointer_selected_monolith_diff_and_series_search_are_semantic() {
    let source = historical_source();
    assert_eq!(source.previous["mode"], "monolithic");
    let outside = tempfile::tempdir().unwrap();
    let monolith = source
        .checkpoint
        .join(source.previous["active_root"]["path"].as_str().unwrap());
    let current = source.checkpoint.join("current.json");
    let query = title_query("later");

    // A direct root is accepted only because previous.json selects it.
    let direct_report: Value = serde_json::from_slice(
        &run(
            outside.path(),
            &[
                "query",
                "--checkpoint",
                monolith.to_str().unwrap(),
                "--json",
                &title_query("first"),
            ],
        )
        .stdout,
    )
    .unwrap();
    assert_eq!(
        direct_report["generation"]["generation_id"],
        source.previous["generation_id"]
    );

    let diff: Value = serde_json::from_slice(
        &run(
            outside.path(),
            &[
                "sync",
                "diff",
                monolith.to_str().unwrap(),
                current.to_str().unwrap(),
            ],
        )
        .stdout,
    )
    .unwrap();
    assert_eq!(diff["importable"], false);
    assert!(diff["issue_deltas"]
        .as_array()
        .unwrap()
        .iter()
        .any(|delta| delta["identity"] == source.first_id && delta["change"] == "changed"));
    assert!(diff["issue_deltas"]
        .as_array()
        .unwrap()
        .iter()
        .any(|delta| delta["identity"] == source.later_id && delta["change"] == "added"));
    assert!(!diff["event_deltas"].as_array().unwrap().is_empty());

    let bisect: Value = serde_json::from_slice(
        &run(
            outside.path(),
            &[
                "sync",
                "bisect",
                "--checkpoint",
                monolith.to_str().unwrap(),
                "--checkpoint",
                current.to_str().unwrap(),
                "--query",
                &query,
            ],
        )
        .stdout,
    )
    .unwrap();
    assert_eq!(bisect["importable"], false);
    assert_eq!(bisect["matches"].as_array().unwrap().len(), 1);
    assert_eq!(
        bisect["matches"][0]["generation"]["generation_id"],
        source.current["generation_id"]
    );
    assert_eq!(
        bisect["matches"][0]["matching_issue_ids"],
        serde_json::json!([source.later_id])
    );
}

#[test]
fn pointer_selected_manifest_is_queryable() {
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path();
    run(workspace, &["init", "--prefix", "sharded"]);
    suppress_auto_flush(workspace);
    let config_path = workspace.join(".beads/config.json");
    let mut config: Value = serde_json::from_slice(&fs::read(&config_path).unwrap()).unwrap();
    config["checkpoint"]["mode"] = Value::String("sharded".to_string());
    fs::write(&config_path, serde_json::to_vec_pretty(&config).unwrap()).unwrap();
    let issue_id = create_issue(workspace, "sharded archaeology issue");
    run(workspace, &["sync", "flush-only"]);

    let checkpoint = workspace.join(".beads/checkpoint");
    let pointer: Value =
        serde_json::from_slice(&fs::read(checkpoint.join("current.json")).unwrap()).unwrap();
    assert_eq!(pointer["mode"], "sharded");
    let manifest = checkpoint.join(pointer["active_root"]["path"].as_str().unwrap());
    let outside = tempfile::tempdir().unwrap();
    let report: Value = serde_json::from_slice(
        &run(
            outside.path(),
            &[
                "query",
                "--checkpoint",
                manifest.to_str().unwrap(),
                "--json",
                &title_query("sharded"),
            ],
        )
        .stdout,
    )
    .unwrap();
    assert_eq!(report["results"][0]["id"], issue_id);
}

#[test]
fn a_tampered_root_is_refused_before_an_archaeology_view_is_served() {
    let source = historical_source();
    let root = source
        .checkpoint
        .join(source.previous["active_root"]["path"].as_str().unwrap());
    fs::write(&root, "{}\n").unwrap();
    let outside = tempfile::tempdir().unwrap();

    bead(outside.path())
        .args([
            "query",
            "--checkpoint",
            root.to_str().unwrap(),
            "--json",
            &title_query("first"),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Unverified restore source"))
        .stderr(predicate::str::contains("hash mismatch"));
}

#[test]
fn archaeology_output_is_rejected_by_import_only() {
    let source = historical_source();
    let outside = tempfile::tempdir().unwrap();
    let output = run(
        outside.path(),
        &[
            "query",
            "--checkpoint",
            source.checkpoint.join("previous.json").to_str().unwrap(),
            "--json",
            &title_query("first"),
        ],
    );
    let view = outside.path().join("historical-view.json");
    fs::write(&view, output.stdout).unwrap();

    let target = tempfile::tempdir().unwrap();
    run(target.path(), &["init", "--prefix", "target"]);
    bead(target.path())
        .args([
            "sync",
            "import-only",
            "--input",
            view.to_str().unwrap(),
            "--merge",
            "--actor",
            "archaeologist",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("R029 checkpoint archaeology view"))
        .stderr(predicate::str::contains("explicitly non-importable"));

    bead(target.path())
        .args([
            "sync",
            "import-only",
            "--input",
            view.to_str().unwrap(),
            "--restore-into-empty",
            "--actor",
            "archaeologist",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("R029 checkpoint archaeology view"));

    bead(target.path())
        .args([
            "sync",
            "import-only",
            "--input",
            view.to_str().unwrap(),
            "--diagnostics",
            "--dry-run",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("R029 checkpoint archaeology view"));
}

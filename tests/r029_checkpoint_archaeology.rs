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

fn copy_checkpoint_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_checkpoint_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

struct Source {
    _dir: TempDir,
    _frozen_dir: TempDir,
    workspace: PathBuf,
    checkpoint: PathBuf,
    /// Immutable copy of the checkpoint set as of the first generation —
    /// the shape a git commit retains under R026, and the same generation
    /// the live workspace's `previous.json` selects.
    frozen_first: PathBuf,
    previous: Value,
    current: Value,
    first_id: String,
    later_id: String,
}

fn historical_source() -> Source {
    historical_source_in_mode(None)
}

/// Build a two-generation historical fixture. `mode` pins the workspace's
/// checkpoint mode before the first publication; `None` keeps the default,
/// which publishes `set-v2` for every fresh workspace (5d4c08a6).
fn historical_source_in_mode(mode: Option<&str>) -> Source {
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path().to_path_buf();
    run(&workspace, &["init", "--prefix", "history"]);
    suppress_auto_flush(&workspace);
    if let Some(mode) = mode {
        let config_path = workspace.join(".beads/config.json");
        let mut config: Value = serde_json::from_slice(&fs::read(&config_path).unwrap()).unwrap();
        config["checkpoint"]["mode"] = Value::String(mode.to_string());
        fs::write(&config_path, serde_json::to_vec_pretty(&config).unwrap()).unwrap();
    }

    let first_id = create_issue(&workspace, "first historical title");
    run(&workspace, &["sync", "flush-only"]);

    // Freeze the first generation as an immutable snapshot. Under set-v2 a
    // later publication appends to the same open tail, so the live
    // checkpoint directory stops verifying the retained generation (a
    // verifier defect tracked separately); the frozen copy is the shape
    // archaeology actually consumes — a committed generation.
    let frozen_dir = tempfile::tempdir().unwrap();
    let frozen_first = frozen_dir.path().join("checkpoint");
    copy_checkpoint_tree(&workspace.join(".beads/checkpoint"), &frozen_first);

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
    match mode {
        None => assert_eq!(
            current["mode"], "set-v2",
            "a fresh workspace must publish set-v2 by default"
        ),
        Some(mode) => {
            assert_eq!(current["mode"], mode);
            assert_eq!(previous["mode"], mode);
        }
    }

    Source {
        _dir: dir,
        _frozen_dir: frozen_dir,
        workspace,
        checkpoint,
        frozen_first,
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

fn base_status_query(status: &str) -> String {
    serde_json::json!({
        "version": "v1",
        "predicates": [{
            "field": "base_status",
            "operator": "equals",
            "value": status
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
    let first_generation = source.frozen_first.join("current.json");
    let query = title_query("first");

    let output = run(
        outside.path(),
        &[
            "query",
            "--checkpoint",
            first_generation.to_str().unwrap(),
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
    assert_eq!(report["generation"]["mode"], "set-v2");
    assert_eq!(
        report["generation"]["generation_id"], source.previous["generation_id"],
        "the frozen snapshot must be the generation previous.json retains"
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
    let source = historical_source_in_mode(Some("monolithic"));
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
fn pointer_selected_set_v2_manifest_is_queryable() {
    // The sharded-v1 writer is no longer selectable (5d4c08a6: forced
    // sharded resolves to set-v2), so the pointer-selected-manifest case
    // is exercised against the default set-v2 layout's `set/` manifest.
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path();
    run(workspace, &["init", "--prefix", "setv2"]);
    suppress_auto_flush(workspace);
    let issue_id = create_issue(workspace, "set-v2 archaeology issue");
    run(workspace, &["sync", "flush-only"]);

    let checkpoint = workspace.join(".beads/checkpoint");
    let pointer: Value =
        serde_json::from_slice(&fs::read(checkpoint.join("current.json")).unwrap()).unwrap();
    assert_eq!(pointer["mode"], "set-v2");
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
                &title_query("set-v2"),
            ],
        )
        .stdout,
    )
    .unwrap();
    assert_eq!(report["results"][0]["id"], issue_id);
    assert_eq!(report["generation"]["mode"], "set-v2");
}

#[test]
fn set_v2_pointer_bisect_queries_the_default_workspace_layout() {
    // beadrs-745ef223: every fresh workspace publishes set-v2, so bisect --
    // a read-only archaeology query -- must answer the ordinary query
    // grammar against current.json instead of failing on the layout.
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path();
    run(workspace, &["init", "--prefix", "bisect"]);
    let issue_id = create_issue(workspace, "bisect archaeology target");

    let checkpoint = workspace.join(".beads/checkpoint");
    let current: Value =
        serde_json::from_slice(&fs::read(checkpoint.join("current.json")).unwrap()).unwrap();
    assert_eq!(current["mode"], "set-v2");

    let outside = tempfile::tempdir().unwrap();
    let bisect: Value = serde_json::from_slice(
        &run(
            outside.path(),
            &[
                "sync",
                "bisect",
                "--checkpoint",
                checkpoint.join("current.json").to_str().unwrap(),
                "--query",
                &base_status_query("open"),
            ],
        )
        .stdout,
    )
    .unwrap();
    assert_eq!(bisect["importable"], false);
    let matches = bisect["matches"].as_array().unwrap();
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0]["generation"]["mode"], "set-v2");
    assert_eq!(
        matches[0]["generation"]["generation_id"],
        current["generation_id"]
    );
    assert_eq!(
        matches[0]["matching_issue_ids"],
        serde_json::json!([issue_id])
    );
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
            source.frozen_first.join("current.json").to_str().unwrap(),
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

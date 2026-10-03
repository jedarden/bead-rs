//! `bead sync configure`: operator control of the checkpoint section of
//! `.beads/config.json` (plan 6.1.1 -- operators may force monolithic or
//! sharded output; thresholds are versioned configuration).
//!
//! These tests verify that the command:
//! - switches a workspace to sharded output with every object under the
//!   requested byte bound, and later mutations stay sharded
//! - keeps every unrelated key of `.beads/config.json`
//! - writes nothing on a dry run, on invalid input, or when nothing changes
//! - restores the previous configuration when publication under the new one
//!   fails

use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

fn run_bead(workspace: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_bead"))
        .args(args)
        .current_dir(workspace)
        .output()
        .expect("failed to run bead")
}

fn run_ok(workspace: &Path, args: &[&str]) -> Output {
    let output = run_bead(workspace, args);
    assert!(
        output.status.success(),
        "`bead {}` failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn init_workspace(dir: &Path) {
    fs::create_dir_all(dir).unwrap();
    run_ok(dir, &["init", "--skip-foreign-workspace"]);
}

fn create_issues(workspace: &Path, count: usize) {
    for i in 1..=count {
        run_ok(
            workspace,
            &[
                "create",
                "--title",
                &format!("Issue {i}"),
                "--description",
                &"x".repeat(2_000),
            ],
        );
    }
}

fn read_json(path: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

fn pointer(workspace: &Path) -> Value {
    read_json(&workspace.join(".beads/checkpoint/current.json"))
}

fn config_bytes(workspace: &Path) -> Vec<u8> {
    fs::read(workspace.join(".beads/config.json")).unwrap()
}

fn configure_json(workspace: &Path, args: &[&str]) -> Value {
    let mut full = vec!["sync", "configure", "--json"];
    full.extend_from_slice(args);
    let output = run_ok(workspace, &full);
    serde_json::from_slice(&output.stdout).expect("configure --json prints JSON")
}

/// Largest generation object the active pointer can reach, in bytes
fn largest_object_bytes(workspace: &Path) -> u64 {
    let checkpoint = workspace.join(".beads/checkpoint");
    ["objects", "manifests"]
        .iter()
        .filter_map(|dir| fs::read_dir(checkpoint.join(dir)).ok())
        .flatten()
        .map(|entry| entry.unwrap().metadata().unwrap().len())
        .max()
        .unwrap_or(0)
}

#[test]
fn sharded_switch_bounds_objects_and_preserves_unrelated_keys() {
    let temp = tempfile::tempdir().unwrap();
    let ws = temp.path().join("ws");
    init_workspace(&ws);
    create_issues(&ws, 40);
    assert_eq!(pointer(&ws)["mode"], "monolithic");
    let identity_before = read_json(&ws.join(".beads/config.json"));

    let limit: u64 = 1024 * 1024;
    let result = configure_json(
        &ws,
        &[
            "--mode",
            "sharded",
            "--max-object-bytes",
            &limit.to_string(),
        ],
    );
    assert_eq!(result["changed"], true);
    assert_eq!(result["published"]["mode"], "sharded");
    assert_eq!(pointer(&ws)["mode"], "sharded");

    let config = read_json(&ws.join(".beads/config.json"));
    for key in identity_before.as_object().unwrap().keys() {
        assert_eq!(
            config[key], identity_before[key],
            "configure must keep unrelated key {key}"
        );
    }
    assert_eq!(config["checkpoint"]["mode"], "sharded");
    let thresholds = &config["checkpoint"]["thresholds"];
    assert_eq!(thresholds["max_shard_bytes"], json!(limit));
    assert_eq!(thresholds["max_event_object_bytes"], json!(limit));
    assert!(thresholds["max_record_line_bytes"].as_u64().unwrap() <= limit);
    assert!(largest_object_bytes(&ws) <= limit);

    // The checkpoint is consistent under the new configuration.
    let status = run_ok(&ws, &["sync", "status", "--format", "json"]);
    let status: Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(status["dirty"], false, "status after configure: {status}");

    // An ordinary mutation keeps publishing sharded.
    create_issues(&ws, 1);
    assert_eq!(pointer(&ws)["mode"], "sharded");
}

#[test]
fn dry_run_and_invalid_input_write_nothing() {
    let temp = tempfile::tempdir().unwrap();
    let ws = temp.path().join("ws");
    init_workspace(&ws);
    create_issues(&ws, 3);
    let config_before = config_bytes(&ws);
    let pointer_before = pointer(&ws);

    let result = configure_json(&ws, &["--mode", "sharded", "--dry-run"]);
    assert_eq!(result["dry_run"], true);
    assert_eq!(result["changed"], true);
    assert_eq!(result["checkpoint_after"]["mode"], "sharded");
    assert!(result["published"].is_null());

    for args in [
        vec!["sync", "configure"],
        vec!["sync", "configure", "--mode", "striped"],
        vec!["sync", "configure", "--max-object-bytes", "1000"],
    ] {
        let output = run_bead(&ws, &args);
        assert_eq!(
            output.status.code(),
            Some(2),
            "`bead {}` must be a validation error: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    assert_eq!(config_bytes(&ws), config_before);
    assert_eq!(pointer(&ws), pointer_before);
}

#[test]
fn unchanged_request_publishes_nothing_and_adaptive_returns_to_monolith() {
    let temp = tempfile::tempdir().unwrap();
    let ws = temp.path().join("ws");
    init_workspace(&ws);
    create_issues(&ws, 3);

    configure_json(&ws, &["--mode", "sharded"]);
    let generation = pointer(&ws)["generation_id"].clone();

    let again = configure_json(&ws, &["--mode", "sharded"]);
    assert_eq!(again["changed"], false);
    assert!(again["published"].is_null());
    assert_eq!(pointer(&ws)["generation_id"], generation);

    // A small workspace selects the monolith again once the mode is adaptive.
    let adaptive = configure_json(&ws, &["--mode", "adaptive"]);
    assert_eq!(adaptive["published"]["mode"], "monolithic");
    assert_eq!(pointer(&ws)["mode"], "monolithic");
}

#[test]
fn failed_publication_restores_the_previous_configuration() {
    let temp = tempfile::tempdir().unwrap();
    let ws = temp.path().join("ws");
    init_workspace(&ws);
    create_issues(&ws, 5);

    // A tiny monolith limit makes a forced monolith unpublishable.
    configure_json(&ws, &["--mode", "sharded"]);
    let config_path = ws.join(".beads/config.json");
    let mut config = read_json(&config_path);
    config["checkpoint"]["thresholds"] = json!({ "max_monolith_total_bytes": 1024 });
    fs::write(&config_path, serde_json::to_vec(&config).unwrap()).unwrap();
    let config_before = config_bytes(&ws);
    let pointer_before = pointer(&ws);

    let output = run_bead(&ws, &["sync", "configure", "--mode", "monolithic"]);
    assert!(
        !output.status.success(),
        "a forced oversize monolith must fail"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("restored"),
        "the error must say the configuration was restored: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(config_bytes(&ws), config_before);
    assert_eq!(pointer(&ws), pointer_before);
}

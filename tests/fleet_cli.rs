use assert_cmd::Command;
use serde_json::Value;
use std::fs;

#[test]
fn fleet_status_from_a_workspace_uses_the_requested_root() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("project");
    fs::create_dir(&workspace).unwrap();
    Command::cargo_bin("bead")
        .unwrap()
        .current_dir(&workspace)
        .args(["init", "--prefix", "fleettest", "--skip-foreign-workspace"])
        .assert()
        .success();

    let output = Command::cargo_bin("bead")
        .unwrap()
        .current_dir(&workspace)
        .args(["fleet", "status", "--root"])
        .arg(temp.path())
        .args(["--max-depth", "1", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let document: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(document["action"], "status");
    assert_eq!(document["totals"]["canonical_workspaces"], 1);
    assert_eq!(
        document["workspaces"][0]["path"],
        workspace.to_str().unwrap()
    );
    assert!(document["errors"].as_array().unwrap().is_empty());
}

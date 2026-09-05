//! End-to-end contract tests for claim-epoch issuance and visibility.

use assert_cmd::Command;
use serde_json::Value;
use std::path::Path;

fn run(workspace: &Path, args: &[&str]) -> std::process::Output {
    let output = Command::cargo_bin("bead")
        .unwrap()
        .current_dir(workspace)
        .arg("--skip-foreign-workspace")
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "bead {args:?} failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    output
}

fn claim(workspace: &Path, assignee: &str, leased: bool) -> Value {
    let mut args = vec!["claim", "--assignee", assignee, "--json"];
    if leased {
        args.extend(["--lease-ttl", "300"]);
    }
    serde_json::from_slice(&run(workspace, &args).stdout).unwrap()
}

fn shown_issue(workspace: &Path, id: &str) -> Value {
    let shown: Value =
        serde_json::from_slice(&run(workspace, &["show", id, "--json"]).stdout).unwrap();
    shown.as_array().unwrap()[0].clone()
}

fn checkpoint_issue(workspace: &Path, id: &str) -> Value {
    let forensic = std::fs::read_to_string(
        workspace
            .join(".beads")
            .join("checkpoint")
            .join("forensic.jsonl"),
    )
    .unwrap();
    forensic
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .find_map(|record| {
            (record["record_type"] == "issue" && record["issue"]["id"] == id)
                .then(|| record["issue"].clone())
        })
        .expect("claimed issue in published checkpoint")
}

#[test]
fn every_claim_mints_a_visible_monotonic_epoch_that_survives_rebuild() {
    let workspace = tempfile::tempdir().unwrap();
    run(workspace.path(), &["init", "--prefix", "epoch"]);
    let id = String::from_utf8(
        run(
            workspace.path(),
            &["create", "--title", "claim epoch target"],
        )
        .stdout,
    )
    .unwrap()
    .trim()
    .to_string();

    let first = claim(workspace.path(), "worker-one", false);
    let first_epoch = first["claim_epoch"].as_i64().expect("plain claim epoch");
    assert!(first_epoch > 0);
    assert!(first["lease"].is_null());
    assert_eq!(
        shown_issue(workspace.path(), &id)["claim_epoch"],
        first_epoch
    );
    assert_eq!(
        checkpoint_issue(workspace.path(), &id)["claim_epoch"],
        first_epoch
    );

    // Issuance/visibility is intentionally backward-compatible in this
    // transition child: requiring the credential lands in the next child.
    run(workspace.path(), &["release", &id]);
    let second = claim(workspace.path(), "worker-two", true);
    let second_epoch = second["claim_epoch"].as_i64().expect("leased claim epoch");
    assert!(second_epoch > first_epoch);
    assert_eq!(second["lease"]["fencing_token"], second_epoch);

    // Simulate clone/restart recovery from the auto-published checkpoint.
    let saved_checkpoint = workspace.path().join("saved-forensic.jsonl");
    std::fs::copy(
        workspace
            .path()
            .join(".beads")
            .join("checkpoint")
            .join("forensic.jsonl"),
        &saved_checkpoint,
    )
    .unwrap();
    std::fs::remove_file(workspace.path().join(".beads").join("beads.db")).unwrap();
    run(workspace.path(), &["init"]);
    run(
        workspace.path(),
        &[
            "sync",
            "import-only",
            "--input",
            saved_checkpoint.to_str().unwrap(),
            "--restore-into-empty",
            "--actor",
            "claim-epoch-test",
        ],
    );
    assert_eq!(
        shown_issue(workspace.path(), &id)["claim_epoch"],
        second_epoch
    );

    run(workspace.path(), &["release", &id]);
    let third = claim(workspace.path(), "worker-three", false);
    assert!(third["claim_epoch"].as_i64().unwrap() > second_epoch);
}

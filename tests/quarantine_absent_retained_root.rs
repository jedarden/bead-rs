//! beadrs-f41a01c8: a retained checkpoint generation whose well-formed
//! pointer selects an absent object must not make a clean workspace
//! permanently unpublishable.
//!
//! The audited sanitized reset (`bead redact --all-blocking` on a quarantined
//! workspace whose live store scans clean) republishes a fresh generation
//! from the live store, records each discarded absent generation in its
//! `secret_quarantine_reset` event, and never deletes live data. When the
//! live store is not clean the refusal is unchanged.

use assert_cmd::Command;
use rusqlite::params;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

const ACTOR: &str = "absent-root-test";
const REASON: &str = "republish the clean live store over an absent retained root";

fn bead(workspace: &Path) -> Command {
    let mut command = Command::cargo_bin("bead").unwrap();
    command
        .current_dir(workspace)
        .env("HOME", workspace)
        .env_remove("BEAD_ORG_SECRET_SCANNER");
    command
}

fn init_workspace(name: &str) -> tempfile::TempDir {
    let workspace = tempfile::Builder::new()
        .prefix(&format!("bead-absent-root-{name}-"))
        .tempdir_in("/var/tmp")
        .unwrap();
    bead(workspace.path())
        .args(["init", "--skip-foreign-workspace", "--prefix", "absent"])
        .assert()
        .success();
    workspace
}

fn create(root: &Path, title: &str) {
    bead(root)
        .args(["create", "--title", title])
        .assert()
        .success();
}

fn checkpoint(root: &Path) -> PathBuf {
    root.join(".beads/checkpoint")
}

fn database(root: &Path) -> PathBuf {
    root.join(".beads/beads.db")
}

fn pointer(root: &Path, name: &str) -> Value {
    serde_json::from_slice(&fs::read(checkpoint(root).join(name)).unwrap()).unwrap()
}

fn root_path(pointer: &Value) -> String {
    pointer["active_root"]["path"].as_str().unwrap().to_string()
}

fn generation(pointer: &Value) -> String {
    pointer["generation_id"].as_str().unwrap().to_string()
}

fn issue_count(root: &Path) -> i64 {
    let conn = rusqlite::Connection::open(database(root)).unwrap();
    conn.query_row("SELECT COUNT(*) FROM issues", [], |row| row.get(0))
        .unwrap()
}

fn quarantine_rows(root: &Path) -> i64 {
    let conn = rusqlite::Connection::open(database(root)).unwrap();
    conn.query_row("SELECT COUNT(*) FROM secret_quarantine", [], |row| {
        row.get(0)
    })
    .unwrap()
}

fn reset_event_details(root: &Path) -> Vec<Value> {
    let conn = rusqlite::Connection::open(database(root)).unwrap();
    let mut statement = conn
        .prepare(
            "SELECT detail FROM events WHERE kind = 'secret_quarantine_reset' ORDER BY sequence",
        )
        .unwrap();
    statement
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .map(|detail| serde_json::from_str(&detail.unwrap()).unwrap())
        .collect()
}

fn output_text(output: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// The ordinary publisher discovers the incomplete retained coverage and
/// opens the durable hold, exactly as the field workspace did.
fn quarantine_by_flush(root: &Path) {
    let refused = bead(root).args(["sync", "flush-only"]).output().unwrap();
    assert!(!refused.status.success(), "{}", output_text(&refused));
    assert!(
        output_text(&refused).contains("secret_quarantined"),
        "{}",
        output_text(&refused)
    );
    assert_eq!(quarantine_rows(root), 1);
}

fn sanitized_republish(root: &Path) {
    let dry_run = bead(root)
        .args([
            "redact",
            "--all-blocking",
            "--actor",
            ACTOR,
            "--reason",
            REASON,
            "--dry-run",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(dry_run.status.success(), "{}", output_text(&dry_run));
    let dry_run: Value = serde_json::from_slice(&dry_run.stdout).unwrap();
    assert_eq!(dry_run["dry_run"], true);
    assert_eq!(dry_run["sanitized_republish"], true);
    assert_eq!(quarantine_rows(root), 1, "a dry run changes nothing");

    let reset = bead(root)
        .args([
            "redact",
            "--all-blocking",
            "--actor",
            ACTOR,
            "--reason",
            REASON,
            "--json",
        ])
        .output()
        .unwrap();
    assert!(reset.status.success(), "{}", output_text(&reset));
    let reset: Value = serde_json::from_slice(&reset.stdout).unwrap();
    assert_eq!(reset["sanitized_republish"], true);
    assert_eq!(reset["previous_generation_reset"], true);
}

fn assert_recovered(root: &Path, issues_before: i64, discarded: &[(&str, &str)]) {
    let current = pointer(root, "current.json");
    let previous = pointer(root, "previous.json");
    assert_eq!(current["generation_id"], previous["generation_id"]);
    assert_eq!(current["active_root"], previous["active_root"]);
    assert!(checkpoint(root).join(root_path(&current)).is_file());
    for (_, old_generation) in discarded {
        assert_ne!(generation(&current), *old_generation);
        assert!(current["superseded_generations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|superseded| superseded == old_generation));
    }

    let status: Value = serde_json::from_slice(
        &bead(root)
            .args(["sync", "status", "--format", "json"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    assert_eq!(status["root_verified"], true, "{status}");
    assert_eq!(
        status["covered_sequence"], status["live_sequence"],
        "{status}"
    );

    // Live data is never deleted by the reset.
    assert_eq!(issue_count(root), issues_before);
    assert_eq!(quarantine_rows(root), 0);

    let details = reset_event_details(root);
    assert_eq!(details.len(), 1, "{details:?}");
    assert_eq!(details[0]["reason"], REASON);
    let recorded: Vec<(String, String, String)> = details[0]["discarded_absent_generations"]
        .as_array()
        .expect("the reset must record every discarded absent generation")
        .iter()
        .map(|entry| {
            (
                entry["pointer"].as_str().unwrap().to_string(),
                entry["generation_id"].as_str().unwrap().to_string(),
                entry["reason"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    let expected: Vec<(String, String, String)> = discarded
        .iter()
        .map(|(name, generation)| {
            (
                name.to_string(),
                generation.to_string(),
                "selected_object_absent".to_string(),
            )
        })
        .collect();
    assert_eq!(recorded, expected);

    // The audit fact is itself published: the new checkpoint covers it.
    let root_bytes = fs::read(checkpoint(root).join(root_path(&current))).unwrap();
    assert!(String::from_utf8_lossy(&root_bytes).contains("discarded_absent_generations"));

    // The secret inventory is complete again, and ordinary writes publish.
    let doctor = bead(root)
        .args(["doctor", "--scope", "secrets", "--format", "json"])
        .output()
        .unwrap();
    let doctor: Value = serde_json::from_slice(&doctor.stdout).unwrap();
    let secret_scan = doctor["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|check| check["name"] == "secret_scan")
        .unwrap();
    assert_eq!(
        secret_scan["details"]["coverage_complete"], true,
        "{secret_scan}"
    );
    assert_eq!(
        secret_scan["details"]["quarantined"], false,
        "{secret_scan}"
    );

    create(root, "written after recovery");
    assert_eq!(issue_count(root), issues_before + 1);
    let status: Value = serde_json::from_slice(
        &bead(root)
            .args(["sync", "status", "--format", "json"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    assert_eq!(status["root_verified"], true, "{status}");
    assert_eq!(
        status["covered_sequence"], status["live_sequence"],
        "{status}"
    );
}

#[test]
fn absent_current_root_is_discarded_by_sanitized_republish() {
    let workspace = init_workspace("current");
    let root = workspace.path();
    create(root, "first");
    create(root, "second");
    let current = pointer(root, "current.json");
    let previous = pointer(root, "previous.json");
    assert_ne!(root_path(&current), root_path(&previous));
    fs::remove_file(checkpoint(root).join(root_path(&current))).unwrap();

    quarantine_by_flush(root);
    sanitized_republish(root);
    assert_recovered(root, 2, &[("current", &generation(&current))]);
}

#[test]
fn absent_previous_root_is_discarded_by_sanitized_republish() {
    let workspace = init_workspace("previous");
    let root = workspace.path();
    create(root, "first");
    create(root, "second");
    let current = pointer(root, "current.json");
    let previous = pointer(root, "previous.json");
    assert_ne!(root_path(&current), root_path(&previous));
    fs::remove_file(checkpoint(root).join(root_path(&previous))).unwrap();

    quarantine_by_flush(root);
    sanitized_republish(root);
    assert_recovered(root, 2, &[("previous", &generation(&previous))]);
}

/// The field shape (argo-workflows-exporter, 2026-10-16): two retained
/// generations select one content-addressed root that is gone from disk.
#[test]
fn both_pointers_selecting_one_absent_root_are_discarded() {
    let workspace = init_workspace("both");
    let root = workspace.path();
    create(root, "first");
    create(root, "second");
    let current = pointer(root, "current.json");
    let mut previous = current.clone();
    let previous_generation = "gen-0123456789abcdef0123456789abcdef".to_string();
    assert_ne!(generation(&current), previous_generation);
    previous["generation_id"] = Value::String(previous_generation.clone());
    fs::write(
        checkpoint(root).join("previous.json"),
        serde_json::to_vec_pretty(&previous).unwrap(),
    )
    .unwrap();
    fs::remove_file(checkpoint(root).join(root_path(&current))).unwrap();

    quarantine_by_flush(root);
    sanitized_republish(root);
    assert_recovered(
        root,
        2,
        &[
            ("current", &generation(&current)),
            ("previous", &previous_generation),
        ],
    );
}

/// With a blocking live finding the absent root is no excuse to publish:
/// nothing is reset, the hold stays, and no value is printed.
#[test]
fn absent_root_does_not_bypass_an_unclean_live_store() {
    let workspace = init_workspace("unclean");
    let root = workspace.path();
    create(root, "first");
    create(root, "second");
    // Assemble the candidate at runtime and keep it out of all diagnostics.
    let secret = ["AK", "IA", "7Q9W2E4R6T8Y1U3I"].concat();
    let conn = rusqlite::Connection::open(database(root)).unwrap();
    conn.execute(
        "INSERT INTO issues (
            id, title, description, notes, priority, issue_type, base_status,
            created_at, updated_at, revision
         ) VALUES ('absent-live', 'stable title', ?1, 'stable notes', 2, 'task', 'open',
                   '2026-10-16T00:00:00Z', '2026-10-16T00:00:00Z', 1)",
        params![secret],
    )
    .unwrap();
    drop(conn);
    let current = pointer(root, "current.json");
    fs::remove_file(checkpoint(root).join(root_path(&current))).unwrap();
    quarantine_by_flush(root);
    let current_before = fs::read(checkpoint(root).join("current.json")).unwrap();
    let previous_before = fs::read(checkpoint(root).join("previous.json")).unwrap();

    let refused = bead(root)
        .args([
            "redact",
            "--all-blocking",
            "--actor",
            ACTOR,
            "--reason",
            REASON,
            "--json",
        ])
        .output()
        .unwrap();
    assert!(!refused.status.success(), "{}", output_text(&refused));
    assert!(!output_text(&refused).contains(&secret));
    assert_eq!(
        fs::read(checkpoint(root).join("current.json")).unwrap(),
        current_before
    );
    assert_eq!(
        fs::read(checkpoint(root).join("previous.json")).unwrap(),
        previous_before
    );
    assert_eq!(quarantine_rows(root), 1);
    assert!(reset_event_details(root).is_empty());
    assert_eq!(issue_count(root), 3);
}

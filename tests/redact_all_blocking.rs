//! `bead redact --all-blocking` (beadrs-1c110ec3): every blocking finding in
//! one atomic batch, overlapping matches collapsed to one covering selection,
//! and organization-scanner parity when a scanner is configured.
//!
//! Secret-shaped values are assembled at runtime from fragments; no complete
//! credential appears in this file.

use bead_rs::store::{open_configured_connection, SqliteStore};
use std::path::Path;
use std::process::{Command, Output};

use serde_json::Value;

fn bead(workspace: &Path, args: &[&str], org_scanner: Option<&str>) -> Output {
    Command::new(env!("CARGO_BIN_EXE_bead"))
        .args(args)
        .current_dir(workspace)
        .env("BEAD_ORG_SECRET_SCANNER", org_scanner.unwrap_or("off"))
        .output()
        .expect("run bead")
}

fn ok(workspace: &Path, args: &[&str], org_scanner: Option<&str>) -> String {
    let output = bead(workspace, args, org_scanner);
    assert!(
        output.status.success(),
        "bead {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Private test construction of history predating the public write boundary.
/// Managed builds must never need an acknowledgment to exercise remediation.
fn historical_copy(workspace: &Path, id: &str, description: &str, notes: &str) {
    let mut store = SqliteStore::from_conn(
        open_configured_connection(&workspace.join(".beads/beads.db")).unwrap(),
    );
    store
        .conn()
        .execute(
            "UPDATE issues SET description=?1,notes=?2,revision=revision+1 WHERE id=?3",
            rusqlite::params![description, notes, id],
        )
        .unwrap();
    store
        .conn()
        .execute(
            "INSERT INTO events (issue_id,kind,actor,time,detail)
         VALUES (?1,'fixture_history','fixture-operator','2026-10-06T00:00:00Z','{}')",
            [id],
        )
        .unwrap();
    let base = workspace.join(".beads");
    let config = bead_rs::service::load_checkpoint_config(&base).unwrap();
    bead_rs::service::publish_forensic_checkpoint(&mut store, &config, &base).unwrap();
}

fn revision(workspace: &Path, id: &str) -> i64 {
    let connection = open_configured_connection(&workspace.join(".beads/beads.db")).unwrap();
    connection
        .query_row("SELECT revision FROM issues WHERE id=?1", [id], |row| {
            row.get(0)
        })
        .unwrap()
}

/// Feed invented candidate bytes through a private file, not process arguments.
fn create_manifest(workspace: &Path, description: &str, scanner: Option<&str>) -> Output {
    let input = serde_json::json!({"manifest_version":1,"operations":[{
        "op":"create","local_id":"probe","title":"probe","description":description
    }]});
    let mut file = tempfile::NamedTempFile::new_in(workspace).unwrap();
    serde_json::to_writer(file.as_file_mut(), &input).unwrap();
    bead(
        workspace,
        &[
            "manifest",
            "commit",
            "--input",
            file.path().to_str().unwrap(),
        ],
        scanner,
    )
}

fn workspace() -> tempfile::TempDir {
    let dir = tempfile::Builder::new()
        .prefix("all-blocking-")
        .tempdir_in("/var/tmp")
        .unwrap();
    std::fs::create_dir(dir.path().join(".beads")).unwrap();
    let uuid = uuid::Uuid::new_v4().to_string();
    std::fs::write(
        dir.path().join(".beads/config.json"),
        serde_json::to_vec(&serde_json::json!({"version":1,"uuid":uuid,"prefix":"all"})).unwrap(),
    )
    .unwrap();
    ok(dir.path(), &["init", "--no-auto-flush"], None);
    let connection = open_configured_connection(&dir.path().join(".beads/beads.db")).unwrap();
    let initialized: String = connection
        .query_row("SELECT uuid FROM workspace", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        initialized, uuid,
        "fixture escaped its private identity fence"
    );
    dir
}

fn bytes_present(workspace: &Path, needles: &[&str]) -> Vec<String> {
    let mut hits = Vec::new();
    let mut pending = vec![workspace.join(".beads")];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory).expect("read dir") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                pending.push(path);
            } else {
                let content = std::fs::read(&path).expect("read file");
                if needles.iter().any(|needle| {
                    content
                        .windows(needle.len())
                        .any(|window| window == needle.as_bytes())
                }) {
                    hits.push(path.display().to_string());
                }
            }
        }
    }
    hits
}

fn issue_id(workspace: &Path, title: &str) -> String {
    ok(workspace, &["list", "--json"], None)
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find(|issue| issue["title"] == title)
        .and_then(|issue| issue["id"].as_str().map(str::to_string))
        .expect("issue listed")
}

#[test]
fn all_blocking_redacts_every_copy_in_one_epoch_and_is_idempotent() {
    let dir = workspace();
    let root = dir.path();
    let key_id = ["AKIA", "Q7XZ3M", "PL9RTW4K2B"].concat();
    let secret = ["wJalrXUtnF", "EMI/K7MDENG", "/bPxRfiCYQ9x7Rk2Lp4"].concat();
    ok(root, &["create", "--title", "probe"], None);
    let id = issue_id(root, "probe");
    let description = format!("aws_access_key_id = {key_id} and aws_secret_access_key = {secret}");
    historical_copy(root, &id, &description, "");
    historical_copy(root, &id, &description, &format!("again {key_id}"));
    let prior_revision = revision(root, &id);
    assert!(!bytes_present(root, &[&key_id, &secret]).is_empty());

    let preview = ok(
        root,
        &[
            "redact",
            "--all-blocking",
            "--actor",
            "tester",
            "--reason",
            "sweep",
            "--dry-run",
            "--json",
        ],
        None,
    );
    assert!(!preview.contains(&key_id) && !preview.contains(&secret));
    assert!(
        !bytes_present(root, &[&key_id, &secret]).is_empty(),
        "dry run changes nothing"
    );
    assert_eq!(revision(root, &id), prior_revision);

    let output = ok(
        root,
        &[
            "redact",
            "--all-blocking",
            "--actor",
            "tester",
            "--reason",
            "sweep",
            "--json",
        ],
        None,
    );
    assert!(!output.contains(&key_id) && !output.contains(&secret));
    let receipts: Value = serde_json::from_str(&output).expect("json");
    let receipts = receipts["receipts"].as_array().expect("receipts");
    assert!(receipts.len() >= 3, "three distinct ranges: {receipts:?}");
    let epochs: std::collections::BTreeSet<String> = receipts
        .iter()
        .map(|receipt| receipt["epoch_id"].to_string())
        .collect();
    assert_eq!(epochs.len(), 1, "one atomic epoch");
    assert_eq!(revision(root, &id), prior_revision + 1);

    assert_eq!(
        bytes_present(root, &[&key_id, &secret]),
        Vec::<String>::new()
    );
    let doctor = bead(root, &["doctor", "--scope", "secrets"], None);
    let doctor = format!(
        "{}{}",
        String::from_utf8_lossy(&doctor.stdout),
        String::from_utf8_lossy(&doctor.stderr)
    );
    assert!(doctor.contains("No secret findings"), "{doctor}");
    ok(root, &["show", &id], None);

    let again = ok(
        root,
        &[
            "redact",
            "--all-blocking",
            "--actor",
            "tester",
            "--reason",
            "sweep",
        ],
        None,
    );
    assert!(again.contains("nothing to redact"), "{again}");
}

/// Runs only where an organization scanner with `--serve` is configured
/// through `BEAD_TEST_ORG_SCANNER` (CI builders carry none).
#[test]
fn org_scanner_findings_block_writes_and_redact_with_native_overlaps() {
    let Ok(scanner_path) = std::env::var("BEAD_TEST_ORG_SCANNER") else {
        eprintln!("skipped: BEAD_TEST_ORG_SCANNER is not set");
        return;
    };
    let scanner = Some(scanner_path.as_str());
    let dir = workspace();
    let root = dir.path();
    let token = ["A7bQ9xL2", "mN4pR8sT", "3vW6yZ1c", "D5fG0hJk"].concat();
    let password = ["Zq8Lm2Np", "Kx7Rt4Vw", "9Hs3Jd6F"].concat();
    let description = format!("deploy notes: service_token = {token} (rotate quarterly)");

    // Ruleset 4 now detects the assignment natively as well. Both paths must
    // reject it without leaking bytes or admitting a partial mutation.
    let native_rejected = create_manifest(root, &description, None);
    assert!(!native_rejected.status.success());
    assert!(String::from_utf8_lossy(&native_rejected.stderr).contains("credential-assignment"));
    assert!(!String::from_utf8_lossy(&native_rejected.stderr).contains(&token));
    let rejected = create_manifest(root, &description, scanner);
    assert!(!rejected.status.success());
    let stderr = String::from_utf8_lossy(&rejected.stderr);
    assert!(stderr.contains("org-scanner:"), "{stderr}");
    assert!(!stderr.contains(&token));

    ok(root, &["create", "--title", "probe"], None);
    let id = issue_id(root, "probe");
    // Both the native URI rule and the organization basic-auth rule match
    // this password with different ranges.
    historical_copy(root, &id, &description, "");
    historical_copy(
        root,
        &id,
        &description,
        &format!("db: postgresql://app:{password}@db.internal:5432/app"),
    );

    let output = ok(
        root,
        &[
            "redact",
            "--all-blocking",
            "--actor",
            "tester",
            "--reason",
            "sweep",
            "--json",
        ],
        scanner,
    );
    assert!(!output.contains(&token) && !output.contains(&password));
    assert_eq!(
        bytes_present(root, &[&token, &password]),
        Vec::<String>::new()
    );
    let checkpoint_scan = Command::new(&scanner_path)
        .args(["--max-bytes", "52428800"])
        .args(
            std::fs::read_dir(root.join(".beads/checkpoint/objects"))
                .expect("objects")
                .map(|entry| entry.expect("entry").path()),
        )
        .output()
        .expect("run org scanner");
    assert_eq!(
        String::from_utf8_lossy(&checkpoint_scan.stdout),
        "",
        "the Git-side scanner finds nothing left in the checkpoint"
    );
}

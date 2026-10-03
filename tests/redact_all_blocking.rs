//! `bead redact --all-blocking` (beadrs-1c110ec3): every blocking finding in
//! one atomic batch, overlapping matches collapsed to one covering selection,
//! and organization-scanner parity when a scanner is configured.
//!
//! Secret-shaped values are assembled at runtime from fragments; no complete
//! credential appears in this file.

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

/// Run a mutation, admitting each blocking finding by its exact fingerprint
/// (simulating secrets that entered before a rule existed).
fn admit(workspace: &Path, args: &[&str], org_scanner: Option<&str>) {
    let mut acknowledgments: Vec<String> = Vec::new();
    for _ in 0..12 {
        let mut full: Vec<&str> = args.to_vec();
        for fingerprint in &acknowledgments {
            full.push("--acknowledge-secret");
            full.push(fingerprint);
        }
        let output = bead(workspace, &full, org_scanner);
        if output.status.success() {
            return;
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        let fingerprint = stderr
            .split("--acknowledge-secret ")
            .nth(1)
            .and_then(|rest| rest.get(..64))
            .unwrap_or_else(|| panic!("mutation failed without a fingerprint: {stderr}"))
            .to_string();
        acknowledgments.push(fingerprint);
    }
    panic!("too many findings to admit");
}

fn workspace() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    ok(dir.path(), &["init", "--skip-foreign-workspace"], None);
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
    admit(
        root,
        &[
            "create",
            "--title",
            "probe",
            "--description",
            &format!("aws_access_key_id = {key_id} and aws_secret_access_key = {secret}"),
        ],
        None,
    );
    let id = issue_id(root, "probe");
    admit(
        root,
        &["update", &id, "--notes", &format!("again {key_id}")],
        None,
    );
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

    // Native rules alone admit this text; the organization scanner blocks it.
    ok(
        root,
        &[
            "create",
            "--title",
            "native-only",
            "--description",
            &description,
        ],
        None,
    );
    let rejected = bead(
        root,
        &["create", "--title", "probe", "--description", &description],
        scanner,
    );
    assert!(!rejected.status.success());
    let stderr = String::from_utf8_lossy(&rejected.stderr);
    assert!(stderr.contains("org-scanner:"), "{stderr}");
    assert!(!stderr.contains(&token));

    admit(
        root,
        &["create", "--title", "probe", "--description", &description],
        scanner,
    );
    let id = issue_id(root, "probe");
    // Both the native URI rule and the organization basic-auth rule match
    // this password with different ranges.
    admit(
        root,
        &[
            "update",
            &id,
            "--notes",
            &format!("db: postgresql://app:{password}@db.internal:5432/app"),
        ],
        scanner,
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

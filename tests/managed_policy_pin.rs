//! Acceptance checks for the committed managed-fleet binary pin.
//!
//! The source-level managed-profile tests prove the policy in a Cargo build.
//! These checks prove that the exact artifact distributed to fleet workers is
//! the strict profile too, and exercise its workspace tamper boundary.

use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;

/// First existing tempdir base whose ancestor chain carries no `.beads`.
///
/// Workspace discovery stops at the first `.beads` found walking upward, so
/// a fresh workspace created under a stray ancestor store refuses its `bead
/// init` or silently binds every later invocation to that foreign store.
/// Hosts carry such debris (a shared smoke-test workspace lives at
/// `/var/tmp/.beads` on some machines), so the historical `/var/tmp` base —
/// originally chosen to dodge a stray `/tmp/.beads` — is only the first
/// choice, falling back to TMPDIR and then /dev/shm. Same selection as
/// cli_init.rs's `temp_workspace_dir`.
fn clean_beads_free_base() -> std::path::PathBuf {
    let mut bases = vec![
        std::path::PathBuf::from("/var/tmp"),
        std::env::temp_dir(),
        std::path::PathBuf::from("/dev/shm"),
    ];
    bases.retain(|base| base.exists() && !has_beads_ancestor(base));
    bases.first().cloned().expect(
        "no tempdir base with a .beads-free ancestor chain: clean up the stray \
             .beads above /var/tmp, TMPDIR, or /dev/shm",
    )
}

/// True when the directory or any ancestor carries a `.beads` directory.
fn has_beads_ancestor(start: &std::path::Path) -> bool {
    let mut current = Some(start);
    while let Some(dir) = current {
        if dir.join(".beads").exists() {
            return true;
        }
        current = dir.parent();
    }
    false
}

const ROLE: &str = "managed_secret_policy";

fn pin() -> anyhow::Result<(PathBuf, Value)> {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("pinned-binaries");
    let registry: Value = serde_json::from_slice(&std::fs::read(directory.join("commits.json"))?)?;
    let entry = registry
        .get(ROLE)
        .ok_or_else(|| anyhow::anyhow!("managed pin role is missing"))?;
    let name = entry
        .get("binary_name")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("managed pin has no binary_name"))?;
    let path = directory.join(name);
    anyhow::ensure!(path.is_file(), "managed pin is absent: {}", path.display());
    let metadata: Value = serde_json::from_slice(&std::fs::read(
        directory.join(format!("{name}.metadata.json")),
    )?)?;
    Ok((path, metadata))
}

fn run(binary: &Path, root: &Path, args: &[&str]) -> std::process::Output {
    Command::new(binary)
        .current_dir(root)
        .args(args)
        .output()
        .expect("managed pin must execute")
}

#[test]
fn managed_pin_is_hash_checked_and_rejects_workspace_downgrade() {
    let (binary, metadata) = pin().unwrap();
    let bytes = std::fs::read(&binary).unwrap();
    let digest = hex::encode(Sha256::digest(&bytes));
    assert_eq!(metadata["binary_sha256"], digest);
    assert_eq!(metadata["build_features"], "managed-secret-policy");
    assert_eq!(
        metadata["git_commit_sha"],
        "cd4986c89110469ff88e50494122bc72020457e7"
    );

    let capabilities = run(
        binary.as_path(),
        &clean_beads_free_base(),
        &["capabilities"],
    );
    assert!(capabilities.status.success(), "capabilities failed");
    let capabilities: Value = serde_json::from_slice(&capabilities.stdout).unwrap();
    assert_eq!(
        capabilities["secret_scan"]["compiled_policy"],
        "managed-enforce-no-ack"
    );
    assert_eq!(capabilities["secret_scan"]["effective_mode"], "enforce");
    assert_eq!(
        capabilities["secret_scan"]["exact_fingerprint_acknowledgment"],
        false
    );

    let root = tempfile::Builder::new()
        .prefix("bead-managed-pin-")
        .tempdir_in(clean_beads_free_base())
        .unwrap();
    assert!(
        run(binary.as_path(), root.path(), &["init", "--no-auto-flush"])
            .status
            .success()
    );

    let config_path = root.path().join(".beads/config.json");
    let mut config: Value = serde_json::from_slice(&std::fs::read(&config_path).unwrap()).unwrap();
    config["secret_scan"] = serde_json::json!({"mode": "off"});
    std::fs::write(&config_path, serde_json::to_vec(&config).unwrap()).unwrap();

    let candidate = format!("aws_secret_access_key = {}", "A".repeat(40));
    let rejected = run(
        binary.as_path(),
        root.path(),
        &["create", "--title", candidate.as_str()],
    );
    assert!(!rejected.status.success());
    let stderr = String::from_utf8_lossy(&rejected.stderr);
    assert!(stderr.contains("managed_secret_policy"), "{stderr}");
    assert!(
        !stderr.contains(&candidate),
        "diagnostic disclosed candidate"
    );

    config["secret_scan"] = serde_json::json!({"mode": candidate});
    std::fs::write(&config_path, serde_json::to_vec(&config).unwrap()).unwrap();
    let diagnostic = run(
        binary.as_path(),
        root.path(),
        &["doctor", "--scope", "secrets"],
    );
    assert!(!diagnostic.status.success());
    assert!(!String::from_utf8_lossy(&diagnostic.stderr).contains(&candidate));
}

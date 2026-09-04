//! Integration tests for `bead capabilities` command

use assert_cmd::Command;
use bead_rs::service::AUTO_FLUSH_COMPILED_DEFAULT;
use serde_json::Value;
use serial_test::serial;
use std::fs;
use std::path::Path;

#[test]
fn secret_scan_capability_uses_compiled_default_without_workspace() {
    let workspace = tempfile::tempdir().unwrap();
    let output = Command::cargo_bin("bead")
        .unwrap()
        .current_dir(workspace.path())
        .arg("capabilities")
        .output()
        .unwrap();
    assert!(output.status.success());
    let capabilities: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        capabilities["secret_scan"]["contract_identity"],
        "urn:bead-rs:spec:secret-rejection:v1"
    );
    assert_eq!(capabilities["secret_scan"]["ruleset_version"], 2);
    assert_eq!(capabilities["secret_scan"]["effective_mode"], "enforce");
    assert_eq!(capabilities["secret_scan"]["blocking"], true);
    assert_eq!(capabilities["secret_scan"]["advisory"], true);
    assert_eq!(
        capabilities["secret_scan"]["exact_fingerprint_acknowledgment"],
        true
    );
}

#[test]
fn secret_scan_capability_reports_effective_workspace_mode() {
    let workspace = tempfile::Builder::new()
        .prefix("bead-secret-capability-")
        .tempdir_in("/var/tmp")
        .unwrap();
    Command::cargo_bin("bead")
        .unwrap()
        .current_dir(workspace.path())
        .args(["init", "--no-auto-flush"])
        .assert()
        .success();
    let config_path = workspace.path().join(".beads/config.json");
    let mut config: Value = serde_json::from_slice(&std::fs::read(&config_path).unwrap()).unwrap();
    config["secret_scan"] = serde_json::json!({"mode": "off"});
    std::fs::write(&config_path, serde_json::to_vec_pretty(&config).unwrap()).unwrap();

    let output = Command::cargo_bin("bead")
        .unwrap()
        .current_dir(workspace.path())
        .arg("capabilities")
        .output()
        .unwrap();
    assert!(output.status.success());
    let capabilities: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(capabilities["secret_scan"]["effective_mode"], "off");
}

#[test]
#[serial]
fn test_capabilities_no_workspace() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Save original directory to restore later
    let original_dir = std::env::current_dir().unwrap().canonicalize().unwrap();

    // Capabilities should work even without a workspace
    let result = Command::cargo_bin("bead")
        .unwrap()
        .args(["capabilities"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output = std::str::from_utf8(&result).unwrap();
    // Verify it's valid JSON
    let _: Value = serde_json::from_str(output).unwrap();

    // Restore original directory before dropping temp
    std::env::set_current_dir(original_dir).unwrap();
}

#[test]
#[serial]
fn test_capabilities_native_profile() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Save original directory to restore later
    let original_dir = std::env::current_dir().unwrap().canonicalize().unwrap();

    // Test with native-v1 profile
    let result = Command::cargo_bin("bead")
        .unwrap()
        .args(["capabilities", "--profile", "native-v1"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output = std::str::from_utf8(&result).unwrap();
    let caps: Value = serde_json::from_str(output).unwrap();

    // Verify structure
    assert_eq!(caps["contract"], "native-v1");
    assert_eq!(caps["implementation"], "bead-rs");
    assert_eq!(caps["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(caps["store_layout"], 1);
    assert_eq!(caps["atomic_claim"], true);
    assert_eq!(caps["priorities"]["min"], 0);
    assert_eq!(caps["priorities"]["max"], 4);
    assert_eq!(caps["priorities"]["default"], 2);
    assert_eq!(caps["priorities"]["p4_claimable_by_fifo"], true);

    // Verify statuses array (only stored statuses, not derived presentation statuses)
    let statuses = caps["statuses"].as_array().unwrap();
    assert!(statuses.contains(&Value::String("open".to_string())));
    assert!(statuses.contains(&Value::String("closed".to_string())));
    assert!(statuses.contains(&Value::String("in_progress".to_string())));
    assert!(statuses.contains(&Value::String("deferred".to_string())));
    // Blocked is a derived presentation status, not a stored status
    assert!(!statuses.contains(&Value::String("blocked".to_string())));

    // Verify checkpoint modes
    let modes = caps["checkpoint_modes"].as_array().unwrap();
    assert!(modes.contains(&Value::String("monolithic".to_string())));
    assert!(modes.contains(&Value::String("sharded".to_string())));

    // Verify checkpoint formats
    let formats = caps["checkpoint_formats"].as_array().unwrap();
    assert!(formats.contains(&Value::String("issues-jsonl-v1".to_string())));
    assert!(formats.contains(&Value::String("checkpoint-set-v1".to_string())));

    // Verify schema_ref
    assert_eq!(
        caps["schema_ref"],
        "urn:bead-rs:schema:capabilities:native-v1"
    );

    // Verify schemas array
    let schemas = caps["schemas"].as_array().unwrap();
    assert!(!schemas.is_empty());

    // Verify commands array
    let commands = caps["commands"].as_array().unwrap();
    assert!(commands.contains(&Value::String("capabilities".to_string())));
    assert!(commands.contains(&Value::String("claim".to_string())));
    assert!(commands.contains(&Value::String("create".to_string())));
    assert!(commands.contains(&Value::String("list".to_string())));

    // Restore original directory before dropping temp
    std::env::set_current_dir(original_dir).unwrap();
}

#[test]
#[serial]
fn test_capabilities_needle_profile() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Save original directory to restore later
    let original_dir = std::env::current_dir().unwrap().canonicalize().unwrap();

    // Test with needle-v1 profile
    let result = Command::cargo_bin("bead")
        .unwrap()
        .args(["capabilities", "--profile", "needle-v1"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output = std::str::from_utf8(&result).unwrap();
    let caps: Value = serde_json::from_str(output).unwrap();

    // Verify contract is needle-v1
    assert_eq!(caps["contract"], "needle-v1");

    // Restore original directory before dropping temp
    std::env::set_current_dir(original_dir).unwrap();
}

#[test]
#[serial]
fn test_capabilities_invalid_profile() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Save original directory to restore later
    let original_dir = std::env::current_dir().unwrap().canonicalize().unwrap();

    // Test with invalid profile
    Command::cargo_bin("bead")
        .unwrap()
        .args(["capabilities", "--profile", "invalid-profile"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("Unsupported profile"));

    // Restore original directory before dropping temp
    std::env::set_current_dir(original_dir).unwrap();
}

#[test]
#[serial]
fn test_capabilities_default_profile() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Save original directory to restore later
    let original_dir = std::env::current_dir().unwrap().canonicalize().unwrap();

    // Test with default profile (no --profile flag)
    let result = Command::cargo_bin("bead")
        .unwrap()
        .args(["capabilities"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output = std::str::from_utf8(&result).unwrap();
    let caps: Value = serde_json::from_str(output).unwrap();

    // Verify default profile is native-v1
    assert_eq!(caps["contract"], "native-v1");

    // Restore original directory before dropping temp
    std::env::set_current_dir(original_dir).unwrap();
}

#[test]
#[serial]
fn test_capabilities_schema_entries() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Save original directory to restore later
    let original_dir = std::env::current_dir().unwrap().canonicalize().unwrap();

    // Test schema entries
    let result = Command::cargo_bin("bead")
        .unwrap()
        .args(["capabilities"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output = std::str::from_utf8(&result).unwrap();
    let caps: Value = serde_json::from_str(output).unwrap();

    // Verify issue schema entry
    let schemas = caps["schemas"].as_array().unwrap();
    let issue_schema = schemas
        .iter()
        .find(|s| s["schema_ref"] == "urn:bead-rs:schema:issue:native-v1")
        .expect("Issue schema not found");

    assert_eq!(issue_schema["document_kind"], "issue");
    assert_eq!(issue_schema["validate"], true);
    assert!(issue_schema["consume"]
        .as_array()
        .unwrap()
        .contains(&Value::String("sync.import-only".to_string())));
    assert!(issue_schema["emit"]
        .as_array()
        .unwrap()
        .contains(&Value::String("sync.flush-only".to_string())));

    // Restore original directory before dropping temp
    std::env::set_current_dir(original_dir).unwrap();
}

/// Run `bead ARGS` in `dir` and parse its stdout as one JSON document.
fn capabilities_json(dir: &Path, args: &[&str]) -> Value {
    let output = Command::cargo_bin("bead")
        .unwrap()
        .current_dir(dir)
        .args(args)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice(&output).unwrap()
}

/// The additive R026 handshake field (plan section 11): `auto_flush` is
/// absent until the compiled default flips on, then present reporting
/// exactly that default -- in both profiles, so a fleet detects the
/// behavior by handshake rather than inferring it from a version number.
#[test]
#[serial]
fn auto_flush_field_tracks_the_compiled_default() {
    let nowhere = tempfile::tempdir().unwrap();
    for profile in ["native-v1", "needle-v1"] {
        let caps = capabilities_json(nowhere.path(), &["capabilities", "--profile", profile]);
        if AUTO_FLUSH_COMPILED_DEFAULT {
            assert_eq!(
                caps.get("auto_flush"),
                Some(&Value::Bool(true)),
                "{profile}: with the compiled default on, auto_flush must be \
                 present and report it"
            );
        } else {
            assert_eq!(
                caps.get("auto_flush"),
                None,
                "{profile}: auto_flush must be absent while the compiled default \
                 is off -- the field appears only once the R026 gate flips it"
            );
        }
    }
}

/// The field reports the compiled default, never workspace state (plan
/// section 11): a workspace that disables publication through
/// `checkpoint.auto_flush`, and an invocation suppressed by
/// `--no-auto-flush`, advertise exactly what a workspace-less invocation
/// advertises.
#[test]
#[serial]
fn auto_flush_advertisement_ignores_workspace_state() {
    let nowhere = tempfile::tempdir().unwrap();
    let baseline = capabilities_json(nowhere.path(), &["capabilities"]);

    let workspace = tempfile::tempdir().unwrap();
    Command::cargo_bin("bead")
        .unwrap()
        .current_dir(workspace.path())
        .args(["init", "--prefix", "handshake"])
        .assert()
        .success();

    // Durably opt the workspace out of automatic publication.
    let config_path = workspace.path().join(".beads/config.json");
    let mut config: Value =
        serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
    config
        .as_object_mut()
        .unwrap()
        .entry("checkpoint")
        .or_insert(Value::Object(Default::default()))
        .as_object_mut()
        .unwrap()
        .insert("auto_flush".into(), Value::Bool(false));
    fs::write(&config_path, serde_json::to_string_pretty(&config).unwrap()).unwrap();

    let opted_out = capabilities_json(workspace.path(), &["capabilities"]);
    assert_eq!(
        opted_out.get("auto_flush"),
        baseline.get("auto_flush"),
        "checkpoint.auto_flush = false changed the advertisement; it changes \
         behavior, never what the binary advertises"
    );

    let suppressed = capabilities_json(workspace.path(), &["capabilities", "--no-auto-flush"]);
    assert_eq!(
        suppressed.get("auto_flush"),
        baseline.get("auto_flush"),
        "--no-auto-flush changed the advertisement; it suppresses one \
         invocation, never what the binary advertises"
    );
}

/// The advertised value matches actual binary behavior: in a workspace
/// with no `checkpoint.auto_flush` key, a plain mutating invocation
/// publishes a covering checkpoint generation exactly when the document
/// advertises `auto_flush: true`. The assertion holds on both sides of
/// the R026 gate, and `sync --status` is what reports the resulting
/// state either way -- it remains the only authority on whether this
/// workspace is actually clean.
#[test]
#[serial]
fn auto_flush_advertisement_matches_binary_behavior() {
    let nowhere = tempfile::tempdir().unwrap();
    let advertised = capabilities_json(nowhere.path(), &["capabilities"])
        .get("auto_flush")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let workspace = tempfile::tempdir().unwrap();
    for args in [
        vec!["init", "--prefix", "advertise"],
        vec!["create", "--title", "handshake behavior probe"],
    ] {
        Command::cargo_bin("bead")
            .unwrap()
            .current_dir(workspace.path())
            .args(&args)
            .assert()
            .success();
    }

    let report = capabilities_json(workspace.path(), &["sync", "status", "--format", "json"]);
    let live = report["live_sequence"].as_i64().unwrap();
    assert!(live >= 1, "setup: the mutation must have committed");
    if advertised {
        assert_eq!(
            report["checkpoint_present"],
            Value::Bool(true),
            "auto_flush is advertised but a plain mutation published no checkpoint"
        );
        assert_eq!(
            report["covered_sequence"], report["live_sequence"],
            "auto_flush is advertised but the durable checkpoint is behind the database"
        );
    } else {
        assert_eq!(
            report["checkpoint_present"],
            Value::Bool(false),
            "the binary published a checkpoint while the capability document \
             does not advertise auto_flush -- the handshake must match behavior"
        );
        assert_eq!(
            report["covered_sequence"],
            Value::Null,
            "sync --status must report the truth: nothing was published, so no \
             sequence is covered"
        );
    }
}

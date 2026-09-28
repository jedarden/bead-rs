//! A bare `.beads/issues.jsonl` interchange import must leave the workspace
//! restorable
//!
//! The bare issue-only interchange shape carries no store identity: no
//! event and no receipt for the stager to copy a `store_uuid` from.
//! Merging such an input used to publish a merge receipt whose
//! `source_store_uuid` was the stager's empty default — accepted silently
//! at import and flush time, then fatal at recovery:
//! `validate_forensic_receipts` refuses any receipt with an empty required
//! field, so every generation published after the merge was unrecoverable
//! by `bead restore`.
//!
//! Two regressions walk the recovery paths. The first covers workspaces
//! merged after the fix: the published merge receipt must carry the
//! deterministic bare-interchange source identity, and restoring the
//! generation must rebuild the store it names. The second covers
//! workspaces poisoned before the fix: a generation whose merge receipt
//! still names no source store must restore anyway, and the healed
//! identity must be re-published so the workspace stops carrying the
//! poison. A third pins the identity's determinism end to end: the same
//! interchange file re-imported through the real stager must merge under
//! the same source identity both times. A fourth covers the other entry
//! point the empty identity used to reach: a bare interchange restored
//! into an empty workspace adopts the derived identity as the recovering
//! workspace's own store identity, so the generations it publishes
//! afterward verify under `bead restore` too.

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use serial_test::serial;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

fn bead(dir: &Path) -> Command {
    let mut command = Command::cargo_bin("bead").unwrap();
    command.current_dir(dir);
    command.arg("--skip-foreign-workspace");
    command
}

/// A bare interchange file: issue records only. No events, no receipts, no
/// store identity anywhere — the shape `.beads/issues.jsonl` hands another
/// workspace.
const BARE_ISSUES_JSONL: &str = concat!(
    r#"{"id":"test-0000000000000001","title":"Bare Issue 1","description":"Imported from a bare interchange file","priority":2,"issue_type":"task","base_status":"open","manual_blocked":false,"created_at":"2026-09-27T12:00:00Z","updated_at":"2026-09-27T12:00:00Z","profile":"native-v1","schema_ref":"urn:bead-rs:schema:issue:native-v1"}"#,
    "\n",
    r#"{"id":"test-0000000000000002","title":"Bare Issue 2","description":"Second record in the bare interchange file","priority":1,"issue_type":"bug","base_status":"open","manual_blocked":false,"created_at":"2026-09-27T12:00:00Z","updated_at":"2026-09-27T12:00:00Z","profile":"native-v1","schema_ref":"urn:bead-rs:schema:issue:native-v1"}"#,
    "\n",
);

/// A fresh workspace with the bare interchange merged in and one generation
/// published. The tempdir must outlive every use of the returned workspace
/// path. Workspaces live under /var/tmp, not the TMPDIR default: workspace
/// discovery stops at the first featureless `.beads` above the working
/// directory (R030), and a shared /tmp can carry unrelated `.beads` debris
/// that would hijack the fresh workspace's init — the same reason
/// cli_init.rs pins its workspaces to /var/tmp.
fn workspace_with_bare_merge() -> (tempfile::TempDir, PathBuf) {
    let temp_dir = tempfile::Builder::new()
        .prefix("bead-test-")
        .tempdir_in("/var/tmp")
        .unwrap();
    let workspace = temp_dir.path().to_path_buf();

    bead(&workspace)
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    let import_path = workspace.join("issues.jsonl");
    fs::write(&import_path, BARE_ISSUES_JSONL).unwrap();

    // Merge the bare interchange into the fresh workspace. This is the
    // step that used to stamp the empty source identity, silently.
    bead(&workspace)
        .args([
            "sync",
            "import-only",
            "--input",
            import_path.to_str().unwrap(),
            "--merge",
            "--actor",
            "bare-merge-worker",
        ])
        .assert()
        .success();

    // Publish a generation; it carries the merge receipt forward.
    bead(&workspace)
        .args(["sync", "flush-only"])
        .assert()
        .success();

    (temp_dir, workspace)
}

/// The provenance-receipt records of a published generation object.
fn published_receipt_records(workspace: &Path) -> Vec<Value> {
    let pointer: Value = serde_json::from_str(
        &fs::read_to_string(workspace.join(".beads/checkpoint/current.json")).unwrap(),
    )
    .unwrap();
    let root_path = pointer["active_root"]["path"].as_str().unwrap();
    let published =
        fs::read_to_string(workspace.join(".beads/checkpoint").join(root_path)).unwrap();
    published
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .filter(|record| record["record_type"] == "provenance_receipt")
        .collect()
}

/// The source identities of every provenance receipt in a published
/// generation object.
fn published_receipt_sources(workspace: &Path) -> Vec<String> {
    published_receipt_records(workspace)
        .iter()
        .map(|record| {
            record["provenance_receipt"]["source_store_uuid"]
                .as_str()
                .unwrap_or_default()
                .to_string()
        })
        .collect()
}

/// The SHA-256 of the bare interchange input, exactly as the stager computes
/// it over the file bytes.
fn bare_input_hash() -> String {
    let mut hasher = Sha256::new();
    hasher.update(BARE_ISSUES_JSONL.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// The source identity the fixed writer stamps for the bare interchange
/// input: the shape tag plus the leading hex of the input's content hash.
fn bare_interchange_identity() -> String {
    format!("issues-jsonl-v1-{}", &bare_input_hash()[..16])
}

/// Rewrite the published generation so its merge receipt carries the empty
/// source identity the pre-fix writer stamped, then republish the object
/// under its recomputed SHA-256 and repoint the checkpoint pointer at it —
/// exactly the shape a workspace poisoned before the fix carries. Returns
/// the generation id and the merge receipt's source identity before
/// poisoning.
fn poison_published_merge_receipt(workspace: &Path) -> (String, String) {
    let pointer_path = workspace.join(".beads/checkpoint/current.json");
    let mut pointer: Value =
        serde_json::from_str(&fs::read_to_string(&pointer_path).unwrap()).unwrap();

    let root_path = pointer["active_root"]["path"].as_str().unwrap().to_string();
    let original =
        fs::read_to_string(workspace.join(".beads/checkpoint").join(&root_path)).unwrap();

    let mut poisoned_identity = None;
    let mut lines = Vec::new();
    for line in original.lines() {
        let mut record: Value = serde_json::from_str(line).unwrap();
        if record["record_type"] == "provenance_receipt"
            && record["provenance_receipt"]["source_store_uuid"]
                .as_str()
                .unwrap_or_default()
                .starts_with("issues-jsonl-v1-")
        {
            poisoned_identity = Some(
                record["provenance_receipt"]["source_store_uuid"]
                    .as_str()
                    .unwrap()
                    .to_string(),
            );
            record["provenance_receipt"]["source_store_uuid"] = Value::String(String::new());
        }
        lines.push(serde_json::to_string(&record).unwrap());
    }
    let identity = poisoned_identity.unwrap_or_else(|| {
        panic!("published generation carries no bare-interchange merge receipt")
    });

    let poisoned = format!("{}\n", lines.join("\n"));
    let mut hasher = Sha256::new();
    hasher.update(poisoned.as_bytes());
    let digest = format!("{:x}", hasher.finalize());
    let poisoned_path = format!("objects/{digest}.jsonl");
    fs::write(
        workspace.join(".beads/checkpoint").join(&poisoned_path),
        poisoned,
    )
    .unwrap();

    pointer["active_root"] = serde_json::json!({ "path": poisoned_path, "sha256": digest });
    fs::write(
        &pointer_path,
        serde_json::to_string_pretty(&pointer).unwrap(),
    )
    .unwrap();

    (
        pointer["generation_id"].as_str().unwrap().to_string(),
        identity,
    )
}

#[test]
#[serial]
fn bare_interchange_merge_import_flush_restore_round_trip() {
    let (_temp_dir, workspace) = workspace_with_bare_merge();

    // Every receipt in the published generation must name a real source
    // store, and the merge receipt's source must be the deterministic
    // bare-interchange identity derived from the input content — stable
    // across re-imports of the same file, never a real store's UUIDv4.
    let sources = published_receipt_sources(&workspace);
    assert!(
        !sources.is_empty(),
        "published generation carries no provenance receipt"
    );
    for identity in &sources {
        assert!(
            !identity.trim().is_empty(),
            "receipt published with an empty source_store_uuid"
        );
    }
    let expected_identity = bare_interchange_identity();
    assert!(
        sources.contains(&expected_identity),
        "merge receipt source {sources:?} is not the derived identity {expected_identity}"
    );

    // The identity is cut from the input content hash the receipt also
    // carries as its root — that redundancy is what lets a legacy receipt
    // be reconstructed from its own fields alone, so pin it end to end.
    let merge_root_hash = published_receipt_records(&workspace)
        .iter()
        .map(|record| &record["provenance_receipt"])
        .find(|receipt| receipt["kind"] == "merge")
        .expect("published generation carries the bare interchange merge receipt")
        ["source_root_sha256"]
        .clone();
    assert_eq!(
        merge_root_hash,
        bare_input_hash(),
        "the merge receipt must carry the full input hash its source identity is cut from"
    );

    // Provenance sanity: a merge receipt must name the store the input came
    // from, never the workspace it was merged into — every published source
    // differs from its target.
    for record in published_receipt_records(&workspace) {
        let receipt = &record["provenance_receipt"];
        assert_ne!(
            receipt["source_store_uuid"], receipt["target_store_uuid"],
            "receipt {record} names its own target as its source"
        );
    }

    // The merge did not only publish receipts — the bare interchange's
    // issues must be live in the working store before any recovery runs,
    // or the round trip below would be testing an empty restore.
    bead(&workspace)
        .args(["list", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("test-0000000000000001"))
        .stdout(predicate::str::contains("test-0000000000000002"));

    // Recovery: restore the generation this workspace just published. This
    // is the step that used to bail with "Provenance receipt 'merge-…' has
    // invalid required fields", leaving every post-merge generation
    // unrestorable.
    let pointer: Value = serde_json::from_str(
        &fs::read_to_string(workspace.join(".beads/checkpoint/current.json")).unwrap(),
    )
    .unwrap();
    let generation = pointer["generation_id"].as_str().unwrap();
    bead(&workspace)
        .args([
            "restore",
            "--source",
            workspace.join(".beads/checkpoint").to_str().unwrap(),
            "--generation",
            generation,
            "--actor",
            "recovery-worker",
            "--allow-non-empty",
            "--format",
            "json",
        ])
        .assert()
        .success();

    // The restore rebuilt the store from the verified generation.
    bead(&workspace)
        .args(["list", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("test-0000000000000001"))
        .stdout(predicate::str::contains("test-0000000000000002"));

    // Field recovery: a second, fresh workspace restores the same published
    // generation. Restoring into an empty target adopts the generation's
    // store identity, and the inherited merge receipt must arrive under the
    // derived identity so the recovering workspace's next generation is
    // clean — the poison must not cross a recovery boundary.
    let recovery_dir = tempfile::Builder::new()
        .prefix("bead-recovery-")
        .tempdir_in("/var/tmp")
        .unwrap();
    let recovery = recovery_dir.path().to_path_buf();
    bead(&recovery)
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    let report_stdout = bead(&recovery)
        .args([
            "restore",
            "--source",
            workspace.join(".beads/checkpoint").to_str().unwrap(),
            "--generation",
            generation,
            "--actor",
            "recovery-worker",
            "--format",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: Value = serde_json::from_slice(&report_stdout).unwrap();
    let source_uuid = report["source_store_uuid"].as_str().unwrap_or_default();
    assert!(
        !source_uuid.trim().is_empty(),
        "restore report names no source store: {report}"
    );
    assert_eq!(
        source_uuid,
        report["target_store_uuid"].as_str().unwrap_or_default(),
        "restoring into a fresh workspace must adopt the generation's store identity"
    );

    bead(&recovery)
        .args(["list", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("test-0000000000000001"))
        .stdout(predicate::str::contains("test-0000000000000002"));

    bead(&recovery)
        .args(["sync", "flush-only"])
        .assert()
        .success();
    let inherited = published_receipt_sources(&recovery);
    assert!(
        !inherited.is_empty(),
        "recovered workspace published no provenance receipt"
    );
    assert!(
        inherited.iter().all(|source| !source.trim().is_empty()),
        "recovered workspace republished an empty source_store_uuid: {inherited:?}"
    );
    assert!(
        inherited.contains(&expected_identity),
        "recovered workspace lost the bare interchange merge receipt under \
         {expected_identity}, got {inherited:?}"
    );
}

#[test]
#[serial]
fn legacy_poisoned_generation_restores_through_the_bare_interchange_heal() {
    let (_temp_dir, workspace) = workspace_with_bare_merge();

    // Simulate the pre-fix publish: the generation now carries a merge
    // receipt whose source_store_uuid is empty, behind a pointer whose
    // hashes all verify. Restoring this generation is the recovery path
    // every workspace poisoned before the fix is stuck on.
    let (generation, identity) = poison_published_merge_receipt(&workspace);
    assert_eq!(
        identity,
        bare_interchange_identity(),
        "the poisoned generation must carry the derived identity before poisoning"
    );

    // The heal at staging — not relaxed validation — is what lets recovery
    // through: the receipt is rewritten to the identity derived from its
    // untouched root hash before validation ever sees it.
    bead(&workspace)
        .args([
            "restore",
            "--source",
            workspace.join(".beads/checkpoint").to_str().unwrap(),
            "--generation",
            &generation,
            "--actor",
            "recovery-worker",
            "--allow-non-empty",
            "--format",
            "json",
        ])
        .assert()
        .success();

    // The restore rebuilt the store the poisoned generation described.
    bead(&workspace)
        .args(["list", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("test-0000000000000001"))
        .stdout(predicate::str::contains("test-0000000000000002"));

    // The heal must stick: the restore reactivated the receipt under the
    // identity derived from its root hash, so the next generation the
    // workspace publishes stops carrying the poison instead of passing it
    // to whatever restores it next.
    bead(&workspace)
        .args(["sync", "flush-only"])
        .assert()
        .success();
    let sources = published_receipt_sources(&workspace);
    assert!(
        !sources.is_empty(),
        "republished generation carries no provenance receipt"
    );
    for source in &sources {
        assert!(
            !source.trim().is_empty(),
            "republished generation still carries an empty source_store_uuid"
        );
    }
    assert!(
        sources.contains(&identity),
        "healed merge receipt was not republished under its derived identity {identity}, got {sources:?}"
    );
}

#[test]
#[serial]
fn bare_interchange_reimport_merges_under_the_same_source_identity() {
    let (_temp_dir, workspace) = workspace_with_bare_merge();

    // Re-merge the very same interchange file. The source identity comes
    // from the input's content hash as the stager computes it, not from
    // anything ambient, so the second import must land under the identical
    // source — the generation then names the same bare interchange twice
    // under one stable identity, and both receipts stay restorable.
    let import_path = workspace.join("issues.jsonl");
    bead(&workspace)
        .args([
            "sync",
            "import-only",
            "--input",
            import_path.to_str().unwrap(),
            "--merge",
            "--actor",
            "bare-merge-worker",
        ])
        .assert()
        .success();

    // Publish a second generation carrying both merge receipts.
    bead(&workspace)
        .args(["sync", "flush-only"])
        .assert()
        .success();

    let identity = bare_interchange_identity();
    let sources = published_receipt_sources(&workspace);
    assert_eq!(
        sources.len(),
        2,
        "the republished generation must carry both merge receipts, got {sources:?}"
    );
    assert!(
        sources.iter().all(|source| source == &identity),
        "re-importing the same bare input must reuse the source identity \
         {identity}, got {sources:?}"
    );
}

#[test]
#[serial]
fn bare_restore_into_empty_adopts_the_derived_source_identity() {
    // The other entry point the empty identity used to reach:
    // --restore-into-empty adopts the staged identity as the recovering
    // workspace's own, so a bare interchange used to reset the workspace's
    // uuid to the empty string — poisoning the pointer, the restore
    // receipt's source and target, and every generation after. With the
    // fix the workspace adopts the derived identity instead.
    let temp_dir = tempfile::Builder::new()
        .prefix("bead-test-")
        .tempdir_in("/var/tmp")
        .unwrap();
    let workspace = temp_dir.path().to_path_buf();

    bead(&workspace)
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    let import_path = workspace.join("issues.jsonl");
    fs::write(&import_path, BARE_ISSUES_JSONL).unwrap();

    bead(&workspace)
        .args([
            "sync",
            "import-only",
            "--input",
            import_path.to_str().unwrap(),
            "--restore-into-empty",
            "--actor",
            "bare-recovery-worker",
        ])
        .assert()
        .success();

    // The restore-into-empty import writes no summary event, so the store's
    // covered sequence still matches the init generation the pointer names
    // and `flush-only` alone would call the checkpoint current — the stale
    // init pointer would keep naming the pre-adoption identity. Mutate the
    // store once to advance the sequence, then publish the first
    // post-adoption generation; its pointer must name the adopted identity —
    // verify_restore_source refuses a pointer whose store_uuid is empty,
    // so an empty adoption would make even this generation unrestorable.
    bead(&workspace)
        .args([
            "create",
            "--title",
            "First mutation after the bare interchange adoption",
            "--issue-type",
            "task",
        ])
        .assert()
        .success();
    bead(&workspace)
        .args(["sync", "flush-only"])
        .assert()
        .success();

    let pointer: Value = serde_json::from_str(
        &fs::read_to_string(workspace.join(".beads/checkpoint/current.json")).unwrap(),
    )
    .unwrap();
    let generation = pointer["generation_id"].as_str().unwrap();
    assert_eq!(
        pointer["store_uuid"].as_str().unwrap_or_default(),
        bare_interchange_identity(),
        "the recovering workspace must adopt the derived identity as its own"
    );
    let sources = published_receipt_sources(&workspace);
    assert!(
        sources.iter().all(|source| !source.trim().is_empty()),
        "restore receipt published with an empty store identity: {sources:?}"
    );
    assert!(
        sources.contains(&bare_interchange_identity()),
        "restore receipt must name the derived identity, got {sources:?}"
    );

    // A second, fresh workspace restores that generation and inherits the
    // same identity — the bare interchange's provenance chain stays
    // convergent across recovery boundaries.
    let recovery_dir = tempfile::Builder::new()
        .prefix("bead-recovery-")
        .tempdir_in("/var/tmp")
        .unwrap();
    let recovery = recovery_dir.path().to_path_buf();
    bead(&recovery)
        .args(["init", "--prefix", "test"])
        .assert()
        .success();

    bead(&recovery)
        .args([
            "restore",
            "--source",
            workspace.join(".beads/checkpoint").to_str().unwrap(),
            "--generation",
            generation,
            "--actor",
            "recovery-worker",
            "--format",
            "json",
        ])
        .assert()
        .success();

    bead(&recovery)
        .args(["list", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("test-0000000000000001"))
        .stdout(predicate::str::contains("test-0000000000000002"));
}

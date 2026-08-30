//! R027 conformance: remote-advanced checkpoint reconcile.
//!
//! The Git-transported workflow commits `.beads/checkpoint/` and not
//! `.beads/beads.db`, so after one clone publishes and another pulls, the
//! durable checkpoint can contain work the live database does not. These
//! scenarios pin the state taxonomy and the `bead sync reconcile` contract
//! from `research/specs/remote-advanced-reconcile-v1.md`: only a verified
//! pointer whose event stream is a superset of live state is
//! remote-advanced, every other covered-ahead-of-live shape stays a
//! fail-closed integrity failure, and no test ever runs Git — the pull is
//! a filesystem copy of the checkpoint set, which is all bead-rs observes.

use assert_cmd::Command;
use rusqlite::Connection;
use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
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

fn status(dir: &Path) -> Value {
    let output = run(dir, &["sync", "status", "--format", "json"]);
    serde_json::from_slice(&output.stdout).unwrap()
}

fn relationship(dir: &Path) -> String {
    status(dir)["relationship"].as_str().unwrap().to_string()
}

fn pointer(dir: &Path) -> Value {
    serde_json::from_slice(&fs::read(dir.join(".beads/checkpoint/current.json")).unwrap()).unwrap()
}

/// Replace one workspace's checkpoint set with another's, the filesystem
/// equivalent of what `git pull` delivers. The receiving database is left
/// untouched: the whole point of the remote-advanced state is that the
/// pointer moved and the live store did not.
fn pull_checkpoint(from: &Path, to: &Path) {
    fs::remove_dir_all(to.join(".beads/checkpoint")).unwrap();
    copy_tree(
        &from.join(".beads/checkpoint"),
        &to.join(".beads/checkpoint"),
    );
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let source = entry.path();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&source, &target);
        } else {
            fs::copy(source, target).unwrap();
        }
    }
}

/// Open the live store for direct assertions the CLI does not surface.
fn live_db(dir: &Path) -> Connection {
    Connection::open(dir.join(".beads/beads.db")).unwrap()
}

/// Every wire identity in the live events table, with a NULL-origin count
/// so canonicalization (or its absence) is observable.
fn live_identities(dir: &Path) -> (Vec<(String, i64)>, usize) {
    let conn = live_db(dir);
    let mut stmt = conn
        .prepare(
            "SELECT origin_store_uuid, origin_event_sequence
             FROM events ORDER BY sequence ASC",
        )
        .unwrap();
    let mut identities = Vec::new();
    let mut null_origin = 0;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, Option<String>>(0)?,
                row.get::<_, Option<i64>>(1)?,
            ))
        })
        .unwrap();
    for row in rows {
        let (uuid, sequence) = row.unwrap();
        match (uuid, sequence) {
            (Some(uuid), Some(sequence)) => identities.push((uuid, sequence)),
            _ => null_origin += 1,
        }
    }
    (identities, null_origin)
}

/// Every event wire identity carried by the pointer-selected generation
/// object, in wire order.
fn generation_event_identities(dir: &Path) -> Vec<(String, i64)> {
    let pointer = pointer(dir);
    let root = pointer["active_root"]["path"].as_str().unwrap();
    let mut identities = Vec::new();
    for line in fs::read_to_string(dir.join(".beads/checkpoint").join(root))
        .unwrap()
        .lines()
    {
        let record: Value = serde_json::from_str(line).unwrap();
        if record["record_type"] == "event" {
            identities.push((
                record["event"]["origin_store_uuid"]
                    .as_str()
                    .unwrap()
                    .to_string(),
                record["event"]["origin_event_sequence"].as_i64().unwrap(),
            ));
        }
    }
    identities
}

/// Two workspaces sharing one store UUID, modeling one Git-transported
/// repository cloned on two machines. The clone is built exactly the way
/// the README's fresh-clone recovery recipe builds it — config and
/// checkpoint only, `bead init`, verified restore — and then advances
/// `advancements` further issues and publishes each one. The origin is
/// left lagging: its live store still holds only the shared history while
/// the clone's published checkpoint (copied back into the origin by
/// [`pull_checkpoint`]) is ahead of it.
struct LaggingPair {
    _origin_dir: TempDir,
    origin: std::path::PathBuf,
    _clone_dir: TempDir,
    #[allow(dead_code)]
    clone: std::path::PathBuf,
    shared_issue: String,
    advancement: Vec<String>,
}

fn lagging_pair(advancements: usize) -> LaggingPair {
    let origin_dir = tempfile::tempdir().unwrap();
    let origin = origin_dir.path().to_path_buf();
    run(&origin, &["init", "--prefix", "r027"]);
    let shared_issue = create_issue(&origin, "shared history");
    run(&origin, &["sync", "flush-only"]);

    let clone_dir = tempfile::tempdir().unwrap();
    let clone = clone_dir.path().join("clone");
    fs::create_dir_all(clone.join(".beads")).unwrap();
    fs::copy(
        origin.join(".beads/config.json"),
        clone.join(".beads/config.json"),
    )
    .unwrap();
    copy_tree(
        &origin.join(".beads/checkpoint"),
        &clone.join(".beads/checkpoint"),
    );
    run(&clone, &["init"]);
    let generation = pointer(&clone)["generation_id"]
        .as_str()
        .unwrap()
        .to_string();
    run(
        &clone,
        &[
            "restore",
            "--source",
            ".beads/checkpoint",
            "--generation",
            &generation,
            "--actor",
            "clone-operator",
        ],
    );

    let mut advancement = Vec::new();
    for index in 0..advancements {
        advancement.push(create_issue(&clone, &format!("advancement {index}")));
    }

    LaggingPair {
        _origin_dir: origin_dir,
        origin,
        _clone_dir: clone_dir,
        clone,
        shared_issue,
        advancement,
    }
}

/// A lagging pair with the clone's checkpoint already pulled into the
/// origin: the canonical remote-advanced state.
fn remote_advanced_pair(advancements: usize) -> LaggingPair {
    let pair = lagging_pair(advancements);
    pull_checkpoint(&pair.clone, &pair.origin);
    assert_eq!(relationship(&pair.origin), "remote-advanced");
    pair
}

/// Tamper with the pointer-selected root object without breaking its
/// JSONL shape, so the first failed qualifier is the root hash mismatch.
fn tamper_active_root(dir: &Path) {
    let root = pointer(dir)["active_root"]["path"]
        .as_str()
        .unwrap()
        .to_string();
    let path = dir.join(".beads/checkpoint").join(&root);
    let mut bytes = fs::read(&path).unwrap();
    bytes.push(b' ');
    fs::write(path, bytes).unwrap();
}

/// A workspace whose live store has advanced beyond a checkpoint from an
/// unrelated store UUID, with covered strictly greater than live.
fn foreign_checkpoint_target() -> TempDir {
    let target = tempfile::tempdir().unwrap();
    let target_path = target.path().to_path_buf();
    run(&target_path, &["init", "--prefix", "r027"]);
    create_issue(&target_path, "local history");

    let foreign = tempfile::tempdir().unwrap();
    let foreign_path = foreign.path().to_path_buf();
    run(&foreign_path, &["init", "--prefix", "othr"]);
    create_issue(&foreign_path, "foreign one");
    create_issue(&foreign_path, "foreign two");
    run(&foreign_path, &["sync", "flush-only"]);

    pull_checkpoint(&foreign_path, &target_path);
    target
}

#[test]
fn pull_yields_remote_advanced_relationship_in_text_and_json() {
    let pair = remote_advanced_pair(0);
    let report = status(&pair.origin);

    assert_eq!(report["relationship"], "remote-advanced");
    let covered = report["covered_sequence"].as_i64().unwrap();
    let live = report["live_sequence"].as_i64().unwrap();
    assert!(covered > live, "covered {covered} must exceed live {live}");
    assert_eq!(report["ready_to_commit"], false);
    let reasons: Vec<&str> = report["not_ready_reasons"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|reason| reason.as_str())
        .collect();
    assert!(
        reasons
            .iter()
            .any(|reason| reason.contains("bead sync reconcile")),
        "the remote-advanced reasons must name the reconcile remedy: {reasons:?}"
    );

    let text = String::from_utf8(run(&pair.origin, &["sync", "status"]).stdout).unwrap();
    assert!(
        text.contains("Relationship: remote-advanced"),
        "text status must report the relationship: {text}"
    );
}

#[test]
fn reconcile_merges_publishes_and_becomes_aligned() {
    let pair = remote_advanced_pair(1);
    let advancement = pair.advancement[0].clone();

    let output = run(&pair.origin, &["sync", "reconcile", "--actor", "jed"]);
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("Reconcile completed"), "{text}");
    assert_eq!(relationship(&pair.origin), "aligned");

    // The pulled advancement is live: the issue exists and shows its events.
    let shown = String::from_utf8(run(&pair.origin, &["show", &advancement]).stdout).unwrap();
    assert!(
        shown.contains(&advancement),
        "the reconciled issue must be present: {shown}"
    );

    // The merge receipt is attributed to the reconcile actor.
    let conn = live_db(&pair.origin);
    let (kind, actor): (String, String) = conn
        .query_row(
            "SELECT kind, actor FROM provenance_receipts
             WHERE kind = 'merge' ORDER BY rowid DESC LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(kind, "merge");
    assert_eq!(actor, "jed");

    // Under the automatic publication default the post-commit chokepoint
    // published the generation covering the merge: covered equals live and
    // the workspace is ready to commit.
    let report = status(&pair.origin);
    assert_eq!(report["covered_sequence"], report["live_sequence"]);
    assert_eq!(report["dirty"], false);
    assert_eq!(report["ready_to_commit"], true);

    // Idempotent at the state level: a second invocation refuses.
    let second = bead(&pair.origin)
        .args(["sync", "reconcile", "--actor", "jed"])
        .assert()
        .failure()
        .code(2)
        .get_output()
        .clone();
    assert!(
        String::from_utf8(second.stderr)
            .unwrap()
            .contains("nothing to reconcile"),
        "second reconcile must refuse with nothing to reconcile"
    );
}

#[test]
fn reconcile_with_publication_suppressed_leaves_behind_for_flush_only() {
    let pair = remote_advanced_pair(0);

    run(
        &pair.origin,
        &["--no-auto-flush", "sync", "reconcile", "--actor", "jed"],
    );

    // The merge committed but no generation was published: the workspace is
    // dirty and behind its own pointer-less state, exactly like any other
    // committed mutation with publication suppressed.
    assert_eq!(relationship(&pair.origin), "behind");
    run(&pair.origin, &["sync", "flush-only"]);
    assert_eq!(relationship(&pair.origin), "aligned");
}

#[test]
fn reconcile_and_import_merge_leave_one_row_per_wire_identity() {
    // Reconcile path.
    let pair = remote_advanced_pair(0);
    run(&pair.origin, &["sync", "reconcile", "--actor", "jed"]);

    let (identities, null_origin) = live_identities(&pair.origin);
    assert!(
        identities.iter().collect::<HashSet<_>>().len() == identities.len(),
        "every wire identity must appear exactly once in the live store: {identities:?}"
    );
    // Canonicalization runs before the import, so the only row that may
    // still carry NULL origin columns is the merge summary event the merge
    // itself wrote afterwards.
    assert_eq!(
        null_origin, 1,
        "only the merge summary event may remain NULL-origin after canonicalization"
    );

    let published = generation_event_identities(&pair.origin);
    assert!(
        published.iter().collect::<HashSet<_>>().len() == published.len(),
        "every wire identity must appear exactly once in the published generation"
    );
    // The published generation derives the summary event's identity too, so
    // it carries every explicit identity plus that one.
    assert_eq!(published.len(), identities.len() + null_origin);

    // The equivalent `sync import-only --merge` of the same shape must not
    // duplicate either (spec, "Local-identity canonicalization").
    let second = remote_advanced_pair(0);
    run(
        &second.origin,
        &[
            "sync",
            "import-only",
            "--input",
            ".beads/checkpoint",
            "--merge",
            "--actor",
            "import-operator",
        ],
    );
    let (imported, null_after_import) = live_identities(&second.origin);
    assert!(
        imported.iter().collect::<HashSet<_>>().len() == imported.len(),
        "import-only --merge must not duplicate wire identities: {imported:?}"
    );
    assert_eq!(null_after_import, 1, "only the merge summary event");
}

#[test]
fn dry_run_reconcile_mutates_nothing() {
    let pair = remote_advanced_pair(1);

    let live_before = status(&pair.origin)["live_sequence"].as_i64().unwrap();
    let pointer_before = fs::read(pair.origin.join(".beads/checkpoint/current.json")).unwrap();
    let receipts_before: i64 = live_db(&pair.origin)
        .query_row("SELECT COUNT(*) FROM provenance_receipts", [], |row| {
            row.get(0)
        })
        .unwrap();

    let output = run(
        &pair.origin,
        &["sync", "reconcile", "--actor", "jed", "--dry-run"],
    );
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("Dry-run reconcile analysis"), "{text}");

    assert_eq!(
        status(&pair.origin)["live_sequence"].as_i64().unwrap(),
        live_before
    );
    assert_eq!(
        fs::read(pair.origin.join(".beads/checkpoint/current.json")).unwrap(),
        pointer_before,
        "dry-run must leave the checkpoint pointer untouched"
    );
    let receipts_after: i64 = live_db(&pair.origin)
        .query_row("SELECT COUNT(*) FROM provenance_receipts", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(receipts_after, receipts_before);
    assert_eq!(relationship(&pair.origin), "remote-advanced");
}

#[test]
fn reconcile_refuses_behind_aligned_and_absent_with_exit_2() {
    // behind: live has unflushed work; the refusal names flush-only.
    let behind = tempfile::tempdir().unwrap();
    let behind_path = behind.path().to_path_buf();
    run(&behind_path, &["init", "--prefix", "r027"]);
    create_issue(&behind_path, "published");
    run(
        &behind_path,
        &["--no-auto-flush", "create", "--title", "unflushed"],
    );
    assert_eq!(relationship(&behind_path), "behind");
    let output = bead(&behind_path)
        .args(["sync", "reconcile", "--actor", "jed"])
        .assert()
        .failure()
        .code(2)
        .get_output()
        .clone();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("flush-only"),
        "the behind refusal must name flush-only: {stderr}"
    );

    // aligned: nothing to reconcile.
    let aligned = tempfile::tempdir().unwrap();
    let aligned_path = aligned.path().to_path_buf();
    run(&aligned_path, &["init", "--prefix", "r027"]);
    create_issue(&aligned_path, "covered");
    assert_eq!(relationship(&aligned_path), "aligned");
    bead(&aligned_path)
        .args(["sync", "reconcile", "--actor", "jed"])
        .assert()
        .failure()
        .code(2);

    // absent: `bead init --no-auto-flush` publishes nothing, so the workspace
    // has no checkpoint at all.
    let absent = tempfile::tempdir().unwrap();
    let absent_path = absent.path().to_path_buf();
    run(
        &absent_path,
        &["init", "--prefix", "r027", "--no-auto-flush"],
    );
    assert_eq!(relationship(&absent_path), "absent");
    bead(&absent_path)
        .args(["sync", "reconcile", "--actor", "jed"])
        .assert()
        .failure()
        .code(2);
}

#[test]
fn reconcile_refuses_tampered_root_with_exit_5() {
    let pair = remote_advanced_pair(0);
    tamper_active_root(&pair.origin);
    assert_eq!(
        relationship(&pair.origin),
        "covered-ahead-integrity-failure"
    );

    let output = bead(&pair.origin)
        .args(["sync", "reconcile", "--actor", "jed"])
        .assert()
        .failure()
        .code(5)
        .get_output()
        .clone();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("verified pointer") && stderr.contains("hash"),
        "the refusal must name the failed qualifier: {stderr}"
    );

    // The refusal mutates nothing, including the tampered evidence.
    assert_eq!(
        relationship(&pair.origin),
        "covered-ahead-integrity-failure"
    );
}

#[test]
fn reconcile_refuses_foreign_pointer_uuid_with_exit_5() {
    let target = foreign_checkpoint_target();
    assert_eq!(
        relationship(target.path()),
        "covered-ahead-integrity-failure"
    );

    let output = bead(target.path())
        .args(["sync", "reconcile", "--actor", "jed"])
        .assert()
        .failure()
        .code(5)
        .get_output()
        .clone();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("same origin"),
        "a foreign-UUID checkpoint must fail the same-origin qualifier: {stderr}"
    );
}

#[test]
fn flush_only_refuses_remote_advanced_with_exit_4() {
    let pair = remote_advanced_pair(0);
    let pointer_before = fs::read(pair.origin.join(".beads/checkpoint/current.json")).unwrap();
    let objects_before: Vec<String> = fs::read_dir(pair.origin.join(".beads/checkpoint/objects"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
        .collect();

    let output = bead(&pair.origin)
        .args(["sync", "flush-only"])
        .assert()
        .failure()
        .code(4)
        .get_output()
        .clone();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("bead sync reconcile"),
        "the remote-advanced flush refusal must name reconcile: {stderr}"
    );

    // The pointer and its objects are left untouched.
    assert_eq!(
        fs::read(pair.origin.join(".beads/checkpoint/current.json")).unwrap(),
        pointer_before
    );
    let objects_after: Vec<String> = fs::read_dir(pair.origin.join(".beads/checkpoint/objects"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(objects_after, objects_before);
}

#[test]
fn flush_only_refuses_integrity_failure_with_exit_5() {
    let pair = remote_advanced_pair(0);
    tamper_active_root(&pair.origin);
    let pointer_before = fs::read(pair.origin.join(".beads/checkpoint/current.json")).unwrap();

    let output = bead(&pair.origin)
        .args(["sync", "flush-only"])
        .assert()
        .failure()
        .code(5)
        .get_output()
        .clone();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("covered-ahead integrity failure"),
        "the refusal must name the integrity failure: {stderr}"
    );
    assert_eq!(
        fs::read(pair.origin.join(".beads/checkpoint/current.json")).unwrap(),
        pointer_before
    );
}

#[test]
fn doctor_distinguishes_remote_advanced_from_integrity_failure() {
    // Remote-advanced: an actionable diagnostic carrying the stable state
    // marker and the reconcile remedy, not an integrity failure.
    let pair = remote_advanced_pair(0);
    let output = run(&pair.origin, &["doctor", "--json"]);
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    let freshness = report["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|check| check["name"] == "checkpoint_freshness")
        .unwrap();
    assert_eq!(freshness["status"], "warning");
    assert_eq!(freshness["details"]["state"], "remote-advanced");
    assert!(freshness["details"]["remedy"]
        .as_str()
        .unwrap()
        .contains("bead sync reconcile"));
    let text_output = bead(&pair.origin)
        .args(["doctor"])
        .assert()
        .success()
        .get_output()
        .clone();
    let text = String::from_utf8(text_output.stderr).unwrap();
    assert!(text.contains("remote-advanced"), "{text}");

    // Covered-ahead integrity failure: names the failed qualifier instead,
    // and the two outputs differ.
    let tampered = remote_advanced_pair(0);
    tamper_active_root(&tampered.origin);
    let tampered_output = bead(&tampered.origin)
        .args(["doctor", "--json"])
        .assert()
        .success()
        .get_output()
        .clone();
    let tampered_report: Value = serde_json::from_slice(&tampered_output.stdout).unwrap();
    let tampered_freshness = tampered_report["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|check| check["name"] == "checkpoint_freshness")
        .unwrap();
    assert!(
        tampered_freshness["message"]
            .as_str()
            .unwrap()
            .contains("covered-ahead integrity failure"),
        "{}",
        tampered_freshness["message"]
    );
    assert!(tampered_freshness["details"]["state"].is_null());
}

#[test]
fn divergence_stays_fail_closed_across_reconcile_flush_and_doctor() {
    // A local mutation committed while remote-advanced leaves a live event
    // the pulled checkpoint lacks: same-store divergence with no common
    // extension. Everything refuses, and nothing is merged or published.
    let pair = remote_advanced_pair(1);
    run(
        &pair.origin,
        &["--no-auto-flush", "create", "--title", "divergent"],
    );
    assert_eq!(
        relationship(&pair.origin),
        "covered-ahead-integrity-failure"
    );

    let reconcile = bead(&pair.origin)
        .args(["sync", "reconcile", "--actor", "jed"])
        .assert()
        .failure()
        .code(5)
        .get_output()
        .clone();
    assert!(
        String::from_utf8(reconcile.stderr)
            .unwrap()
            .contains("event-stream superset"),
        "the refusal must name the superset qualifier"
    );

    bead(&pair.origin)
        .args(["sync", "flush-only"])
        .assert()
        .failure()
        .code(5);

    let doctor_output = bead(&pair.origin)
        .args(["doctor"])
        .assert()
        .success()
        .get_output()
        .clone();
    let text = String::from_utf8(doctor_output.stderr).unwrap();
    assert!(
        text.contains("covered-ahead integrity failure"),
        "doctor must report the integrity failure: {text}"
    );
    assert!(
        !text.contains("remote-advanced"),
        "divergence must not be diagnosed as the benign state: {text}"
    );
}

#[test]
fn shared_history_survives_reconcile_as_prefix() {
    // The reconciled store keeps the shared history byte-identical at the
    // wire-identity level: every pre-pull live identity is still present
    // with the same identity, and the pulled suffix extends it.
    let pair = lagging_pair(1);
    let (before, _) = live_identities(&pair.origin);
    pull_checkpoint(&pair.clone, &pair.origin);
    run(&pair.origin, &["sync", "reconcile", "--actor", "jed"]);

    let (after, _) = live_identities(&pair.origin);
    for identity in &before {
        assert!(
            after.contains(identity),
            "reconcile must not displace existing identities: {identity:?}"
        );
    }
    assert!(after.len() > before.len());
    assert_eq!(
        status(&pair.origin)["covered_sequence"],
        status(&pair.origin)["live_sequence"]
    );

    // The shared issue is untouched by the merge.
    let shown = String::from_utf8(run(&pair.origin, &["show", &pair.shared_issue]).stdout).unwrap();
    assert!(shown.contains(&pair.shared_issue), "{shown}");
}

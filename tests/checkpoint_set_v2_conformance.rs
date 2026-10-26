//! Checkpoint Set v2 conformance scenarios (research/specs/checkpoint-set-v2.md)
//!
//! The set-v2 writer is normative for the adaptive layout; these tests bind
//! the publication behavior the specification names:
//!
//! - `set-v2-budget-single-issue-update` (sections 11, 12.3): a 3,000-issue
//!   workspace's single-issue publications stay within the 256 KiB new-blob
//!   budget, measured with the section 11 `git rev-list --objects` plus
//!   `git cat-file --batch-check='%(objecttype) %(objectsize) %(rest)'`
//!   procedure (the `%(rest)` field is required, or the size lookup
//!   silently returns nothing).
//! - `set-v2-shard-split-on-update-storm` (12.4): a prefix class pushed past
//!   `SHARD_SPLIT` splits, children stay within the bound, coverage is
//!   unchanged, and an equivalent re-publication is byte-identical.
//! - `set-v2-shard-merge-on-deletion` (12.5): a full-sibling group shrunk to
//!   `SHARD_MERGE` or below coalesces into the parent prefix.
//! - `set-v2-tail-seal-and-immutability` (12.6): a tail pushed past
//!   `SEG_SEAL` seals to its content address and never changes again.
//! - `set-v2-oversize-record-once-cost` (12.7): a record above
//!   `RECORD_INLINE` lands in `set/rec/` exactly once.
//! - `set-v2-pointer-boundedness` (12.8): pointers stay under 4 KiB across
//!   fifty-plus publications and the manifest carries no record payload.
//! - `set-v2-forensic-frozen` (12.9): `forensic.jsonl` is never written, and
//!   restore succeeds with the file absent.
//! - `set-v2-crash-ordering` (12.10): orphans from an interrupted
//!   publication are reaped by the next one, and the pointer swap leaves a
//!   complete, verifiable generation on both sides of the crash window.
//! - `set-v2-migration-full-rewrite` (12.1): a monolithic workspace at the
//!   safety limits' operating point and a sharded-v1 workspace (from the
//!   committed fixture, since the writer no longer produces sharded-v1)
//!   each publish once under adaptive mode; `sync --status` reported the
//!   pending full rewrite beforehand, the publication writes the complete
//!   `set/` tree, and the old generation remains restorable via
//!   `previous.json`.
//! - `set-v2-dual-layout-reader-equivalence` (12.2): restore into an empty
//!   store, fork import, reconcile, and verified restore each succeed from
//!   a monolithic source and from a `set-v2` source of the same store,
//!   with the restored stores passing v1 restore-equivalence against the
//!   source.
//!
//! Fixtures drive the real `bead` binary over real workspaces so the
//! scenarios exercise the publication path end to end: `bead init`, CLI
//! mutations, `sync flush-only` publications, and (for the budget
//! scenario) git commits measured with the section 11 procedure. Record
//! removal for the merge scenario goes through the store database directly,
//! the same accepted fixture pattern as the direct-inserted unknown fields
//! in the round-trip conformance test.
//!
//! The out-of-band fixtures (beadrs-15cca551) generalize the two red
//! scenarios' interference — a direct store deletion and orphan `set/`
//! artifacts — so any baseline-green scenario can be re-run against it
//! (`set_v2_green_scenarios_under_out_of_band_change`). That grid
//! characterizes which scenarios survive the change and records the exact
//! failure text for the rest, feeding the covered-ahead refusal fix in the
//! later slices of the beadrs-40f0cc4e auto-split.

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use std::io::Write as _;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;
use std::process::{Command, Output};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn bead_binary() -> String {
    env!("CARGO_BIN_EXE_bead").to_string()
}

fn run_bead(workspace: &Path, args: &[&str]) -> Output {
    Command::new(bead_binary())
        .args(args)
        .current_dir(workspace)
        .output()
        .expect("failed to run bead")
}

fn run_ok(workspace: &Path, args: &[&str]) {
    let output = run_bead(workspace, args);
    assert!(
        output.status.success(),
        "`bead {}` failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn init_workspace(dir: &Path) {
    fs::create_dir_all(dir).unwrap();
    // Test runners may place their temporary directory below an unrelated
    // ancestor `.beads` store; `--skip-foreign-workspace` is the documented
    // opt-in for deliberately isolated fixture workspaces.
    run_ok(dir, &["init", "--skip-foreign-workspace"]);
}

/// Merge a `checkpoint` section entry into the workspace's `.beads/config.json`
fn set_checkpoint_config(workspace: &Path, key: &str, value: Value) {
    let config_path = workspace.join(".beads/config.json");
    let mut config: Value = serde_json::from_str(&fs::read_to_string(&config_path).unwrap())
        .expect(".beads/config.json must exist after init");
    config
        .as_object_mut()
        .unwrap()
        .entry("checkpoint".to_string())
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .unwrap()
        .insert(key.to_string(), value);
    fs::write(&config_path, serde_json::to_string(&config).unwrap()).unwrap();
}

fn flush_ok(workspace: &Path) {
    run_ok(workspace, &["sync", "flush-only"]);
}

fn git_out(workspace: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(workspace)
        .env("GIT_AUTHOR_NAME", "test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .output()
        .expect("failed to run git");
    assert!(
        output.status.success(),
        "`git {}` failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

/// Commit everything under `.beads/` and return the new commit's object id
fn git_commit_checkpoint(workspace: &Path, message: &str) -> String {
    git_out(workspace, &["add", ".beads"]);
    git_out(workspace, &["commit", "-q", "-m", message]);
    git_out(workspace, &["rev-parse", "HEAD"])
        .trim()
        .to_string()
}

/// The section 11 measurement of one publication's new blob bytes
///
/// Canonical procedure for parent commit `parent` and publication commit
/// `commit`: the object ids added under `.beads/` by `commit`, size-looked
/// up with `%(rest)` carried through so paths survive the batch.
fn new_blob_bytes(workspace: &Path, parent: &str, commit: &str) -> u64 {
    let range = format!("{}..{}", parent, commit);
    let new_objects = git_out(
        workspace,
        &["rev-list", "--objects", &range, "--", ".beads/"],
    );
    let old_objects = git_out(
        workspace,
        &["rev-list", "--objects", parent, "--", ".beads/"],
    );
    let old_ids: HashSet<&str> = old_objects
        .lines()
        .filter_map(|line| line.split(' ').next())
        .collect();
    let added: Vec<&str> = new_objects
        .lines()
        .filter(|line| match line.split(' ').next() {
            Some(id) => !old_ids.contains(id),
            None => false,
        })
        .collect();
    if added.is_empty() {
        return 0;
    }

    let mut batch = Command::new("git")
        .args([
            "cat-file",
            "--batch-check=%(objecttype) %(objectsize) %(rest)",
        ])
        .current_dir(workspace)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("failed to run git cat-file");
    batch
        .stdin
        .take()
        .unwrap()
        .write_all(added.join("\n").as_bytes())
        .unwrap();
    let out = batch.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "git cat-file failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| {
            let mut fields = line.split(' ');
            let object_type = fields.next()?;
            let size: u64 = fields.next()?.parse().ok()?;
            if object_type == "blob" {
                Some(size)
            } else {
                None
            }
        })
        .sum()
}

fn read_pointer(workspace: &Path) -> Value {
    serde_json::from_str(
        &fs::read_to_string(workspace.join(".beads/checkpoint/current.json")).unwrap(),
    )
    .unwrap()
}

fn read_manifest(workspace: &Path, pointer: &Value) -> Value {
    let rel = pointer["active_root"]["path"].as_str().unwrap();
    let body = fs::read_to_string(workspace.join(".beads/checkpoint").join(rel)).unwrap();
    serde_json::from_str(&body).unwrap()
}

fn current_manifest(workspace: &Path) -> Value {
    let pointer = read_pointer(workspace);
    read_manifest(workspace, &pointer)
}

fn sha256_file(path: &Path) -> String {
    format!("{:x}", Sha256::digest(fs::read(path).unwrap().as_slice()))
}

fn live_issue_ids(workspace: &Path) -> Vec<String> {
    let out = run_bead(workspace, &["list", "--json", "--limit", "999999"]);
    assert!(
        out.status.success(),
        "list failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice::<Vec<Value>>(&out.stdout)
        .expect("list --json emits one JSON array")
        .into_iter()
        .map(|issue| issue["id"].as_str().unwrap().to_string())
        .collect()
}

fn set_notes(workspace: &Path, id: &str, notes: &str) {
    run_ok(workspace, &["update", id, "--notes", notes]);
}

/// Replace an issue's notes by piping the text through `--notes-file -`, so
/// an oversize payload never has to fit in the argument list
fn set_notes_via_stdin(workspace: &Path, id: &str, notes: &str) {
    let mut child = Command::new(bead_binary())
        .args(["update", id, "--notes-file", "-"])
        .current_dir(workspace)
        .stdin(std::process::Stdio::piped())
        .spawn()
        .expect("failed to run bead update --notes-file -");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(notes.as_bytes())
        .unwrap();
    let status = child.wait().expect("failed to wait for bead update");
    assert!(status.success(), "oversize notes update failed");
}

/// A long line of low-entropy filler sized to `bytes`
fn filler(prefix: &str, bytes: usize) -> String {
    let mut text = String::with_capacity(bytes);
    while text.len() < bytes {
        text.push_str(prefix);
        for i in 0..20 {
            text.push_str(&format!(" word{}", i));
        }
    }
    text.truncate(bytes);
    text
}

/// A fresh set-v2 workspace with `count` issues and auto-flush disabled, so
/// fixture setup pays one publication instead of `count`
fn fixture_with_issues(count: usize, title_detail: &str) -> TempDir {
    let workspace = TempDir::new().unwrap();
    init_workspace(workspace.path());
    set_checkpoint_config(workspace.path(), "auto_flush", json!(false));
    for i in 0..count {
        run_ok(
            workspace.path(),
            &[
                "create",
                "--title",
                &format!("{} issue {} {}", title_detail, i, filler("filler", 40)),
            ],
        );
    }
    flush_ok(workspace.path());
    workspace
}

/// Delete issues directly in the store database (no CLI deletion exists);
/// the accepted fixture pattern for states no command produces
fn delete_issues_directly(workspace: &Path, ids: &[String]) {
    let db = rusqlite::Connection::open(workspace.join(".beads/beads.db")).unwrap();
    for id in ids {
        db.execute("DELETE FROM issues WHERE id = ?1", [id])
            .unwrap();
    }
}

// ---------------------------------------------------------------------------
// out-of-band-change fixtures (beadrs-15cca551)
// ---------------------------------------------------------------------------

/// A store or checkpoint change no `bead` command produced — the shape the
/// two red scenarios (shard-merge-on-deletion, crash-ordering) introduce
/// between publications. Generalized here so any baseline-green scenario can
/// be re-run against the same interference.
#[derive(Clone)]
enum OutOfBandChange {
    /// Direct `DELETE FROM issues` on the first `count` live issues: the
    /// store loses records with no accompanying event. This is the merge
    /// scenario's delete-storm mechanism (which targets a chosen id list).
    StoreDeletion { count: usize },
    /// Orphan artifacts written straight under `set/`, as an interrupted
    /// publication (a crash before the section 7.4 pointer swap) leaves
    /// them: one unreferenced file in each artifact directory.
    OrphanSetFiles,
}

impl OutOfBandChange {
    fn label(&self) -> &'static str {
        match self {
            OutOfBandChange::StoreDeletion { .. } => "store-deletion",
            OutOfBandChange::OrphanSetFiles => "orphan-set-files",
        }
    }

    /// Apply the change to `workspace`, returning how many live issues were
    /// removed from the store (zero for changes that leave the store alone)
    fn apply(&self, workspace: &Path) -> usize {
        match self {
            OutOfBandChange::StoreDeletion { count } => {
                let ids: Vec<String> = live_issue_ids(workspace).into_iter().take(*count).collect();
                assert!(
                    !ids.is_empty(),
                    "store-deletion fixture found no live issues to remove"
                );
                delete_issues_directly(workspace, &ids);
                ids.len()
            }
            OutOfBandChange::OrphanSetFiles => {
                inject_orphan_set_files(&workspace.join(".beads/checkpoint"));
                0
            }
        }
    }
}

/// Write the crash-ordering fixture's orphan artifacts: one unreferenced
/// file per `set/` artifact directory, each named like a content address so
/// only a manifest reference distinguishes it from a live artifact
fn inject_orphan_set_files(checkpoint_dir: &Path) {
    for orphan_dir in ["set/shard", "set/seg", "set/rec"] {
        fs::create_dir_all(checkpoint_dir.join(orphan_dir)).unwrap();
    }
    fs::write(
        checkpoint_dir.join(
            "set/shard/0000000000000000000000000000000000000000000000000000000000000000.jsonl",
        ),
        b"orphan shard\n",
    )
    .unwrap();
    fs::write(
        checkpoint_dir
            .join("set/seg/1111111111111111111111111111111111111111111111111111111111111111.jsonl"),
        b"orphan segment\n",
    )
    .unwrap();
    fs::write(
        checkpoint_dir
            .join("set/rec/2222222222222222222222222222222222222222222222222222222222222222.jsonl"),
        b"orphan record\n",
    )
    .unwrap();
}

/// Apply `out_of_band` (if any) between a fixture's base publication and
/// its scenario driver: the driver's first flush is then the first
/// publication after the change, which is where the red pair's covered-ahead
/// integrity refusal fires. Returns the number of live issues the change
/// removed from the store.
fn apply_out_of_band(workspace: &Path, out_of_band: Option<&OutOfBandChange>) -> usize {
    out_of_band.map_or(0, |change| change.apply(workspace))
}

/// The manifest's issue-shard rows
fn issue_shards(manifest: &Value) -> Vec<Value> {
    manifest["shards"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["class"] == "issue")
        .cloned()
        .collect()
}

// ---------------------------------------------------------------------------
// set-v2-budget-single-issue-update (sections 11, 12.3)
// ---------------------------------------------------------------------------

#[test]
fn set_v2_budget_single_issue_update() {
    run_scenario("set-v2-budget-single-issue-update", None);
}

fn budget_single_issue_update(out_of_band: Option<&OutOfBandChange>) {
    let workspace = fixture_with_issues(3000, "budget");
    let workspace = workspace.path();
    let removed = apply_out_of_band(workspace, out_of_band);

    let first = read_pointer(workspace);
    assert_eq!(first["mode"], "set-v2", "adaptive publishes set-v2");

    git_out(workspace, &["init", "-q", "."]);
    let mut parent = git_commit_checkpoint(workspace, "checkpoint-set-v2 base generation");

    let ids = live_issue_ids(workspace);
    assert!(ids.len() + removed >= 3000);

    // 100 consecutive single-issue updates, each measured per section 11.
    // Different issues per round, matching the spec's consecutive-update
    // fixture; every publication must stay within the 256 KiB budget,
    // pointer files included.
    for round in 0..100 {
        let id = &ids[round % ids.len()];
        set_notes(
            workspace,
            id,
            &format!("budget round {} {}", round, filler("budget", 200)),
        );
        flush_ok(workspace);
        let commit = git_commit_checkpoint(workspace, &format!("budget publication {}", round));
        let bytes = new_blob_bytes(workspace, &parent, &commit);
        assert!(
            bytes <= 256 * 1024,
            "publication {} added {} new blob bytes, budget is {}",
            round,
            bytes,
            256 * 1024
        );
        parent = commit;
    }
}

// ---------------------------------------------------------------------------
// set-v2-shard-split-on-update-storm (12.4)
// ---------------------------------------------------------------------------

#[test]
fn set_v2_shard_split_on_update_storm() {
    run_scenario("set-v2-shard-split-on-update-storm", None);
}

fn shard_split_on_update_storm(out_of_band: Option<&OutOfBandChange>) {
    let workspace = fixture_with_issues(400, "storm");
    let workspace = workspace.path();
    let _removed = apply_out_of_band(workspace, out_of_band);

    let before = current_manifest(workspace);
    let before_rows = issue_shards(&before);
    let before_total: usize = before_rows
        .iter()
        .map(|row| row["record_count"].as_u64().unwrap() as usize)
        .sum();
    let before_count = before_rows.len();

    // Concentrate updates on the largest prefix class until it exceeds
    // SHARD_SPLIT (96 KiB): three ~40 KiB notes land in one shard while
    // staying under RECORD_INLINE (64 KiB) per record.
    let target = before_rows
        .iter()
        .max_by_key(|row| row["record_count"].as_u64().unwrap())
        .unwrap()
        .clone();
    let prefix = target["prefix"].as_str().unwrap().to_string();
    let live: Vec<String> = live_issue_ids(workspace)
        .into_iter()
        .filter(|id| id.starts_with(&prefix))
        .collect();
    assert!(live.len() >= 3, "target shard lacks three members");
    for id in live.iter().take(3) {
        set_notes(workspace, id, &filler("storm", 40 * 1024));
    }
    flush_ok(workspace);

    let after = current_manifest(workspace);
    let after_rows = issue_shards(&after);
    let after_total: usize = after_rows
        .iter()
        .map(|row| row["record_count"].as_u64().unwrap() as usize)
        .sum();

    // Split happened: strictly more issue shards than before.
    assert!(
        after_rows.len() > before_count,
        "expected the storm prefix class to split, {} shards before, {} after",
        before_count,
        after_rows.len()
    );
    // Coverage of all IDs is unchanged.
    assert_eq!(after_total, before_total, "shard coverage lost records");
    // Children at or under SHARD_SPLIT, except a shard may exceed it only
    // when it cannot split (a single record).
    for row in &after_rows {
        let bytes = row["byte_length"].as_u64().unwrap() as usize;
        let count = row["record_count"].as_u64().unwrap() as usize;
        assert!(
            bytes <= 96 * 1024 || count <= 1,
            "shard {} is {} bytes over SHARD_SPLIT with {} records",
            row["prefix"],
            bytes,
            count
        );
    }

    // Re-publication from an equivalent state produces byte-identical
    // children: the partition is a pure function of shard contents.
    flush_ok(workspace);
    let republished = current_manifest(workspace);
    let signature = |manifest: &Value| -> Vec<(String, String, u64, u64)> {
        issue_shards(manifest)
            .into_iter()
            .map(|row| {
                (
                    row["prefix"].as_str().unwrap().to_string(),
                    row["sha256"].as_str().unwrap().to_string(),
                    row["byte_length"].as_u64().unwrap(),
                    row["record_count"].as_u64().unwrap(),
                )
            })
            .collect()
    };
    assert_eq!(
        signature(&after),
        signature(&republished),
        "equivalent re-publication changed the shard set"
    );
}

// ---------------------------------------------------------------------------
// set-v2-shard-merge-on-deletion (12.5)
// ---------------------------------------------------------------------------

#[test]
fn set_v2_shard_merge_on_deletion() {
    let workspace = fixture_with_issues(400, "merge");
    let workspace = workspace.path();

    // Split one prefix class first (same storm driver).
    let manifest = current_manifest(workspace);
    let rows = issue_shards(&manifest);
    let target = rows
        .iter()
        .max_by_key(|row| row["record_count"].as_u64().unwrap())
        .unwrap()
        .clone();
    let prefix = target["prefix"].as_str().unwrap().to_string();
    let live: Vec<String> = live_issue_ids(workspace)
        .into_iter()
        .filter(|id| id.starts_with(&prefix))
        .collect();
    assert!(live.len() >= 10, "target shard too small to drive a merge");
    for id in live.iter().take(3) {
        set_notes(workspace, id, &filler("merge", 40 * 1024));
    }
    flush_ok(workspace);

    let split = current_manifest(workspace);
    let split_rows = issue_shards(&split);
    let split_children: Vec<&Value> = split_rows
        .iter()
        .filter(|row| {
            let p = row["prefix"].as_str().unwrap();
            p.starts_with(&prefix) && p.len() > prefix.len()
        })
        .collect();
    assert!(
        !split_children.is_empty(),
        "expected the prefix class to split before the merge"
    );

    // Delete records until the full-sibling group is at or below
    // SHARD_MERGE (24 KiB): keep only the last eight small members. The
    // delete storm is the `StoreDeletion` fixture's mechanism (direct
    // DELETE FROM issues) applied to a targeted id list.
    let delete: Vec<String> = live[..live.len() - 8].to_vec();
    delete_issues_directly(workspace, &delete);
    flush_ok(workspace);

    let merged = current_manifest(workspace);
    let merged_rows = issue_shards(&merged);
    // The group coalesced into the parent prefix: no deeper children of the
    // class remain, and the parent prefix row is back.
    let deeper: Vec<&Value> = merged_rows
        .iter()
        .filter(|row| {
            let p = row["prefix"].as_str().unwrap();
            p.starts_with(&prefix) && p.len() > prefix.len()
        })
        .collect();
    assert!(
        deeper.is_empty(),
        "expected the sibling group to coalesce into prefix {:?}, found {:?}",
        prefix,
        deeper
            .iter()
            .map(|r| r["prefix"].clone())
            .collect::<Vec<_>>()
    );
    let parent_row = merged_rows
        .iter()
        .find(|row| row["prefix"].as_str() == Some(prefix.as_str()))
        .expect("coalesced parent prefix row missing");
    assert!(
        parent_row["byte_length"].as_u64().unwrap() as usize <= 24 * 1024,
        "coalesced shard is above SHARD_MERGE"
    );

    // Coverage: shard rows account for every live issue.
    let total: usize = merged_rows
        .iter()
        .map(|row| row["record_count"].as_u64().unwrap() as usize)
        .sum();
    let remaining = live_issue_ids(workspace).len();
    assert_eq!(
        total, remaining,
        "shard rows account for {} records, store holds {}",
        total, remaining
    );

    // The retained set stays bounded by the two referenced generations
    // (spec 7.3): after the merge publication, nothing survives under
    // set/shard that neither current.json (the merged generation) nor
    // previous.json (the split generation) references. The split children
    // are exactly one generation old here, so they legitimately remain —
    // a split-then-merge is precisely the shape where consecutive
    // generations share almost no shard files, and a bound against the
    // current rows alone is unsatisfiable.
    let set_dir = workspace.join(".beads/checkpoint/set");
    let on_disk_shards = || -> HashSet<String> {
        fs::read_dir(set_dir.join("shard"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
            .collect()
    };
    let referenced_by = |manifest: &Value| -> HashSet<String> {
        issue_shards(manifest)
            .into_iter()
            .map(|row| {
                row["path"]
                    .as_str()
                    .unwrap()
                    .rsplit('/')
                    .next()
                    .unwrap()
                    .to_string()
            })
            .collect()
    };
    let merged_referenced = referenced_by(&merged);
    let split_referenced = referenced_by(&split);
    let retained: HashSet<String> = merged_referenced
        .union(&split_referenced)
        .cloned()
        .collect();
    let orphans: Vec<String> = on_disk_shards().difference(&retained).cloned().collect();
    assert!(
        orphans.is_empty(),
        "shard directory holds files referenced by neither the merged nor \
         the split generation: {:?}",
        orphans
    );

    // The next real publication slides the two-generation window past the
    // split children and reaps them (spec 7.3). The idempotent
    // short-circuit publishes nothing on an unchanged store, so the window
    // needs one genuine mutation to advance.
    set_notes(
        workspace,
        live.last().unwrap(),
        "merge retention-window driver",
    );
    flush_ok(workspace);
    let after = current_manifest(workspace);
    let after_referenced = referenced_by(&after);
    let on_disk = on_disk_shards();
    assert!(
        on_disk.len() <= after_referenced.len() + 2,
        "shard directory holds {} files for {} referenced rows after the \
         split generation left the retention window",
        on_disk.len(),
        after_referenced.len()
    );
    let reaped: Vec<String> = split_referenced
        .difference(&after_referenced)
        .filter(|name| on_disk.contains(*name))
        .cloned()
        .collect();
    assert!(
        reaped.is_empty(),
        "split children survived the publication that left them unreferenced: {:?}",
        reaped
    );
}

// ---------------------------------------------------------------------------
// set-v2-tail-seal-and-immutability (12.6)
// ---------------------------------------------------------------------------

#[test]
fn set_v2_tail_seal_and_immutability() {
    run_scenario("set-v2-tail-seal-and-immutability", None);
}

fn tail_seal_and_immutability(out_of_band: Option<&OutOfBandChange>) {
    let workspace = fixture_with_issues(50, "seal");
    let workspace = workspace.path();
    let _removed = apply_out_of_band(workspace, out_of_band);
    let ids = live_issue_ids(workspace);

    // Append until the tail passes SEG_SEAL (64 KiB): ~2.5 KiB events seal
    // within roughly thirty updates.
    let mut sealed_after = None;
    for round in 0..240 {
        set_notes(
            workspace,
            &ids[round % ids.len()],
            &format!("seal round {} {}", round, filler("seal", 2200)),
        );
        flush_ok(workspace);
        let manifest = current_manifest(workspace);
        if manifest["segments"].as_array().map(|s| !s.is_empty()) == Some(true) {
            sealed_after = Some(round);
            break;
        }
    }
    let sealed_after = sealed_after.expect("tail never sealed past SEG_SEAL");

    let manifest = current_manifest(workspace);
    let sealed: Vec<(String, String, u64)> = manifest["segments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            (
                row["path"].as_str().unwrap().to_string(),
                row["sha256"].as_str().unwrap().to_string(),
                row["byte_length"].as_u64().unwrap(),
            )
        })
        .collect();
    assert!(!sealed.is_empty());

    // A sealed segment is immutable for the life of the workspace: its
    // name, digest, and bytes are unchanged across at least five further
    // publications.
    for round in sealed_after..sealed_after + 6 {
        set_notes(
            workspace,
            &ids[round % ids.len()],
            &format!("post-seal round {} {}", round, filler("post", 80)),
        );
        flush_ok(workspace);
        let manifest = current_manifest(workspace);
        let now: Vec<(String, String, u64)> = manifest["segments"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| {
                (
                    row["path"].as_str().unwrap().to_string(),
                    row["sha256"].as_str().unwrap().to_string(),
                    row["byte_length"].as_u64().unwrap(),
                )
            })
            .collect();
        for (path, sha, len) in &sealed {
            let row = now
                .iter()
                .find(|(p, s, _)| p == path && s == sha)
                .unwrap_or_else(|| panic!("sealed segment {} vanished or changed digest", path));
            assert_eq!(&row.2, len, "sealed segment {} changed length", path);
            let bytes = fs::read(workspace.join(".beads/checkpoint").join(path)).unwrap();
            assert_eq!(
                format!("{:x}", Sha256::digest(&bytes)),
                *sha,
                "sealed segment {} bytes changed on disk",
                path
            );
        }
    }

    // The manifest never records two sealed segments with the same digest.
    let manifest = current_manifest(workspace);
    let digests: Vec<&str> = manifest["segments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["sha256"].as_str().unwrap())
        .collect();
    let unique: HashSet<&str> = digests.iter().copied().collect();
    assert_eq!(
        digests.len(),
        unique.len(),
        "duplicate sealed segment digest"
    );
}

// ---------------------------------------------------------------------------
// set-v2-oversize-record-once-cost (12.7)
// ---------------------------------------------------------------------------

#[test]
fn set_v2_oversize_record_once_cost() {
    run_scenario("set-v2-oversize-record-once-cost", None);
}

fn oversize_record_once_cost(out_of_band: Option<&OutOfBandChange>) {
    let workspace = fixture_with_issues(60, "oversize");
    let workspace = workspace.path();
    let _removed = apply_out_of_band(workspace, out_of_band);
    git_out(workspace, &["init", "-q", "."]);
    let mut parent = git_commit_checkpoint(workspace, "oversize base");

    let ids = live_issue_ids(workspace);
    let subject = &ids[0];

    // A 200 KiB record: above RECORD_INLINE (64 KiB), so it lands in
    // set/rec/ once and the referencing shard stores a reference row.
    set_notes_via_stdin(workspace, subject, &filler("oversize", 200 * 1024));
    flush_ok(workspace);
    let commit = git_commit_checkpoint(workspace, "oversize record publication");
    let _ = new_blob_bytes(workspace, &parent, &commit);
    parent = commit;

    let rec_dir = workspace.join(".beads/checkpoint/set/rec");
    let rec_files: Vec<_> = fs::read_dir(&rec_dir)
        .expect("set/rec/ must exist after an oversize record")
        .map(|e| e.unwrap())
        .collect();
    assert_eq!(rec_files.len(), 1, "expected exactly one oversize object");
    let rec_path_rel = format!("set/rec/{}", rec_files[0].file_name().to_str().unwrap());
    // Content addressing: the file's bytes hash to its name.
    let bytes = fs::read(rec_files[0].path()).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        rec_files[0]
            .file_name()
            .to_str()
            .unwrap()
            .trim_end_matches(".jsonl"),
        "oversize object is not content-addressed"
    );

    // The referencing shard stays small: it carries the reference row, not
    // the payload. Find the shard row containing the subject's ID.
    let manifest = current_manifest(workspace);
    let mut shard_paths = Vec::new();
    for row in issue_shards(&manifest) {
        shard_paths.push(row["path"].as_str().unwrap().to_string());
    }
    let mut referencing_shard = None;
    for path in &shard_paths {
        let body = fs::read_to_string(workspace.join(".beads/checkpoint").join(path)).unwrap();
        if body.contains(&format!("\"path\":\"{}\"", rec_path_rel))
            || body.contains(&format!("\"path\": \"{ek}\"", ek = rec_path_rel))
        {
            referencing_shard = Some((path.clone(), body));
            break;
        }
    }
    let (shard_path, shard_body) =
        referencing_shard.expect("no shard references the oversize object");
    assert!(
        shard_body.len() < 8 * 1024,
        "referencing shard {} carries {} bytes, expected a small reference row",
        shard_path,
        shard_body.len()
    );

    // Subsequent unrelated single-issue publications add no bytes for the
    // referenced record.
    for (round, id) in ids.iter().enumerate().take(4).skip(1) {
        set_notes(
            workspace,
            id,
            &format!("unrelated round {} {}", round, filler("unrelated", 100)),
        );
        flush_ok(workspace);
        let commit = git_commit_checkpoint(workspace, &format!("unrelated publication {}", round));
        let range = format!("{}..{}", parent, commit);
        let added = git_out(
            workspace,
            &["rev-list", "--objects", &range, "--", ".beads/"],
        );
        assert!(
            !added.contains("set/rec/"),
            "publication {} re-added oversize object bytes",
            round
        );
        let cost = new_blob_bytes(workspace, &parent, &commit);
        assert!(
            cost <= 256 * 1024,
            "publication {} added {} bytes",
            round,
            cost
        );
        parent = commit;
    }
}

// ---------------------------------------------------------------------------
// set-v2-pointer-boundedness (12.8)
// ---------------------------------------------------------------------------

#[test]
fn set_v2_pointer_boundedness() {
    run_scenario("set-v2-pointer-boundedness", None);
}

fn pointer_boundedness(out_of_band: Option<&OutOfBandChange>) {
    let workspace = fixture_with_issues(40, "bounded");
    let workspace = workspace.path();
    let _removed = apply_out_of_band(workspace, out_of_band);
    let ids = live_issue_ids(workspace);

    // Distinctive payload markers that must never appear in the manifest.
    let markers: Vec<String> = (0..5)
        .map(|i| format!("payload-marker-{}-7f31d90c", i))
        .collect();

    for round in 0..55 {
        set_notes(
            workspace,
            &ids[round % ids.len()],
            &format!(
                "bounded round {} {} {}",
                round,
                filler("bounded", 60),
                markers[round % markers.len()]
            ),
        );
        flush_ok(workspace);
        let current = fs::read_to_string(workspace.join(".beads/checkpoint/current.json")).unwrap();
        let previous =
            fs::read_to_string(workspace.join(".beads/checkpoint/previous.json")).unwrap();
        assert!(
            current.len() < 4096,
            "current.json grew to {} bytes at publication {}",
            current.len(),
            round
        );
        assert!(
            previous.len() < 4096,
            "previous.json grew to {} bytes at publication {}",
            previous.len(),
            round
        );
    }

    // The manifest contains table rows only, never record payload: none of
    // the payload markers may appear in it, and it stays within the
    // structural bound for a small workspace.
    let manifest_text = fs::read_to_string(
        workspace.join(".beads/checkpoint").join(
            read_pointer(workspace)["active_root"]["path"]
                .as_str()
                .unwrap(),
        ),
    )
    .unwrap();
    for marker in &markers {
        assert!(
            !manifest_text.contains(marker.as_str()),
            "manifest leaks record payload marker {:?}",
            marker
        );
    }
    assert!(
        manifest_text.len() < 128 * 1024,
        "manifest grew to {} bytes",
        manifest_text.len()
    );
}

// ---------------------------------------------------------------------------
// set-v2-forensic-frozen (12.9)
// ---------------------------------------------------------------------------

#[test]
fn set_v2_forensic_frozen() {
    run_scenario("set-v2-forensic-frozen", None);
}

fn forensic_frozen(out_of_band: Option<&OutOfBandChange>) {
    let workspace = fixture_with_issues(30, "frozen");
    let workspace = workspace.path();
    let _removed = apply_out_of_band(workspace, out_of_band);
    let forensic = workspace.join(".beads/checkpoint/forensic.jsonl");

    let initial = fs::read(&forensic).ok();
    let initial_mtime = fs::metadata(&forensic).ok().map(|m| m.modified().unwrap());

    for round in 0..12 {
        set_notes(
            workspace,
            &live_issue_ids(workspace)[0],
            &format!("frozen round {} {}", round, filler("frozen", 60)),
        );
        flush_ok(workspace);
        let now_bytes = fs::read(&forensic).ok();
        let now_mtime = fs::metadata(&forensic).ok().map(|m| m.modified().unwrap());
        assert_eq!(
            now_bytes.as_deref(),
            initial.as_deref(),
            "forensic.jsonl was rewritten at publication {}",
            round
        );
        assert_eq!(
            now_mtime, initial_mtime,
            "forensic.jsonl mtime changed at publication {}",
            round
        );
    }

    // Restore succeeds with the file absent (it is absent by default under
    // this layout, and no reader may require it): restore the final
    // generation into a fresh empty workspace.
    let generation = read_pointer(workspace)["generation_id"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(!forensic.exists() || initial.is_none());
    let target = TempDir::new().unwrap();
    init_workspace(target.path());
    let source = workspace
        .join(".beads/checkpoint")
        .to_string_lossy()
        .to_string();
    run_ok(
        target.path(),
        &[
            "restore",
            "--source",
            &source,
            "--generation",
            &generation,
            "--actor",
            "conformance-test",
        ],
    );
}

// ---------------------------------------------------------------------------
// set-v2-crash-ordering (12.10)
// ---------------------------------------------------------------------------

#[test]
fn set_v2_crash_ordering() {
    let workspace = fixture_with_issues(40, "crash");
    let workspace = workspace.path();
    let ids = live_issue_ids(workspace);
    let checkpoint_dir = workspace.join(".beads/checkpoint");

    // --- interrupted before the pointer swap: orphans on disk ---
    // A crash before step 3 leaves the old generation authoritative and
    // orphan artifacts under set/. Simulate the orphans (the
    // `OrphanSetFiles` fixture), then publish: the old generation stays
    // authoritative until the new publication's structural retention reaps
    // them.
    let before_generation = read_pointer(workspace)["generation_id"].clone();
    let _ = OutOfBandChange::OrphanSetFiles.apply(workspace);

    set_notes(
        workspace,
        &ids[1],
        &format!("crash driver {}", filler("crash", 50)),
    );
    flush_ok(workspace);

    assert_ne!(
        read_pointer(workspace)["generation_id"],
        before_generation,
        "sanity: publication did not advance the generation"
    );
    assert!(
        !checkpoint_dir
            .join(
                "set/shard/0000000000000000000000000000000000000000000000000000000000000000.jsonl"
            )
            .exists(),
        "orphan shard was not reaped by the next publication"
    );
    assert!(
        !checkpoint_dir
            .join("set/seg/1111111111111111111111111111111111111111111111111111111111111111.jsonl")
            .exists(),
        "orphan segment was not reaped by the next publication"
    );
    assert!(
        !checkpoint_dir
            .join("set/rec/2222222222222222222222222222222222222222222222222222222222222222.jsonl")
            .exists(),
        "orphan record was not reaped by the next publication"
    );

    // --- after the pointer swap: the new generation is authoritative ---
    // The pointer names one complete generation: the manifest bytes hash to
    // the pointer's recorded digest, and every artifact row verifies.
    for round in 0..3 {
        set_notes(
            workspace,
            &ids[round + 2],
            &format!("post-swap round {} {}", round, filler("post-swap", 50)),
        );
        flush_ok(workspace);
        let pointer = read_pointer(workspace);
        let manifest_rel = pointer["active_root"]["path"].as_str().unwrap().to_string();
        let manifest_sha = pointer["active_root"]["sha256"].as_str().unwrap();
        let manifest_path = checkpoint_dir.join(&manifest_rel);
        let manifest_bytes = fs::read(&manifest_path).unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(&manifest_bytes)),
            manifest_sha,
            "pointer digest disagrees with manifest bytes"
        );
        // The filename and the recorded generation agree.
        assert!(
            manifest_rel.contains(pointer["generation_id"].as_str().unwrap()),
            "manifest filename does not name the pointer's generation"
        );
        // The tail row verifies against the live tail bytes.
        let manifest: Value = serde_json::from_slice(&manifest_bytes).unwrap();
        if let Some(tail) = manifest.get("tail").filter(|t| !t.is_null()) {
            let tail_bytes = fs::read(checkpoint_dir.join("set/tail.jsonl")).unwrap();
            assert_eq!(
                format!("{:x}", Sha256::digest(&tail_bytes)),
                tail["sha256"].as_str().unwrap(),
                "tail row digest disagrees with tail bytes"
            );
        }
    }

    // Sealed-tail removal is idempotent: after a seal, a repeated
    // publication leaves one complete generation, one tail, and no
    // duplicate sealed digests.
    //
    // An update event is a ~330-byte marker — the changed-notes payload
    // lives in the issue shard, not the event — so reaching the 64 KiB
    // seal threshold takes well over a hundred publications, not the
    // handful the 2.2 KiB fillers suggest (same shape as the tail-seal
    // scenario's 240-round budget). Each round captures the outgoing
    // tail's bytes: the publication that finally crosses SEG_SEAL must
    // seal exactly those bytes under their content address (spec 6.2
    // renames the tail; 7.4 step 4 removes the sealed tail's old file).
    let mut old_tail_sha = sha256_file(&checkpoint_dir.join("set/tail.jsonl"));
    let mut sealed = false;
    for round in 0..240 {
        set_notes(
            workspace,
            &ids[(round + 5) % ids.len()],
            &format!("seal driver {} {}", round, filler("idempotent", 2200)),
        );
        flush_ok(workspace);
        flush_ok(workspace); // immediate re-publication: the idempotency probe
        let manifest = current_manifest(workspace);
        let digests: HashSet<String> = manifest["segments"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["sha256"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(
            digests.len(),
            manifest["segments"].as_array().unwrap().len(),
            "duplicate sealed segment digests after re-publication"
        );
        if !sealed {
            let has_segments = !manifest["segments"].as_array().unwrap().is_empty();
            if has_segments {
                // The old tail's bytes became a content-addressed segment.
                let segments = manifest["segments"].as_array().unwrap();
                let old_tail_sealed = segments.iter().any(|row| {
                    let bytes =
                        fs::read(checkpoint_dir.join(row["path"].as_str().unwrap())).unwrap();
                    format!("{:x}", Sha256::digest(&bytes)) == old_tail_sha
                });
                assert!(
                    old_tail_sealed,
                    "the sealed tail's old bytes are not retained under their content address"
                );
                sealed = true;
            }
        }
        if sealed {
            break;
        }
        // The next round's outgoing tail: what its first flush will seal
        // if this round's appends left it under SEG_SEAL and the next one
        // crosses.
        old_tail_sha = sha256_file(&checkpoint_dir.join("set/tail.jsonl"));
    }
    assert!(sealed, "tail never sealed during the idempotency probe");
}

// ---------------------------------------------------------------------------
// out-of-band re-verification grid (beadrs-15cca551)
// ---------------------------------------------------------------------------

/// Run one named baseline-green scenario, optionally under an out-of-band
/// change. The two red scenarios are not dispatchable: their setups ARE the
/// fixtures above.
fn run_scenario(name: &str, out_of_band: Option<&OutOfBandChange>) {
    match name {
        "set-v2-budget-single-issue-update" => budget_single_issue_update(out_of_band),
        "set-v2-shard-split-on-update-storm" => shard_split_on_update_storm(out_of_band),
        "set-v2-tail-seal-and-immutability" => tail_seal_and_immutability(out_of_band),
        "set-v2-oversize-record-once-cost" => oversize_record_once_cost(out_of_band),
        "set-v2-pointer-boundedness" => pointer_boundedness(out_of_band),
        "set-v2-forensic-frozen" => forensic_frozen(out_of_band),
        other => panic!(
            "unknown or non-dispatchable conformance scenario {other:?}; \
             shard-merge-on-deletion and crash-ordering are the out-of-band \
             scenarios themselves, not probe targets"
        ),
    }
}

/// The baseline-green scenarios whose behavior under out-of-band change this
/// grid characterizes: the red pair dies at the first flush after such a
/// change with a covered-ahead integrity refusal, and which of the six
/// survivors tolerate the same interference is the open question.
const GREEN_SCENARIOS: [&str; 6] = [
    "set-v2-budget-single-issue-update",
    "set-v2-shard-split-on-update-storm",
    "set-v2-tail-seal-and-immutability",
    "set-v2-oversize-record-once-cost",
    "set-v2-pointer-boundedness",
    "set-v2-forensic-frozen",
];

/// Run one scenario under one fixture, converting a scenario panic into its
/// exact failure text so the grid can record reds without aborting
fn probe_one(name: &str, fixture: &OutOfBandChange) -> Result<(), String> {
    catch_unwind(AssertUnwindSafe(|| run_scenario(name, Some(fixture)))).map_err(|payload| {
        if let Some(text) = payload.downcast_ref::<String>() {
            text.clone()
        } else if let Some(text) = payload.downcast_ref::<&str>() {
            (*text).to_string()
        } else {
            "<non-string panic payload>".to_string()
        }
    })
}

/// Re-run every baseline-green scenario under each out-of-band fixture and
/// record the outcome. This is a characterization grid, not a regression
/// gate: a combination going red is a recorded finding — the covered-ahead
/// refusal and its blast radius are later slices of the beadrs-40f0cc4e
/// auto-split to fix — so reds do not fail this test. What DOES fail it is
/// a grid that cannot classify: a combination that never ran, or a red
/// without its exact failure text.
///
/// Narrow the grid for a focused re-run with comma-separated env filters:
/// `SET_V2_OOB_PROBE_SCENARIOS` (scenario names) and
/// `SET_V2_OOB_PROBE_FIXTURES` (fixture labels, e.g. `store-deletion`).
/// Unset, both run the full grid. The report goes to captured stderr — read
/// it with `-- --show-output` (or `--nocapture`).
#[test]
fn set_v2_green_scenarios_under_out_of_band_change() {
    let scenarios: Vec<String> = match std::env::var("SET_V2_OOB_PROBE_SCENARIOS") {
        Ok(list) => list
            .split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_string)
            .collect(),
        Err(_) => GREEN_SCENARIOS
            .iter()
            .map(|name| name.to_string())
            .collect(),
    };
    for name in &scenarios {
        assert!(
            GREEN_SCENARIOS.contains(&name.as_str()),
            "SET_V2_OOB_PROBE_SCENARIOS names {name:?}, which is not a baseline-green scenario"
        );
    }
    let all_fixtures = [
        OutOfBandChange::StoreDeletion { count: 8 },
        OutOfBandChange::OrphanSetFiles,
    ];
    let fixtures: Vec<OutOfBandChange> = match std::env::var("SET_V2_OOB_PROBE_FIXTURES") {
        Ok(list) => all_fixtures
            .iter()
            .filter(|fixture| {
                list.split(',')
                    .map(str::trim)
                    .any(|label| label == fixture.label())
            })
            .cloned()
            .collect(),
        Err(_) => all_fixtures.to_vec(),
    };
    assert!(
        !scenarios.is_empty() && !fixtures.is_empty(),
        "probe grid is empty: scenarios {scenarios:?}, fixtures {:?}",
        fixtures.iter().map(|f| f.label()).collect::<Vec<_>>()
    );

    let mut report: Vec<(String, &'static str, Result<(), String>)> = Vec::new();
    for name in &scenarios {
        for fixture in &fixtures {
            eprintln!("OOB-PROBE start: {name} under {}", fixture.label());
            let outcome = probe_one(name, fixture);
            report.push((name.clone(), fixture.label(), outcome));
        }
    }

    let mut green = 0usize;
    for (name, label, outcome) in &report {
        match outcome {
            Ok(()) => {
                green += 1;
                eprintln!("OOB-PROBE GREEN: {name} under {label}");
            }
            Err(text) => {
                eprintln!("OOB-PROBE RED: {name} under {label}");
                eprintln!("OOB-PROBE RED failure text: {text}");
            }
        }
    }
    eprintln!(
        "OOB-PROBE summary: {green}/{} combinations stayed green",
        report.len()
    );

    assert_eq!(
        report.len(),
        scenarios.len() * fixtures.len(),
        "every scenario x fixture combination must run and be classified"
    );
    for (name, label, outcome) in &report {
        if let Err(text) = outcome {
            assert!(
                !text.trim().is_empty(),
                "combination {name} under {label} went red without failure text"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// spec 12.1 + 12.2: migration and the reader family across the boundary
// ---------------------------------------------------------------------------

/// The id of the one live issue titled exactly `title`
fn extract_created_id(workspace: &Path, title: &str) -> String {
    live_issue_summaries(workspace)
        .into_iter()
        .find(|(_, t)| t == title)
        .map(|(id, _)| id)
        .unwrap_or_else(|| panic!("no live issue titled {title:?}"))
}

/// The parsed `sync status --json` report
fn status_report(workspace: &Path) -> Value {
    let out = run_bead(workspace, &["sync", "status", "--json"]);
    assert!(
        out.status.success(),
        "sync status --json failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("status --json emits one JSON object")
}

/// Sorted (id, title) pairs of the live store: the semantic content the
/// v1 restore-equivalence comparison needs
fn live_issue_summaries(workspace: &Path) -> Vec<(String, String)> {
    let out = run_bead(workspace, &["list", "--json", "--limit", "999999"]);
    assert!(
        out.status.success(),
        "list failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let issues: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
    let mut rows: Vec<(String, String)> = issues
        .into_iter()
        .map(|issue| {
            (
                issue["id"].as_str().unwrap().to_string(),
                issue["title"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    rows.sort();
    rows
}

/// Restore one named generation of `workspace`'s checkpoint set (through
/// the named pointer, current.json or previous.json) into a fresh
/// workspace. The restore verifier checks the set-v2 closure, hashes,
/// counts, and event continuity, so success is the completeness proof for
/// the published `set/` tree.
fn restore_generation_fresh(workspace: &Path, pointer: &str, generation: &str) -> TempDir {
    let source = workspace.join(".beads/checkpoint").join(pointer);
    let target = TempDir::new().unwrap();
    init_workspace(target.path());
    run_ok(
        target.path(),
        &[
            "restore",
            "--source",
            source.to_str().unwrap(),
            "--generation",
            generation,
            "--actor",
            "set-v2-conformance",
        ],
    );
    target
}

/// Replace `to`'s checkpoint set with the checkpoint set at `from_dir` --
/// a directory that directly holds current.json (a workspace's
/// `.beads/checkpoint`, or a committed fixture root). The git-transport
/// pull of the r027 remote-advanced fixture pattern.
fn pull_checkpoint(from_dir: &Path, to: &Path) {
    let target = to.join(".beads/checkpoint");
    if target.exists() {
        fs::remove_dir_all(&target).unwrap();
    }
    fn walk(from: &Path, to: &Path) {
        fs::create_dir_all(to).unwrap();
        for entry in fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            let target = to.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                walk(&entry.path(), &target);
            } else {
                fs::copy(entry.path(), target).unwrap();
            }
        }
    }
    walk(from_dir, &target);
}

/// Spec 12.1: the first publication of an existing monolithic or
/// sharded-v1 workspace under adaptive mode is the documented full
/// rewrite. `sync --status` reports the pending upgrade before it
/// happens; the publication writes the complete set/ tree; and the old
/// generation remains restorable via previous.json until retention reaps
/// it -- restoring both generations reproduces the live store each
/// covers, exactly. Publications are triggered by mutations (dirty
/// checkpoints publish; an aligned one short-circuits), which is also
/// the real upgrade path: the first mutation after the binary upgrade
/// pays the one-time rewrite.
#[test]
fn set_v2_migration_full_rewrite() {
    // Leg 1: a monolithic workspace at the safety limits' operating
    // point. The recorded threshold table is scaled down so the fixture
    // sits just inside the forced-monolith refusal -- the operating point
    // the scenario names -- without a 64 MiB fixture.
    let fixture = fixture_with_issues(6, "migration-monolith");
    let ws = fixture.path();
    set_checkpoint_config(ws, "mode", json!("monolithic"));
    set_checkpoint_config(
        ws,
        "thresholds",
        json!({
            "version": 1,
            "max_monolith_issue_records": 50,
            "max_monolith_total_bytes": 1_048_576,
            "max_record_line_bytes": 65_536
        }),
    );
    // A mutation dirties the aligned fixture checkpoint, so this flush
    // really publishes -- under the forced monolith.
    run_ok(
        ws,
        &["create", "--title", "migration-monolith pre-upgrade tail"],
    );
    flush_ok(ws);
    assert_eq!(read_pointer(ws)["mode"], "monolithic");
    let monolith_generation = read_pointer(ws)["generation_id"]
        .as_str()
        .unwrap()
        .to_string();
    let expected_old = live_issue_summaries(ws);

    // The policy keeping the workspace on its published layout reports no
    // upgrade: the pending upgrade exists only when the layouts differ.
    let report = status_report(ws);
    assert!(
        report["pending_upgrade"].is_null(),
        "forced monolith on a monolith workspace must not report an upgrade: {report}"
    );

    // Spec 10: once the workspace's policy selects set-v2 -- here the
    // recorded forced mode is lifted back to adaptive -- status reports
    // the pending upgrade BEFORE the migration, in text and JSON.
    set_checkpoint_config(ws, "mode", json!("adaptive"));
    let report = status_report(ws);
    assert_eq!(report["pending_upgrade"]["current_mode"], "monolithic");
    assert_eq!(report["pending_upgrade"]["target_mode"], "set-v2");
    let notice = report["pending_upgrade"]["notice"].as_str().unwrap();
    assert!(notice.contains("rewrites the full store"), "{notice}");
    assert!(notice.contains("proportional to store size"), "{notice}");
    let text = String::from_utf8(run_bead(ws, &["sync", "status"]).stdout).unwrap();
    assert!(
        text.contains("Pending upgrade: monolithic -> set-v2"),
        "{text}"
    );

    // The first adaptive publication -- the real upgrade path, a mutation
    // after the policy change -- is the full rewrite, and the old
    // generation survives as previous.json.
    run_ok(
        ws,
        &["create", "--title", "migration-monolith post-upgrade"],
    );
    flush_ok(ws);
    let pointer = read_pointer(ws);
    assert_eq!(pointer["mode"], "set-v2");
    let setv2_generation = pointer["generation_id"].as_str().unwrap().to_string();
    assert_ne!(setv2_generation, monolith_generation);
    let previous: Value = serde_json::from_str(
        &fs::read_to_string(ws.join(".beads/checkpoint/previous.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        previous["generation_id"].as_str().unwrap(),
        monolith_generation
    );

    // The upgrade is no longer pending once the layouts agree.
    let report = status_report(ws);
    assert!(report["pending_upgrade"].is_null(), "{report}");

    // Round trip across the boundary: BOTH generations restore into a
    // fresh workspace and reproduce the live store each covers exactly.
    assert_eq!(
        live_issue_summaries(
            restore_generation_fresh(fixture.path(), "previous.json", &monolith_generation).path()
        ),
        expected_old,
        "the pre-migration monolith generation must stay restorable"
    );
    let expected_new = live_issue_summaries(ws);
    assert_ne!(
        expected_new, expected_old,
        "the upgrade mutation is covered"
    );
    assert_eq!(
        live_issue_summaries(
            restore_generation_fresh(fixture.path(), "current.json", &setv2_generation).path()
        ),
        expected_new,
        "the migrated set-v2 generation must restore equivalently"
    );

    // Leg 2: a sharded-v1 workspace migrates by the same path. The writer
    // no longer produces sharded-v1 (forced sharded resolves to set-v2),
    // so the fixture starts from the committed sharded fixture: importing
    // it into an empty workspace adopts its UUID and live state, and its
    // pointer stands in for the pre-migration sharded generation.
    let sharded_fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/attempts/new/sharded");
    let sharded_dir = TempDir::new().unwrap();
    let ws2 = sharded_dir.path();
    init_workspace(ws2);
    set_checkpoint_config(ws2, "auto_flush", json!(false));
    run_ok(
        ws2,
        &[
            "sync",
            "import-only",
            "--input",
            sharded_fixture.to_str().unwrap(),
            "--restore-into-empty",
            "--actor",
            "migration-scenario",
        ],
    );
    pull_checkpoint(&sharded_fixture, ws2);

    // The pending upgrade names the sharded layout before the migration;
    // the default policy is already adaptive, so no config edit is needed.
    let report = status_report(ws2);
    assert_eq!(report["mode"], "sharded");
    assert_eq!(report["pending_upgrade"]["current_mode"], "sharded");
    assert_eq!(report["pending_upgrade"]["target_mode"], "set-v2");
    let sharded_generation = report["generation_id"].as_str().unwrap().to_string();

    // The same-path migration: a mutation publishes the full set-v2
    // rewrite, and the sharded generation survives as previous.json.
    run_ok(
        ws2,
        &["create", "--title", "migration-sharded post-upgrade"],
    );
    flush_ok(ws2);
    assert_eq!(read_pointer(ws2)["mode"], "set-v2");
    let previous2: Value = serde_json::from_str(
        &fs::read_to_string(ws2.join(".beads/checkpoint/previous.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        previous2["generation_id"].as_str().unwrap(),
        sharded_generation
    );
    let expected2 = live_issue_summaries(ws2);
    assert!(
        !expected2.is_empty(),
        "the sharded fixture must import live records for the round trip"
    );
    // The sharded generation's own live snapshot: import-only state plus
    // nothing else -- the upgrade mutation is covered by the set-v2 side,
    // so restore the sharded generation and compare against its content.
    let sharded_restore = restore_generation_fresh(ws2, "previous.json", &sharded_generation);
    let sharded_view = live_issue_summaries(sharded_restore.path());
    assert_eq!(
        sharded_view,
        expected2
            .iter()
            .filter(|(_, title)| !title.contains("migration-sharded post-upgrade"))
            .cloned()
            .collect::<Vec<_>>(),
        "the pre-migration sharded-v1 generation must stay restorable"
    );
    assert!(
        !sharded_view.is_empty(),
        "the sharded fixture must carry records into the round trip"
    );
}

/// Spec 12.2: restore into an empty store, fork import, reconcile, and
/// verified restore each succeed from a monolithic source and from a
/// set-v2 source of the same store, and the restored stores pass v1
/// restore-equivalence against the source. Each leg establishes its
/// layout's base generation and runs every family before retention reaps
/// the base (the retained set is bounded by the two referenced
/// generations).
#[test]
fn set_v2_dual_layout_reader_equivalence() {
    let fixture = fixture_with_issues(5, "dual-layout");
    let ws = fixture.path();
    let mut advancements = 0usize;

    for (label, mode) in [("monolithic", "monolithic"), ("set-v2", "adaptive")] {
        // Base generation for this leg, published in the leg's layout.
        // A mutation dirties the prior leg's aligned checkpoint, so the
        // flush really publishes under the configured mode.
        set_checkpoint_config(ws, "mode", json!(mode));
        run_ok(
            ws,
            &["create", "--title", &format!("dual-layout base {label}")],
        );
        flush_ok(ws);
        let pointer = read_pointer(ws);
        assert_eq!(pointer["mode"], label, "{label}: base generation layout");
        let base_generation = pointer["generation_id"].as_str().unwrap().to_string();
        let base = live_issue_summaries(ws);
        let source = ws
            .join(".beads/checkpoint/current.json")
            .to_str()
            .unwrap()
            .to_string();

        // 1. Restore into an empty store.
        let restored = restore_generation_fresh(ws, "current.json", &base_generation);
        assert_eq!(
            live_issue_summaries(restored.path()),
            base,
            "{label}: restore into an empty store"
        );

        // 2. Fork import: import-only --restore-into-empty adopts the
        //    source identity into an empty initialized workspace.
        let fork = TempDir::new().unwrap();
        init_workspace(fork.path());
        run_ok(
            fork.path(),
            &[
                "sync",
                "import-only",
                "--input",
                &source,
                "--restore-into-empty",
                "--actor",
                "dual-layout",
            ],
        );
        assert_eq!(
            live_issue_summaries(fork.path()),
            base,
            "{label}: fork import"
        );

        // 4. Verified restore over a non-empty live store.
        let occupied = TempDir::new().unwrap();
        init_workspace(occupied.path());
        run_ok(
            occupied.path(),
            &["create", "--title", "pre-existing local work"],
        );
        run_ok(
            occupied.path(),
            &[
                "restore",
                "--source",
                &source,
                "--generation",
                &base_generation,
                "--actor",
                "dual-layout",
                "--allow-non-empty",
            ],
        );
        assert_eq!(
            live_issue_summaries(occupied.path()),
            base,
            "{label}: verified restore"
        );

        // 3. Reconcile, on the r027 lagging-pair shape: a clone of this
        //    workspace at the base generation advances by one issue and
        //    publishes in the leg's layout, and the pull of that
        //    checkpoint back into this workspace -- whose live store has
        //    never restored or imported, so every live event appears in
        //    the pulled checkpoint -- reads remote-advanced and merges.
        let clone_dir = TempDir::new().unwrap();
        let clone = clone_dir.path();
        fs::create_dir_all(clone.join(".beads")).unwrap();
        fs::copy(
            ws.join(".beads/config.json"),
            clone.join(".beads/config.json"),
        )
        .unwrap();
        let clone_checkpoint = clone.join(".beads/checkpoint");
        fs::create_dir_all(&clone_checkpoint).unwrap();
        fn walk_copy(from: &Path, to: &Path) {
            fs::create_dir_all(to).unwrap();
            for entry in fs::read_dir(from).unwrap() {
                let entry = entry.unwrap();
                let target = to.join(entry.file_name());
                if entry.file_type().unwrap().is_dir() {
                    walk_copy(&entry.path(), &target);
                } else {
                    fs::copy(entry.path(), target).unwrap();
                }
            }
        }
        walk_copy(&ws.join(".beads/checkpoint"), &clone_checkpoint);
        init_workspace(clone);
        run_ok(
            clone,
            &[
                "restore",
                "--source",
                ".beads/checkpoint",
                "--generation",
                &base_generation,
                "--actor",
                "dual-layout-clone",
            ],
        );
        run_ok(
            clone,
            &[
                "create",
                "--title",
                &format!("dual-layout advancement {advancements}"),
            ],
        );
        advancements += 1;
        flush_ok(clone);

        pull_checkpoint(&clone_checkpoint, ws);
        let relationship = status_report(ws)["relationship"]
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(
            relationship, "remote-advanced",
            "{label}: the pulled advancement must read remote-advanced"
        );
        run_ok(ws, &["sync", "reconcile", "--actor", "dual-layout"]);
        let mut reconciled_expected = base.clone();
        reconciled_expected.push((
            extract_created_id(ws, &format!("dual-layout advancement {}", advancements - 1)),
            format!("dual-layout advancement {}", advancements - 1),
        ));
        reconciled_expected.sort();
        assert_eq!(
            live_issue_summaries(ws),
            reconciled_expected,
            "{label}: reconcile"
        );
    }
}

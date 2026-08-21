//! Checkpoint mode selection from recorded thresholds (plan 6.1.1, ADR-003 P4)
//!
//! `sync flush-only` selects monolithic versus sharded output from the
//! recorded threshold table (`.beads/config.json` `checkpoint` section, the
//! previous manifest's recorded thresholds, or the plan 6.1.1 defaults) --
//! never from a hardcoded constant. These tests verify:
//!
//! - mode selection tracks the recorded thresholds and operator overrides
//! - crossing a threshold publishes a mode transition whose changed-path set
//!   carries the new root, objects, pointer replacement, and tombstones for
//!   the superseded root
//! - one mutation republishes only the changed issue shard plus the event
//!   tail, at two workspace sizes an order of magnitude apart
//! - monolithic and sharded restores are semantically equivalent

use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

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

fn stderr_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

fn init_workspace(dir: &Path) {
    fs::create_dir_all(dir).unwrap();
    run_ok(dir, &["init"]);
}

/// Merge a `checkpoint` section into the workspace's `.beads/config.json`
///
/// `bead init` records workspace identity in this file, so the checkpoint
/// configuration is merged in rather than replacing the whole document.
fn write_checkpoint_config(workspace: &Path, checkpoint: Value) {
    let config_path = workspace.join(".beads/config.json");
    let mut config: Value = if config_path.exists() {
        serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap()
    } else {
        json!({})
    };
    config
        .as_object_mut()
        .unwrap()
        .insert("checkpoint".to_string(), checkpoint);
    fs::write(&config_path, serde_json::to_string(&config).unwrap()).unwrap();
}

/// Remove the `checkpoint` section, leaving the rest of the recorded
/// configuration untouched
fn remove_checkpoint_config(workspace: &Path) {
    let config_path = workspace.join(".beads/config.json");
    let mut config: Value =
        serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
    config
        .as_object_mut()
        .unwrap()
        .remove("checkpoint")
        .expect("checkpoint section must exist before removal");
    fs::write(&config_path, serde_json::to_string(&config).unwrap()).unwrap();
}

fn flush(workspace: &Path) -> Output {
    run_bead(workspace, &["sync", "flush-only"])
}

fn flush_ok(workspace: &Path) {
    let output = flush(workspace);
    assert!(
        output.status.success(),
        "flush-only failed: {}",
        stderr_of(&output)
    );
}

fn create_issues(workspace: &Path, count: usize) {
    for i in 1..=count {
        run_ok(workspace, &["create", "--title", &format!("Issue {}", i)]);
    }
}

fn first_issue_id(workspace: &Path) -> String {
    let output = run_bead(workspace, &["list", "--json"]);
    assert!(
        output.status.success(),
        "list --json failed: {}",
        stderr_of(&output)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let first_line = stdout
        .lines()
        .next()
        .unwrap_or_else(|| panic!("list --json produced no output"));
    serde_json::from_str::<Value>(first_line)
        .expect("list --json must emit JSONL")
        .get("id")
        .and_then(|v| v.as_str())
        .expect("issue object must carry an id")
        .to_string()
}

fn checkpoint_dir(workspace: &Path) -> PathBuf {
    workspace.join(".beads/checkpoint")
}

fn read_current_pointer(workspace: &Path) -> Value {
    let content = fs::read_to_string(checkpoint_dir(workspace).join("current.json")).unwrap();
    serde_json::from_str(&content).unwrap()
}

fn read_active_manifest(workspace: &Path) -> Value {
    let pointer = read_current_pointer(workspace);
    let root = pointer["active_root"]["path"].as_str().unwrap();
    let content = fs::read_to_string(checkpoint_dir(workspace).join(root)).unwrap();
    serde_json::from_str(&content).unwrap()
}

fn pointer_mode(workspace: &Path) -> String {
    read_current_pointer(workspace)["mode"]
        .as_str()
        .unwrap()
        .to_string()
}

fn pointer_paths(pointer: &Value, key: &str) -> Vec<String> {
    pointer[key]
        .as_array()
        .map(|paths| {
            paths
                .iter()
                .filter_map(|p| p.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

/// Snapshot the checkpoint object set: checkpoint-relative path -> byte length
fn object_inventory(workspace: &Path) -> BTreeMap<String, u64> {
    let base = checkpoint_dir(workspace);
    let mut inventory = BTreeMap::new();
    for dir in ["objects", "manifests"] {
        let entries = match fs::read_dir(base.join(dir)) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => panic!("failed to read {}: {}", dir, e),
        };
        for entry in entries {
            let entry = entry.unwrap();
            let name = entry.file_name().to_str().unwrap().to_string();
            if name.ends_with(".tmp") || !entry.file_type().unwrap().is_file() {
                continue;
            }
            inventory.insert(
                format!("{}/{}", dir, name),
                fs::metadata(entry.path()).unwrap().len(),
            );
        }
    }
    inventory
}

fn sum_bytes(inventory: &BTreeMap<String, u64>) -> u64 {
    inventory.values().sum()
}

/// Paths present in `after` but not `before` -- the objects this flush wrote
fn added_objects<'a>(
    before: &'a BTreeMap<String, u64>,
    after: &'a BTreeMap<String, u64>,
) -> Vec<String> {
    after
        .keys()
        .filter(|path| !before.contains_key(*path))
        .cloned()
        .collect()
}

fn temp_workspace(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "test-mode-selection-{}-{}",
        tag,
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Threshold table with only the mode-selection record limit overridden
fn thresholds_with(overrides: Value) -> Value {
    let mut table = json!({
        "version": 1,
        "max_monolith_issue_records": 50_000,
        "max_monolith_total_bytes": 64 * 1024 * 1024,
        "max_record_line_bytes": 8 * 1024 * 1024,
        "max_shard_issue_records": 10_000,
        "max_shard_bytes": 50 * 1024 * 1024,
        "max_event_object_events": 100_000,
        "max_event_object_bytes": 64 * 1024 * 1024,
    });
    let target = table.as_object_mut().unwrap();
    for (key, value) in overrides.as_object().unwrap() {
        target.insert(key.clone(), value.clone());
    }
    table
}

// ---------------------------------------------------------------------------
// Acceptance 1: mode is selected from recorded configuration/thresholds
// ---------------------------------------------------------------------------

#[test]
fn mode_is_selected_from_recorded_thresholds() {
    // Same four issues, two recorded threshold tables an operator could have
    // written: the selected mode must follow the table, not a constant.
    for (limit, expected_mode) in [(4u64, "monolithic"), (3, "sharded")] {
        let workspace = temp_workspace(&format!("thresholds-{}", limit));
        init_workspace(&workspace);
        write_checkpoint_config(
            &workspace,
            json!({ "thresholds": thresholds_with(json!({ "max_monolith_issue_records": limit })) }),
        );
        create_issues(&workspace, 4);
        flush_ok(&workspace);
        assert_eq!(
            pointer_mode(&workspace),
            expected_mode,
            "4 issues against a {}-record limit must select {}",
            limit,
            expected_mode
        );
        let _ = fs::remove_dir_all(&workspace);
    }
}

#[test]
fn mode_defaults_to_monolithic_under_the_recorded_thresholds() {
    let workspace = temp_workspace("default-thresholds");
    init_workspace(&workspace);
    // No checkpoint section at all: the plan 6.1.1 default table applies.
    create_issues(&workspace, 4);
    flush_ok(&workspace);
    assert_eq!(pointer_mode(&workspace), "monolithic");
    let _ = fs::remove_dir_all(&workspace);
}

#[test]
fn operator_forced_mode_wins_over_adaptive_selection() {
    // Forcing sharded publishes shards below every threshold...
    let workspace = temp_workspace("forced-sharded");
    init_workspace(&workspace);
    write_checkpoint_config(&workspace, json!({ "mode": "sharded" }));
    create_issues(&workspace, 2);
    flush_ok(&workspace);
    assert_eq!(pointer_mode(&workspace), "sharded");
    let _ = fs::remove_dir_all(&workspace);

    // ...and forcing monolithic never bypasses the recorded safety limits.
    let workspace = temp_workspace("forced-monolithic");
    init_workspace(&workspace);
    write_checkpoint_config(
        &workspace,
        json!({
            "mode": "monolithic",
            "thresholds": thresholds_with(json!({ "max_monolith_issue_records": 3 })),
        }),
    );
    create_issues(&workspace, 4);
    let output = flush(&workspace);
    assert!(
        !output.status.success(),
        "forced monolith above the recorded limit must be refused"
    );
    assert!(
        stderr_of(&output).contains("exceed recorded safety limits"),
        "refusal must name the recorded limits: {}",
        stderr_of(&output)
    );
    let _ = fs::remove_dir_all(&workspace);
}

#[test]
fn invalid_recorded_checkpoint_config_is_rejected() {
    for (tag, section, expected) in [
        ("bad-mode", json!({ "mode": "bogus" }), "checkpoint.mode"),
        (
            "zero-limit",
            json!({ "thresholds": json!({ "version": 1, "max_monolith_issue_records": 0 }) }),
            "checkpoint.thresholds",
        ),
    ] {
        let workspace = temp_workspace(tag);
        init_workspace(&workspace);
        write_checkpoint_config(&workspace, section);
        create_issues(&workspace, 1);
        let output = flush(&workspace);
        assert!(
            !output.status.success(),
            "invalid checkpoint config ({}) must fail the flush",
            tag
        );
        assert!(
            stderr_of(&output).contains(expected),
            "error must name the offending key: {}",
            stderr_of(&output)
        );
        let _ = fs::remove_dir_all(&workspace);
    }
}

// ---------------------------------------------------------------------------
// Acceptance 2: threshold crossing publishes a mode transition
// ---------------------------------------------------------------------------

#[test]
fn mode_transition_tombstones_the_superseded_root() {
    let workspace = temp_workspace("transition");
    init_workspace(&workspace);
    write_checkpoint_config(
        &workspace,
        json!({ "thresholds": thresholds_with(json!({ "max_monolith_issue_records": 3 })) }),
    );

    // Below the limit: monolithic generation.
    create_issues(&workspace, 3);
    flush_ok(&workspace);
    assert_eq!(pointer_mode(&workspace), "monolithic");
    let monolith_root = read_current_pointer(&workspace)["active_root"]["path"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        checkpoint_dir(&workspace).join(&monolith_root).exists(),
        "monolithic root must exist while it is active"
    );

    // Crossing the limit republishes as sharded.
    run_ok(&workspace, &["create", "--title", "Issue 4"]);
    flush_ok(&workspace);

    let pointer = read_current_pointer(&workspace);
    assert_eq!(pointer["mode"].as_str().unwrap(), "sharded");

    let new_root = pointer["active_root"]["path"].as_str().unwrap();
    assert!(
        new_root.starts_with("manifests/"),
        "sharded active root must be a manifest: {}",
        new_root
    );

    // The changed-path set carries the new root, the objects it references,
    // and the pointer replacement.
    let added = pointer_paths(&pointer, "added_paths");
    let replaced = pointer_paths(&pointer, "replaced_paths");
    let deleted = pointer_paths(&pointer, "deleted_paths");
    assert!(
        added.contains(&new_root.to_string()),
        "added paths must include the new root: {:?}",
        added
    );
    assert!(
        added.iter().any(|p| p.starts_with("objects/")),
        "added paths must include the new generation's objects: {:?}",
        added
    );
    assert!(
        replaced.contains(&"current.json".to_string()),
        "the pointer replacement must be a changed path: {:?}",
        replaced
    );

    // ...and tombstones for the superseded root, actually applied.
    assert!(
        deleted.contains(&monolith_root),
        "deleted paths must tombstone the superseded root {:?}: {:?}",
        monolith_root,
        deleted
    );
    assert!(
        !checkpoint_dir(&workspace).join(&monolith_root).exists(),
        "tombstoned root must be removed after the pointer commits"
    );

    // The transitioned checkpoint must still be whole: ready to commit.
    run_ok(&workspace, &["sync", "status"]);
    let _ = fs::remove_dir_all(&workspace);
}

// ---------------------------------------------------------------------------
// Acceptance 3: publication cost tracks the delta, not the workspace
// ---------------------------------------------------------------------------

/// Build a sharded workspace of `count` issues, flush it, apply exactly one
/// mutation, flush again, and report what the second flush wrote.
struct IncrementalRun {
    corpus_bytes: u64,
    added: Vec<String>,
    added_bytes: u64,
    event_object_count: usize,
}

fn incremental_sharded_run(count: usize) -> IncrementalRun {
    let workspace = temp_workspace(&format!("incremental-{}", count));
    init_workspace(&workspace);
    write_checkpoint_config(
        &workspace,
        json!({
            "mode": "sharded",
            // Seal event objects at five records so the tail is short and
            // sealed-object reuse is visible at both workspace sizes.
            "thresholds": thresholds_with(json!({ "max_event_object_events": 5 })),
        }),
    );
    create_issues(&workspace, count);
    flush_ok(&workspace);
    assert_eq!(pointer_mode(&workspace), "sharded");

    let before = object_inventory(&workspace);
    let corpus_bytes = sum_bytes(&before);

    // One mutation on one existing issue: it rewrites that issue's shard and
    // appends one audit event to the tail.
    let issue_id = first_issue_id(&workspace);
    run_ok(
        &workspace,
        &["update", &issue_id, "--notes", "one mutation"],
    );
    flush_ok(&workspace);

    let after = object_inventory(&workspace);
    // The manifest is rewritten on every publication (it carries a creation
    // timestamp); the delta claim is about the data objects it selects.
    let added: Vec<String> = added_objects(&before, &after)
        .into_iter()
        .filter(|p| p.starts_with("objects/"))
        .collect();
    let added_bytes: u64 = added.iter().map(|p| after[p]).sum();

    let manifest = read_active_manifest(&workspace);
    let event_object_count = manifest["event_shards"].as_array().unwrap().len();

    let _ = fs::remove_dir_all(&workspace);
    IncrementalRun {
        corpus_bytes,
        added,
        added_bytes,
        event_object_count,
    }
}

/// The manifest role of an object path, for classifying what was written
fn role_of_path(workspace: &Path, path: &str) -> String {
    let manifest = read_active_manifest(workspace);
    for key in ["issue_shards", "event_shards", "receipt_shards"] {
        for shard in manifest[key].as_array().unwrap() {
            if shard["path"].as_str() == Some(path) {
                return shard["role"].as_str().unwrap().to_string();
            }
        }
    }
    panic!("path {} is not referenced by the active manifest", path)
}

#[test]
fn one_mutation_republishes_one_shard_and_the_event_tail() {
    // Two workspace sizes an order of magnitude apart.
    let small = incremental_sharded_run(30);
    let large = incremental_sharded_run(300);

    assert!(
        large.corpus_bytes > 8 * small.corpus_bytes,
        "the two runs must be an order of magnitude apart: {} vs {}",
        small.corpus_bytes,
        large.corpus_bytes
    );

    for run in [&small, &large] {
        assert_eq!(
            run.added.len(),
            2,
            "exactly one changed issue shard and one event tail object must be written, got {:?}",
            run.added
        );
        assert!(
            run.added_bytes * 16 <= run.corpus_bytes,
            "written bytes ({}) must stay within one shard's share of the corpus ({})",
            run.added_bytes,
            run.corpus_bytes
        );
    }

    // The large workspace has many sealed event objects; the flush must have
    // rewritten only the tail, not the sealed history.
    assert!(
        large.event_object_count >= 30,
        "the large workspace must have sealed event objects to reuse, got {}",
        large.event_object_count
    );

    // Classify the two written objects by re-running the small workspace and
    // consulting its manifest roles.
    let workspace = temp_workspace("incremental-roles");
    init_workspace(&workspace);
    write_checkpoint_config(
        &workspace,
        json!({
            "mode": "sharded",
            "thresholds": thresholds_with(json!({ "max_event_object_events": 5 })),
        }),
    );
    create_issues(&workspace, 30);
    flush_ok(&workspace);
    let before = object_inventory(&workspace);
    let issue_id = first_issue_id(&workspace);
    run_ok(
        &workspace,
        &["update", &issue_id, "--notes", "one mutation"],
    );
    flush_ok(&workspace);
    let after = object_inventory(&workspace);
    let added: Vec<String> = added_objects(&before, &after)
        .into_iter()
        .filter(|p| p.starts_with("objects/"))
        .collect();
    let roles: Vec<String> = added.iter().map(|p| role_of_path(&workspace, p)).collect();
    assert!(
        roles.contains(&"issues".to_string()) && roles.contains(&"events".to_string()),
        "written objects must be the changed issue shard and the event tail, got {:?} for {:?}",
        roles,
        added
    );
    let _ = fs::remove_dir_all(&workspace);
}

#[test]
fn monolithic_mode_rewrites_the_whole_corpus_for_contrast() {
    // The same single mutation under monolithic mode rewrites the entire
    // corpus, which is what threshold-selected sharding avoids.
    let workspace = temp_workspace("monolith-contrast");
    init_workspace(&workspace);
    // No checkpoint section: adaptive selection stays monolithic at 300
    // issues, far under every default threshold.
    create_issues(&workspace, 300);
    flush_ok(&workspace);
    assert_eq!(pointer_mode(&workspace), "monolithic");

    let before = object_inventory(&workspace);
    let issue_id = first_issue_id(&workspace);
    run_ok(
        &workspace,
        &["update", &issue_id, "--notes", "one mutation"],
    );
    flush_ok(&workspace);
    let after = object_inventory(&workspace);
    let added = added_objects(&before, &after);
    let added_bytes: u64 = added.iter().map(|p| after[p]).sum();
    let corpus_bytes = sum_bytes(&before);

    assert_eq!(
        added.len(),
        1,
        "monolithic publication rewrites one whole-corpus object"
    );
    assert!(
        added_bytes * 2 > corpus_bytes,
        "monolithic rewrite must carry (nearly) the whole corpus: {} of {}",
        added_bytes,
        corpus_bytes
    );
    let _ = fs::remove_dir_all(&workspace);
}

// ---------------------------------------------------------------------------
// Acceptance 4: monolithic and sharded restores are semantically equivalent
// ---------------------------------------------------------------------------

/// Populate a workspace with issues, graph edges, and audit history, then
/// round-trip it once through a restore so the corpus carries a provenance
/// receipt as well.
fn populated_source(tag: &str) -> PathBuf {
    let workspace = temp_workspace(tag);
    init_workspace(&workspace);
    for i in 1..=6 {
        run_ok(&workspace, &["create", "--title", &format!("Source {}", i)]);
    }
    let ids: Vec<String> = {
        let output = run_bead(&workspace, &["list", "--json"]);
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(|line| {
                serde_json::from_str::<Value>(line).unwrap()["id"]
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect()
    };
    run_ok(&workspace, &["label", "add", &ids[0], "--label", "source"]);
    run_ok(&workspace, &["dep", "add", &ids[0], &ids[1]]);
    run_ok(&workspace, &["update", &ids[2], "--status", "in_progress"]);
    run_ok(
        &workspace,
        &["update", &ids[3], "--notes", "carried through restore"],
    );

    // One restore round-trip seeds a provenance receipt into the corpus.
    flush_ok(&workspace);
    let restored = temp_workspace(&format!("{}-seed", tag));
    init_workspace(&restored);
    run_ok(
        &restored,
        &[
            "sync",
            "import-only",
            "--input",
            checkpoint_dir(&workspace).to_str().unwrap(),
            "--restore-into-empty",
            "--actor",
            "equivalence-seed",
        ],
    );
    let _ = fs::remove_dir_all(&workspace);
    restored
}

fn restore_copy(tag: &str, source: &Path, mode: &str) -> PathBuf {
    // Publish `mode` from the source, then copy the checkpoint set aside so
    // both representations exist independently.
    write_checkpoint_config(source, json!({ "mode": mode }));
    flush_ok(source);
    let copy = std::env::temp_dir().join(format!(
        "test-mode-selection-{}-cp-{}-{}",
        tag,
        mode,
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&copy);
    fs_extra_copy_dir(&checkpoint_dir(source), &copy);
    copy
}

fn fs_extra_copy_dir(from: &Path, to: &Path) {
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
    walk(from, to);
}

fn restore_into(tag: &str, checkpoint: &Path) -> PathBuf {
    let workspace = temp_workspace(tag);
    init_workspace(&workspace);
    run_ok(
        &workspace,
        &[
            "sync",
            "import-only",
            "--input",
            checkpoint.to_str().unwrap(),
            "--restore-into-empty",
            "--actor",
            "equivalence-test",
        ],
    );
    workspace
}

fn captured_json_lines(workspace: &Path, args: &[&str]) -> String {
    let output = run_bead(workspace, args);
    assert!(
        output.status.success(),
        "`bead {}` failed: {}",
        args.join(" "),
        stderr_of(&output)
    );
    String::from_utf8_lossy(&output.stdout).to_string()
}

#[test]
fn monolithic_and_sharded_restores_are_equivalent() {
    let source = populated_source("equivalence");

    let monolith_set = restore_copy("equivalence", &source, "monolithic");
    let sharded_set = restore_copy("equivalence", &source, "sharded");

    // Both representations of the same state must agree on the record counts.
    let monolith_pointer: Value = {
        let content = fs::read_to_string(monolith_set.join("current.json")).unwrap();
        serde_json::from_str(&content).unwrap()
    };
    let sharded_pointer: Value = {
        let content = fs::read_to_string(sharded_set.join("current.json")).unwrap();
        serde_json::from_str(&content).unwrap()
    };
    for field in [
        "issue_count",
        "event_count",
        "receipt_count",
        "total_record_count",
    ] {
        assert_eq!(
            monolith_pointer[field], sharded_pointer[field],
            "mode representations must agree on {}",
            field
        );
    }

    let from_monolith = restore_into("equivalence-rm", &monolith_set);
    let from_sharded = restore_into("equivalence-rs", &sharded_set);

    // Canonical public state: identical issue corpus.
    let monolith_list = captured_json_lines(&from_monolith, &["list", "--json"]);
    let sharded_list = captured_json_lines(&from_sharded, &["list", "--json"]);
    assert_eq!(
        monolith_list, sharded_list,
        "restored issue corpora must be identical"
    );
    assert!(
        monolith_list.lines().count() == monolith_pointer["issue_count"].as_u64().unwrap() as usize,
        "both restores must return the pointer-declared issue count"
    );

    // Audit-event history: identical for every event the checkpoints carried
    // (each restore then appends its own receipt event, which is not part of
    // the representation under comparison).
    let carried_events = monolith_pointer["event_count"].as_u64().unwrap() as usize;
    for workspace in [&from_monolith, &from_sharded] {
        let feed: Value = {
            let raw = captured_json_lines(workspace, &["changes", "--since", "0", "--json"]);
            serde_json::from_str(&raw).expect("changes --json must be one JSON document")
        };
        let mutations = feed["mutations"].as_array().unwrap();
        assert!(
            mutations.len() >= carried_events,
            "restore must carry the checkpoint's {} events",
            carried_events
        );
    }
    let monolith_feed: Value = {
        let raw = captured_json_lines(&from_monolith, &["changes", "--since", "0", "--json"]);
        serde_json::from_str(&raw).unwrap()
    };
    let sharded_feed: Value = {
        let raw = captured_json_lines(&from_sharded, &["changes", "--since", "0", "--json"]);
        serde_json::from_str(&raw).unwrap()
    };
    let monolith_mutations = &monolith_feed["mutations"].as_array().unwrap()[..carried_events];
    let sharded_mutations = &sharded_feed["mutations"].as_array().unwrap()[..carried_events];
    assert_eq!(
        monolith_mutations, sharded_mutations,
        "restored audit-event histories must be identical"
    );

    // Both restored workspaces must themselves be whole.
    run_ok(&from_monolith, &["sync", "status"]);
    run_ok(&from_sharded, &["sync", "status"]);

    for dir in [
        &source,
        &monolith_set,
        &sharded_set,
        &from_monolith,
        &from_sharded,
    ] {
        let _ = fs::remove_dir_all(dir);
    }
}

// ---------------------------------------------------------------------------
// Recorded-threshold and partition-plan retention across sharded flushes
// ---------------------------------------------------------------------------

#[test]
fn sharded_partition_plan_and_thresholds_are_retained_across_flushes() {
    let workspace = temp_workspace("retention");
    init_workspace(&workspace);
    write_checkpoint_config(
        &workspace,
        json!({
            // 40 issues over 16 prefixes force several splits; the 3-record
            // monolith limit selects sharded in the first place.
            "thresholds": thresholds_with(json!({
                "max_monolith_issue_records": 3,
                "max_shard_issue_records": 2
            })),
        }),
    );
    create_issues(&workspace, 40);
    flush_ok(&workspace);
    assert_eq!(pointer_mode(&workspace), "sharded");

    let manifest = read_active_manifest(&workspace);
    let partition: Vec<String> = manifest["issue_partition"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p.as_str().unwrap().to_string())
        .collect();
    assert!(
        partition.len() > 16,
        "overflowing prefixes must have split: {:?}",
        partition
    );

    // Drop the workspace override: the next flush must consult the
    // thresholds recorded in the previous manifest and keep its plan.
    remove_checkpoint_config(&workspace);
    let issue_id = first_issue_id(&workspace);
    run_ok(
        &workspace,
        &["update", &issue_id, "--notes", "retained plan"],
    );
    flush_ok(&workspace);
    assert_eq!(pointer_mode(&workspace), "sharded");

    let new_manifest = read_active_manifest(&workspace);
    let new_partition: Vec<String> = new_manifest["issue_partition"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p.as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        new_partition, partition,
        "an existing valid partition plan must be retained, not rebuilt"
    );
    assert_eq!(
        new_manifest["partition_thresholds"], manifest["partition_thresholds"],
        "the recorded threshold table must be retained across flushes"
    );

    let _ = fs::remove_dir_all(&workspace);
}

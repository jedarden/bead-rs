//! Checkpoint mode selection at the CLI boundary (checkpoint-set-v2 spec 9)
//!
//! Adaptive selection -- the default, and any configuration that records
//! thresholds without an explicit `checkpoint.mode` -- publishes `set-v2`
//! for every workspace. An operator may still force monolithic output,
//! which keeps its existing safety limits: within them the flush publishes
//! a monolith, beyond any of them the flush is refused with exit 1 and no
//! pointer change (spec 12 scenario `set-v2-forced-monolith-limits`).
//! Forcing sharded-v1 output is not offered; the request resolves to
//! `set-v2`, whose incremental-layout behavior (budget, split, merge,
//! seal) is covered by `checkpoint_set_v2_conformance.rs`. Selection never
//! affects reading: restore and import dispatch on the pointer and accept
//! `monolithic`, `sharded`, and `set-v2` (spec 3), so a forced monolith
//! and a set-v2 generation of the same store are interchangeable restore
//! sources.
//!
//! `sync flush-only` reads the operator's `.beads/config.json`
//! `checkpoint` section -- never a hardcoded constant. These tests verify:
//!
//! - adaptive resolves to set-v2 on a fresh workspace, with and without a
//!   recorded threshold table (the table keeps only its forced-monolith
//!   safety role)
//! - a forced monolith is honored within the limits and refused beyond
//!   them, leaving the pointer byte-identical
//! - forced monolithic output rewrites the whole corpus (the per-publication
//!   cost the adaptive layout exists to avoid)
//! - monolithic and set-v2 restores are semantically equivalent

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
    // Test runners may place their temporary directory below an unrelated
    // ancestor `.beads` store.  Initializing this deliberately isolated test
    // workspace is exactly the opt-in case for the discovery override; all
    // subsequent commands still discover the local bead-rs workspace
    // normally.
    run_ok(dir, &["init", "--skip-foreign-workspace"]);
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
    serde_json::from_str::<Value>(&stdout)
        .expect("list --json must emit one JSON array")
        .as_array()
        .and_then(|issues| issues.first())
        .expect("list --json must return at least one issue")
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

fn pointer_mode(workspace: &Path) -> String {
    read_current_pointer(workspace)["mode"]
        .as_str()
        .unwrap()
        .to_string()
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
// Adaptive selection resolves to set-v2 (spec 9)
// ---------------------------------------------------------------------------

#[test]
fn adaptive_publishes_set_v2_on_a_fresh_workspace() {
    let workspace = temp_workspace("adaptive-fresh");
    init_workspace(&workspace);
    // No checkpoint section at all: adaptive selection applies.
    create_issues(&workspace, 4);
    flush_ok(&workspace);
    assert_eq!(pointer_mode(&workspace), "set-v2");
    let root = read_current_pointer(&workspace)["active_root"]["path"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        root.starts_with("set/manifest-"),
        "the set-v2 active root must be a generation manifest: {}",
        root
    );
    let _ = fs::remove_dir_all(&workspace);
}

#[test]
fn recorded_thresholds_keep_only_the_forced_monolith_safety_role() {
    // The same four issues against a recorded 3-record limit: under the
    // pre-set-v2 policy this crossed the limit and republished as sharded.
    // Adaptive selection now publishes set-v2 regardless, because the
    // threshold table bounds only a forced monolith (spec 9).
    let workspace = temp_workspace("thresholds-safety-role");
    init_workspace(&workspace);
    write_checkpoint_config(
        &workspace,
        json!({ "thresholds": thresholds_with(json!({ "max_monolith_issue_records": 3 })) }),
    );
    create_issues(&workspace, 4);
    flush_ok(&workspace);
    assert_eq!(pointer_mode(&workspace), "set-v2");
    let _ = fs::remove_dir_all(&workspace);
}

#[test]
fn forced_set_v2_and_sharded_requests_publish_set_v2() {
    // Forcing sharded-v1 output is not offered (spec 9): the request
    // resolves to set-v2 ...
    let workspace = temp_workspace("forced-sharded");
    init_workspace(&workspace);
    write_checkpoint_config(&workspace, json!({ "mode": "sharded" }));
    create_issues(&workspace, 2);
    flush_ok(&workspace);
    assert_eq!(pointer_mode(&workspace), "set-v2");
    let _ = fs::remove_dir_all(&workspace);

    // ...and an explicit set-v2 request publishes set-v2.
    let workspace = temp_workspace("forced-set-v2");
    init_workspace(&workspace);
    write_checkpoint_config(&workspace, json!({ "mode": "set-v2" }));
    create_issues(&workspace, 2);
    flush_ok(&workspace);
    assert_eq!(pointer_mode(&workspace), "set-v2");
    let _ = fs::remove_dir_all(&workspace);
}

// ---------------------------------------------------------------------------
// Forced monolithic output keeps its safety limits
// (spec 12 scenario `set-v2-forced-monolith-limits`)
// ---------------------------------------------------------------------------

#[test]
fn set_v2_forced_monolith_limits() {
    // Within the limits: forcing monolithic output succeeds, and the
    // generation really is a monolith.
    let workspace = temp_workspace("forced-monolith-within");
    init_workspace(&workspace);
    write_checkpoint_config(&workspace, json!({ "mode": "monolithic" }));
    create_issues(&workspace, 4);
    flush_ok(&workspace);
    assert_eq!(pointer_mode(&workspace), "monolithic");
    let _ = fs::remove_dir_all(&workspace);

    // Beyond the limits: the flush is refused with exit 1 and no pointer
    // change. Automatic publication is suppressed so the refusal belongs to
    // the explicit flush instead of a create failing mid-mutation, and the
    // workspace first publishes within the limit so the pointer exists to
    // be compared.
    let workspace = temp_workspace("forced-monolith-beyond");
    init_workspace(&workspace);
    write_checkpoint_config(
        &workspace,
        json!({
            "mode": "monolithic",
            "auto_flush": false,
            "thresholds": thresholds_with(json!({ "max_monolith_issue_records": 3 })),
        }),
    );
    create_issues(&workspace, 2);
    flush_ok(&workspace);
    assert_eq!(pointer_mode(&workspace), "monolithic");
    let pointer_before = fs::read(checkpoint_dir(&workspace).join("current.json")).unwrap();

    // Cross the limit: three more issues (distinct titles) put the corpus
    // at 5 issue records against a 3-record limit.
    for i in 1..=3 {
        run_ok(
            &workspace,
            &["create", "--title", &format!("Overflow {}", i)],
        );
    }
    let output = flush(&workspace);
    assert_eq!(
        output.status.code(),
        Some(1),
        "the refusal must exit 1: {:?}",
        output.status
    );
    assert!(
        stderr_of(&output).contains("exceed recorded safety limits"),
        "refusal must name the recorded limits: {}",
        stderr_of(&output)
    );
    let pointer_after = fs::read(checkpoint_dir(&workspace).join("current.json")).unwrap();
    assert_eq!(
        pointer_before, pointer_after,
        "a refused forced-monolith flush must not change the pointer"
    );
    let _ = fs::remove_dir_all(&workspace);
}

#[test]
fn invalid_recorded_checkpoint_config_is_rejected() {
    for (tag, section, expected) in [
        ("bad-mode", json!({ "mode": "bogus" }), "checkpoint.mode"),
        (
            "bad-auto-flush",
            json!({ "auto_flush": "sometimes" }),
            "checkpoint.auto_flush",
        ),
        (
            "zero-limit",
            json!({ "thresholds": json!({ "version": 1, "max_monolith_issue_records": 0 }) }),
            "checkpoint.thresholds",
        ),
    ] {
        let workspace = temp_workspace(tag);
        init_workspace(&workspace);
        write_checkpoint_config(&workspace, section);
        // The invalid setting also prevents automatic publication. Suppress
        // that post-commit path explicitly so this test can reach and assert
        // the explicit flush's configuration validation; R026's mutation
        // behavior is covered in post_commit_publication.rs.
        run_ok(
            &workspace,
            &["--no-auto-flush", "create", "--title", "Issue 1"],
        );
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
// Forced monolithic output rewrites the whole corpus (the contrast that
// motivates the adaptive layout, spec 1)
// ---------------------------------------------------------------------------

#[test]
fn forced_monolithic_rewrites_the_whole_corpus() {
    let workspace = temp_workspace("monolith-contrast");
    init_workspace(&workspace);
    write_checkpoint_config(
        &workspace,
        json!({ "mode": "monolithic", "auto_flush": false }),
    );
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
// Monolithic and set-v2 restores are semantically equivalent
// (spec 10 dual-layout readers, spec 12 scenario `set-v2-forced-monolith-limits`
// within-limits leg)
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
        let output = run_bead(&workspace, &["list", "--jsonl"]);
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
fn monolithic_and_set_v2_restores_are_equivalent() {
    let source = populated_source("equivalence");

    let monolith_set = restore_copy("equivalence", &source, "monolithic");
    let setv2_set = restore_copy("equivalence", &source, "set-v2");

    // Both representations of the same state must agree on the record counts.
    let monolith_pointer: Value = {
        let content = fs::read_to_string(monolith_set.join("current.json")).unwrap();
        serde_json::from_str(&content).unwrap()
    };
    let setv2_pointer: Value = {
        let content = fs::read_to_string(setv2_set.join("current.json")).unwrap();
        serde_json::from_str(&content).unwrap()
    };
    assert_eq!(
        monolith_pointer["mode"], setv2_pointer["mode"],
        "the two representations must really be different layouts"
    );
    for field in ["issue_count", "event_count", "receipt_count"] {
        assert_eq!(
            monolith_pointer[field], setv2_pointer[field],
            "mode representations must agree on {}",
            field
        );
    }

    let from_monolith = restore_into("equivalence-rm", &monolith_set);
    let from_setv2 = restore_into("equivalence-rs", &setv2_set);

    // Canonical public state: identical issue corpus.
    let monolith_list = captured_json_lines(&from_monolith, &["list", "--jsonl"]);
    let setv2_list = captured_json_lines(&from_setv2, &["list", "--jsonl"]);
    assert_eq!(
        monolith_list, setv2_list,
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
    for workspace in [&from_monolith, &from_setv2] {
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
    let setv2_feed: Value = {
        let raw = captured_json_lines(&from_setv2, &["changes", "--since", "0", "--json"]);
        serde_json::from_str(&raw).unwrap()
    };
    let monolith_mutations = &monolith_feed["mutations"].as_array().unwrap()[..carried_events];
    let setv2_mutations = &setv2_feed["mutations"].as_array().unwrap()[..carried_events];
    assert_eq!(
        monolith_mutations, setv2_mutations,
        "restored audit-event histories must be identical"
    );

    // Both restored workspaces must themselves be whole.
    run_ok(&from_monolith, &["sync", "status"]);
    run_ok(&from_setv2, &["sync", "status"]);

    for dir in [
        &source,
        &monolith_set,
        &setv2_set,
        &from_monolith,
        &from_setv2,
    ] {
        let _ = fs::remove_dir_all(dir);
    }
}

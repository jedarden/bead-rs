//! beadrs-43d4bcb6: the `forensic.jsonl` compatibility view exists only while
//! the active generation is a monolith. A sharded-v1 generation retires it (a
//! stale view would let the documented `import-only --input forensic.jsonl`
//! recovery restore old state), and a later monolithic generation recreates
//! it byte-identical to the pointer-selected root.
//!
//! Since checkpoint-set-v2 spec 9 a forced sharded request publishes
//! set-v2, whose layout freezes the view instead (spec 8: the retained
//! file is a frozen historical view of the last monolith; readers must
//! use the pointer-selected segments). This file pins the reachable
//! pair: set-v2 freezes, monolith recreates byte-identical.

use std::path::Path;
use std::process::Command;

use serde_json::Value;

fn ok(workspace: &Path, args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_bead"))
        .args(args)
        .current_dir(workspace)
        .env("BEAD_ORG_SECRET_SCANNER", "off")
        .output()
        .expect("run bead");
    assert!(
        output.status.success(),
        "bead {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn git(workspace: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(workspace)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn set_v2_generations_freeze_the_view_and_monolithic_ones_restore_it() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    git(root, &["init", "-q"]);
    ok(root, &["init", "--skip-foreign-workspace"]);
    force_monolithic_mode(root);
    for index in 0..5 {
        ok(root, &["create", "--title", &format!("issue {index}")]);
    }
    let view = root.join(".beads/checkpoint/forensic.jsonl");
    assert!(view.exists(), "a monolithic workspace carries the view");
    let view_before = std::fs::read(&view).expect("view bytes");
    git(root, &["add", ".beads/checkpoint"]);
    git(
        root,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.invalid",
            "commit",
            "-qm",
            "baseline",
        ],
    );

    ok(root, &["sync", "configure", "--mode", "sharded"]);
    // Forcing sharded output publishes set-v2 (checkpoint-set-v2 spec 9),
    // and a set-v2 generation freezes the view rather than retiring it
    // (spec 8, scenario set-v2-forensic-frozen): the bytes stay exactly as
    // the last monolithic generation wrote them, and the frozen file is
    // not staged for removal.
    assert!(
        view.exists(),
        "the set-v2 generation retains the view frozen"
    );
    assert_eq!(
        std::fs::read(&view).expect("view bytes"),
        view_before,
        "the frozen view must keep the retiring monolith's bytes verbatim"
    );
    let staged = git(root, &["diff", "--cached", "--name-status"]);
    assert!(
        !staged
            .lines()
            .any(|line| line.starts_with('D') && line.ends_with("forensic.jsonl")),
        "the frozen view must not be staged for removal: {staged}"
    );
    // Later set-v2 publications leave the frozen view untouched — bytes
    // and mtime (spec 9, set-v2-forensic-frozen).
    let mtime_before = std::fs::metadata(&view)
        .expect("view metadata")
        .modified()
        .unwrap();
    ok(root, &["create", "--title", "after sharding"]);
    let pointer: Value = serde_json::from_str(
        &std::fs::read_to_string(root.join(".beads/checkpoint/current.json")).expect("pointer"),
    )
    .expect("pointer json");
    assert_eq!(pointer["mode"], "set-v2");
    assert_eq!(
        std::fs::read(&view).expect("view bytes"),
        view_before,
        "publications under set-v2 must not rewrite the frozen view"
    );
    assert_eq!(
        std::fs::metadata(&view)
            .expect("view metadata")
            .modified()
            .unwrap(),
        mtime_before,
        "publications under set-v2 must not touch the frozen view"
    );

    ok(root, &["sync", "configure", "--mode", "monolithic"]);
    assert!(view.exists(), "a monolithic generation recreates the view");
    let pointer: Value = serde_json::from_str(
        &std::fs::read_to_string(root.join(".beads/checkpoint/current.json")).expect("pointer"),
    )
    .expect("pointer json");
    let root_path = pointer["active_root"]["path"].as_str().expect("root path");
    assert_eq!(
        std::fs::read(&view).expect("view"),
        std::fs::read(root.join(".beads/checkpoint").join(root_path)).expect("root"),
        "the view is byte-identical to the pointer-selected monolith"
    );
}

/// The adaptive default resolves to set-v2 since 5d4c08a6 (checkpoint-set-v2
/// spec 9); these legs assert the monolith layout, so pin the mode they were
/// written against.
fn force_monolithic_mode(workspace: &std::path::Path) {
    let path = workspace.join(".beads/config.json");
    let mut config: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    config
        .as_object_mut()
        .unwrap()
        .entry("checkpoint")
        .or_insert(serde_json::Value::Object(Default::default()))
        .as_object_mut()
        .unwrap()
        .insert(
            "mode".into(),
            serde_json::Value::String("monolithic".to_string()),
        );
    std::fs::write(&path, serde_json::to_string_pretty(&config).unwrap()).unwrap();
}

//! beadrs-43d4bcb6: the `forensic.jsonl` compatibility view exists only while
//! the active generation is a monolith. A sharded generation retires it (a
//! stale view would let the documented `import-only --input forensic.jsonl`
//! recovery restore old state), and a later monolithic generation recreates
//! it byte-identical to the pointer-selected root.

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
fn sharded_generations_retire_the_view_and_monolithic_ones_restore_it() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    git(root, &["init", "-q"]);
    ok(root, &["init", "--skip-foreign-workspace"]);
    for index in 0..5 {
        ok(root, &["create", "--title", &format!("issue {index}")]);
    }
    let view = root.join(".beads/checkpoint/forensic.jsonl");
    assert!(view.exists(), "a monolithic workspace carries the view");
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
    assert!(!view.exists(), "the sharded generation retires the view");
    // The retirement is staged so the next commit carries it.
    let staged = git(root, &["diff", "--cached", "--name-status"]);
    assert!(
        staged
            .lines()
            .any(|line| line.starts_with('D') && line.ends_with("forensic.jsonl")),
        "view removal staged: {staged}"
    );
    // Later sharded generations do not resurrect it.
    ok(root, &["create", "--title", "after sharding"]);
    assert!(!view.exists());
    let pointer: Value = serde_json::from_str(
        &std::fs::read_to_string(root.join(".beads/checkpoint/current.json")).expect("pointer"),
    )
    .expect("pointer json");
    assert_eq!(pointer["mode"], "sharded");

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

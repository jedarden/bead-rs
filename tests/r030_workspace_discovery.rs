//! R030: self-defending workspace discovery (plan section 12 R030,
//! ideas-ledger 2026-08-15 finalist 3).
//!
//! Discovery walks up from the current directory and stops at the FIRST
//! `.beads` directory it encounters. When that directory carries the bead-rs
//! workspace fingerprint (`.beads/config.json`) it is the workspace, as
//! before. When it does not, discovery fails closed instead of continuing to
//! an unrelated parent workspace -- the 2026-08-14 incident class, where a
//! command silently operated on the wrong store and a misdirected repair
//! reinitialized it with the wrong schema.
//!
//! The contract pinned here:
//!
//! - the walk terminates at any `.beads` directory, recognized or not;
//! - the fail-closed diagnostic names the directory's path and claims only
//!   that it is not a bead-rs workspace -- it never identifies which foreign
//!   format occupies it (clean-room boundary: the message is derived from
//!   the absence of the fingerprint alone, never from inspecting contents);
//! - `--skip-foreign-workspace` is the explicit override for legitimate
//!   nesting: discovery continues past the unrecognized `.beads` and operates
//!   on the bead-rs workspace above, from either argument position, and a
//!   mutation under the override writes to that workspace while leaving the
//!   unrecognized directory untouched;
//! - the override only widens the search. `init` never writes into a `.beads`
//!   it does not recognize, with or without the flag; with the flag and a
//!   workspace above, `init` reports that workspace instead;
//! - `doctor` reports the state fail-closed rather than diagnosing a parent
//!   workspace;
//! - a plain subdirectory of a real workspace still discovers it (the walk
//!   stopping at `.beads` directories changes nothing when the first one is
//!   ours).

use assert_cmd::Command;
use std::fs;
use std::path::{Path, PathBuf};

/// The foreign `.beads` fixture: a directory whose contents are deliberately
/// meaningless to bead-rs. Nothing in the implementation inspects them --
/// only the absence of `config.json` decides -- so any non-empty payload
/// stands in for "some other tool's store" without copying any real foreign
/// layout across the clean-room boundary.
const FOREIGN_MARKER: &str = "not-a-bead-rs-store";

fn bead(dir: &Path) -> Command {
    let mut cmd = Command::cargo_bin("bead").unwrap();
    cmd.current_dir(dir);
    cmd
}

fn run(dir: &Path, args: &[&str]) -> std::process::Output {
    bead(dir).args(args).assert().success().get_output().clone()
}

/// Nested layout: a real bead-rs workspace at `parent`, an unrecognized
/// `.beads` at `child`, and `deep` inside `child` where discovery starts.
///
/// The layout's `init` runs under the override so the fixture is hermetic:
/// the walk from `parent` leaves the layout upward, and a machine may keep
/// featureless `.beads` debris above the temporary root (this one keeps it
/// in `$HOME` above `$TMPDIR`). Creating a workspace nested under
/// unrecognized `.beads` directories is exactly what the override permits,
/// and it is a no-op on a machine whose ancestry is clean.
fn nested_layout() -> (tempfile::TempDir, PathBuf, PathBuf, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let parent = temp.path().join("parent");
    let child = parent.join("tool");
    let deep = child.join("deep");
    fs::create_dir_all(&deep).unwrap();

    run(&parent, &["--skip-foreign-workspace", "init"]);

    let foreign_beads = child.join(".beads");
    fs::create_dir_all(&foreign_beads).unwrap();
    fs::write(foreign_beads.join("store.txt"), FOREIGN_MARKER).unwrap();

    (temp, parent, child, deep)
}

fn create_issue(dir: &Path, title: &str) -> String {
    let output = run(dir, &["create", "--title", title]);
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

/// Assert the foreign `.beads` still holds exactly the fixture file and
/// nothing bead-rs wrote.
fn assert_foreign_untouched(foreign_beads: &Path) {
    assert!(
        !foreign_beads.join("config.json").exists(),
        "bead-rs wrote a config.json into the unrecognized {foreign_beads:?}"
    );
    assert!(
        !foreign_beads.join("beads.db").exists(),
        "bead-rs wrote a database into the unrecognized {foreign_beads:?}"
    );
    let entries: Vec<_> = fs::read_dir(foreign_beads)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(
        entries,
        vec!["store.txt".to_string()],
        "unrecognized {foreign_beads:?} was modified: {entries:?}"
    );
    assert_eq!(
        fs::read_to_string(foreign_beads.join("store.txt")).unwrap(),
        FOREIGN_MARKER
    );
}

#[test]
fn discovery_stops_at_first_beads_and_fails_closed() {
    let (_temp, parent, child, deep) = nested_layout();

    let output = bead(&deep)
        .args(["list"])
        .assert()
        .failure()
        .code(3)
        .get_output()
        .clone();
    let stderr = String::from_utf8(output.stderr).unwrap();

    // Names the unrecognized directory itself...
    assert!(
        stderr.contains(&child.join(".beads").display().to_string()),
        "diagnostic must name the unrecognized .beads path: {stderr}"
    );
    // ...claims only that it is not a bead-rs workspace...
    assert!(
        stderr.contains("not a bead-rs workspace"),
        "diagnostic must state the not-a-bead-rs-workspace claim: {stderr}"
    );
    // ...and never the parent workspace it refused to reach for, nor any
    // foreign format identification.
    assert!(
        !stderr.contains(&parent.join(".beads").display().to_string()),
        "diagnostic must not point at the parent workspace: {stderr}"
    );
    assert!(
        !stderr.contains("forge") && !stderr.contains("yaml") && !stderr.contains("bead-forge"),
        "diagnostic must not identify the foreign format: {stderr}"
    );
}

#[test]
fn empty_beads_directory_also_stops_the_walk() {
    let temp = tempfile::tempdir().unwrap();
    let parent = temp.path().join("parent");
    fs::create_dir_all(&parent).unwrap();
    let empty_beads = parent.join(".beads");
    fs::create_dir_all(&empty_beads).unwrap();

    let output = bead(&parent)
        .args(["list"])
        .assert()
        .failure()
        .code(3)
        .get_output()
        .clone();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains(&empty_beads.display().to_string())
            && stderr.contains("not a bead-rs workspace"),
        "an empty .beads is not a bead-rs workspace either: {stderr}"
    );
}

#[test]
fn override_flag_operates_on_workspace_above_from_both_positions() {
    let (_temp, parent, _child, deep) = nested_layout();
    let id = create_issue(&parent, "flag position");

    // Without the flag the walk stops at the unrecognized `.beads`.
    bead(&deep)
        .args(["list", "--json"])
        .assert()
        .failure()
        .code(3);

    // With it, `list` resolves the parent workspace from either argument
    // position (the flag is global, like --no-auto-flush).
    for args in [
        vec!["--skip-foreign-workspace", "list", "--json"],
        vec!["list", "--json", "--skip-foreign-workspace"],
    ] {
        bead(&deep)
            .args(&args)
            .assert()
            .success()
            .stdout(predicates::str::contains(&id));
    }
}

#[test]
fn mutation_under_override_writes_to_parent_not_foreign_directory() {
    let (_temp, parent, child, deep) = nested_layout();

    // The create runs with the flag and prints the new bead's ID.
    let output = bead(&deep)
        .args([
            "--skip-foreign-workspace",
            "create",
            "--title",
            "via override",
        ])
        .assert()
        .success()
        .get_output()
        .clone();
    let id = String::from_utf8(output.stdout).unwrap().trim().to_string();
    assert!(!id.is_empty(), "create printed its bead ID");

    // The bead landed in the parent workspace and is visible from below
    // only through the override.
    bead(&deep)
        .args(["--skip-foreign-workspace", "list", "--json"])
        .assert()
        .success()
        .stdout(predicates::str::contains(&id));
    bead(&deep)
        .args(["list", "--json"])
        .assert()
        .failure()
        .code(3);

    assert!(parent.join(".beads/beads.db").exists());
    assert_foreign_untouched(&child.join(".beads"));
}

#[test]
fn init_refuses_to_write_into_foreign_beads_even_with_override() {
    // No bead-rs workspace anywhere above: with or without the override,
    // there is nothing to find, and init must still not write here.
    let temp = tempfile::tempdir().unwrap();
    let child = temp.path().join("tool");
    fs::create_dir_all(&child).unwrap();
    let foreign_beads = child.join(".beads");
    fs::create_dir_all(&foreign_beads).unwrap();
    fs::write(foreign_beads.join("store.txt"), FOREIGN_MARKER).unwrap();

    for args in [vec!["init"], vec!["--skip-foreign-workspace", "init"]] {
        let output = bead(&child)
            .args(&args)
            .assert()
            .failure()
            .code(3)
            .get_output()
            .clone();
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(
            stderr.contains(&foreign_beads.display().to_string())
                && stderr.contains("not a bead-rs workspace"),
            "init must refuse with the fail-closed diagnostic: {stderr}"
        );
    }

    assert_foreign_untouched(&foreign_beads);
}

#[test]
fn init_with_override_reports_workspace_above_instead() {
    let (_temp, parent, child, deep) = nested_layout();

    let output = bead(&deep)
        .args(["--skip-foreign-workspace", "init"])
        .assert()
        .success()
        .get_output()
        .clone();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("Workspace already exists"),
        "init under the override should report the parent workspace: {stderr}"
    );
    assert!(stderr.contains(&parent.display().to_string()));

    assert_foreign_untouched(&child.join(".beads"));
}

#[test]
fn doctor_reports_foreign_beads_fail_closed() {
    let (_temp, parent, child, deep) = nested_layout();

    let output = bead(&deep)
        .args(["doctor"])
        .assert()
        .failure()
        .code(3)
        .get_output()
        .clone();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let combined = format!("{stdout}{stderr}");

    assert!(
        combined.contains(&child.join(".beads").display().to_string()),
        "doctor must name the unrecognized .beads path: {combined}"
    );
    assert!(
        combined.contains("not a bead-rs workspace"),
        "doctor must state the not-a-bead-rs-workspace claim: {combined}"
    );
    assert!(
        !combined.contains(&parent.join(".beads").display().to_string()),
        "doctor must not diagnose the parent workspace: {combined}"
    );

    // Under the override, doctor runs against the workspace above.
    bead(&deep)
        .args(["--skip-foreign-workspace", "doctor"])
        .assert()
        .success();
}

#[test]
fn plain_subdirectory_still_discovers_its_workspace() {
    let (_temp, parent, _child, _deep) = nested_layout();
    let id = create_issue(&parent, "regression");

    let sub = parent.join("a").join("b");
    fs::create_dir_all(&sub).unwrap();
    bead(&sub)
        .args(["list", "--json"])
        .assert()
        .success()
        .stdout(predicates::str::contains(&id));
}

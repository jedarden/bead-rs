//! Regression test for the binary-build rule (beadrs-3804efb4, parent
//! beadrs-5a0dc962): a `scripts/build-from-archive.sh` run must never mutate
//! the shared checkout.
//!
//! The 2026-09-01/02 incident (docs/plan/plan.md, "Binary builds never mutate
//! the shared checkout") was pinned-binary builds stashing, resetting and
//! checking out inside the single shared NEEDLE checkout, one of which erased
//! another worker's uncommitted hour of work. The script is the only
//! sanctioned build path precisely because it builds from a `git archive`
//! extraction under `~/scratch` and never touches the shared tree. This file
//! encodes that promise as assertions on the shared checkout's own state,
//! snapshotted immediately before and after a full script run:
//!
//! - `git stash list` -- a run must not stash;
//! - `git reflog` -- a run must not reset, checkout or commit;
//! - `git rev-parse HEAD` -- the same violation class, checked directly,
//!   because the reflog can legitimately be empty (it is in this checkout:
//!   it was expired while recovering the incident) and an empty reflog makes
//!   the reflog comparison vacuous.
//!
//! Both runs are end to end. The success run extracts HEAD and compiles it
//! (`cargo build --release`, tens of seconds), so the checkout-untouched
//! assertions cover the whole arc of the script, not just its prologue. The
//! failure run appends `--features` with a value no package has: cargo
//! rejects that after the archive is extracted and the scratch dir exists,
//! before a single crate compiles, so the failing path is reachable quickly
//! and deterministically and the "scratch dir left in place for diagnosis"
//! rule can be observed.
//!
//! Both runs pass `--out` into a throwaway directory outside the shared
//! checkout, so a test run never leaves pin artifacts in `pinned-binaries/`
//! for the next worker's `git status` to trip over. The script supports that
//! flag for exactly this kind of redirected pinning.
//!
//! Concurrency caveat: another worker committing in this shared checkout
//! while the test runs legitimately moves HEAD and appends to the reflog and
//! will fail the comparison. The failure output shows both snapshots, so
//! check `git log` / `git reflog` timestamps for concurrent activity before
//! blaming the script.
//!
//! A packaged or `git archive` source tree has no local `.git` marker. In
//! that environment these VCS-state assertions are inapplicable and skip
//! explicitly; package verification exercises the script build separately.
//! If the marker exists, every Git discovery failure still fails the test.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use serial_test::serial;

const SCRIPT: &str = "scripts/build-from-archive.sh";
/// Something cargo rejects -- after the archive is extracted and the scratch
/// dir exists, before anything compiles -- so the script's failure path is
/// reachable quickly and deterministically.
const NO_SUCH_FEATURE: &str = "beadrs-archive-test-no-such-feature";

/// The script is landed by beadrs-53e55a45; until that commit arrives this
/// test has nothing to run and skips loudly instead of failing the suite.
fn require_script() -> Option<PathBuf> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(SCRIPT);
    if !path.exists() {
        let msg = format!(
            "skipping: {SCRIPT} does not exist yet (beadrs-53e55a45 has not landed); \
             there is no build path to hold to the checkout-untouched contract"
        );
        println!("{msg}");
        eprintln!("{msg}");
        return None;
    }
    Some(path)
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn require_vcs_checkout(root: &Path) -> bool {
    if root.join(".git").exists() {
        return true;
    }
    let msg = format!(
        "skipping checkout-state assertion: {} has no local .git marker \
         (expected for cargo package and git-archive verification)",
        root.display()
    );
    println!("{msg}");
    eprintln!("{msg}");
    false
}

fn scratch_base() -> PathBuf {
    // The script extracts into `mktemp -d -p ~/scratch`; CI builders have no
    // ~/scratch yet, and creating it is idempotent where it already exists.
    let base = PathBuf::from(std::env::var_os("HOME").expect("HOME is set")).join("scratch");
    std::fs::create_dir_all(&base).expect("create ~/scratch");
    base
}

fn git(root: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .expect("spawn git");
    assert!(
        out.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// The shared-checkout state whose equality across a script run is the whole
/// point of this file.
#[derive(Debug, Clone)]
struct SharedState {
    stash_list: String,
    reflog: String,
    head: String,
}

fn snapshot(root: &Path) -> SharedState {
    SharedState {
        stash_list: git(root, &["stash", "list"]),
        reflog: git(root, &["reflog"]),
        head: git(root, &["rev-parse", "HEAD"]),
    }
}

fn assert_shared_untouched(before: &SharedState, after: &SharedState) {
    let mut changed = Vec::new();
    if before.stash_list != after.stash_list {
        changed.push("git stash list");
    }
    if before.reflog != after.reflog {
        changed.push("git reflog");
    }
    if before.head != after.head {
        changed.push("git rev-parse HEAD");
    }
    assert!(
        changed.is_empty(),
        "the build-from-archive run mutated the shared checkout: {changed:?}\n\
         before: {before:#?}\n\
         after: {after:#?}\n\
         (a concurrent worker committing during the run also moves this state; \
         check git log / git reflog timestamps before blaming the script)"
    );
}

fn dir_entries(dir: &Path) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    if let Ok(read) = std::fs::read_dir(dir) {
        for entry in read.flatten() {
            names.insert(entry.file_name().to_string_lossy().into_owned());
        }
    }
    names
}

/// Entries present in `after` but not in `before`.
fn added_entries(before: &BTreeSet<String>, after: &BTreeSet<String>) -> Vec<String> {
    after.difference(before).cloned().collect()
}

/// Best-effort removal of entries a test run added under `dir` (depth 1), so
/// a green run leaves the directory exactly as it found it. Anything the test
/// fails to remove is named on stdout.
fn remove_added_entries(dir: &Path, before: &BTreeSet<String>, after: &BTreeSet<String>) {
    for name in added_entries(before, after) {
        let path = dir.join(&name);
        let result = if path.is_dir() {
            std::fs::remove_dir_all(&path)
        } else {
            std::fs::remove_file(&path)
        };
        match result {
            Ok(()) => println!("removed artifact the run added: {}", path.display()),
            Err(err) => println!(
                "could not remove artifact the run added ({}): {err}",
                path.display()
            ),
        }
    }
}

fn run_script(script: &Path, cwd: &Path, args: &[&str]) -> (Option<i32>, String) {
    let out = Command::new(script)
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("spawn scripts/build-from-archive.sh; is the exec bit committed?");
    let log = format!(
        "exit: {:?}\n--- stdout ---\n{}--- stderr ---\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.code(), log)
}

/// A throwaway pin destination outside the shared checkout, so the run's
/// binary and metadata never land in `pinned-binaries/`. Removed on success;
/// deliberately left behind on failure for diagnosis.
fn pin_out_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "beadrs-archive-test-{}-{}",
        label,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir); // a previous crashed run's leftover
    std::fs::create_dir_all(&dir).expect("create pin out dir");
    dir
}

#[test]
#[serial]
fn archive_build_leaves_shared_checkout_untouched() {
    let Some(script) = require_script() else {
        return;
    };
    let root = repo_root();
    if !require_vcs_checkout(&root) {
        return;
    }
    let sha = git(&root, &["rev-parse", "HEAD"]);
    let before = snapshot(&root);
    let scratch_before = dir_entries(&scratch_base());
    let out_dir = pin_out_dir("success");

    let (code, log) = run_script(
        &script,
        &root,
        &[
            &sha,
            "--name",
            "bead-archive-test",
            "--out",
            &out_dir.display().to_string(),
        ],
    );
    assert_eq!(
        code,
        Some(0),
        "scripts/build-from-archive.sh {sha} failed:\n{log}"
    );

    // The run really built and pinned something -- guards against a vacuous
    // exit 0 from a script that silently stopped early.
    assert!(
        out_dir.join("bead-archive-test").is_file()
            && out_dir.join("bead-archive-test.metadata.json").is_file(),
        "the run exited 0 but did not produce the pin and its metadata in {}:\n{log}",
        out_dir.display()
    );

    assert_shared_untouched(&before, &snapshot(&root));

    let leaked = added_entries(&scratch_before, &dir_entries(&scratch_base()));
    assert!(
        leaked.is_empty(),
        "the run succeeded but left scratch dir(s) behind in ~/scratch: {leaked:?}\n{log}"
    );

    let _ = std::fs::remove_dir_all(&out_dir);
}

#[test]
#[serial]
fn failed_archive_build_retains_scratch_dir() {
    let Some(script) = require_script() else {
        return;
    };
    let root = repo_root();
    if !require_vcs_checkout(&root) {
        return;
    }
    let sha = git(&root, &["rev-parse", "HEAD"]);
    let before = snapshot(&root);
    let scratch_before = dir_entries(&scratch_base());
    let out_dir = pin_out_dir("failure");

    let (code, log) = run_script(
        &script,
        &root,
        &[
            &sha,
            "--features",
            NO_SUCH_FEATURE,
            "--name",
            "bead-archive-test",
            "--out",
            &out_dir.display().to_string(),
        ],
    );
    assert_ne!(
        code,
        Some(0),
        "expected --features {NO_SUCH_FEATURE} to fail the build, but the script succeeded:\n{log}"
    );

    let retained = added_entries(&scratch_before, &dir_entries(&scratch_base()));
    assert!(
        !retained.is_empty(),
        "the run failed but no scratch dir remains under ~/scratch: it either failed \
         before creating one or removed its scratch dir on failure, so the \
         'leave it in place on failure for diagnosis' rule cannot hold\n{log}"
    );

    assert_shared_untouched(&before, &snapshot(&root));

    // The output above is the diagnostic record; do not leave the retained
    // dir to accumulate in the shared ~/scratch on every run.
    remove_added_entries(
        &scratch_base(),
        &scratch_before,
        &dir_entries(&scratch_base()),
    );
    let _ = std::fs::remove_dir_all(&out_dir);
}

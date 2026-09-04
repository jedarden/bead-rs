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
//! - `git rev-parse HEAD` -- a run must not commit or move the branch;
//! - `git reflog` -- the same violation class through the ref, checked
//!   separately because the reflog can legitimately be empty (it is in this
//!   checkout: it was expired while recovering the incident) and an empty
//!   reflog makes the reflog comparison vacuous;
//! - `git stash list` -- a run must not stash;
//! - `git ls-files -s` -- the index itself, as mode + blob + stage per path,
//!   so a `git add` the script never runs is caught even when HEAD and the
//!   worktree still look clean;
//! - `git status --porcelain` -- tracked working-tree content and untracked
//!   litter. This is the probe that carries the actual harm from the
//!   incident: `git checkout -- .` and `git restore` erase uncommitted work,
//!   and `git clean -fd` deletes untracked files, while moving no ref and
//!   appending nothing to any reflog, so neither HEAD, reflog, stash list
//!   nor the index notices. Verified 2026-09-04: after `git checkout -- .`
//!   discarded a tracked modification, all three VCS probes read back
//!   byte-identical to before it ran.
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
//! Concurrency caveat: this is a shared checkout on a box where several
//! workers work at once, and the build itself takes minutes. Another worker
//! committing, `git add`-ing, or editing a tracked file during the run
//! legitimately moves HEAD, appends to the reflog or changes the worktree and
//! fails the comparison. The one exception is `.beads/`, which every worker's
//! bead mutations republish as a git-tracked side effect and which a build
//! script has no way to write: the worktree probe excludes it so the test
//! measures this run, not the fleet. The failure output shows both snapshots,
//! so check `git log` / `git reflog` timestamps for concurrent activity
//! before blaming the script.
//!
//! A packaged or `git archive` source tree has no local `.git` marker. In
//! that environment these VCS-state assertions are inapplicable and skip
//! explicitly; package verification exercises the script build separately.
//! If the marker exists, every Git discovery failure still fails the test.
//!
//! Skipping on `.git` absence alone would be careless: absence is also what
//! a checkout looks like after its `.git` is destroyed, and what any
//! directory looks like once git's upward discovery walks out of it and
//! resolves a parent work tree. The second case is the dangerous one -- the
//! probes would snapshot a repository this test did not select, and the
//! script resolves its own `REPO` the same way, so the run would measure and
//! build from the wrong checkout. git is therefore asked to confirm the
//! absence, and disagreement is a failure rather than a skip.
//!
//! The checkout under test defaults to the tree this binary was compiled in
//! (`CARGO_MANIFEST_DIR`) and can be aimed elsewhere with the
//! `BEADRS_ARCHIVE_TEST_CHECKOUT` fixture path, so one compiled binary can be
//! proven against a real checkout and against an archive extraction. The
//! override chooses which environment is measured; it never weakens what is
//! measured.

use std::path::{Path, PathBuf};
use std::process::Command;

use serial_test::serial;

const SCRIPT: &str = "scripts/build-from-archive.sh";
/// Something cargo rejects -- after the archive is extracted and the scratch
/// dir exists, before anything compiles -- so the script's failure path is
/// reachable quickly and deterministically.
const NO_SUCH_FEATURE: &str = "beadrs-archive-test-no-such-feature";

/// The script's own report that it removed the scratch dir it created.
const SCRATCH_REMOVED: &str = "scratch dir removed: ";
/// The script's own report that it kept the scratch dir for diagnosis.
const SCRATCH_RETAINED: &str = "scratch dir left in place for diagnosis: ";

/// Fixture path to the checkout whose untouched-ness is asserted. Unset means
/// the tree this test binary was compiled in, which is the real checkout for
/// a plain `cargo test` and an archive extraction for the packaged suite.
const CHECKOUT_OVERRIDE: &str = "BEADRS_ARCHIVE_TEST_CHECKOUT";

/// The script is landed by beadrs-53e55a45; until that commit arrives this
/// test has nothing to run and skips loudly instead of failing the suite.
fn require_script(root: &Path) -> Option<PathBuf> {
    let path = root.join(SCRIPT);
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
    match std::env::var_os(CHECKOUT_OVERRIDE) {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => PathBuf::from(env!("CARGO_MANIFEST_DIR")),
    }
}

/// Whether the VCS-state assertions apply to `root`.
#[derive(Debug, PartialEq)]
enum CheckoutContext {
    /// `.git` is present, so every git probe must succeed and the full
    /// checkout-untouched invariant holds.
    Real,
    /// An exported tree -- a `git archive` extraction or an unpacked package.
    /// There is no VCS state to snapshot, so those assertions are skipped.
    ExportedTree,
}

/// Classify `root`, refusing to skip on `.git` absence alone.
///
/// `.git` missing is expected for an exported tree, but it is also what a
/// checkout looks like after its `.git` has been destroyed, and what any
/// directory looks like when git's upward discovery leaves it and resolves a
/// parent work tree. The second case is the dangerous one: the probes would
/// snapshot a repository this test did not select, and the script resolves
/// its own `REPO` the same way, so the run would be measuring -- and building
/// from -- the wrong checkout. git is therefore asked to confirm the absence;
/// disagreement is a failure, never a skip.
fn checkout_context(root: &Path) -> CheckoutContext {
    if root.join(".git").exists() {
        return CheckoutContext::Real;
    }
    let discovery = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()
        .expect("spawn git");
    if discovery.status.success() {
        let resolved = String::from_utf8_lossy(&discovery.stdout).trim().to_owned();
        panic!(
            "{} has no .git marker but git resolved a work tree from it \
             (is-inside-work-tree = {resolved:?}); the checkout-state probes \
             would measure a repository this test did not select, and \
             {SCRIPT} would build from it. Move this exported tree out from \
             inside any enclosing repository instead of skipping.",
            root.display()
        );
    }
    let msg = format!(
        "skipping checkout-state assertion: {} is an exported tree -- no .git \
         marker and git confirms it is not inside a work tree (expected for \
         cargo package and git-archive verification); the script build itself \
         is exercised from a real checkout",
        root.display()
    );
    println!("{msg}");
    eprintln!("{msg}");
    CheckoutContext::ExportedTree
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
    head: String,
    reflog: String,
    stash_list: String,
    /// The index as `git ls-files -s` renders it -- mode, blob and stage per
    /// path. Independent of HEAD and of the worktree, so staged-only
    /// tampering is visible on its own.
    index: String,
    /// Tracked working-tree content plus untracked litter. `.beads/` is
    /// excluded: any worker's bead mutations republish that git-tracked
    /// checkpoint, and a build script cannot write it, so including it would
    /// make the probe measure the fleet instead of this run.
    worktree: String,
}

fn snapshot(root: &Path) -> SharedState {
    SharedState {
        head: git(root, &["rev-parse", "HEAD"]),
        reflog: git(root, &["reflog"]),
        stash_list: git(root, &["stash", "list"]),
        index: git(root, &["ls-files", "-s"]),
        worktree: git(
            root,
            &["status", "--porcelain", "--", ".", ":(exclude).beads"],
        ),
    }
}

fn assert_shared_untouched(before: &SharedState, after: &SharedState) {
    let mut changed = Vec::new();
    if before.head != after.head {
        changed.push("git rev-parse HEAD");
    }
    if before.reflog != after.reflog {
        changed.push("git reflog");
    }
    if before.stash_list != after.stash_list {
        changed.push("git stash list");
    }
    if before.index != after.index {
        changed.push("git ls-files -s (index)");
    }
    if before.worktree != after.worktree {
        changed.push("git status --porcelain (tracked worktree + untracked, .beads excluded)");
    }
    assert!(
        changed.is_empty(),
        "the build-from-archive run mutated the shared checkout: {changed:?}\n\
         before: {before:#?}\n\
         after: {after:#?}\n\
         (a concurrent worker committing, staging or editing during the run \
         also moves this state; check git log / git reflog timestamps before \
         blaming the script)"
    );
}

/// The scratch dir the script says it created, for the outcome `marker`
/// names. Asserting on this reported path -- instead of diffing all of
/// `~/scratch` -- is what keeps the cleanup assertions meaningful on a box
/// where other workers extract archive builds into the same directory
/// concurrently.
fn reported_scratch_dir(log: &str, marker: &str) -> PathBuf {
    let line = log
        .lines()
        .filter(|l| l.contains(marker))
        .next_back()
        .unwrap_or_else(|| panic!("no {marker:?} line in the script output:\n{log}"));
    let path = line
        .split(marker)
        .last()
        .expect("splitting on the marker yields a tail")
        .trim();
    assert!(
        !path.is_empty(),
        "the script printed {marker:?} with no path: {line:?}"
    );
    PathBuf::from(path)
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
    let root = repo_root();
    let Some(script) = require_script(&root) else {
        return;
    };
    if matches!(checkout_context(&root), CheckoutContext::ExportedTree) {
        return;
    }
    let sha = git(&root, &["rev-parse", "HEAD"]);
    let before = snapshot(&root);
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

    // The script reports the scratch dir it removed; the report is only
    // printed after the removal succeeds, so the path must be gone.
    let scratch = reported_scratch_dir(&log, SCRATCH_REMOVED);
    assert!(
        !scratch.exists(),
        "the run reported removing its scratch dir but it is still present: {}\n{log}",
        scratch.display()
    );

    let _ = std::fs::remove_dir_all(&out_dir);
}

#[test]
#[serial]
fn failed_archive_build_retains_scratch_dir() {
    let root = repo_root();
    let Some(script) = require_script(&root) else {
        return;
    };
    if matches!(checkout_context(&root), CheckoutContext::ExportedTree) {
        return;
    }
    let sha = git(&root, &["rev-parse", "HEAD"]);
    let before = snapshot(&root);
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

    // The script names the scratch dir it kept for diagnosis; the rule under
    // test is that the named dir really is still there.
    let retained = reported_scratch_dir(&log, SCRATCH_RETAINED);
    assert!(
        retained.is_dir(),
        "the run failed but its reported scratch dir {} is gone (or was never \
         created), so the 'leave it in place on failure for diagnosis' rule \
         cannot hold\n{log}",
        retained.display()
    );

    assert_shared_untouched(&before, &snapshot(&root));

    // The output above is the diagnostic record; do not leave the retained
    // dir to accumulate in the shared ~/scratch on every run. Only the path
    // this run reported is removed -- never a concurrent worker's.
    let _ = std::fs::remove_dir_all(&retained);
    let _ = std::fs::remove_dir_all(&out_dir);
}

/// The packaged suite reaches these two runs only through the archive-context
/// skip, so the skip is itself part of the contract: it has to be loud, and
/// it has to be reachable only by an exported tree. This test asserts the
/// classification directly, so a real checkout that somehow stopped being
/// probeable fails here instead of quietly borrowing the skip.
#[test]
#[serial]
fn checkout_context_matches_the_environment_it_claims() {
    let root = repo_root();
    match checkout_context(&root) {
        CheckoutContext::ExportedTree => {
            // An exported tree still has to be the tree this file verifies,
            // otherwise the skip is excusing the wrong directory.
            assert!(
                root.join(SCRIPT).exists(),
                "an exported tree without {SCRIPT} is not a tree this test verifies"
            );
            println!(
                "exported tree at {}: checkout-state assertions skipped",
                root.display()
            );
        }
        CheckoutContext::Real => {
            // Proving HEAD is discoverable is what separates "the assertions
            // ran" from "the assertions were skipped and the test went green
            // anyway".
            let head = git(&root, &["rev-parse", "HEAD"]);
            assert_eq!(head.len(), 40, "unexpected HEAD shape: {head:?}");
            println!("real checkout at {}: HEAD {head}", root.display());
        }
    }
}

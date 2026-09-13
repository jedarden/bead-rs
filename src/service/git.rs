//! Read-only Git reachability reporting for the published checkpoint (ADR-013).
//!
//! `bead sync status` answers "ready to commit" from checkpoint internals
//! alone; it says nothing about whether Git can actually reach any of it.
//! This module adds that one reporting surface: which checkpoint files are
//! committed, staged, unstaged, untracked, or ignored, or an explicit
//! unavailable answer (no repository above the workspace, no `git` binary)
//! rather than a silent pass.
//!
//! The boundary is narrowed, not reversed (ADR-013; ADR-009 stands): this
//! module never mutates Git state, never reads remote-tracking or network
//! state, and nothing conditions behavior on what it reports -- the clause
//! scopes the probe itself, which stays strictly read-only. One reporting
//! surface downstream now derives from it by explicit contract (ADR-017):
//! `sync status` folds the probe into the `ready_to_commit` line through
//! [`commit_readiness`], while `sync flush-only`'s idempotent short-circuit
//! deliberately keeps keying on checkpoint internals alone. The mechanism
//! is shelling out to the `git` binary with `--no-optional-locks` (Git's own
//! documented read-only inspection switch) and fsmonitor/untracked-cache
//! disabled for the invocation, so no daemon is spawned and no index
//! extension is written. Repository discovery is a plain walk up from the
//! workspace root for a `.git` directory or file; workspaces outside any
//! repository never spawn a subprocess at all.

use serde::Serialize;
use std::collections::BTreeSet;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;

/// The checkpoint pathspec every Git reachability probe is scoped to,
/// relative to the workspace root.
pub const CHECKPOINT_PATHSPEC: &str = ".beads/checkpoint";

/// Runtime files that live inside the checkpoint directory but are
/// synchronization metadata, not published checkpoint state.
///
/// The publication lock file is the working example: `bead init` writes
/// `.beads/.gitignore` (`*.lock`, among others) precisely so it never
/// travels, it exists only while the workspace is being published
/// against, and a pulled checkpoint carrying someone else's lock would be
/// damage, not state. Its transient presence must not read as an
/// unhealable Git handoff gap -- the `ignored` bucket is reserved for
/// published checkpoint files an ignore rule excludes (ADR-013), and the
/// ADR-017 readiness gate treats that bucket as blocking.
pub const RUNTIME_CHECKPOINT_FILES: &[&str] = &["publish.lock"];

/// Whether a workspace-root-relative checkpoint path is runtime metadata
/// rather than published state (see [`RUNTIME_CHECKPOINT_FILES`]).
fn is_runtime_checkpoint_file(path: &str) -> bool {
    Path::new(path)
        .file_name()
        .is_some_and(|name| RUNTIME_CHECKPOINT_FILES.contains(&name.to_string_lossy().as_ref()))
}

/// How much of the published checkpoint Git can currently reach.
///
/// Serialized first-class into `CheckpointStatusReport` so JSON consumers
/// see the same answer the text line renders. Bucket paths are relative to
/// the workspace root, which is the form an external Git workflow needs in
/// order to include them in a commit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GitReachability {
    /// Overall verdict: `committed`, `staged`, `unstaged`, `untracked`,
    /// `ignored`, or `unavailable`. When several buckets are pending the
    /// verdict names the one that made the least progress toward a commit
    /// (see [`verdict`]); the buckets always carry the full detail.
    pub status: String,
    /// Why Git could not answer. Present only for `unavailable`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unavailable_reason: Option<String>,
    /// Paths whose current content is reachable from HEAD.
    pub committed: Vec<String>,
    /// Paths whose current content is staged but not yet committed.
    pub staged: Vec<String>,
    /// Tracked paths whose current content differs from the index only.
    pub unstaged: Vec<String>,
    /// Paths Git has never been told about.
    pub untracked: Vec<String>,
    /// Paths an ignore rule excludes -- the one shape the Git handoff can
    /// never heal, because the files will never enter a commit.
    pub ignored: Vec<String>,
}

fn unavailable(reason: String) -> GitReachability {
    GitReachability {
        status: "unavailable".to_string(),
        unavailable_reason: Some(reason),
        committed: Vec::new(),
        staged: Vec::new(),
        unstaged: Vec::new(),
        untracked: Vec::new(),
        ignored: Vec::new(),
    }
}

/// The five reachability buckets, before rendering.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct Buckets {
    committed: BTreeSet<String>,
    staged: BTreeSet<String>,
    unstaged: BTreeSet<String>,
    untracked: BTreeSet<String>,
    ignored: BTreeSet<String>,
}

impl Buckets {
    fn pending(&self) -> bool {
        !(self.staged.is_empty()
            && self.unstaged.is_empty()
            && self.untracked.is_empty()
            && self.ignored.is_empty())
    }
}

/// The overall verdict for a set of buckets.
///
/// Everything committed and nothing pending is `committed`. Otherwise the
/// verdict names the pending bucket that made the *least* progress toward a
/// commit, because that is the widest gap in the handoff: ignored files can
/// never be committed at all, untracked files have never reached Git, staged
/// files stopped at the index, and unstaged files are at least carried by an
/// older commit. The buckets carry the full detail either way.
fn verdict(buckets: &Buckets) -> String {
    if !buckets.pending() {
        return "committed".to_string();
    }
    if !buckets.ignored.is_empty() {
        "ignored".to_string()
    } else if !buckets.untracked.is_empty() {
        "untracked".to_string()
    } else if !buckets.staged.is_empty() {
        "staged".to_string()
    } else {
        "unstaged".to_string()
    }
}

/// Classify porcelain entries into buckets.
///
/// `entries` is the parsed `git status --porcelain=v1 -z` output (XY code
/// plus path); `tracked` is the index membership reported by
/// `git ls-files`; `invisible` is the set of on-disk checkpoint files that
/// neither status nor ls-files reported -- invisible because an ignore rule
/// excludes them, which `ignored` (the `check-ignore` answer for exactly
/// that set) confirms. A file with no porcelain entry and index membership
/// is index == HEAD == worktree, i.e. committed.
///
/// A file can land in two buckets at once: `MM` means the staged content is
/// still pending a commit *and* the current content has moved past the index,
/// so neither bucket may swallow the other.
fn bucket(
    entries: &[(u8, u8, String)],
    tracked: &BTreeSet<String>,
    ignored: &BTreeSet<String>,
    invisible: &[String],
) -> Buckets {
    let mut buckets = Buckets::default();
    let mut entered = BTreeSet::new();
    for &(x, y, ref path) in entries {
        entered.insert(path.clone());
        if x == b'?' && y == b'?' {
            if ignored.contains(path) {
                buckets.ignored.insert(path.clone());
            } else {
                buckets.untracked.insert(path.clone());
            }
            continue;
        }
        // Any index-side difference from HEAD -- add, modify, delete,
        // rename, unmerged -- means the staged content is not committed.
        if x != b' ' {
            buckets.staged.insert(path.clone());
        }
        // A worktree-side delta (including on top of a staged change) means
        // the current content is not even staged.
        if y != b' ' {
            buckets.unstaged.insert(path.clone());
        }
    }
    for path in tracked {
        if !entered.contains(path) {
            buckets.committed.insert(path.clone());
        }
    }
    // On-disk files status never mentioned: an ignore rule (the common
    // case), or an exotic exclusion such as a sparse checkout. check-ignore
    // decided for the checkpoint's invisible set; anything else still is
    // not reachable, so it reports as untracked rather than vanishing.
    for path in invisible {
        if ignored.contains(path) {
            buckets.ignored.insert(path.clone());
        } else {
            buckets.untracked.insert(path.clone());
        }
    }
    buckets
}

/// Every file under `dir`, as forward-slash paths relative to `base`.
///
/// `git status` does not report ignored files at all, so the on-disk
/// enumeration is what makes the ignored checkpoint visible: a checkpoint
/// file that is neither pending in status nor in the index is asked about
/// directly (check-ignore) instead of silently vanishing from the report.
fn collect_files(base: &Path, dir: &Path, out: &mut Vec<String>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(base, &path, out);
        } else if let Ok(rel) = path.strip_prefix(base) {
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
}

/// Parse `git status --porcelain=v1 -z` output into `(X, Y, path)` entries.
///
/// With `-z` records are NUL-terminated and paths are literal (never
/// quoted). Rename and copy records carry the original path as one extra
/// NUL-separated field, which is consumed and discarded -- the destination
/// path is the one whose reachability matters.
fn parse_porcelain_z(raw: &[u8]) -> Vec<(u8, u8, String)> {
    let fields: Vec<&[u8]> = raw.split(|b| *b == 0).collect();
    let mut entries = Vec::new();
    let mut idx = 0;
    while idx < fields.len() {
        let field = fields[idx];
        idx += 1;
        // Trailing NUL produces one empty field; anything shorter than
        // "XY p" or missing the XY/path separator cannot be a record.
        if field.len() < 4 || field[2] != b' ' {
            continue;
        }
        let x = field[0];
        let y = field[1];
        let path = String::from_utf8_lossy(&field[3..]).into_owned();
        if x == b'R' || x == b'C' || y == b'R' || y == b'C' {
            idx += 1; // original path of a rename/copy record
        }
        entries.push((x, y, path));
    }
    entries
}

/// A `git` invocation that cannot take an optional lock, spawn a daemon, or
/// write an index extension (ADR-013's read-only guarantee).
fn read_only_git(program: &str, workspace_root: &Path) -> Command {
    let mut cmd = Command::new(program);
    cmd.current_dir(workspace_root)
        .arg("--no-optional-locks")
        .args(["-c", "core.fsmonitor=false"])
        .args(["-c", "core.untrackedCache=false"]);
    cmd
}

/// Is there a Git repository at or above `workspace_root`?
///
/// A plain walk up for a `.git` directory or file (a file means a linked
/// worktree or submodule). Outside any repository the caller gets the
/// unavailable answer without spawning a subprocess.
fn repository_encloses(workspace_root: &Path) -> bool {
    let mut current = Some(workspace_root);
    while let Some(dir) = current {
        if dir.join(".git").exists() {
            return true;
        }
        current = dir.parent();
    }
    false
}

/// Run one read-only Git command, returning its stdout or an unavailable
/// verdict that says why Git could not answer.
fn run_git(program: &str, workspace_root: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let subcommand = args.first().copied().unwrap_or("git");
    let output = read_only_git(program, workspace_root)
        .args(args)
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                "git binary not found on PATH".to_string()
            } else {
                format!("git could not be executed: {}", e)
            }
        })?;
    let stderr_line = || {
        String::from_utf8_lossy(&output.stderr)
            .lines()
            .next()
            .unwrap_or("")
            .trim()
            .to_string()
    };
    match output.status.code() {
        Some(0) => Ok(output.stdout),
        Some(128) => {
            let stderr = stderr_line();
            if stderr.contains("not a git repository") {
                Err(format!(
                    "no Git repository above {}",
                    workspace_root.display()
                ))
            } else if stderr.is_empty() {
                Err(format!("git {} failed: exit 128", subcommand))
            } else {
                Err(format!("git {} failed: {}", subcommand, stderr))
            }
        }
        Some(_) => Err(format!("git {} failed: {}", subcommand, stderr_line())),
        // Killed by a signal: whatever stdout held is a truncated answer,
        // never a verdict.
        None => Err(format!("git {} was terminated by a signal", subcommand)),
    }
}

/// Probe the checkpoint's Git reachability from `workspace_root`.
///
/// Three read-only invocations, per ADR-013: `status --porcelain=v1 -z`
/// classifies every pending path, `ls-files` distinguishes tracked from
/// never-tracked, and `check-ignore` distinguishes unreachable-because-
/// excluded from merely not-yet-committed. Best-effort by contract: every
/// failure mode degrades to the explicit `unavailable` answer.
pub fn inspect(workspace_root: &Path, pathspec: &str) -> GitReachability {
    inspect_with("git", workspace_root, pathspec)
}

/// [`inspect`] with the git program named explicitly -- the seam that lets
/// the tests drive the missing-binary path without touching the real
/// environment.
fn inspect_with(program: &str, workspace_root: &Path, pathspec: &str) -> GitReachability {
    if !repository_encloses(workspace_root) {
        return unavailable(format!(
            "no Git repository above {}",
            workspace_root.display()
        ));
    }

    let status_raw = match run_git(
        program,
        workspace_root,
        &[
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=all",
            "--",
            pathspec,
        ],
    ) {
        Ok(raw) => raw,
        Err(reason) => return unavailable(reason),
    };
    let tracked_raw = match run_git(program, workspace_root, &["ls-files", "-z", "--", pathspec]) {
        Ok(raw) => raw,
        Err(reason) => return unavailable(reason),
    };

    // Runtime metadata is carved out of every input (ADR-017): wherever
    // Git would classify it -- tracked, staged, untracked -- the
    // publication lock is not checkpoint state and must not reach a
    // bucket.
    let entries: Vec<_> = parse_porcelain_z(&status_raw)
        .into_iter()
        .filter(|(_, _, path)| !is_runtime_checkpoint_file(path))
        .collect();
    let tracked: BTreeSet<String> = tracked_raw
        .split(|b| *b == 0)
        .filter(|f| !f.is_empty())
        .map(|f| String::from_utf8_lossy(f).into_owned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter(|path| !is_runtime_checkpoint_file(path))
        .collect();

    // `git status` does not report ignored files at all, so the check-ignore
    // question is asked about every on-disk checkpoint file that neither
    // status nor the index accounted for -- that set is exactly where an
    // ignored checkpoint hides. Runtime metadata (the publication lock)
    // is not published state and stays out of every bucket.
    let mut on_disk = Vec::new();
    collect_files(workspace_root, &workspace_root.join(pathspec), &mut on_disk);
    on_disk.retain(|path| !is_runtime_checkpoint_file(path));
    let entered: BTreeSet<String> = entries.iter().map(|(_, _, p)| p.clone()).collect();
    let invisible: Vec<String> = on_disk
        .into_iter()
        .filter(|p| !entered.contains(p) && !tracked.contains(p))
        .collect();
    let ignored = match check_ignore(program, workspace_root, &invisible) {
        Ok(ignored) => ignored,
        // A check-ignore that cannot run must not silently reclassify
        // excluded files as ordinary untracked ones -- say so instead.
        Err(reason) => return unavailable(reason),
    };

    let buckets = bucket(&entries, &tracked, &ignored, &invisible);
    GitReachability {
        status: verdict(&buckets),
        unavailable_reason: None,
        committed: buckets.committed.into_iter().collect(),
        staged: buckets.staged.into_iter().collect(),
        unstaged: buckets.unstaged.into_iter().collect(),
        untracked: buckets.untracked.into_iter().collect(),
        ignored: buckets.ignored.into_iter().collect(),
    }
}

/// Ask `git check-ignore` which of `candidates` an ignore rule excludes.
///
/// Paths go over stdin (`--stdin -z`), never argv, so the invocation is
/// bounded by the checkpoint size rather than the argument limit. Exit 1
/// means "none ignored" and is not an error.
fn check_ignore(
    program: &str,
    workspace_root: &Path,
    candidates: &[String],
) -> Result<BTreeSet<String>, String> {
    if candidates.is_empty() {
        return Ok(BTreeSet::new());
    }
    let mut child = read_only_git(program, workspace_root)
        .args(["check-ignore", "--stdin", "-z"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                "git binary not found on PATH".to_string()
            } else {
                format!("git could not be executed: {}", e)
            }
        })?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "git check-ignore stdin unavailable".to_string())?;
    let mut payload = Vec::new();
    for candidate in candidates {
        payload.extend_from_slice(candidate.as_bytes());
        payload.push(0);
    }
    // Written from a thread so a full stdout pipe cannot deadlock the write:
    // wait_with_output below drains stdout/stderr while the writer runs.
    let writer = thread::spawn(move || stdin.write_all(&payload));
    let output = child
        .wait_with_output()
        .map_err(|e| format!("git check-ignore: {}", e))?;
    match output.status.code() {
        Some(0) | Some(1) => {
            // git answered only for the paths it actually received, so a
            // failed write would make even a clean exit a partial answer.
            match writer.join() {
                Ok(Ok(())) => {}
                Ok(Err(e)) => return Err(format!("git check-ignore stdin: {}", e)),
                Err(_) => return Err("git check-ignore writer thread panicked".to_string()),
            }
            Ok(String::from_utf8_lossy(&output.stdout)
                .split('\0')
                .filter(|p| !p.is_empty())
                .map(str::to_string)
                .collect())
        }
        _ => Err(format!(
            "git check-ignore failed: {}",
            String::from_utf8_lossy(&output.stderr)
                .lines()
                .next()
                .unwrap_or("")
                .trim()
        )),
    }
}

/// Fold the probe into the `ready_to_commit` verdict (ADR-017).
///
/// ADR-013 scoped its "nothing conditions behavior on what it reports"
/// clause to this module, and the module keeps that boundary: the probe is
/// strictly read-only and never mutates Git state. The `sync status`
/// readiness line is the one consumer ADR-017 adds on top: the parent
/// contract requires "Ready to commit" to mean *ready and not already
/// committed*, so a consistent, verified checkpoint that Git cannot reach
/// must not read yes.
///
/// The gate is pure so the three contract cases are pinned by unit tests
/// here rather than only in end-to-end fixtures:
///
/// - **(a)** internally consistent and fully committed -- the checkpoint
///   verdict passes through unchanged;
/// - **(b)** internally consistent with anything uncommitted -- never a
///   bare yes; the reason names the pending buckets and their paths;
/// - **(c)** probe unavailable -- never a silent yes; the probe's own
///   explanation (no repository above the workspace, no `git` binary)
///   becomes a not-ready reason.
///
/// `None` reachability -- no published checkpoint -- does not gate: the
/// internals verdict already names that gap ("no checkpoint published").
/// The internals verdict is preserved separately in
/// `CheckpointStatusReport::checkpoint_consistent`, which `sync
/// flush-only`'s idempotent short-circuit keys on: publication must never
/// wait on the transport (the coupling ADR-013 rejected for this field).
pub fn commit_readiness(
    checkpoint_ready: bool,
    mut checkpoint_reasons: Vec<String>,
    reachability: Option<&GitReachability>,
) -> (bool, Vec<String>) {
    let Some(reach) = reachability else {
        return (checkpoint_ready, checkpoint_reasons);
    };
    if let Some(reason) = &reach.unavailable_reason {
        checkpoint_reasons.push(format!("git reachability unavailable: {}", reason));
        return (false, checkpoint_reasons);
    }
    let pending: Vec<(&str, &Vec<String>)> = [
        ("staged", &reach.staged),
        ("unstaged", &reach.unstaged),
        ("untracked", &reach.untracked),
        ("ignored", &reach.ignored),
    ]
    .into_iter()
    .filter(|(_, paths)| !paths.is_empty())
    .collect();
    if pending.is_empty() {
        return (checkpoint_ready, checkpoint_reasons);
    }
    let detail = pending
        .iter()
        .map(|(bucket, paths)| format!("{}: {}", bucket, paths.join(", ")))
        .collect::<Vec<_>>()
        .join("; ");
    checkpoint_reasons.push(format!(
        "checkpoint not fully committed (Git cannot reach every published file): {}",
        detail
    ));
    (false, checkpoint_reasons)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command as StdCommand;

    /// A hermetic `git` invocation for test setup: no global or system
    /// config leaks in, and the identity never depends on the host.
    fn git(dir: &Path, args: &[&str]) {
        let out = StdCommand::new("git")
            .current_dir(dir)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .args([
                "-c",
                "user.name=beadrs-test",
                "-c",
                "user.email=beadrs-test@invalid",
            ])
            .args(args)
            .output()
            .expect("git should be runnable in tests");
        assert!(
            out.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// A fresh repository (one empty commit) under its own temp dir, with
    /// no `.beads` ancestor and no repository above it.
    fn temp_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(
            !repository_encloses(dir.path()),
            "TMPDIR is inside a git repository; run the suite with a clean TMPDIR"
        );
        git(dir.path(), &["init", "-q"]);
        git(dir.path(), &["commit", "--allow-empty", "-q", "-m", "init"]);
        dir
    }

    fn write_file(base: &Path, rel: &str, contents: &str) {
        let path = base.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("mkdir for test file");
        }
        std::fs::write(path, contents).expect("write test file");
    }

    /// Commit the current checkpoint contents and return the temp dir.
    fn repo_with_committed_checkpoint() -> tempfile::TempDir {
        let repo = temp_repo();
        write_file(repo.path(), ".beads/checkpoint/current.json", "{}\n");
        write_file(repo.path(), ".beads/checkpoint/forensic.jsonl", "e1\n");
        git(repo.path(), &["add", CHECKPOINT_PATHSPEC]);
        git(repo.path(), &["commit", "-q", "-m", "checkpoint"]);
        repo
    }

    fn entry(x: char, y: char, path: &str) -> (u8, u8, String) {
        (x as u8, y as u8, path.to_string())
    }

    #[test]
    fn porcelain_parses_mixed_records() {
        // " M a", staged add "A  b", untracked "?? c", and a rename record
        // whose original path is consumed, all NUL-terminated.
        let raw = b" M a\0A  b\0?? c\0R  d\0d-old\0";
        let parsed = parse_porcelain_z(raw);
        assert_eq!(
            parsed,
            vec![
                entry(' ', 'M', "a"),
                entry('A', ' ', "b"),
                entry('?', '?', "c"),
                entry('R', ' ', "d"),
            ]
        );
    }

    #[test]
    fn porcelain_parses_empty_and_unterminated_output() {
        assert!(parse_porcelain_z(b"").is_empty());
        assert!(parse_porcelain_z(b"\0\0").is_empty());
        // A missing trailing NUL still yields the record.
        assert_eq!(parse_porcelain_z(b"?? x"), vec![entry('?', '?', "x")]);
        // A record shorter than "XY p" is skipped, not misparsed.
        assert_eq!(parse_porcelain_z(b"??\0M  ok\0").len(), 1);
        // A record missing the XY/path separator is skipped, not misparsed.
        assert!(parse_porcelain_z(b"XYno-space\0").is_empty());
    }

    #[test]
    fn buckets_split_untracked_from_ignored() {
        let entries = vec![
            entry('?', '?', ".beads/checkpoint/current.json"),
            entry('?', '?', ".beads/checkpoint/objects/abc.jsonl"),
        ];
        let tracked = BTreeSet::new();
        let ignored: BTreeSet<String> = [".beads/checkpoint/current.json".to_string()]
            .into_iter()
            .collect();
        let buckets = bucket(&entries, &tracked, &ignored, &[]);
        assert_eq!(
            buckets.ignored,
            [".beads/checkpoint/current.json".to_string()].into()
        );
        assert_eq!(
            buckets.untracked,
            [".beads/checkpoint/objects/abc.jsonl".to_string()].into()
        );
        assert_eq!(verdict(&buckets), "ignored");
    }

    #[test]
    fn invisible_files_are_asked_not_assumed() {
        // An ignored checkpoint produces no status entries at all: the only
        // trace is the on-disk enumeration plus check-ignore's answer.
        let invisible = vec![
            ".beads/checkpoint/current.json".to_string(),
            ".beads/checkpoint/forensic.jsonl".to_string(),
        ];
        let ignored: BTreeSet<String> = invisible.iter().cloned().collect();
        let buckets = bucket(&[], &BTreeSet::new(), &ignored, &invisible);
        assert_eq!(buckets.ignored.len(), 2);
        assert!(buckets.committed.is_empty());
        assert_eq!(verdict(&buckets), "ignored");

        // Invisible without an ignore rule (exotic exclusion): still not
        // reachable, so it must report as untracked rather than vanish.
        let buckets = bucket(&[], &BTreeSet::new(), &BTreeSet::new(), &invisible);
        assert_eq!(buckets.untracked.len(), 2);
        assert_eq!(verdict(&buckets), "untracked");
    }

    #[test]
    fn buckets_name_staged_unstaged_and_committed() {
        let entries = vec![
            entry('A', ' ', ".beads/checkpoint/objects/new.jsonl"),
            entry(' ', 'M', ".beads/checkpoint/current.json"),
        ];
        let tracked: BTreeSet<String> = [
            ".beads/checkpoint/current.json".to_string(),
            ".beads/checkpoint/forensic.jsonl".to_string(),
        ]
        .into_iter()
        .collect();
        let buckets = bucket(&entries, &tracked, &BTreeSet::new(), &[]);
        assert_eq!(
            buckets.staged,
            [".beads/checkpoint/objects/new.jsonl".to_string()].into()
        );
        assert_eq!(
            buckets.unstaged,
            [".beads/checkpoint/current.json".to_string()].into()
        );
        // Tracked with no porcelain entry: index == HEAD == worktree.
        assert_eq!(
            buckets.committed,
            [".beads/checkpoint/forensic.jsonl".to_string()].into()
        );
        // With nothing ignored or untracked pending, the staged delta is
        // the widest remaining gap: it has never reached a commit at all.
        assert_eq!(verdict(&buckets), "staged");
    }

    #[test]
    fn staged_then_modified_lands_in_both_buckets() {
        // "MM": the staged version is pending a commit and the current
        // content has moved past the index. Neither bucket may swallow the
        // other -- the report would understate one gap or the other.
        let entries = vec![entry('M', 'M', ".beads/checkpoint/current.json")];
        let tracked: BTreeSet<String> = [".beads/checkpoint/current.json".to_string()]
            .into_iter()
            .collect();
        let buckets = bucket(&entries, &tracked, &BTreeSet::new(), &[]);
        assert!(buckets.staged.contains(".beads/checkpoint/current.json"));
        assert!(buckets.unstaged.contains(".beads/checkpoint/current.json"));
        assert!(buckets.committed.is_empty());
        // The staged delta is the wider gap: stopped at the index.
        assert_eq!(verdict(&buckets), "staged");
    }

    #[test]
    fn worktree_deletion_is_unstaged_pending() {
        // " D": deleted from the worktree, deletion not staged. The current
        // content is gone from the worktree, so the handoff is pending.
        let entries = vec![entry(' ', 'D', ".beads/checkpoint/current.json")];
        let tracked: BTreeSet<String> = [".beads/checkpoint/current.json".to_string()]
            .into_iter()
            .collect();
        let buckets = bucket(&entries, &tracked, &BTreeSet::new(), &[]);
        assert!(buckets.unstaged.contains(".beads/checkpoint/current.json"));
        assert!(buckets.committed.is_empty());
        assert_eq!(verdict(&buckets), "unstaged");
    }

    #[test]
    fn verdict_ranks_least_progressed_bucket() {
        let mut buckets = Buckets::default();
        buckets.committed.insert("a".to_string());
        assert_eq!(verdict(&buckets), "committed");

        buckets.unstaged.insert("b".to_string());
        assert_eq!(verdict(&buckets), "unstaged");
        buckets.staged.insert("c".to_string());
        assert_eq!(verdict(&buckets), "staged");
        buckets.untracked.insert("d".to_string());
        assert_eq!(verdict(&buckets), "untracked");
        buckets.ignored.insert("e".to_string());
        assert_eq!(verdict(&buckets), "ignored");
    }

    #[test]
    fn staged_deletion_counts_as_pending() {
        let entries = vec![entry('D', ' ', ".beads/checkpoint/current.json")];
        let tracked: BTreeSet<String> = [".beads/checkpoint/current.json".to_string()]
            .into_iter()
            .collect();
        let buckets = bucket(&entries, &tracked, &BTreeSet::new(), &[]);
        assert!(buckets.staged.contains(".beads/checkpoint/current.json"));
        assert!(buckets.committed.is_empty());
        assert_eq!(verdict(&buckets), "staged");
    }

    #[test]
    fn repoless_workspace_is_unavailable_without_spawning_git() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(
            !repository_encloses(dir.path()),
            "TMPDIR is inside a git repository; run the suite with a clean TMPDIR"
        );
        // A program name that could not succeed if a subprocess were
        // spawned: the repository walk must answer before the probe reaches
        // it, which is what keeps the repo-less case subprocess-free.
        let report = inspect_with("beadrs-no-such-git-binary", dir.path(), CHECKPOINT_PATHSPEC);
        assert_eq!(report.status, "unavailable");
        assert!(
            report
                .unavailable_reason
                .as_deref()
                .unwrap_or_default()
                .starts_with("no Git repository above"),
            "unexpected reason: {:?}",
            report.unavailable_reason
        );
        assert!(report.committed.is_empty());
        assert!(report.staged.is_empty());
        assert!(report.unstaged.is_empty());
        assert!(report.untracked.is_empty());
        assert!(report.ignored.is_empty());
    }

    #[test]
    fn missing_git_binary_is_unavailable_not_silent() {
        let repo = temp_repo();
        let report = inspect_with(
            "beadrs-no-such-git-binary",
            repo.path(),
            CHECKPOINT_PATHSPEC,
        );
        assert_eq!(report.status, "unavailable");
        assert_eq!(
            report.unavailable_reason.as_deref(),
            Some("git binary not found on PATH")
        );
    }

    #[test]
    fn committed_checkpoint_reports_committed() {
        let repo = repo_with_committed_checkpoint();
        let report = inspect(repo.path(), CHECKPOINT_PATHSPEC);
        assert_eq!(report.status, "committed");
        assert!(report.unavailable_reason.is_none());
        assert_eq!(
            report.committed,
            vec![
                ".beads/checkpoint/current.json".to_string(),
                ".beads/checkpoint/forensic.jsonl".to_string(),
            ]
        );
        assert!(report.staged.is_empty());
        assert!(report.unstaged.is_empty());
        assert!(report.untracked.is_empty());
        assert!(report.ignored.is_empty());
    }

    #[test]
    fn staged_files_report_staged() {
        let repo = repo_with_committed_checkpoint();
        write_file(
            repo.path(),
            ".beads/checkpoint/current.json",
            "{ \"v\": 2 }\n",
        );
        git(repo.path(), &["add", CHECKPOINT_PATHSPEC]);
        let report = inspect(repo.path(), CHECKPOINT_PATHSPEC);
        assert_eq!(report.status, "staged");
        assert_eq!(
            report.staged,
            vec![".beads/checkpoint/current.json".to_string()]
        );
        // The untouched file's committed content is still reachable.
        assert_eq!(
            report.committed,
            vec![".beads/checkpoint/forensic.jsonl".to_string()]
        );
        assert!(report.unstaged.is_empty());
        assert!(report.untracked.is_empty());
        assert!(report.ignored.is_empty());
    }

    #[test]
    fn unstaged_files_report_unstaged() {
        let repo = repo_with_committed_checkpoint();
        write_file(
            repo.path(),
            ".beads/checkpoint/forensic.jsonl",
            "e1 changed\n",
        );
        let report = inspect(repo.path(), CHECKPOINT_PATHSPEC);
        assert_eq!(report.status, "unstaged");
        assert_eq!(
            report.unstaged,
            vec![".beads/checkpoint/forensic.jsonl".to_string()]
        );
        assert_eq!(
            report.committed,
            vec![".beads/checkpoint/current.json".to_string()]
        );
        assert!(report.staged.is_empty());
        assert!(report.untracked.is_empty());
        assert!(report.ignored.is_empty());
    }

    #[test]
    fn untracked_files_report_untracked_by_file_path() {
        let repo = repo_with_committed_checkpoint();
        // A fresh subdirectory: --untracked-files=all must surface the file
        // itself, not the directory.
        write_file(repo.path(), ".beads/checkpoint/objects/abc.jsonl", "obj\n");
        let report = inspect(repo.path(), CHECKPOINT_PATHSPEC);
        assert_eq!(report.status, "untracked");
        assert_eq!(
            report.untracked,
            vec![".beads/checkpoint/objects/abc.jsonl".to_string()]
        );
        assert_eq!(report.committed.len(), 2);
        assert!(report.staged.is_empty());
        assert!(report.unstaged.is_empty());
        assert!(report.ignored.is_empty());
    }

    #[test]
    fn ignored_checkpoint_reports_ignored() {
        let repo = temp_repo();
        // The one shape the handoff can never heal: the ignore rule is in
        // place before the checkpoint ever exists, so status reports
        // nothing and only the on-disk walk plus check-ignore can see it.
        write_file(repo.path(), ".gitignore", ".beads/\n");
        git(repo.path(), &["add", ".gitignore"]);
        git(repo.path(), &["commit", "-q", "-m", "ignore checkpoint"]);
        write_file(repo.path(), ".beads/checkpoint/current.json", "{}\n");
        write_file(repo.path(), ".beads/checkpoint/objects/abc.jsonl", "obj\n");
        let report = inspect(repo.path(), CHECKPOINT_PATHSPEC);
        assert_eq!(report.status, "ignored");
        assert_eq!(
            report.ignored,
            vec![
                ".beads/checkpoint/current.json".to_string(),
                ".beads/checkpoint/objects/abc.jsonl".to_string(),
            ]
        );
        assert!(report.committed.is_empty());
        assert!(report.staged.is_empty());
        assert!(report.unstaged.is_empty());
        assert!(report.untracked.is_empty());
    }

    #[test]
    fn runtime_lock_file_is_carved_out_of_every_bucket() {
        let repo = temp_repo();
        write_file(repo.path(), ".beads/checkpoint/current.json", "{}\n");
        write_file(repo.path(), ".beads/checkpoint/forensic.jsonl", "e1\n");
        git(repo.path(), &["add", CHECKPOINT_PATHSPEC]);
        git(repo.path(), &["commit", "-q", "-m", "checkpoint"]);
        // The publication lock: on disk, ignored by init's own `*.lock`
        // rule, and by design never part of the published checkpoint. An
        // unhealable `ignored` verdict for it would make every workspace
        // with a lock file present permanently not-ready (ADR-017).
        write_file(repo.path(), ".beads/.gitignore", "*.lock\n");
        write_file(repo.path(), ".beads/checkpoint/publish.lock", "");
        let report = inspect(repo.path(), CHECKPOINT_PATHSPEC);
        assert_eq!(report.status, "committed", "{report:?}");
        assert!(report.staged.is_empty(), "{report:?}");
        assert!(report.unstaged.is_empty(), "{report:?}");
        assert!(report.untracked.is_empty(), "{report:?}");
        assert!(report.ignored.is_empty(), "{report:?}");
        assert_eq!(report.committed.len(), 2, "{report:?}");
    }

    #[test]
    fn runtime_lock_file_shows_nothing_even_when_git_would_classify_it() {
        // A workspace whose `.beads/.gitignore` predates the `*.lock`
        // rule: the lock would classify as untracked. It is still not
        // checkpoint state, so the probe must not report it.
        let repo = temp_repo();
        write_file(repo.path(), ".beads/checkpoint/current.json", "{}\n");
        git(repo.path(), &["add", CHECKPOINT_PATHSPEC]);
        git(repo.path(), &["commit", "-q", "-m", "checkpoint"]);
        write_file(repo.path(), ".beads/checkpoint/publish.lock", "");
        let report = inspect(repo.path(), CHECKPOINT_PATHSPEC);
        assert_eq!(report.status, "committed", "{report:?}");
        assert!(report.untracked.is_empty(), "{report:?}");
    }

    /// A `GitReachability` with explicit buckets for the gate tests. The
    /// `status` verdict is a deliberate red herring: the gate reads the
    /// buckets -- the full detail -- not the summary string.
    fn reach(
        committed: &[&str],
        staged: &[&str],
        unstaged: &[&str],
        untracked: &[&str],
        ignored: &[&str],
    ) -> GitReachability {
        GitReachability {
            status: "committed".to_string(),
            unavailable_reason: None,
            committed: committed.iter().map(|s| s.to_string()).collect(),
            staged: staged.iter().map(|s| s.to_string()).collect(),
            unstaged: unstaged.iter().map(|s| s.to_string()).collect(),
            untracked: untracked.iter().map(|s| s.to_string()).collect(),
            ignored: ignored.iter().map(|s| s.to_string()).collect(),
        }
    }

    // ---- commit_readiness: the ADR-017 gate's three contract cases ----

    #[test]
    fn gate_case_a_consistent_and_fully_committed_reads_yes() {
        let reach = reach(&[".beads/checkpoint/current.json"], &[], &[], &[], &[]);
        let (ready, reasons) = commit_readiness(true, Vec::new(), Some(&reach));
        assert!(ready);
        assert!(reasons.is_empty());
    }

    #[test]
    fn gate_case_b_anything_uncommitted_is_never_a_bare_yes() {
        // The parent's live reproduction shape: a consistent checkpoint
        // whose tracked view is modified and whose new objects are
        // untracked. The reason must name the buckets and the paths.
        let reach = reach(
            &[".beads/checkpoint/current.json"],
            &[],
            &[".beads/checkpoint/forensic.jsonl"],
            &[".beads/checkpoint/objects/abc.jsonl"],
            &[],
        );
        let (ready, reasons) = commit_readiness(true, Vec::new(), Some(&reach));
        assert!(!ready);
        assert_eq!(reasons.len(), 1, "{reasons:?}");
        assert!(
            reasons[0].starts_with(
                "checkpoint not fully committed (Git cannot reach every published file)"
            ),
            "{reasons:?}"
        );
        assert!(
            reasons[0].contains("unstaged: .beads/checkpoint/forensic.jsonl"),
            "{reasons:?}"
        );
        assert!(
            reasons[0].contains("untracked: .beads/checkpoint/objects/abc.jsonl"),
            "{reasons:?}"
        );
    }

    #[test]
    fn gate_case_b_staged_alone_still_blocks() {
        let reach = reach(&["committed.txt"], &["staged.txt"], &[], &[], &[]);
        let (ready, reasons) = commit_readiness(true, Vec::new(), Some(&reach));
        assert!(!ready);
        assert!(reasons[0].contains("staged: staged.txt"), "{reasons:?}");
    }

    #[test]
    fn gate_case_b_ignored_blocks_the_never_healable_shape() {
        // The one configuration the Git handoff can never repair: the
        // verdict is honest only if it is still a NO.
        let reach = reach(&[], &[], &[], &[], &[".beads/checkpoint/current.json"]);
        let (ready, reasons) = commit_readiness(true, Vec::new(), Some(&reach));
        assert!(!ready);
        assert!(
            reasons[0].contains("ignored: .beads/checkpoint/current.json"),
            "{reasons:?}"
        );
    }

    #[test]
    fn gate_case_c_unavailable_is_explicit_never_a_silent_yes() {
        let reach = unavailable("no Git repository above /tmp/ws".to_string());
        let (ready, reasons) = commit_readiness(true, Vec::new(), Some(&reach));
        assert!(!ready);
        assert_eq!(reasons.len(), 1, "{reasons:?}");
        assert!(
            reasons[0].starts_with("git reachability unavailable: no Git repository above"),
            "{reasons:?}"
        );
    }

    #[test]
    fn gate_none_reachability_passes_the_internals_verdict_through() {
        // No published checkpoint: the internals reasons already name that
        // gap, and there is nothing for Git to reach.
        let (ready, reasons) = commit_readiness(
            false,
            vec!["no checkpoint published (run `bead sync flush-only`)".to_string()],
            None,
        );
        assert!(!ready);
        assert_eq!(
            reasons,
            vec!["no checkpoint published (run `bead sync flush-only`)".to_string()]
        );
    }

    #[test]
    fn gate_fully_committed_leaves_an_internally_not_ready_verdict_alone() {
        // Fully committed but internally damaged: the probe adds nothing.
        let reach = reach(&[".beads/checkpoint/current.json"], &[], &[], &[], &[]);
        let (ready, reasons) = commit_readiness(
            false,
            vec!["root hash mismatch: objects/abc".to_string()],
            Some(&reach),
        );
        assert!(!ready);
        assert_eq!(reasons, vec!["root hash mismatch: objects/abc".to_string()]);
    }

    #[test]
    fn gate_accumulates_behind_existing_internals_reasons() {
        let reach = reach(&[], &[], &[], &[".beads/checkpoint/objects/abc.jsonl"], &[]);
        let (ready, reasons) = commit_readiness(
            false,
            vec!["checkpoint dirty: covered=1, live=2".to_string()],
            Some(&reach),
        );
        assert!(!ready);
        assert_eq!(reasons.len(), 2, "{reasons:?}");
        assert_eq!(reasons[0], "checkpoint dirty: covered=1, live=2");
        assert!(
            reasons[1].contains("untracked: .beads/checkpoint/objects/abc.jsonl"),
            "{reasons:?}"
        );
    }
}

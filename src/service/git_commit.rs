//! The explicit checkpoint commit: `bead sync commit` (ADR-019).
//!
//! ADR-018 left the Git handoff at the index: publications stage the
//! verified fileset, and whoever commits next picks it up. Assembling that
//! commit stayed manual, and committing a checkpoint correctly by hand is
//! easy to get wrong in exactly the ways ADR-018 catalogued -- a bare
//! pathspec omits freshly written untracked objects, compaction stales a
//! hand-typed pathspec, and a shared index means the wrong invocation
//! sweeps unrelated work into history (commit 780243a in commitgraph).
//!
//! Automatic committing on every mutation was considered and rejected
//! (bead beadrs-b63a06e5, observed 2026-09-02): a pre-commit gate held
//! checkpoint commits for hours on an unrelated regression, so an
//! automatic committer must either be hostage to unrelated code health or
//! bypass the gate; shared checkouts would contend for the index lock with
//! agents' own staging; and branch, history shape, and message are policy,
//! not storage. A commit also does not propagate without a push, and
//! automatic pushing is a substantially larger hammer.
//!
//! This module is the sanctioned middle ground: one explicit subcommand
//! that keeps the decision with whoever owns the branch and removes every
//! *mechanical* way to get the commit wrong.
//!
//! The contract, in order:
//!
//! - **Refuse before touching anything.** No published checkpoint, a dirty
//!   checkpoint (the live store has unflushed work), a remote-advanced
//!   checkpoint (a pull delivered work the store has not reconciled), a
//!   covered-ahead integrity failure, an internally inconsistent
//!   checkpoint, a detached HEAD (the commit would be reachable from no
//!   branch), Git unavailability, and checkpoint files an ignore rule
//!   excludes -- each is a refusal naming its remedy, before the index or
//!   history is touched. The refusals key on the same
//!   [`crate::service::checkpoint::CheckpointStatusReport`] `sync status`
//!   reports, so the command and the report can never disagree.
//! - **Stage the verified set.** The fileset the current generation makes
//!   authoritative -- both pointers, every object either pointer still
//!   references, the compatibility view when one exists, and the removal
//!   of every tombstoned object Git tracks -- is staged through
//!   [`crate::service::git_stage::stage_published_checkpoint`]. A
//!   referenced file missing from disk is checkpoint damage and refuses
//!   the commit rather than enshrine the damage in history.
//! - **Commit with a bead-only pathspec.** `git commit -- <verified set>`
//!   records exactly those paths and leaves every other staged path --
//!   another worker's staging on a shared index -- staged and untouched.
//! - **Never bypass the gates.** No `--no-verify`: a pre-commit hook that
//!   rejects the commit rejects it here too, and the failure surfaces
//!   verbatim. History shape stays Git's and the operator's; the only
//!   policy this command supplies is the message, and `--message`
//!   overrides the conventional default.
//!
//! The mechanism is shelling out to `git`, per ADR-013's settled choice,
//! with fsmonitor and the untracked cache disabled for every invocation.

use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

use super::checkpoint;
use super::git;
use super::git_stage;
use super::reconcile::{self, SyncRelationship};
use crate::error::{Error, Result};
use crate::store::SqliteStore;

/// The commit message used when `--message` is absent and the pointer
/// declares no generation to name. The `chore(beads)` prefix and the
/// `[bead-rs]` suffix follow the checkpoint-commit convention this
/// repository's own history already uses.
pub const DEFAULT_COMMIT_MESSAGE: &str = "chore(beads): sync checkpoint [bead-rs]";

/// What one `sync commit` invocation decided and did. `committed` is
/// `false` for the idempotent short-circuit: a fully consistent checkpoint
/// whose verified set Git already reaches commits nothing and exits 0.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CommitReport {
    pub dry_run: bool,
    pub committed: bool,
    pub generation_id: Option<String>,
    /// The branch the commit landed on (or would land on, under
    /// `--dry-run`). A detached HEAD refuses before this point.
    pub branch: Option<String>,
    /// The new commit hash, when a commit was created.
    pub commit: Option<String>,
    /// Workspace-relative paths staged as additions or modifications.
    pub staged: Vec<String>,
    /// Workspace-relative paths staged as removals (tracked tombstones).
    pub removed: Vec<String>,
}

/// Commit the published checkpoint's verified fileset (or report what a
/// commit would carry, under `--dry-run`).
///
/// `checkpoint_base` is the workspace's `.beads` directory; the workspace
/// root is its parent. Every refusal fires before the index or history is
/// touched, so an error return leaves the workspace exactly as it was.
pub fn commit_verified_checkpoint(
    store: &mut SqliteStore,
    checkpoint_base: &Path,
    message: Option<&str>,
    dry_run: bool,
) -> Result<CommitReport> {
    commit_with("git", store, checkpoint_base, message, dry_run)
}

/// [`commit_verified_checkpoint`] with the git program named explicitly --
/// the seam that lets tests drive a failing binary without touching the
/// real environment.
fn commit_with(
    program: &str,
    store: &mut SqliteStore,
    checkpoint_base: &Path,
    message: Option<&str>,
    dry_run: bool,
) -> Result<CommitReport> {
    let workspace_root = checkpoint_base.parent().unwrap_or(checkpoint_base);
    let checkpoint_dir = checkpoint_base.join("checkpoint");

    // Outside any repository there is no index and no history to record
    // into; refusing here keeps every later invocation repository-clean.
    if !git::repository_encloses(workspace_root) {
        return Err(Error::cli_usage(format!(
            "sync commit refused: no Git repository above {} - the checkpoint \
             handoff needs one; run `git init` or commit by hand",
            workspace_root.display()
        )));
    }

    // A commit on a detached HEAD is reachable from no branch: the next
    // checkout abandons it. Git itself allows the shape, so this guard is
    // the only thing standing between the operator and a silently dropped
    // checkpoint commit.
    let branch = current_branch(program, workspace_root)?;
    let Some(branch) = branch else {
        return Err(Error::cli_usage(
            "sync commit refused: HEAD is detached and a commit made here would be \
             reachable from no branch - check out a branch first",
        ));
    };

    // The checkpoint gates. These key on the same report `sync status`
    // prints, so the command and the report cannot disagree about
    // readiness; see [`refuse_gates`] for the per-gate remedies.
    let report = checkpoint::forensic_checkpoint_status(store, checkpoint_base)?;
    refuse_gates(&report)?;

    // The verified set, from the artifacts alone. Every referenced file
    // must exist: a pointer selecting a missing file is checkpoint damage,
    // and committing the pointer would enshrine it in history.
    let verified = verified_fileset(&checkpoint_dir)?;
    let missing: Vec<&String> = verified
        .present
        .iter()
        .filter(|path| !checkpoint_dir.join(path).is_file())
        .collect();
    if !missing.is_empty() {
        return Err(Error::integrity(format!(
            "sync commit refused: the checkpoint references files missing from disk: {} - \
             the checkpoint is damaged; run `bead sync flush-only` to republish from the \
             live store, or `bead doctor` to inspect",
            missing
                .iter()
                .map(|path| path.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }

    let message = resolve_message(message, report.generation_id.as_deref())?;

    if dry_run {
        // The same workspace-root-relative mapping the staging module
        // applies: the probe and the report both need it, and a
        // checkpoint-relative pathspec would silently match nothing.
        let checkpoint_rel = checkpoint_dir.strip_prefix(workspace_root).map_err(|_| {
            Error::integrity(format!(
                "checkpoint directory {} is not inside workspace {}",
                checkpoint_dir.display(),
                workspace_root.display()
            ))
        })?;
        let to_workspace_relative =
            |path: &String| checkpoint_rel.join(path).to_string_lossy().into_owned();
        let removed = tracked_deletions(program, workspace_root, &verified.deleted);
        let mut pathspec: Vec<String> =
            verified.present.iter().map(to_workspace_relative).collect();
        pathspec.extend(removed.iter().cloned());
        pathspec.sort();
        let committed =
            !pathspec.is_empty() && pending_against_head(program, workspace_root, &pathspec)?;
        return Ok(CommitReport {
            dry_run: true,
            committed,
            generation_id: report.generation_id,
            branch: Some(branch),
            commit: None,
            staged: pathspec
                .iter()
                .filter(|path| !removed.contains(path))
                .cloned()
                .collect(),
            removed,
        });
    }

    // Stage exactly the verified set (ADR-018's staging module: additions
    // resolved against disk, deletions filtered through `git ls-files`).
    // Unlike publication's best-effort staging, a failure here is fatal:
    // committing without the verified set staged is precisely the damage
    // this command exists to prevent.
    let present: Vec<String> = verified.present.iter().cloned().collect();
    let deleted: Vec<String> = verified.deleted.iter().cloned().collect();
    let staged_pathspecs =
        git_stage::stage_published_checkpoint(workspace_root, &checkpoint_dir, &present, &deleted)
            .map_err(|reason| {
                Error::integrity(format!(
                    "sync commit refused: staging the verified checkpoint fileset failed: {} - \
             nothing was committed; resolve the staging failure and re-run",
                    reason
                ))
            })?;

    // Idempotent short-circuit: after staging, index == worktree for every
    // verified path, so an empty index-vs-HEAD delta over exactly the
    // staged pathspec means the commit would carry nothing.
    if staged_pathspecs.is_empty()
        || !pending_against_head(program, workspace_root, &staged_pathspecs)?
    {
        return Ok(CommitReport {
            dry_run: false,
            committed: false,
            generation_id: report.generation_id,
            branch: Some(branch),
            commit: None,
            staged: Vec::new(),
            removed: Vec::new(),
        });
    }

    // The commit records exactly the verified set -- a pathspec commit
    // ignores every other staged path, which is what keeps another
    // worker's index on a shared checkout out of this history. The report
    // splits the pathspec the same way the staging plan did, so additions
    // and tombstone removals read separately.
    let removed = tracked_deletions(program, workspace_root, &verified.deleted);
    let staged = staged_pathspecs
        .iter()
        .filter(|path| !removed.contains(path))
        .cloned()
        .collect();
    run_git(
        program,
        workspace_root,
        std::iter::once("commit".to_string())
            .chain(std::iter::once("-m".to_string()))
            .chain(std::iter::once(message.to_string()))
            .chain(std::iter::once("--".to_string()))
            .chain(staged_pathspecs.iter().cloned())
            .collect::<Vec<_>>()
            .as_slice(),
    )?;
    let commit = String::from_utf8_lossy(&run_git(
        program,
        workspace_root,
        &["rev-parse".to_string(), "HEAD".to_string()],
    )?)
    .trim()
    .to_string();

    Ok(CommitReport {
        dry_run: false,
        committed: true,
        generation_id: report.generation_id,
        branch: Some(branch),
        commit: Some(commit),
        staged,
        removed,
    })
}

/// The per-gate refusals, each naming its remedy. Order matters: the
/// integrity failure and the remote-advanced state are diagnosed before
/// the ordinary dirty checkpoint, because their remedies (`bead doctor`,
/// `bead sync reconcile`) differ from flush-only's.
fn refuse_gates(report: &checkpoint::CheckpointStatusReport) -> Result<()> {
    if !report.checkpoint_present {
        return Err(Error::cli_usage(
            "sync commit refused: no checkpoint published - run `bead sync flush-only` first",
        ));
    }
    if report.relationship == SyncRelationship::CoveredAheadIntegrityFailure.as_str() {
        return Err(Error::integrity(format!(
            "sync commit refused: covered-ahead integrity failure - {}",
            report
                .not_ready_reasons
                .first()
                .map(String::as_str)
                .unwrap_or(
                    "the checkpoint is ahead of the live store but failed its qualification"
                )
        )));
    }
    if report.relationship == SyncRelationship::RemoteAdvanced.as_str() {
        return Err(Error::conflict(format!(
            "sync commit refused: the checkpoint is remote-advanced (covered {} > live {}) - {}",
            report.covered_sequence.unwrap_or_default(),
            report.live_sequence,
            reconcile::REMOTE_ADVANCED_REMEDY
        )));
    }
    if report.dirty {
        return Err(Error::cli_usage(format!(
            "sync commit refused: the checkpoint is dirty (covered {} < live {}) and \
             committing now would record a stale checkpoint - run `bead sync flush-only` first",
            report.covered_sequence.unwrap_or_default(),
            report.live_sequence
        )));
    }
    if !report.checkpoint_consistent {
        return Err(Error::cli_usage(format!(
            "sync commit refused: the checkpoint is not internally consistent - {}; \
             run `bead sync flush-only` to republish, or `bead doctor` to inspect",
            report.not_ready_reasons.join("; ")
        )));
    }
    let Some(reach) = &report.git_reachability else {
        return Err(Error::cli_usage(
            "sync commit refused: no Git reachability verdict is available for the published \
             checkpoint - run `bead sync status` to inspect",
        ));
    };
    if let Some(reason) = &reach.unavailable_reason {
        return Err(Error::cli_usage(format!(
            "sync commit refused: git reachability unavailable: {}",
            reason
        )));
    }
    if !reach.ignored.is_empty() {
        return Err(Error::validation(format!(
            "sync commit refused: ignore rules exclude checkpoint files, and files Git \
             ignores can never be committed: {} - amend the ignore rule \
             (`.beads/.gitignore` tracks the checkpoint, `*.lock` files stay out)",
            reach.ignored.join(", ")
        )));
    }
    Ok(())
}

/// The checkpoint-relative fileset the current generation makes
/// authoritative: everything either pointer references (which includes
/// both pointers themselves and every retained object), the compatibility
/// view when one exists, and every tombstone either pointer declares.
/// Runtime files are carved out (ADR-017's rule, applied to the write
/// side); paths that would escape the checkpoint directory are damage and
/// refuse the commit.
struct VerifiedFileset {
    present: BTreeSet<String>,
    deleted: BTreeSet<String>,
}

fn verified_fileset(checkpoint_dir: &Path) -> Result<VerifiedFileset> {
    let current = checkpoint_dir.join("current.json");
    let previous = checkpoint_dir.join("previous.json");

    let mut present: BTreeSet<String> = checkpoint::read_pointer_referenced_files(&current)?
        .into_iter()
        .collect();
    let mut deleted = pointer_deleted_paths(&current)?;
    if previous.exists() {
        // The retained set: objects the outgoing pointer still references
        // must stay selectable until it is retired, so the commit carries
        // them even when the current generation no longer names them.
        present.extend(checkpoint::read_pointer_referenced_files(&previous)?);
        deleted.extend(pointer_deleted_paths(&previous)?);
        present.insert("previous.json".to_string());
    }
    if checkpoint_dir.join("forensic.jsonl").is_file() {
        present.insert("forensic.jsonl".to_string());
    }
    present.retain(|path| !git::is_runtime_checkpoint_file(path));
    deleted.retain(|path| !git::is_runtime_checkpoint_file(path));
    for path in present.iter().chain(deleted.iter()) {
        if !safe_checkpoint_relative_path(path) {
            return Err(Error::integrity(format!(
                "sync commit refused: the pointer declares the path {:?}, which escapes the \
                 checkpoint directory - the pointer is damaged; run `bead doctor` to inspect",
                path
            )));
        }
    }
    Ok(VerifiedFileset { present, deleted })
}

/// Whether a checkpoint-relative path stays inside the checkpoint
/// directory: relative, no `..` component, no backslashes, no empty
/// component. The same shape [`checkpoint::is_generation_object_path`]
/// demands of tombstones, applied to every staged path.
fn safe_checkpoint_relative_path(path: &str) -> bool {
    !path.starts_with('/')
        && !path.contains('\\')
        && !path.contains("..")
        && path.split('/').all(|component| !component.is_empty())
}

/// The checkpoint-relative paths one pointer's `deleted_paths` declares
/// tombstoned. Absent or malformed arrays read as no tombstones: the
/// pointer gates (`sync status`) own pointer health, this assembly only
/// mirrors what it declares.
fn pointer_deleted_paths(pointer_path: &Path) -> Result<BTreeSet<String>> {
    let content = std::fs::read_to_string(pointer_path).map_err(|e| {
        Error::integrity(format!("Failed to read {}: {}", pointer_path.display(), e))
    })?;
    let pointer: serde_json::Value = serde_json::from_str(&content).map_err(|e| {
        Error::integrity(format!("Failed to parse {}: {}", pointer_path.display(), e))
    })?;
    Ok(pointer
        .get("deleted_paths")
        .and_then(|v| v.as_array())
        .map(|paths| {
            paths
                .iter()
                .filter_map(|v| v.as_str())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default())
}

/// The current branch name, or `None` for a detached HEAD.
fn current_branch(program: &str, workspace_root: &Path) -> Result<Option<String>> {
    let raw = run_git(
        program,
        workspace_root,
        &[
            "symbolic-ref".to_string(),
            "-q".to_string(),
            "--short".to_string(),
            "HEAD".to_string(),
        ],
    );
    match raw {
        Ok(stdout) => {
            let name = String::from_utf8_lossy(&stdout).trim().to_string();
            Ok((!name.is_empty()).then_some(name))
        }
        // `symbolic-ref -q` exits nonzero exactly for a detached HEAD.
        Err(_) => Ok(None),
    }
}

/// Whether any path in `pathspec` currently differs from HEAD (staged,
/// unstaged, or untracked). Used to decide whether a commit would carry
/// anything.
fn pending_against_head(program: &str, workspace_root: &Path, pathspec: &[String]) -> Result<bool> {
    let raw = run_git(
        program,
        workspace_root,
        std::iter::once("status".to_string())
            .chain(std::iter::once("--porcelain=v1".to_string()))
            .chain(std::iter::once("-z".to_string()))
            .chain(std::iter::once("--".to_string()))
            .chain(pathspec.iter().cloned())
            .collect::<Vec<_>>()
            .as_slice(),
    )?;
    // Any record at all is a pending delta: `??` untracked, XY staged or
    // unstaged. A file identical to HEAD in index and worktree is silent.
    Ok(String::from_utf8_lossy(&raw)
        .split('\0')
        .any(|entry| !entry.is_empty()))
}

/// Which of `candidates` Git tracks -- only a tracked path can stage as a
/// removal, the same filter [`git_stage`] applies before its `git add`.
fn tracked_deletions(
    program: &str,
    workspace_root: &Path,
    candidates: &BTreeSet<String>,
) -> Vec<String> {
    if candidates.is_empty() {
        return Vec::new();
    }
    let raw = match run_git(
        program,
        workspace_root,
        std::iter::once("ls-files".to_string())
            .chain(std::iter::once("-z".to_string()))
            .chain(std::iter::once("--".to_string()))
            .chain(candidates.iter().cloned())
            .collect::<Vec<_>>()
            .as_slice(),
    ) {
        Ok(raw) => raw,
        // A dry-run report must not fabricate removals it could not
        // verify; the real path re-derives this through `git_stage`, which
        // surfaces the failure properly.
        Err(_) => return Vec::new(),
    };
    String::from_utf8_lossy(&raw)
        .split('\0')
        .filter(|entry| !entry.is_empty())
        .map(str::to_string)
        .collect()
}

/// The commit message: `--message` when given (nonempty after trimming),
/// otherwise the conventional default naming the published generation.
fn resolve_message(message: Option<&str>, generation_id: Option<&str>) -> Result<String> {
    match message {
        Some(m) if !m.trim().is_empty() => Ok(m.to_string()),
        Some(_) => Err(Error::cli_usage(
            "sync commit refused: --message cannot be empty",
        )),
        None => Ok(match generation_id {
            Some(generation) => format!("chore(beads): checkpoint {} [bead-rs]", generation),
            None => DEFAULT_COMMIT_MESSAGE.to_string(),
        }),
    }
}

/// Run one Git command from the workspace root. Like the staging module's
/// invocation: fsmonitor and the untracked cache disabled, optional locks
/// *not* suppressed where the command writes (staging, commit), stderr
/// captured so a rejecting pre-commit gate surfaces verbatim.
fn run_git(program: &str, workspace_root: &Path, args: &[String]) -> Result<Vec<u8>> {
    let subcommand = args.first().map(String::as_str).unwrap_or("git");
    let output = Command::new(program)
        .current_dir(workspace_root)
        .args(["-c", "core.fsmonitor=false"])
        .args(["-c", "core.untrackedCache=false"])
        .args(args)
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Error::integrity("git binary not found on PATH")
            } else {
                Error::integrity(format!("git could not be executed: {}", e))
            }
        })?;
    if output.status.success() {
        return Ok(output.stdout);
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let detail = stderr.trim();
    if detail.is_empty() {
        Err(Error::integrity(format!(
            "git {} failed with exit code {}",
            subcommand,
            output.status.code().unwrap_or(-1)
        )))
    } else {
        Err(Error::integrity(format!(
            "git {} failed: {}",
            subcommand, detail
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_pointer(dir: &Path, name: &str, body: serde_json::Value) {
        fs::write(dir.join(name), serde_json::to_string_pretty(&body).unwrap()).unwrap();
    }

    fn pointer_body(root: &str, added: &[&str], deleted: &[&str]) -> serde_json::Value {
        serde_json::json!({
            "generation_id": "gen-1",
            "active_root": {"path": root, "sha256": "deadbeef"},
            "added_paths": added,
            "replaced_paths": [],
            "deleted_paths": deleted,
            "snapshot_sequence": 3
        })
    }

    #[test]
    fn verified_set_spans_both_pointers_the_view_and_the_tombstones() {
        let dir = tempfile::tempdir().unwrap();
        let checkpoint_dir = dir.path().join("checkpoint");
        fs::create_dir_all(&checkpoint_dir).unwrap();
        write_pointer(
            &checkpoint_dir,
            "current.json",
            pointer_body(
                "objects/aaa.jsonl",
                &["objects/bbb.jsonl"],
                &["objects/gone.jsonl"],
            ),
        );
        write_pointer(
            &checkpoint_dir,
            "previous.json",
            pointer_body(
                "objects/aaa.jsonl",
                &["objects/retained.jsonl"],
                &["objects/older-gone.jsonl"],
            ),
        );
        fs::write(checkpoint_dir.join("forensic.jsonl"), "view\n").unwrap();
        fs::write(checkpoint_dir.join("publish.lock"), "").unwrap();

        let verified = verified_fileset(&checkpoint_dir).unwrap();
        for expected in [
            "current.json",
            "previous.json",
            "objects/aaa.jsonl",
            "objects/bbb.jsonl",
            "objects/retained.jsonl",
            "forensic.jsonl",
        ] {
            assert!(
                verified.present.contains(expected),
                "{} missing from the verified set: {:?}",
                expected,
                verified.present
            );
        }
        for expected in ["objects/gone.jsonl", "objects/older-gone.jsonl"] {
            assert!(
                verified.deleted.contains(expected),
                "{} missing from the tombstone set: {:?}",
                expected,
                verified.deleted
            );
        }
        assert!(
            !verified.present.iter().any(|p| p.contains("publish.lock")),
            "runtime file leaked into the verified set: {:?}",
            verified.present
        );
    }

    #[test]
    fn a_pointer_path_escaping_the_checkpoint_refuses() {
        let dir = tempfile::tempdir().unwrap();
        let checkpoint_dir = dir.path().join("checkpoint");
        fs::create_dir_all(&checkpoint_dir).unwrap();
        write_pointer(
            &checkpoint_dir,
            "current.json",
            pointer_body("objects/aaa.jsonl", &["../../../etc/passwd"], &[]),
        );
        let err = verified_fileset(&checkpoint_dir)
            .err()
            .expect("an escaping path must refuse the commit");
        assert!(err.to_string().contains("escapes"), "{}", err);
    }

    #[test]
    fn message_resolution_rejects_empty_and_names_the_generation_by_default() {
        assert!(resolve_message(Some("  "), None).is_err());
        assert_eq!(resolve_message(Some("custom"), None).unwrap(), "custom");
        assert_eq!(
            resolve_message(None, Some("abc123")).unwrap(),
            "chore(beads): checkpoint abc123 [bead-rs]"
        );
        assert_eq!(resolve_message(None, None).unwrap(), DEFAULT_COMMIT_MESSAGE);
    }

    #[test]
    fn safe_paths_reject_traversal_absolute_and_backslash() {
        assert!(safe_checkpoint_relative_path("objects/ab.jsonl"));
        assert!(safe_checkpoint_relative_path("current.json"));
        assert!(!safe_checkpoint_relative_path("../escape"));
        assert!(!safe_checkpoint_relative_path("/absolute"));
        assert!(!safe_checkpoint_relative_path("objects/a\\b.jsonl"));
        assert!(!safe_checkpoint_relative_path("objects//double.jsonl"));
    }
}

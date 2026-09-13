//! Best-effort Git index staging of the published checkpoint fileset (ADR-018).
//!
//! Publishing is automatic since R026, but assembling the Git commit that
//! carries the published set stayed manual, and two failure modes are
//! structural rather than carelessness: `git commit <pathspec>` takes
//! tracked modifications but not freshly written untracked objects, and
//! compaction renames objects, so a hand-written pathspec is stale the
//! moment it is typed. Both leave a committed pointer selecting objects the
//! tree never carried. This module closes the class at the source: after a
//! successful publication, the exact fileset that publication made
//! authoritative is staged into the index, so whoever commits next picks up
//! a correct fileset no matter how they invoke Git.
//!
//! The boundary this module operates under (ADR-018, narrowing ADR-003 and
//! ADR-013 the way ADR-013 narrowed ADR-009):
//!
//! - **Staging, never committing.** A commit publishes history; staging
//!   only marks shared checkpoint state as ready for the commit that was
//!   always going to carry it. Branch policy, history shape, and commit
//!   message remain entirely the caller's business, and the rejection of
//!   automatic Git publication (ADR-003) stands.
//! - **Best-effort, never gating.** A staging failure cannot fail the
//!   mutation or the publication: the checkpoint is already durable, and
//!   publication decisions key on checkpoint internals alone (ADR-017).
//!   Workspaces outside any repository never spawn a subprocess; a failed
//!   or skipped attempt is visible through the ordinary `sync status`
//!   reachability buckets instead of an error path.
//! - **The staged set is exactly the published set** -- both pointers, the
//!   compatibility view when the generation wrote one, every object the new
//!   generation references, every retained object the outgoing pointer
//!   still references, and the removal of every tombstoned object Git
//!   tracks. Runtime files ([`crate::service::git::RUNTIME_CHECKPOINT_FILES`])
//!   are excluded: a pulled checkpoint carrying someone else's publication
//!   lock would be damage, not state (ADR-017's carve-out, applied to the
//!   write side).
//!
//! The mechanism is shelling out to `git`, per ADR-013's settled choice:
//! auto-staging is a `git add`, and the write side must not inherit a
//! library dependency adopted for the read side. fsmonitor and the
//! untracked cache stay disabled for the invocation so no daemon is spawned
//! and no index extension is written; `--no-optional-locks` is deliberately
//! *not* passed, because updating the index is the operation, not an
//! opportunistic side effect.
//!
//! The automatic half is wired to the `checkpoint.auto_stage` workspace key
//! (compiled default on, resolved by
//! [`crate::service::checkpoint::CheckpointConfig::auto_stage_enabled`]);
//! the capability document advertises the compiled default as `auto_stage`,
//! the same additive handshake R026 established for `auto_flush`.

use std::path::Path;
use std::process::Command;

use super::git;

/// Stage one publication's verified fileset into the Git index.
///
/// `present` holds checkpoint-relative paths of files on disk that the
/// publication made authoritative (both pointers, the generation's
/// referenced objects, the compatibility view when written); `deleted`
/// holds checkpoint-relative paths the publication tombstoned, which are
/// staged as removals when and only when Git tracks them -- a deletion
/// pathspec for a never-tracked path would abort the whole `git add`.
///
/// Returns the workspace-root-relative pathspecs handed to `git add`, empty
/// when there was nothing to do. `Err` carries a one-line reason suitable
/// for a warning; the caller decides that a staging failure is not a
/// publication failure.
pub fn stage_published_checkpoint(
    workspace_root: &Path,
    checkpoint_dir: &Path,
    present: &[String],
    deleted: &[String],
) -> Result<Vec<String>, String> {
    stage_published_checkpoint_with("git", workspace_root, checkpoint_dir, present, deleted)
}

/// [`stage_published_checkpoint`] with the git program named explicitly --
/// the seam that lets tests drive a failing or missing binary without
/// touching the real environment.
fn stage_published_checkpoint_with(
    program: &str,
    workspace_root: &Path,
    checkpoint_dir: &Path,
    present: &[String],
    deleted: &[String],
) -> Result<Vec<String>, String> {
    // Outside any repository there is no index to stage into and no handoff
    // to serve; this is the common shape for tests and throwaway workspaces,
    // so it must stay a no-op that never spawns a subprocess (ADR-013's
    // discovery rule, reused for the write side).
    if !git::repository_encloses(workspace_root) {
        return Ok(Vec::new());
    }

    let plan = stage_plan(workspace_root, checkpoint_dir, present, deleted)?;

    // Only deletions of *tracked* paths can be staged; asking Git to add a
    // tombstoned object no commit ever carried would abort the invocation.
    let tracked_deletions = if plan.deletions.is_empty() {
        Vec::new()
    } else {
        let raw = run_git(
            program,
            workspace_root,
            std::iter::once("ls-files".to_string())
                .chain(std::iter::once("-z".to_string()))
                .chain(std::iter::once("--".to_string()))
                .chain(plan.deletions.iter().cloned())
                .collect::<Vec<_>>()
                .as_slice(),
        )?;
        String::from_utf8_lossy(&raw)
            .split('\0')
            .filter(|entry| !entry.is_empty())
            .map(|entry| entry.to_string())
            .collect()
    };

    let mut pathspecs = plan.additions;
    pathspecs.extend(tracked_deletions);
    if pathspecs.is_empty() {
        return Ok(Vec::new());
    }

    let args = std::iter::once("add".to_string())
        .chain(std::iter::once("--".to_string()))
        .chain(pathspecs.iter().cloned())
        .collect::<Vec<_>>();
    run_git(program, workspace_root, &args)?;
    Ok(pathspecs)
}

/// The workspace-root-relative pathspecs one staging attempt will use:
/// existing published files to add, plus tombstoned paths to consider for
/// removal staging.
struct StagePlan {
    additions: Vec<String>,
    deletions: Vec<String>,
}

/// Resolve the staging set against the workspace: every candidate becomes a
/// workspace-root-relative pathspec, runtime files are carved out, and only
/// candidates that exist on disk are staged as additions. A path that the
/// publication recorded but that is already gone from the worktree (a
/// concurrent publisher's superseding generation) cannot be staged as
/// content; the next publication stages the set that replaced it.
fn stage_plan(
    workspace_root: &Path,
    checkpoint_dir: &Path,
    present: &[String],
    deleted: &[String],
) -> Result<StagePlan, String> {
    let checkpoint_rel = checkpoint_dir.strip_prefix(workspace_root).map_err(|_| {
        format!(
            "checkpoint directory {} is not inside workspace {}",
            checkpoint_dir.display(),
            workspace_root.display()
        )
    })?;

    let mut additions = Vec::new();
    for candidate in present {
        // Runtime files are synchronization metadata, never published state
        // (ADR-017's carve-out); the computed set cannot contain one, so a
        // hit here would mean a caller bug -- exclude it all the same.
        if git::is_runtime_checkpoint_file(candidate) {
            continue;
        }
        let absolute = checkpoint_dir.join(candidate);
        if !absolute.is_file() {
            continue;
        }
        additions.push(
            checkpoint_rel
                .join(candidate)
                .to_string_lossy()
                .into_owned(),
        );
    }

    let deletions = deleted
        .iter()
        .filter(|candidate| !git::is_runtime_checkpoint_file(candidate))
        .map(|candidate| {
            checkpoint_rel
                .join(candidate)
                .to_string_lossy()
                .into_owned()
        })
        .collect();

    Ok(StagePlan {
        additions,
        deletions,
    })
}

/// Run one Git command from the workspace root, returning stdout or a
/// one-line reason. The same invocation hygiene as the read-only probe
/// (fsmonitor and untracked cache disabled), minus `--no-optional-locks`:
/// staging writes the index on purpose.
fn run_git(program: &str, workspace_root: &Path, args: &[String]) -> Result<Vec<u8>, String> {
    let subcommand = args.first().map(String::as_str).unwrap_or("git");
    let output = Command::new(program)
        .current_dir(workspace_root)
        .args(["-c", "core.fsmonitor=false"])
        .args(["-c", "core.untrackedCache=false"])
        .args(args)
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                "git binary not found on PATH".to_string()
            } else {
                format!("git could not be executed: {}", e)
            }
        })?;
    if output.status.success() {
        return Ok(output.stdout);
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let detail = stderr.lines().next().unwrap_or("").trim();
    if detail.is_empty() {
        Err(format!(
            "git {} failed with exit code {}",
            subcommand,
            output.status.code().unwrap_or(-1)
        ))
    } else {
        Err(format!("git {} failed: {}", subcommand, detail))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_file(base: &Path, rel: &str, contents: &str) {
        let path = base.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }

    fn workspace_with_checkpoint() -> (tempfile::TempDir, std::path::PathBuf) {
        let root = tempfile::TempDir::new().unwrap();
        let checkpoint_dir = root.path().join(".beads/checkpoint");
        fs::create_dir_all(&checkpoint_dir).unwrap();
        (root, checkpoint_dir)
    }

    #[test]
    fn plan_maps_candidates_to_workspace_relative_pathspecs() {
        let (root, checkpoint_dir) = workspace_with_checkpoint();
        write_file(&checkpoint_dir, "current.json", "{}");
        write_file(&checkpoint_dir, "objects/ab12.jsonl", "[]");

        let plan = stage_plan(
            root.path(),
            &checkpoint_dir,
            &["current.json".to_string(), "objects/ab12.jsonl".to_string()],
            &[],
        )
        .unwrap();
        assert_eq!(
            plan.additions,
            vec![
                ".beads/checkpoint/current.json",
                ".beads/checkpoint/objects/ab12.jsonl"
            ]
        );
        assert!(plan.deletions.is_empty());
    }

    #[test]
    fn plan_carves_runtime_files_out_of_both_inputs() {
        let (root, checkpoint_dir) = workspace_with_checkpoint();
        write_file(&checkpoint_dir, "publish.lock", "locked");
        write_file(&checkpoint_dir, "current.json", "{}");

        let plan = stage_plan(
            root.path(),
            &checkpoint_dir,
            &["publish.lock".to_string(), "current.json".to_string()],
            &["publish.lock".to_string()],
        )
        .unwrap();
        assert_eq!(plan.additions, vec![".beads/checkpoint/current.json"]);
        assert!(plan.deletions.is_empty());
    }

    #[test]
    fn plan_skips_addition_candidates_that_are_not_on_disk() {
        let (root, checkpoint_dir) = workspace_with_checkpoint();
        write_file(&checkpoint_dir, "current.json", "{}");

        let plan = stage_plan(
            root.path(),
            &checkpoint_dir,
            &[
                "current.json".to_string(),
                "objects/gone-before-stage.jsonl".to_string(),
            ],
            &[],
        )
        .unwrap();
        assert_eq!(plan.additions, vec![".beads/checkpoint/current.json"]);
    }

    #[test]
    fn plan_keeps_deletion_candidates_regardless_of_disk_state() {
        let (root, checkpoint_dir) = workspace_with_checkpoint();
        let plan = stage_plan(
            root.path(),
            &checkpoint_dir,
            &[],
            &["objects/tombstoned.jsonl".to_string()],
        )
        .unwrap();
        assert!(plan.additions.is_empty());
        assert_eq!(
            plan.deletions,
            vec![".beads/checkpoint/objects/tombstoned.jsonl"]
        );
    }

    #[test]
    fn plan_rejects_a_checkpoint_dir_outside_the_workspace() {
        let (root, _checkpoint_dir) = workspace_with_checkpoint();
        let elsewhere = tempfile::TempDir::new().unwrap();
        let foreign = elsewhere.path().join(".beads/checkpoint");
        fs::create_dir_all(&foreign).unwrap();
        let err = stage_plan(root.path(), &foreign, &["current.json".to_string()], &[])
            .err()
            .expect("a foreign checkpoint dir must be rejected");
        assert!(err.contains("not inside workspace"), "{}", err);
    }

    #[test]
    fn repoless_workspace_is_a_noop_without_spawning_git() {
        // No `.git` anywhere above the workspace: must return Ok without
        // attempting an invocation (a missing binary would otherwise read
        // as an error here).
        let (root, checkpoint_dir) = workspace_with_checkpoint();
        write_file(&checkpoint_dir, "current.json", "{}");
        let staged = stage_published_checkpoint(
            root.path(),
            &checkpoint_dir,
            &["current.json".to_string()],
            &[],
        )
        .unwrap();
        assert!(staged.is_empty());
    }

    #[test]
    fn staging_adds_published_files_to_the_index() {
        let (root, checkpoint_dir) = workspace_with_checkpoint();
        let git = |args: &[&str]| {
            Command::new("git")
                .current_dir(root.path())
                .args(["-c", "user.name=t", "-c", "user.email=t@invalid"])
                .args(args)
                .output()
                .unwrap()
        };
        assert!(git(&["init", "-q"]).status.success());
        write_file(&checkpoint_dir, "current.json", "{}");
        write_file(&checkpoint_dir, "objects/ab12.jsonl", "[]");

        let staged = stage_published_checkpoint(
            root.path(),
            &checkpoint_dir,
            &["current.json".to_string(), "objects/ab12.jsonl".to_string()],
            &[],
        )
        .unwrap();
        assert_eq!(staged.len(), 2);

        let index =
            String::from_utf8_lossy(&git(&["diff", "--cached", "--name-only"]).stdout).to_string();
        assert!(
            index.contains(".beads/checkpoint/current.json"),
            "{}",
            index
        );
        assert!(
            index.contains(".beads/checkpoint/objects/ab12.jsonl"),
            "{}",
            index
        );
    }

    #[test]
    fn staging_removal_only_for_tracked_tombstones() {
        let (root, checkpoint_dir) = workspace_with_checkpoint();
        let git = |args: &[&str]| {
            Command::new("git")
                .current_dir(root.path())
                .args(["-c", "user.name=t", "-c", "user.email=t@invalid"])
                .args(args)
                .output()
                .unwrap()
        };
        assert!(git(&["init", "-q"]).status.success());
        write_file(&checkpoint_dir, "current.json", "{}");
        write_file(&checkpoint_dir, "objects/kept.jsonl", "[]");
        write_file(&checkpoint_dir, "objects/tombstoned.jsonl", "old");
        assert_eq!(
            stage_published_checkpoint(
                root.path(),
                &checkpoint_dir,
                &[
                    "current.json".to_string(),
                    "objects/kept.jsonl".to_string(),
                    "objects/tombstoned.jsonl".to_string()
                ],
                &[],
            )
            .unwrap()
            .len(),
            3
        );
        assert!(git(&["commit", "-q", "-m", "checkpoint", "--allow-empty"])
            .status
            .success());

        // The publication tombstoned one object (gone from disk) while a
        // never-tracked deletion candidate also arrives.
        fs::remove_file(checkpoint_dir.join("objects/tombstoned.jsonl")).unwrap();

        let staged = stage_published_checkpoint(
            root.path(),
            &checkpoint_dir,
            &["current.json".to_string(), "objects/kept.jsonl".to_string()],
            &[
                "objects/tombstoned.jsonl".to_string(),
                "objects/never-tracked.jsonl".to_string(),
            ],
        )
        .unwrap();
        // The never-tracked deletion must not have aborted the invocation.
        assert_eq!(staged.len(), 3, "pathspecs: {:?}", staged);

        let index = String::from_utf8_lossy(&git(&["diff", "--cached", "--name-status"]).stdout)
            .to_string();
        assert!(
            index.contains("D\t.beads/checkpoint/objects/tombstoned.jsonl"),
            "{}",
            index
        );
        assert!(
            !index.contains("never-tracked"),
            "untracked deletion leaked into the index: {}",
            index
        );
    }

    #[test]
    fn missing_git_binary_is_an_error_reason_not_a_crash() {
        let (root, checkpoint_dir) = workspace_with_checkpoint();
        // A `.git` directory makes repository discovery pass, then the
        // named program cannot execute.
        fs::create_dir_all(root.path().join(".git")).unwrap();
        write_file(&checkpoint_dir, "current.json", "{}");
        let err = stage_published_checkpoint_with(
            "beadrs-no-such-git-binary",
            root.path(),
            &checkpoint_dir,
            &["current.json".to_string()],
            &[],
        )
        .err()
        .expect("a missing binary must be an error reason");
        assert!(err.contains("git"), "{}", err);
    }
}

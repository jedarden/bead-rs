//! `bead sync resolve`: one-command recovery for a git-conflicted checkpoint
//! set. Normative source: the conflicted-checkpoint section of
//! `research/specs/remote-advanced-reconcile-v1.md`.
//!
//! A pull that stops on `.beads/checkpoint` leaves unmerged index entries
//! and marker text inside the published fileset. Every other bead command
//! fails closed on that state; this module is the one sanctioned recovery.
//! It restores the incoming side's verified pointer and objects, merges the
//! live store with them through the existing reconcile machinery, or — when
//! the two sides diverged — through the existing divergent-branch machinery
//! (`restored_branch`), so neither side's history is lost, and republishes
//! and stages exactly the checkpoint fileset so `git rebase --continue` or
//! `git commit` can proceed.
//!
//! Boundary: the command mutates checkpoint files, the live store, and the
//! Git index for checkpoint paths only. Unmerged entries and conflict
//! markers outside `.beads/checkpoint` are reported, never touched; the
//! caller's Git workflow still owns them.

use super::restored_branch::RestoredBranchReport;
use super::*;
use std::collections::BTreeSet;
use std::path::Path;

/// The remedy every conflicted-checkpoint report names.
pub const SYNC_RESOLVE_REMEDY: &str =
    "run `bead sync resolve` to restore the incoming checkpoint side and merge both histories";

/// The receipt extension naming a conflict-branch merge. Doctor's
/// foreign-origin recognition keys on this exact key, so the imported
/// branch stays doctor-clean (see [`import_conflict_branch`]).
const RECEIPT_EXTENSION: &str = "restored_branch_reconciliation";

/// The per-event extension recording a branch event's original identity —
/// the same key the restored-branch machinery uses, for the same reason.
const EVENT_ORIGIN_EXTENSION: &str = "restored_branch_origin";

/// Which side of a conflicted fileset [`resolve_conflicts`] restored.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum RestoredSide {
    /// The stage-2 ("ours" during a rebase, i.e. the pulled upstream)
    /// version verified and was not the locally recorded generation.
    Ours,
    /// The stage-3 ("theirs" during a merge, i.e. the pulled remote)
    /// version verified and was not the locally recorded generation.
    Theirs,
}

impl RestoredSide {
    pub fn as_str(&self) -> &'static str {
        match self {
            RestoredSide::Ours => "ours (stage 2)",
            RestoredSide::Theirs => "theirs (stage 3)",
        }
    }
}

/// What one [`resolve_conflicts`] run found and did.
#[derive(Debug, Default, serde::Serialize)]
pub struct ResolveReport {
    /// Unmerged index entries under the checkpoint pathspec before the run.
    pub unmerged_paths: Vec<String>,
    /// Checkpoint files whose worktree content carries conflict markers.
    pub marker_paths: Vec<String>,
    /// Which verified side was restored; absent when nothing was conflicted
    /// or the run is a dry run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restored_side: Option<RestoredSide>,
    /// The generation id of the restored incoming pointer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub incoming_generation: Option<String>,
    /// Reconcile merge counts, when the non-divergent machinery merged.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reconciled: Option<serde_json::Value>,
    /// Divergent-branch import counts, when that machinery merged.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch_imported: Option<serde_json::Value>,
    /// Checkpoint-relative paths staged into the Git index.
    pub staged: Vec<String>,
    pub dry_run: bool,
}

impl ResolveReport {
    /// True when the workspace had no checkpoint conflict to resolve.
    pub fn nothing_to_resolve(&self) -> bool {
        self.unmerged_paths.is_empty() && self.marker_paths.is_empty()
    }
}

/// A `git` invocation that reads without taking a lock or spawning a daemon
/// (the ADR-013 read-only discipline; resolve's only index write goes
/// through the ADR-018 staging module).
fn read_only_git(workspace_root: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let output = std::process::Command::new("git")
        .current_dir(workspace_root)
        .arg("--no-optional-locks")
        .args(["-c", "core.fsmonitor=false"])
        .args(["-c", "core.untrackedCache=false"])
        .args(args)
        .output()
        .map_err(|e| format!("git could not be executed: {}", e))?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(String::from_utf8_lossy(&output.stderr)
            .lines()
            .next()
            .unwrap_or("git failed")
            .trim()
            .to_string())
    }
}

/// Unmerged index entries under `pathspec`: (path, object sha, stage).
fn unmerged_entries(
    workspace_root: &Path,
    pathspec: &str,
) -> Result<Vec<(String, String, i64)>, String> {
    let raw = read_only_git(workspace_root, &["ls-files", "-u", "-z", "--", pathspec])?;
    let mut entries = Vec::new();
    for record in raw.split(|b| *b == 0) {
        if record.is_empty() {
            continue;
        }
        let text = String::from_utf8_lossy(record);
        let Some((meta, path)) = text.split_once('\t') else {
            continue;
        };
        let mut fields = meta.split_whitespace();
        let (_mode, sha, stage) = match (fields.next(), fields.next(), fields.next()) {
            (Some(m), Some(s), Some(st)) => (m, s.to_string(), st.parse::<i64>().unwrap_or(0)),
            _ => continue,
        };
        entries.push((path.to_string(), sha, stage));
    }
    Ok(entries)
}

/// The content of one index object.
fn blob_bytes(workspace_root: &Path, sha: &str) -> Result<Vec<u8>, String> {
    read_only_git(workspace_root, &["cat-file", "blob", sha])
}

/// Conflict markers in a file's bytes: a line beginning with a Git merge
/// conflict start or end marker. The `=======` midpoint is deliberately not
/// a signature — it appears in ordinary prose and JSONL prose fields.
fn has_conflict_markers(bytes: &[u8]) -> bool {
    bytes
        .split(|b| *b == b'\n')
        .any(|line| line.starts_with(b"<<<<<<<") || line.starts_with(b">>>>>>>"))
}

/// Every file under `dir`, as checkpoint-relative forward-slash paths.
fn collect_files(dir: &Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, out);
        } else if let Ok(rel) = path.strip_prefix(dir) {
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
}

/// Scan the workspace for a git-conflicted checkpoint: unmerged index
/// entries under `.beads/checkpoint` plus marker text in the fileset's
/// worktree content. Read-only; never spawns a subprocess outside a
/// repository.
pub fn scan_conflicts(workspace_root: &Path, checkpoint_dir: &Path) -> ConflictScan {
    let mut scan = ConflictScan::default();
    if crate::service::git::repository_encloses(workspace_root) {
        scan.unmerged = unmerged_entries(workspace_root, crate::service::git::CHECKPOINT_PATHSPEC)
            .unwrap_or_default()
            .into_iter()
            .map(|(path, _, _)| path)
            .collect::<Vec<_>>();
        scan.unmerged.sort();
        scan.unmerged.dedup();
    }
    let mut files = Vec::new();
    collect_files(checkpoint_dir, &mut files);
    // Only the git-text-merged files can carry markers: pointers and the
    // monolithic view. Object files are content-addressed — a conflicted
    // object is an add/add without markers, covered by the unmerged scan,
    // and hashing catches tampering — so scanning them would read the
    // whole object store on every mutation for no signal.
    files.retain(|path| {
        matches!(
            path.as_str(),
            "current.json" | "previous.json" | "forensic.jsonl"
        )
    });
    for path in files {
        if let Ok(bytes) = std::fs::read(checkpoint_dir.join(&path)) {
            if has_conflict_markers(&bytes) {
                scan.markers.push(path);
            }
        }
    }
    scan
}

/// The conflict scan behind [`resolve_conflicts`], exposed for reporting.
#[derive(Debug, Default, Clone)]
pub struct ConflictScan {
    pub unmerged: Vec<String>,
    pub markers: Vec<String>,
}

impl ConflictScan {
    pub fn is_empty(&self) -> bool {
        self.unmerged.is_empty() && self.markers.is_empty()
    }
}

/// One candidate fileset reconstructed from one index stage.
#[derive(Clone)]
struct Candidate {
    side: RestoredSide,
    generation_id: String,
    root_sha256: String,
    /// checkpoint-relative path -> stage blob bytes.
    files: Vec<(String, Vec<u8>)>,
    /// Unmerged paths this side deleted (no blob at this stage).
    deleted: Vec<String>,
}

impl Candidate {
    fn pointer(&self) -> Result<serde_json::Value, String> {
        let bytes = &self
            .files
            .iter()
            .find(|(path, _)| path == "current.json")
            .ok_or("candidate has no current.json")?
            .1;
        serde_json::from_slice(bytes).map_err(|e| format!("current.json does not parse: {}", e))
    }
}

/// Verify a candidate against the worktree checkpoint directory: pointer
/// shape, root hash, unresolved tombstones, and the monolithic view. Object
/// files are content-addressed, so they do not conflict; they are read
/// from the worktree.
fn verify_candidate(candidate: &Candidate, checkpoint_dir: &Path) -> Result<(), String> {
    let pointer = candidate.pointer()?;
    let mode = pointer
        .get("mode")
        .and_then(|v| v.as_str())
        .ok_or("pointer declares no mode")?;
    if mode != "monolithic" && mode != "sharded" {
        return Err(format!("unsupported pointer mode: {}", mode));
    }
    if pointer
        .get("store_uuid")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .is_empty()
    {
        return Err("pointer has an empty store UUID".to_string());
    }
    let root_path = pointer
        .get("active_root")
        .and_then(|r| r.get("path"))
        .and_then(|v| v.as_str())
        .ok_or("pointer declares no active root path")?;
    let root_sha = pointer
        .get("active_root")
        .and_then(|r| r.get("sha256"))
        .and_then(|v| v.as_str())
        .ok_or("pointer declares no active root hash")?;
    let root_bytes = match candidate.files.iter().find(|(path, _)| path == root_path) {
        Some((_, bytes)) => bytes.clone(),
        None => std::fs::read(checkpoint_dir.join(root_path))
            .map_err(|_| format!("root object {} missing from the worktree", root_path))?,
    };
    let actual = format!("{:x}", Sha256::digest(&root_bytes));
    if actual != root_sha {
        return Err(format!(
            "root object {} hashes to {} but the pointer declares {}",
            root_path, actual, root_sha
        ));
    }
    if let Some(deleted) = pointer.get("deleted_paths").and_then(|v| v.as_array()) {
        for path in deleted.iter().filter_map(|v| v.as_str()) {
            if checkpoint_dir.join(path).exists() && !candidate.deleted.iter().any(|d| d == path) {
                return Err(format!("unresolved tombstone: {}", path));
            }
        }
    }
    if mode == "monolithic" {
        let view = match candidate
            .files
            .iter()
            .find(|(path, _)| path == "forensic.jsonl")
        {
            Some((_, bytes)) => bytes.clone(),
            None => std::fs::read(checkpoint_dir.join("forensic.jsonl"))
                .map_err(|_| "monolithic view forensic.jsonl missing".to_string())?,
        };
        if view != root_bytes {
            return Err(
                "forensic.jsonl view is not byte-identical to the pointer-selected root"
                    .to_string(),
            );
        }
    }
    Ok(())
}

/// The locally recorded generation: (generation id, root hash). The side
/// matching it is the local flush, never the incoming side.
fn recorded_generation(conn: &rusqlite::Connection) -> Option<(String, String)> {
    conn.query_row(
        "SELECT current_generation_id, current_root_sha256 FROM checkpoint_state WHERE id = 1",
        [],
        |row| {
            Ok((
                row.get::<_, Option<String>>(0)?,
                row.get::<_, Option<String>>(1)?,
            ))
        },
    )
    .ok()
    .and_then(|(generation, hash)| match (generation, hash) {
        (Some(g), Some(h)) => Some((g, h)),
        _ => None,
    })
}

/// Reconstruct the stage-2 and stage-3 candidates from the unmerged index
/// entries and keep the ones whose fileset verifies.
fn verified_candidates(
    workspace_root: &Path,
    checkpoint_dir: &Path,
    entries: &[(String, String, i64)],
) -> Vec<Candidate> {
    let mut candidates = Vec::new();
    for (side, stage) in [(RestoredSide::Ours, 2i64), (RestoredSide::Theirs, 3)] {
        let mut files = Vec::new();
        let mut deleted = Vec::new();
        for (path, sha, entry_stage) in entries {
            if *entry_stage == stage {
                match blob_bytes(workspace_root, sha) {
                    Ok(bytes) => files.push((path.clone(), bytes)),
                    Err(_) => continue,
                }
            }
        }
        if files.is_empty() {
            continue;
        }
        for (path, _, _) in entries {
            if !files.iter().any(|(p, _)| p == path) && !deleted.contains(path) {
                deleted.push(path.clone());
            }
        }
        let candidate = Candidate {
            side: side.clone(),
            generation_id: String::new(),
            root_sha256: String::new(),
            files,
            deleted,
        };
        if let Ok(pointer) = candidate.pointer() {
            if verify_candidate(&candidate, checkpoint_dir).is_ok() {
                candidates.push(Candidate {
                    generation_id: pointer
                        .get("generation_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    root_sha256: pointer
                        .get("active_root")
                        .and_then(|r| r.get("sha256"))
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    side,
                    ..candidate
                });
            }
        }
    }
    candidates
}

/// Apply the chosen candidate to the worktree checkpoint directory: write
/// its blobs, remove the paths it deletes, and remove the pointer's
/// tombstones. Returns checkpoint-relative deletions for staging.
fn apply_candidate(candidate: &Candidate, checkpoint_dir: &Path) -> Result<Vec<String>> {
    for (path, bytes) in &candidate.files {
        let target = checkpoint_dir.join(path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| anyhow!("{}: {}", parent.display(), e))?;
        }
        std::fs::write(&target, bytes).map_err(|e| anyhow!("{}: {}", target.display(), e))?;
    }
    let mut removals = candidate.deleted.clone();
    if let Ok(pointer) = candidate.pointer() {
        if let Some(deleted) = pointer.get("deleted_paths").and_then(|v| v.as_array()) {
            for path in deleted.iter().filter_map(|v| v.as_str()) {
                if !removals.iter().any(|r| r == path) {
                    removals.push(path.to_string());
                }
            }
        }
    }
    for path in &removals {
        let target = checkpoint_dir.join(path);
        if target.is_file() {
            std::fs::remove_file(&target).map_err(|e| anyhow!("{}: {}", target.display(), e))?;
        }
    }
    Ok(removals)
}

/// `bead sync resolve`: resolve a git-conflicted checkpoint set.
///
/// Requires `--actor` like every recovery input. Refuses when nothing is
/// conflicted only in the sense of doing nothing: a clean workspace gets an
/// exit-0 no-op report. Divergence follows the existing divergent-branch
/// rules; a suffix that edits an issue it did not create is refused and
/// nothing is silently dropped.
pub fn resolve_conflicts(
    store: &mut SqliteStore,
    workspace_root: &Path,
    checkpoint_base: &Path,
    actor: &str,
    dry_run: bool,
) -> Result<ResolveReport> {
    validate_restore_actor(actor)?;
    let checkpoint_dir = checkpoint_base.join("checkpoint");
    let scan = scan_conflicts(workspace_root, &checkpoint_dir);
    let mut report = ResolveReport {
        unmerged_paths: scan.unmerged.clone(),
        marker_paths: scan.markers.clone(),
        dry_run,
        ..Default::default()
    };
    if scan.is_empty() {
        return Ok(report);
    }
    if !crate::service::git::repository_encloses(workspace_root) {
        return Err(anyhow!(
            "sync resolve refused: the checkpoint is conflicted but the workspace is not in a \
             Git repository, so there is no incoming side to restore"
        ));
    }

    // Serialize against publication and concurrent recovery, exactly like
    // the other explicit recovery paths.
    let publication = acquire_checkpoint_publication_lock(&checkpoint_dir)?;

    let entries = unmerged_entries(workspace_root, crate::service::git::CHECKPOINT_PATHSPEC)
        .map_err(|e| anyhow!("sync resolve: git ls-files failed: {}", e))?;
    if entries.is_empty() {
        // Marker-only conflict: committed marker text. Fail closed naming
        // the manual recipe; markers are damage, not a merge in progress.
        return Err(anyhow!(format!(
            "sync resolve refused: checkpoint files carry conflict markers without unmerged \
             index entries (markers: {}); the damage is already committed and needs operator \
             forensics, not an automated side choice",
            scan.markers.join(", ")
        )));
    }

    let candidates = verified_candidates(workspace_root, &checkpoint_dir, &entries);
    let recorded = {
        let conn = store.conn();
        recorded_generation(conn)
    };
    let is_local = |c: &Candidate| {
        recorded.as_ref().is_some_and(|(generation, hash)| {
            c.generation_id == *generation && c.root_sha256 == *hash
        })
    };
    let incoming: Candidate = match candidates
        .iter()
        .filter(|c| !is_local(c))
        .collect::<Vec<_>>()
        .as_slice()
    {
        [one] => (*one).clone(),
        [] => {
            return Err(anyhow!(format!(
                "sync resolve refused: neither conflicted side verifies as the incoming \
                 checkpoint (verified sides: {}); the local generation is {:?} - recover with \
                 `bead restore --source .beads/checkpoint --generation <GEN> --actor <WHO>`",
                candidates
                    .iter()
                    .map(|c| c.side.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
                recorded.as_ref().map(|(g, _)| g.as_str()).unwrap_or("none")
            )))
        }
        _ => {
            return Err(anyhow!(
                "sync resolve refused: both conflicted sides verify and neither is the \
                 locally recorded generation; which side is incoming cannot be proven, so \
                 nothing is restored"
            ))
        }
    };
    report.incoming_generation = Some(incoming.generation_id.clone());
    if dry_run {
        report.restored_side = Some(incoming.side.clone());
        return Ok(report);
    }
    report.restored_side = Some(incoming.side.clone());
    let removals = apply_candidate(&incoming, &checkpoint_dir)?;

    // Merge the restored incoming side with the live store through the
    // existing machinery: plain reconcile when the incoming stream is a
    // verified superset, the divergent-branch import when the sides
    // diverged, and a fail-closed integrity refusal for any other shape.
    let verdict = {
        let conn = store.conn();
        crate::service::reconcile::classify(conn, checkpoint_base)?
    };
    match verdict.relationship {
        crate::service::reconcile::SyncRelationship::RemoteAdvanced => {
            let result = crate::service::reconcile::reconcile_checkpoint(
                store,
                checkpoint_base,
                actor,
                false,
            )
            .map_err(|e| anyhow!("{}", e))?;
            report.reconciled = Some(serde_json::json!({
                "issues_inserted": result.inserted,
                "issues_updated": result.updated,
                "events_imported": result.events_imported,
            }));
        }
        crate::service::reconcile::SyncRelationship::CoveredAheadIntegrityFailure => {
            let qualifier = verdict.failed_qualifier.clone().unwrap_or_default();
            if !qualifier.contains("event-stream") {
                return Err(anyhow!(format!(
                    "sync resolve refused: the restored incoming checkpoint fails qualifier \
                     `{}` - this is not a merge shape; run `bead doctor`",
                    qualifier
                )));
            }
            let imported = import_conflict_branch(store, &checkpoint_dir, actor)?;
            report.branch_imported = Some(serde_json::json!({
                "events_imported": imported.events_imported,
                "issues_inserted": imported.issues_inserted,
                "branch_origin": imported.branch_origin,
                "boundary": imported.first_original_sequence,
                "already_merged": imported.already_merged,
            }));
        }
        other => {
            return Err(anyhow!(format!(
                "sync resolve refused: after restoring the incoming side the relationship is \
                 `{}`, which names no merge - run `bead sync status` and `bead doctor`",
                other.as_str()
            )));
        }
    }

    // Republish when the merge left the checkpoint behind (automatic
    // publication already covered it otherwise), then stage exactly the
    // verified fileset so `git rebase --continue` or `git commit` proceeds.
    let after = {
        let conn = store.conn();
        crate::service::reconcile::classify(conn, checkpoint_base)?
    };
    if after.relationship == crate::service::reconcile::SyncRelationship::Behind {
        let config = crate::service::load_checkpoint_config(checkpoint_base)?;
        super::publish_forensic_checkpoint_holding(&publication, store, &config, checkpoint_base)?;
    }
    let present = fileset_present(&checkpoint_dir)?;
    let mut deleted: Vec<String> = fileset_deleted(&checkpoint_dir)?;
    for path in &removals {
        if !deleted.iter().any(|d| d == path) {
            deleted.push(path.clone());
        }
    }
    let staged = crate::service::git_stage::stage_published_checkpoint_holding(
        &publication,
        workspace_root,
        &checkpoint_dir,
        &present,
        &deleted,
    )
    .map_err(|e| anyhow!("sync resolve: staging failed: {}", e))?;
    report.staged = staged;
    Ok(report)
}

/// The present half of the published fileset: whatever either pointer
/// references (the same assembly `sync commit` stages, including the
/// set-v2 structural set), plus the compatibility view when one exists.
fn fileset_present(checkpoint_dir: &Path) -> Result<Vec<String>> {
    let mut present = BTreeSet::new();
    present.insert("current.json".to_string());
    for pointer in ["current.json", "previous.json"] {
        let path = checkpoint_dir.join(pointer);
        if !path.is_file() {
            continue;
        }
        present.extend(super::read_pointer_referenced_files(&path)?);
    }
    if checkpoint_dir.join("forensic.jsonl").is_file() {
        present.insert("forensic.jsonl".to_string());
    }
    present.retain(|path| !crate::service::git::is_runtime_checkpoint_file(path));
    Ok(present.into_iter().collect())
}

/// The deleted half of the published fileset: tombstones both pointers
/// declare.
fn fileset_deleted(checkpoint_dir: &Path) -> Result<Vec<String>> {
    let mut deleted = BTreeSet::new();
    for pointer in ["current.json", "previous.json"] {
        let path = checkpoint_dir.join(pointer);
        if !path.is_file() {
            continue;
        }
        let content =
            std::fs::read_to_string(&path).map_err(|e| anyhow!("{}: {}", path.display(), e))?;
        let pointer: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| anyhow!("{} does not parse: {}", pointer, e))?;
        if let Some(paths) = pointer.get("deleted_paths").and_then(|v| v.as_array()) {
            deleted.extend(paths.iter().filter_map(|v| v.as_str()).map(str::to_string));
        }
    }
    deleted.retain(|path| !crate::service::git::is_runtime_checkpoint_file(path));
    Ok(deleted.into_iter().collect())
}

/// The divergent-branch import for a resolved conflict: the restored
/// branch machinery of [`restored_branch::import_restored_branch`], with
/// the boundary derived from the first wire identity the incoming stream
/// does not share with the live store. The same rules hold: the shared
/// prefix must be identical, the incoming suffix may only create new
/// issues and update or label those, and a repeated merge of the same
/// immutable source is a no-op. Doctor recognizes the imported branch
/// through the same receipt contract as a restored branch.
pub fn import_conflict_branch(
    store: &mut SqliteStore,
    source: &Path,
    actor: &str,
) -> Result<RestoredBranchReport> {
    validate_restore_actor(actor)?;
    let _publication_lock = acquire_store_checkpoint_publication_lock(store)?;
    let mut staging = stage_checkpoint_set(source)?;
    validate_forensic_contents(&staging)?;
    let tx = Transaction::new_unchecked(store.conn(), TransactionBehavior::Immediate)?;
    let (_, target_uuid) = get_workspace_state(&tx)?;
    if staging.store_uuid != target_uuid
        || staging
            .events
            .iter()
            .any(|event| event.origin_store_uuid != target_uuid)
    {
        bail!("Conflict branch requires the same single native origin as the target");
    }
    if staging.redaction_record_count != 0 || !read_all_redaction_records(&tx)?.is_empty() {
        bail!("Conflict branch reconciliation does not reidentify redaction history");
    }

    // Derive the divergence boundary: the first incoming identity the live
    // store lacks or holds with different public content. Everything
    // before it must already exist identically, which the scan itself
    // guarantees.
    let live = read_all_events(&tx)?;
    let live_by_identity: HashMap<(&str, i64), &EventRecord> = live
        .iter()
        .map(|event| {
            (
                (
                    event.origin_store_uuid.as_str(),
                    event.origin_event_sequence,
                ),
                event,
            )
        })
        .collect();
    let boundary = staging
        .events
        .iter()
        .find(|event| {
            match live_by_identity.get(&(
                event.origin_store_uuid.as_str(),
                event.origin_event_sequence,
            )) {
                Some(local) => !crate::service::reconcile::public_content_matches(local, event),
                None => true,
            }
        })
        .ok_or_else(|| {
            anyhow!(
                "the restored incoming side adds no event the live store lacks; there is \
                 nothing to branch-import - verify which side is incoming with `bead sync \
                 status`"
            )
        })?
        .origin_event_sequence;

    let branch_origin = format!(
        "resolved-{:x}",
        Sha256::digest(serde_json::to_vec(
            &serde_json::json!({ "input_hash": staging.input_hash, "boundary": boundary }),
        )?)
    );
    let receipt_id = format!("resolved-branch-{}", staging.input_hash);
    let mut report = RestoredBranchReport {
        source_generation: String::new(),
        source_root_sha256: staging.input_hash.clone(),
        original_store_uuid: target_uuid.clone(),
        branch_origin: branch_origin.clone(),
        first_original_sequence: boundary,
        events_imported: 0,
        issues_inserted: 0,
        existing_issues_preserved: 0,
        dry_run: false,
        already_merged: false,
        receipt_id: receipt_id.clone(),
    };
    let prior: Option<String> = tx
        .query_row(
            "SELECT extensions_json FROM provenance_receipts WHERE receipt_id = ?1",
            [&receipt_id],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(prior) = prior {
        let prior: serde_json::Value = serde_json::from_str(&prior)?;
        let proof = &prior["restored_branch_reconciliation"];
        if proof["source_root_sha256"] != staging.input_hash
            || proof["branch_origin"] != branch_origin
            || proof["first_original_sequence"].as_i64() != Some(boundary)
        {
            bail!("Conflict branch reconciliation receipt identity conflict");
        }
        report.already_merged = true;
        return Ok(report);
    }

    let live_attempts: HashMap<String, serde_json::Value> = read_all_attempt_outcomes(&tx)?
        .into_iter()
        .map(|record| Ok((record.receipt_id.clone(), serde_json::to_value(record)?)))
        .collect::<Result<_>>()?;
    for outcome in &staging.attempt_outcomes {
        if live_attempts.get(&outcome.receipt_id) != Some(&serde_json::to_value(outcome)?) {
            bail!("Conflict branch requires attempt outcomes already present identically in the target");
        }
    }
    let mut new_ids = HashSet::new();
    for event in staging
        .events
        .iter()
        .filter(|event| event.origin_event_sequence >= boundary)
    {
        let id = event
            .issue_id
            .as_deref()
            .ok_or_else(|| anyhow!("Conflict branch contains an unsupported workspace event"))?;
        if event.kind == "created" {
            if !new_ids.insert(id.to_owned()) {
                bail!("Conflict branch repeats an issue creation");
            }
        } else if !new_ids.contains(id)
            || !matches!(
                event.kind.as_str(),
                "updated" | "label_added" | "label_removed"
            )
        {
            bail!(
                "Conflict branch edits an existing issue or uses an unsupported event kind \
                 ({}) - the same issue mutated on both sides follows the divergent-branch \
                 rules and is refused rather than silently dropped",
                event.kind
            );
        }
    }
    for issue in &staging.issues {
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM issues WHERE id = ?1)",
            [&issue.id],
            |row| row.get(0),
        )?;
        if exists == new_ids.contains(&issue.id) {
            bail!(
                "Conflict branch issue identity conflict or missing shared issue: {}",
                issue.id
            );
        }
        if exists {
            report.existing_issues_preserved += 1;
        }
    }
    staging.issues.retain(|issue| new_ids.contains(&issue.id));
    staging.labels.retain(|(id, _)| new_ids.contains(id));
    staging
        .dependencies
        .retain(|edge| new_ids.contains(&edge.blocked));
    staging
        .events
        .retain(|event| event.origin_event_sequence >= boundary);
    for event in &mut staging.events {
        let origin = serde_json::json!({
            "origin_store_uuid": event.origin_store_uuid,
            "origin_event_sequence": event.origin_event_sequence,
        });
        if event
            .extensions
            .insert(EVENT_ORIGIN_EXTENSION.to_owned(), origin)
            .is_some()
        {
            bail!("Conflict branch event already uses the reserved provenance extension");
        }
        event.origin_store_uuid = branch_origin.clone();
        event.origin_event_sequence = event.origin_event_sequence - boundary + 1;
    }
    validate_event_prefix(&tx, &staging)?;
    staging.issue_count = staging.issues.len();
    staging.event_count = staging.events.len();
    report.issues_inserted = import_issues(&tx, &staging)?;
    import_dependencies(&tx, &staging)?;
    import_labels(&tx, &staging)?;
    import_events(&tx, &staging)?;
    import_receipts(&tx, &staging)?;
    reject_live_tombstoned_findings(&tx)?;
    report.events_imported = staging.events.len();
    let proof = serde_json::to_value(&report)?;
    let sequence = create_merge_summary(&tx, &staging, actor)?;
    let receipt = create_merge_receipt(&tx, &staging, actor, sequence)?;
    let extensions = serde_json::json!({ RECEIPT_EXTENSION: proof });
    tx.execute(
        "UPDATE provenance_receipts SET receipt_id = ?1, extensions_json = ?2 WHERE receipt_id = ?3",
        params![receipt_id, extensions.to_string(), receipt.receipt_id],
    )?;
    // Receipt IDs participate in the native receipt hash recipe.
    let mut hasher = Sha256::new();
    for field in [
        &receipt_id,
        &receipt.kind,
        &receipt.source_root_sha256,
        &receipt.actor,
        &receipt.created_at,
        &receipt.result,
    ] {
        hasher.update(field);
    }
    tx.execute(
        "UPDATE provenance_receipts SET receipt_sha256 = ?1 WHERE receipt_id = ?2",
        params![format!("{:x}", hasher.finalize()), receipt_id],
    )?;
    tx.execute(
        "UPDATE events SET detail = ?1 WHERE sequence = ?2",
        params![extensions.to_string(), sequence],
    )?;
    crate::service::secret_boundary::quarantine_recovery(&tx)?;
    tx.commit()?;
    Ok(report)
}

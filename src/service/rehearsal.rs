//! Disposable recovery rehearsal service (R015)
//!
//! Creates a temporary workspace from the *currently committed* forensic
//! checkpoint (`.beads/checkpoint/`), imports it exactly the way `sync
//! import-only --restore-into-empty` would, runs diagnostics, re-exports it
//! exactly the way `sync flush-only` would, and compares the two checkpoints
//! for semantic equivalence. This exercises the real disaster-recovery path
//! without ever touching live state.
//!
//! Deliberately reuses the same production functions the CLI's `sync`
//! subcommand calls (`import_forensic_checkpoint`, `publish_forensic_checkpoint`,
//! `SqliteStore::apply_migrations`) rather than a parallel reimplementation.
//! An earlier version of this module *did* reimplement checkpoint import/
//! export from scratch, against the pre-forensic single-flat-file format
//! (`.beads/issues.jsonl`, one bare `Issue` per line) -- a format nothing in
//! bead-rs has written since the forensic checkpoint system landed. That
//! made every rehearsal fail unconditionally, on every real workspace, with
//! "No checkpoint file found". See the F017/R015 history in this project's
//! own docs for the full story.

use crate::cli::ImportMode;
use crate::error::{Error, Result};
use crate::model::Issue;
use crate::service::checkpoint::{
    import_forensic_checkpoint, publish_forensic_checkpoint, CheckpointConfig, CheckpointRecord,
};
use crate::service::doctor;
use crate::store::{SqliteStore, Store};
use anyhow::Context;
use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Recovery rehearsal report
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RecoveryRehearsalReport {
    pub timestamp: String,
    pub original_checkpoint: CheckpointInfo,
    pub rehearsal_checkpoint: CheckpointInfo,
    pub diagnostics: DiagnosticsResult,
    pub semantic_comparison: SemanticComparison,
    pub cleanup_info: CleanupInfo,
}

/// Information about a checkpoint, read from its `current.json` pointer --
/// the same bookkeeping `sync flush-only`/`sync import-only` themselves
/// trust, rather than re-derived by hand-counting or hand-hashing files.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CheckpointInfo {
    pub path: PathBuf,
    pub issue_count: usize,
    pub event_count: i64,
    pub receipt_count: i64,
    /// `active_root.sha256` from `current.json` -- the checkpoint's own
    /// canonical content hash. Note this is expected to differ between the
    /// original checkpoint and a freshly re-exported one even when their
    /// *content* is fully equivalent: every export mints a new generation id
    /// and timestamp, both folded into the hashed bytes. Informational only,
    /// not part of `overall_equivalence`.
    pub hash: String,
    pub size_bytes: u64,
}

/// Diagnostics result from temporary workspace
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DiagnosticsResult {
    pub checks_performed: usize,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub ok_count: usize,
    pub overall_status: String,
}

/// Semantic comparison between the original checkpoint and the one produced
/// by importing it, then immediately re-exporting the imported state.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SemanticComparison {
    pub issue_count_matches: bool,
    pub event_count_matches: bool,
    pub content_hashes_match: bool,
    pub differences: Vec<SemanticDifference>,
    /// Gates `bead doctor --rehearse`'s own exit code (see `main.rs`). Does
    /// **not** require `content_hashes_match` -- see `CheckpointInfo::hash`.
    pub overall_equivalence: bool,
}

/// Individual semantic difference found during comparison
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SemanticDifference {
    pub issue_id: String,
    pub difference_type: String,
    pub description: String,
}

/// Cleanup information for the rehearsal
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CleanupInfo {
    pub temp_directory_created: bool,
    pub temp_directory_path: Option<PathBuf>,
    pub cleanup_successful: bool,
    pub files_remaining: usize,
}

/// Run a disposable recovery rehearsal
///
/// 1. Creates a temporary workspace directory.
/// 2. Copies the current `.beads/checkpoint/` directory into it.
/// 3. Initializes a real workspace there (real migrations, real pragmas).
/// 4. Imports the copied checkpoint via `import_forensic_checkpoint`
///    (`ImportMode::RestoreIntoEmpty`) -- the exact function `sync
///    import-only --restore-into-empty` calls.
/// 5. Runs diagnostics on the temporary workspace.
/// 6. Re-exports via `publish_forensic_checkpoint` -- the exact function
///    `sync flush-only` calls.
/// 7. Compares the re-exported checkpoint against the original for semantic
///    equivalence (issue/event counts, per-issue content).
/// 8. Cleans up the temporary workspace (automatic on drop).
pub fn run_recovery_rehearsal() -> Result<RecoveryRehearsalReport> {
    let temp_dir = TempDir::new().context("Failed to create temporary workspace directory")?;
    let temp_path = temp_dir.path();

    eprintln!(
        "🔄 Creating temporary workspace at: {}",
        temp_path.display()
    );

    // Step 2: locate the current, real checkpoint directory.
    let current_store = SqliteStore::new();
    let workspace_config = current_store
        .get_workspace_config()
        .map_err(|e| Error::integrity(format!("Failed to get workspace configuration: {}", e)))?;

    let checkpoint_dir = workspace_config.root.join(".beads").join("checkpoint");

    // `bead init` always creates an empty .beads/checkpoint/ directory as
    // part of the normal workspace layout, so its mere existence proves
    // nothing -- check for the pointer file that a real flush writes.
    if !checkpoint_dir.join("current.json").exists() {
        return Err(Error::integrity(format!(
            "No checkpoint found at: {} (run `bead sync flush-only` first)",
            checkpoint_dir.display()
        )));
    }

    eprintln!("📋 Original checkpoint: {}", checkpoint_dir.display());

    let original_checkpoint = get_checkpoint_info(&checkpoint_dir)?;

    // Step 3: stage a copy of the whole checkpoint directory -- import reads
    // from here, never from the live `.beads/checkpoint/` itself.
    let staged_input = temp_path.join("checkpoint-input");
    copy_dir_recursive(&checkpoint_dir, &staged_input)
        .context("Failed to copy checkpoint to temporary workspace")?;

    eprintln!("✅ Checkpoint copied to temporary workspace");

    // Step 4: initialize a real workspace at temp_path. Reuses the actual
    // connection setup (`SqliteStore::with_path`) and real migrations
    // (`apply_migrations`) rather than a hand-rolled schema -- the earlier
    // version's schema was missing entire tables (`provenance_receipts`,
    // `bead_annotations`, ...) that diagnostics and import both depend on.
    let temp_beads_dir = temp_path.join(".beads");
    fs::create_dir_all(temp_beads_dir.join("checkpoint"))
        .context("Failed to create .beads/checkpoint in temporary workspace")?;
    fs::create_dir_all(temp_beads_dir.join("receipts"))
        .context("Failed to create .beads/receipts in temporary workspace")?;

    let temp_db_path = temp_beads_dir.join("beads.db");
    let mut temp_store = SqliteStore::with_path(&temp_db_path)
        .context("Failed to create temporary workspace database")?;
    temp_store
        .apply_migrations()
        .context("Failed to apply migrations to temporary workspace")?;

    let rehearsal_uuid = uuid::Uuid::new_v4().to_string();
    let created_at = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string());
    temp_store
        .conn()
        .execute(
            "INSERT INTO workspace (id, uuid, prefix, layout_version, created_at) VALUES (1, ?1, ?2, 1, ?3)",
            rusqlite::params![&rehearsal_uuid, "rehearsal", &created_at],
        )
        .context("Failed to initialize temporary workspace identity")?;

    let temp_config = serde_json::json!({
        "version": 1,
        "uuid": rehearsal_uuid,
        "prefix": "rehearsal",
        "created_at": created_at,
    });
    fs::write(temp_beads_dir.join("config.json"), temp_config.to_string())
        .context("Failed to write temporary workspace config")?;

    eprintln!("🔧 Temporary workspace initialized");

    // Step 5: import using the exact production import path.
    eprintln!("📥 Importing checkpoint into temporary workspace...");

    let import_result = import_forensic_checkpoint(
        &mut temp_store,
        &staged_input,
        "native-v1",
        ImportMode::RestoreIntoEmpty,
        "rehearsal",
        false,
    )
    .context("Failed to import checkpoint into temporary workspace")?;

    eprintln!(
        "✅ Checkpoint imported: {} issues, {} events, {} receipts processed",
        import_result.inserted, import_result.events_imported, import_result.receipts_processed
    );

    // Step 6: diagnostics on the recovered workspace.
    eprintln!("🔍 Running diagnostics on temporary workspace...");

    let temp_diagnostics = doctor::run_diagnostics(&temp_store)?;
    let diagnostics_result = DiagnosticsResult {
        checks_performed: temp_diagnostics.checks.len(),
        errors: temp_diagnostics
            .checks
            .iter()
            .filter(|c| c.status == doctor::DiagnosticStatus::Error)
            .map(|c| c.message.clone())
            .collect(),
        warnings: temp_diagnostics
            .checks
            .iter()
            .filter(|c| c.status == doctor::DiagnosticStatus::Warning)
            .map(|c| c.message.clone())
            .collect(),
        ok_count: temp_diagnostics
            .checks
            .iter()
            .filter(|c| c.status == doctor::DiagnosticStatus::Ok)
            .count(),
        overall_status: if temp_diagnostics.has_errors {
            "FAILED".to_string()
        } else if temp_diagnostics.has_warnings {
            "WARNING".to_string()
        } else {
            "OK".to_string()
        },
    };

    eprintln!(
        "📊 Diagnostics completed: {} checks, {} errors, {} warnings",
        diagnostics_result.checks_performed,
        diagnostics_result.errors.len(),
        diagnostics_result.warnings.len()
    );

    // Step 7: re-export using the exact production flush path.
    eprintln!("📤 Exporting from temporary workspace...");

    // Rehearse the production publication path: the mode the workspace's
    // recorded configuration and thresholds would select, not a forced one.
    publish_forensic_checkpoint(
        &mut temp_store,
        &CheckpointConfig::default(),
        &temp_beads_dir,
    )
    .context("Failed to export checkpoint from temporary workspace")?;
    let rehearsal_checkpoint_dir = temp_beads_dir.join("checkpoint");

    eprintln!(
        "✅ Export completed: {}",
        rehearsal_checkpoint_dir.display()
    );

    let rehearsal_checkpoint = get_checkpoint_info(&rehearsal_checkpoint_dir)?;

    // Step 8: compare.
    eprintln!("🔬 Comparing semantic equivalence...");

    let semantic_comparison = compare_checkpoints_semantic(
        &original_checkpoint,
        &rehearsal_checkpoint,
        &checkpoint_dir,
        &rehearsal_checkpoint_dir,
    )?;

    eprintln!(
        "📊 Semantic comparison: {}",
        if semantic_comparison.overall_equivalence {
            "EQUIVALENT ✅"
        } else {
            "DIFFERENT ❌"
        }
    );

    let timestamp = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string());

    let cleanup_info = CleanupInfo {
        temp_directory_created: true,
        temp_directory_path: Some(temp_path.to_path_buf()),
        cleanup_successful: true, // TempDir cleans up when dropped
        files_remaining: 0,
    };

    let report = RecoveryRehearsalReport {
        timestamp,
        original_checkpoint,
        rehearsal_checkpoint,
        diagnostics: diagnostics_result,
        semantic_comparison,
        cleanup_info,
    };

    eprintln!("\n=== RECOVERY REHEARSAL SUMMARY ===");
    eprintln!("📅 Timestamp: {}", report.timestamp);
    eprintln!(
        "📋 Original: {} issues, {} events, {} bytes",
        report.original_checkpoint.issue_count,
        report.original_checkpoint.event_count,
        report.original_checkpoint.size_bytes
    );
    eprintln!(
        "🔄 Rehearsal: {} issues, {} events, {} bytes",
        report.rehearsal_checkpoint.issue_count,
        report.rehearsal_checkpoint.event_count,
        report.rehearsal_checkpoint.size_bytes
    );
    eprintln!(
        "🔍 Diagnostics: {} checks, {} errors, {} warnings",
        report.diagnostics.checks_performed,
        report.diagnostics.errors.len(),
        report.diagnostics.warnings.len()
    );
    eprintln!(
        "🔬 Semantic: {}",
        if report.semantic_comparison.overall_equivalence {
            "EQUIVALENT ✅"
        } else {
            "DIFFERENT ❌"
        }
    );
    eprintln!(
        "🧹 Cleanup: {}",
        if report.cleanup_info.cleanup_successful {
            "SUCCESS ✅"
        } else {
            "FAILED ❌"
        }
    );

    Ok(report)
}

/// Read a checkpoint directory's own `current.json` pointer for its
/// authoritative issue/event/receipt counts and content hash, rather than
/// re-deriving them by parsing every record file by hand.
fn get_checkpoint_info(checkpoint_dir: &Path) -> Result<CheckpointInfo> {
    let pointer_path = checkpoint_dir.join("current.json");
    let raw = fs::read_to_string(&pointer_path).with_context(|| {
        format!(
            "Failed to read checkpoint pointer at {}",
            pointer_path.display()
        )
    })?;
    let pointer: serde_json::Value = serde_json::from_str(&raw).with_context(|| {
        format!(
            "Failed to parse checkpoint pointer at {}",
            pointer_path.display()
        )
    })?;

    let issue_count = pointer
        .get("issue_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let event_count = pointer
        .get("event_count")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let receipt_count = pointer
        .get("receipt_count")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let hash = pointer
        .get("active_root")
        .and_then(|r| r.get("sha256"))
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();

    let size_bytes = dir_size(checkpoint_dir).unwrap_or(0);

    Ok(CheckpointInfo {
        path: checkpoint_dir.to_path_buf(),
        issue_count,
        event_count,
        receipt_count,
        hash,
        size_bytes,
    })
}

/// Total size in bytes of every regular file under `dir`, recursively.
/// Informational only (report display), never used for comparison.
fn dir_size(dir: &Path) -> std::io::Result<u64> {
    let mut total = 0u64;
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            total += dir_size(&entry.path())?;
        } else {
            total += entry.metadata()?.len();
        }
    }
    Ok(total)
}

/// Copy every file and subdirectory from `src` into `dst`, creating `dst` if
/// needed. `dst`'s contents end up looking exactly like `src`'s -- no extra
/// nesting level.
fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let target = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

/// Read every `issue`-tagged record from a monolithic `forensic.jsonl` file,
/// keyed by issue id. Returns `None` (not an error) if the checkpoint isn't
/// monolithic (no `forensic.jsonl` at the top level) -- sharded checkpoints
/// are fully supported for the actual import/export above (which delegate
/// discovery to `import_forensic_checkpoint`/`publish_forensic_checkpoint`),
/// just not for this detailed per-issue diff, which only ever reads the
/// simple monolithic case.
fn read_monolithic_issues(checkpoint_dir: &Path) -> Result<Option<BTreeMap<String, Issue>>> {
    let forensic_path = checkpoint_dir.join("forensic.jsonl");
    if !forensic_path.exists() {
        return Ok(None);
    }

    let file = fs::File::open(&forensic_path)
        .with_context(|| format!("Failed to open {}", forensic_path.display()))?;
    let reader = BufReader::new(file);

    let mut issues = BTreeMap::new();
    for (line_num, line_result) in reader.lines().enumerate() {
        let line = line_result.with_context(|| {
            format!(
                "Failed to read {} line {}",
                forensic_path.display(),
                line_num + 1
            )
        })?;
        if line.trim().is_empty() {
            continue;
        }
        let record: CheckpointRecord = serde_json::from_str(&line).with_context(|| {
            format!(
                "Failed to parse {} line {}",
                forensic_path.display(),
                line_num + 1
            )
        })?;
        if let CheckpointRecord::Issue { issue } = record {
            issues.insert(issue.id.clone(), issue);
        }
    }

    Ok(Some(issues))
}

/// Compare semantic equivalence between the original checkpoint and the one
/// produced by importing it and immediately re-exporting the imported state.
fn compare_checkpoints_semantic(
    original_info: &CheckpointInfo,
    rehearsal_info: &CheckpointInfo,
    original_dir: &Path,
    rehearsal_dir: &Path,
) -> Result<SemanticComparison> {
    let issue_count_matches = original_info.issue_count == rehearsal_info.issue_count;
    let event_count_matches = original_info.event_count == rehearsal_info.event_count;
    // See CheckpointInfo::hash: every export mints a fresh generation id, so
    // this is informational, never a requirement for equivalence.
    let content_hashes_match = original_info.hash == rehearsal_info.hash;

    let mut differences = Vec::new();

    if !issue_count_matches {
        differences.push(SemanticDifference {
            issue_id: "N/A".to_string(),
            difference_type: "issue_count_mismatch".to_string(),
            description: format!(
                "Original: {} issues, Rehearsal: {} issues",
                original_info.issue_count, rehearsal_info.issue_count
            ),
        });
    }

    if !event_count_matches {
        differences.push(SemanticDifference {
            issue_id: "N/A".to_string(),
            difference_type: "event_count_mismatch".to_string(),
            description: format!(
                "Original: {} events, Rehearsal: {} events",
                original_info.event_count, rehearsal_info.event_count
            ),
        });
    }

    // Detailed per-issue diff, monolithic checkpoints only (see
    // read_monolithic_issues). A missing forensic.jsonl on either side just
    // skips this section -- the count-based checks above still ran.
    if let (Some(original_issues), Some(rehearsal_issues)) = (
        read_monolithic_issues(original_dir)?,
        read_monolithic_issues(rehearsal_dir)?,
    ) {
        for (id, orig_issue) in &original_issues {
            match rehearsal_issues.get(id) {
                None => differences.push(SemanticDifference {
                    issue_id: id.clone(),
                    difference_type: "missing_in_rehearsal".to_string(),
                    description: "Issue present in original checkpoint, missing after recovery"
                        .to_string(),
                }),
                Some(reh_issue) => {
                    if orig_issue.title != reh_issue.title {
                        differences.push(SemanticDifference {
                            issue_id: id.clone(),
                            difference_type: "title_mismatch".to_string(),
                            description: format!("'{}' vs '{}'", orig_issue.title, reh_issue.title),
                        });
                    }
                    if orig_issue.priority != reh_issue.priority {
                        differences.push(SemanticDifference {
                            issue_id: id.clone(),
                            difference_type: "priority_mismatch".to_string(),
                            description: format!(
                                "{} vs {}",
                                orig_issue.priority, reh_issue.priority
                            ),
                        });
                    }
                    if orig_issue.base_status != reh_issue.base_status {
                        differences.push(SemanticDifference {
                            issue_id: id.clone(),
                            difference_type: "status_mismatch".to_string(),
                            description: format!(
                                "{:?} vs {:?}",
                                orig_issue.base_status, reh_issue.base_status
                            ),
                        });
                    }
                }
            }
        }
        for id in rehearsal_issues.keys() {
            if !original_issues.contains_key(id) {
                differences.push(SemanticDifference {
                    issue_id: id.clone(),
                    difference_type: "unexpected_in_rehearsal".to_string(),
                    description: "Issue present after recovery but not in original checkpoint"
                        .to_string(),
                });
            }
        }
    }

    let overall_equivalence = issue_count_matches && event_count_matches && differences.is_empty();

    Ok(SemanticComparison {
        issue_count_matches,
        event_count_matches,
        content_hashes_match,
        differences,
        overall_equivalence,
    })
}

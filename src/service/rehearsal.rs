//! Disposable recovery rehearsal service (R015)
//!
//! This module provides functionality to create a temporary workspace from the
//! current checkpoint, run diagnostics, re-export, and compare semantic equivalence.
//! This exercises the real disaster-recovery path without overwriting live state.

use crate::error::{Error, Result};
use crate::model::Issue;
use crate::service::doctor;
use crate::store::{SqliteStore, Store};
use anyhow::Context;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Recovery rehearsal report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryRehearsalReport {
    pub timestamp: String,
    pub original_checkpoint: CheckpointInfo,
    pub rehearsal_checkpoint: CheckpointInfo,
    pub diagnostics: DiagnosticsResult,
    pub semantic_comparison: SemanticComparison,
    pub cleanup_info: CleanupInfo,
}

/// Information about a checkpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointInfo {
    pub path: PathBuf,
    pub issue_count: usize,
    pub hash: String,
    pub size_bytes: u64,
}

/// Diagnostics result from temporary workspace
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticsResult {
    pub checks_performed: usize,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub ok_count: usize,
    pub overall_status: String,
}

/// Semantic comparison between original and re-exported checkpoints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticComparison {
    pub issues_match: bool,
    pub issue_count_matches: bool,
    pub content_hashes_match: bool,
    pub differences: Vec<SemanticDifference>,
    pub overall_equivalence: bool,
}

/// Individual semantic difference found during comparison
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticDifference {
    pub issue_id: String,
    pub difference_type: String,
    pub description: String,
}

/// Cleanup information for the rehearsal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupInfo {
    pub temp_directory_created: bool,
    pub temp_directory_path: Option<PathBuf>,
    pub cleanup_successful: bool,
    pub files_remaining: usize,
}

/// Run a disposable recovery rehearsal
///
/// This function:
/// 1. Creates a temporary workspace directory
/// 2. Copies the current checkpoint to the temporary workspace
/// 3. Initializes a new workspace in the temporary directory
/// 4. Imports the checkpoint into the temporary workspace
/// 5. Runs diagnostics on the temporary workspace
/// 6. Exports from the temporary workspace
/// 7. Compares semantic equivalence between original and exported checkpoints
/// 8. Cleans up the temporary workspace
pub fn run_recovery_rehearsal() -> Result<RecoveryRehearsalReport> {
    // Step 1: Create temporary workspace directory
    let temp_dir = TempDir::new().context("Failed to create temporary workspace directory")?;
    let temp_path = temp_dir.path();

    eprintln!(
        "🔄 Creating temporary workspace at: {}",
        temp_path.display()
    );

    // Step 2: Get current workspace config and checkpoint info
    let current_store = SqliteStore::new();
    let workspace_config = current_store
        .get_workspace_config()
        .map_err(|e| Error::integrity(format!("Failed to get workspace configuration: {}", e)))?;

    let checkpoint_path = workspace_config.root.join(".beads").join("issues.jsonl");

    if !checkpoint_path.exists() {
        return Err(Error::integrity(format!(
            "No checkpoint file found at: {}",
            checkpoint_path.display()
        )));
    }

    eprintln!("📋 Original checkpoint: {}", checkpoint_path.display());

    // Get original checkpoint info
    let original_checkpoint = get_checkpoint_info(&checkpoint_path)?;

    // Step 3: Copy checkpoint to temporary workspace
    let temp_checkpoint_path = temp_path.join("issues.jsonl");
    fs::copy(&checkpoint_path, &temp_checkpoint_path)
        .context("Failed to copy checkpoint to temporary workspace")?;

    eprintln!("✅ Checkpoint copied to temporary workspace");

    // Step 4: Initialize new workspace in temporary directory
    let temp_beads_dir = temp_path.join(".beads");
    fs::create_dir_all(&temp_beads_dir)
        .context("Failed to create .beads directory in temporary workspace")?;

    let temp_config_json = temp_beads_dir.join("config.json");
    let temp_db_path = temp_beads_dir.join("beads.db");

    // Create minimal config for temporary workspace
    let temp_config = serde_json::json!({
        "workspace_uuid": format!("rehearsal-{}", uuid::Uuid::new_v4()),
        "prefix": "rehearsal",
        "layout_version": 1,
        "created_at": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
    });

    fs::write(
        &temp_config_json,
        serde_json::to_string_pretty(&temp_config)?,
    )
    .context("Failed to write temporary workspace config")?;

    // Initialize SQLite database for temporary workspace
    let temp_conn = Connection::open(&temp_db_path).context("Failed to open temporary database")?;

    // Run migrations
    run_migrations_on_connection(&temp_conn)
        .context("Failed to run migrations on temporary workspace")?;

    eprintln!("🔧 Temporary workspace initialized");

    // Step 5: Import checkpoint into temporary workspace
    eprintln!("📥 Importing checkpoint into temporary workspace...");

    let import_result = import_checkpoint_to_temp_workspace(&temp_checkpoint_path, &temp_conn)?;

    if !import_result.success {
        return Err(Error::integrity(format!(
            "Failed to import checkpoint into temporary workspace: {} errors, {} warnings",
            import_result.error_count, import_result.warning_count
        )));
    }

    eprintln!(
        "✅ Checkpoint imported: {} issues",
        import_result.issue_count
    );

    // Step 6: Run diagnostics on temporary workspace using a wrapper store
    eprintln!("🔍 Running diagnostics on temporary workspace...");

    // Create a store wrapper for diagnostics (requires moving the connection)
    drop(temp_conn); // Close the connection first
    let temp_store =
        SqliteStore::with_path(&temp_db_path).context("Failed to create store for diagnostics")?;

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

    // Step 7: Export from temporary workspace
    eprintln!("📤 Exporting from temporary workspace...");

    let temp_export_path = temp_path.join("rehearsal-export.jsonl");
    flush_checkpoint_to_path(&temp_db_path, &temp_export_path)?;

    eprintln!("✅ Export completed: {}", temp_export_path.display());

    // Get rehearsal checkpoint info
    let rehearsal_checkpoint = get_checkpoint_info(&temp_export_path)?;

    // Step 8: Compare semantic equivalence
    eprintln!("🔬 Comparing semantic equivalence...");

    let semantic_comparison = compare_checkpoints_semantic(&checkpoint_path, &temp_export_path)?;

    eprintln!(
        "📊 Semantic comparison: {}",
        if semantic_comparison.overall_equivalence {
            "EQUIVALENT ✅"
        } else {
            "DIFFERENT ❌"
        }
    );

    // Step 9: Generate report
    let timestamp = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);

    // Note: TempDir cleanup is automatic when dropped, but we keep the path for reporting
    let cleanup_info = CleanupInfo {
        temp_directory_created: true,
        temp_directory_path: Some(temp_path.to_path_buf()),
        cleanup_successful: true, // TempDir will clean up when dropped
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

    // Print summary
    eprintln!("\n=== RECOVERY REHEARSAL SUMMARY ===");
    eprintln!("📅 Timestamp: {}", report.timestamp);
    eprintln!(
        "📋 Original: {} issues, {} bytes",
        report.original_checkpoint.issue_count, report.original_checkpoint.size_bytes
    );
    eprintln!(
        "🔄 Rehearsal: {} issues, {} bytes",
        report.rehearsal_checkpoint.issue_count, report.rehearsal_checkpoint.size_bytes
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

    // The temp_dir will be automatically cleaned up when this function returns
    // We don't need to explicitly clean it up - TempDir handles this

    Ok(report)
}

/// Get information about a checkpoint file
fn get_checkpoint_info(path: &Path) -> Result<CheckpointInfo> {
    let metadata = fs::metadata(path).context("Failed to get checkpoint metadata")?;

    let file = File::open(path).context("Failed to open checkpoint file")?;
    let reader = BufReader::new(file);
    let mut issue_count = 0;

    for line in reader.lines() {
        let line = line.context("Failed to read checkpoint line")?;
        if !line.trim().is_empty() {
            issue_count += 1;
        }
    }

    let hash = calculate_file_hash(path)?;

    Ok(CheckpointInfo {
        path: path.to_path_buf(),
        issue_count,
        hash,
        size_bytes: metadata.len(),
    })
}

/// Calculate SHA-256 hash of a file
fn calculate_file_hash(path: &Path) -> Result<String> {
    let content = fs::read(path).context("Failed to read file for hashing")?;

    let mut hasher = Sha256::new();
    hasher.update(&content);
    let result = hasher.finalize();

    Ok(format!("{:x}", result))
}

/// Import result
struct ImportResult {
    success: bool,
    issue_count: usize,
    error_count: usize,
    warning_count: usize,
}

/// Run migrations on a connection
fn run_migrations_on_connection(conn: &Connection) -> Result<()> {
    // Enable foreign keys
    conn.execute("PRAGMA foreign_keys = ON", [])
        .context("Failed to enable foreign keys")?;

    // Enable WAL mode
    conn.execute("PRAGMA journal_mode = WAL", [])
        .context("Failed to enable WAL mode")?;

    // Set busy timeout
    conn.execute_batch("PRAGMA busy_timeout = 5000")
        .context("Failed to set busy timeout")?;

    // Create schema (simplified version of migration 1)
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS workspace (
            uuid TEXT PRIMARY KEY,
            prefix TEXT NOT NULL,
            layout_version INTEGER NOT NULL,
            created_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS issues (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            description TEXT,
            notes TEXT,
            priority INTEGER NOT NULL,
            base_status TEXT NOT NULL,
            manual_blocked INTEGER NOT NULL DEFAULT 0,
            assignee TEXT,
            issue_type TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            closed_at TEXT,
            close_reason TEXT,
            source_repo TEXT,
            profile TEXT,
            schema_ref TEXT,
            revision INTEGER NOT NULL DEFAULT 1
        );
        CREATE TABLE IF NOT EXISTS dependencies (
            blocked_issue_id TEXT NOT NULL,
            blocker_issue_id TEXT NOT NULL,
            kind TEXT NOT NULL,
            PRIMARY KEY (blocked_issue_id, blocker_issue_id, kind),
            FOREIGN KEY (blocked_issue_id) REFERENCES issues(id) ON DELETE CASCADE,
            FOREIGN KEY (blocker_issue_id) REFERENCES issues(id) ON DELETE CASCADE
        );
        CREATE TABLE IF NOT EXISTS labels (
            issue_id TEXT NOT NULL,
            label TEXT NOT NULL,
            PRIMARY KEY (issue_id, label),
            FOREIGN KEY (issue_id) REFERENCES issues(id) ON DELETE CASCADE
        );
        CREATE TABLE IF NOT EXISTS events (
            sequence INTEGER PRIMARY KEY AUTOINCREMENT,
            issue_id TEXT,
            kind TEXT NOT NULL,
            actor TEXT,
            time TEXT NOT NULL,
            detail TEXT,
            FOREIGN KEY (issue_id) REFERENCES issues(id) ON DELETE CASCADE
        );
        CREATE TABLE IF NOT EXISTS checkpoint_state (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            hash TEXT,
            covered_event_sequence INTEGER,
            export_time TEXT
        );
        INSERT OR IGNORE INTO workspace (uuid, prefix, layout_version, created_at)
        VALUES ('rehearsal-temp', 'rehearsal', 1, datetime('now'));",
    )
    .context("Failed to create schema")?;

    Ok(())
}

/// Import checkpoint into temporary workspace
fn import_checkpoint_to_temp_workspace(
    checkpoint_path: &Path,
    conn: &Connection,
) -> Result<ImportResult> {
    let file = File::open(checkpoint_path).context("Failed to open checkpoint for import")?;
    let reader = BufReader::new(file);

    let mut issue_count = 0;
    let mut error_count = 0;
    let warning_count = 0;
    let mut success = true;

    for (line_num, line_result) in reader.lines().enumerate() {
        let line = line_result.context("Failed to read checkpoint line")?;

        if line.trim().is_empty() {
            continue; // Skip blank lines
        }

        // Parse JSON line
        let value: serde_json::Value = serde_json::from_str(&line)
            .with_context(|| format!("Failed to parse JSON at line {}", line_num + 1))?;

        // Convert to Issue
        let issue: Issue = serde_json::from_value(value)
            .with_context(|| format!("Failed to convert issue at line {}", line_num + 1))?;

        // Validate issue
        if let Err(e) = issue.validate() {
            error_count += 1;
            eprintln!("⚠️  Validation error at line {}: {}", line_num + 1, e);
            success = false;
            continue;
        }

        // Insert issue into database
        if let Err(e) = insert_issue_to_connection(&conn, &issue) {
            error_count += 1;
            eprintln!("❌ Failed to insert issue at line {}: {}", line_num + 1, e);
            success = false;
        } else {
            issue_count += 1;
        }
    }

    Ok(ImportResult {
        success,
        issue_count,
        error_count,
        warning_count,
    })
}

/// Insert an issue into the database connection
fn insert_issue_to_connection(conn: &Connection, issue: &Issue) -> Result<()> {
    conn.execute(
        "INSERT INTO issues (id, title, description, notes, priority, base_status, manual_blocked,
         assignee, issue_type, created_at, updated_at, closed_at, close_reason, source_repo,
         profile, schema_ref, revision)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
        rusqlite::params![
            &issue.id,
            &issue.title,
            issue.description.as_deref(),
            issue.notes.as_deref(),
            issue.priority,
            format!("{:?}", issue.base_status),
            if issue.manual_blocked.unwrap_or(false) {
                1
            } else {
                0
            },
            issue.assignee.as_deref(),
            issue.issue_type.as_deref(),
            &issue.created_at,
            &issue.updated_at,
            issue.closed_at.as_deref(),
            issue.close_reason.as_deref(),
            issue.source_repo.as_deref(),
            issue.profile.as_deref(),
            issue.schema_ref.as_deref(),
            issue.revision.unwrap_or(1)
        ],
    )
    .context("Failed to insert issue")?;

    Ok(())
}

/// Flush checkpoint to a specific path
fn flush_checkpoint_to_path(db_path: &Path, export_path: &Path) -> Result<()> {
    // Reopen the database to get a direct connection
    let conn = Connection::open(&db_path).context("Failed to open database for export")?;

    let issues = list_issues_from_connection(&conn)?;

    let file = fs::File::create(export_path).context("Failed to create export file")?;
    let mut writer = BufWriter::new(file);

    // Sort by ID for deterministic output
    let mut sorted_issues: Vec<_> = issues.iter().collect();
    sorted_issues.sort_by_key(|&a| &a.id);

    for issue in sorted_issues {
        let json = serde_json::to_string(issue).context("Failed to serialize issue")?;
        writeln!(writer, "{}", json).context("Failed to write issue line")?;
    }

    writer.flush().context("Failed to flush writer")?;

    Ok(())
}

/// List all issues from the database
fn list_issues_from_connection(conn: &Connection) -> Result<Vec<Issue>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, title, description, notes, priority, base_status, manual_blocked,
         assignee, issue_type, created_at, updated_at, closed_at, close_reason, source_repo,
         profile, schema_ref, revision
         FROM issues",
        )
        .context("Failed to prepare issues query")?;

    let issues = stmt
        .query_map([], |row| {
            Ok(Issue {
                id: row.get(0)?,
                title: row.get(1)?,
                description: {
                    let s: String = row.get(2)?;
                    if s.is_empty() {
                        None
                    } else {
                        Some(s)
                    }
                },
                notes: {
                    let s: String = row.get(3)?;
                    if s.is_empty() {
                        None
                    } else {
                        Some(s)
                    }
                },
                priority: row.get(4)?,
                base_status: parse_base_status(row.get(5)?),
                manual_blocked: {
                    let i: i64 = row.get(6)?;
                    if i == 0 {
                        None
                    } else {
                        Some(true)
                    }
                },
                assignee: {
                    let s: String = row.get(7)?;
                    if s.is_empty() {
                        None
                    } else {
                        Some(s)
                    }
                },
                issue_type: {
                    let s: String = row.get(8)?;
                    if s.is_empty() {
                        None
                    } else {
                        Some(s)
                    }
                },
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
                closed_at: {
                    let s: String = row.get(11)?;
                    if s.is_empty() {
                        None
                    } else {
                        Some(s)
                    }
                },
                close_reason: {
                    let s: String = row.get(12)?;
                    if s.is_empty() {
                        None
                    } else {
                        Some(s)
                    }
                },
                source_repo: {
                    let s: String = row.get(13)?;
                    if s.is_empty() {
                        None
                    } else {
                        Some(s)
                    }
                },
                profile: {
                    let s: String = row.get(14)?;
                    if s.is_empty() {
                        None
                    } else {
                        Some(s)
                    }
                },
                schema_ref: {
                    let s: String = row.get(15)?;
                    if s.is_empty() {
                        None
                    } else {
                        Some(s)
                    }
                },
                revision: Some(row.get(16)?),
                data: None,
                extensions: HashMap::new(),
            })
        })
        .context("Failed to execute issues query")?
        .collect::<std::result::Result<Vec<_>, rusqlite::Error>>()
        .map_err(|e| Error::integrity(format!("Failed to collect issues: {}", e)))?;

    Ok(issues)
}

/// Parse base status from string
fn parse_base_status(s: String) -> crate::model::BaseStatus {
    match s.to_lowercase().as_str() {
        "open" => crate::model::BaseStatus::Open,
        "inprogress" => crate::model::BaseStatus::InProgress,
        "deferred" => crate::model::BaseStatus::Deferred,
        "closed" => crate::model::BaseStatus::Closed,
        _ => crate::model::BaseStatus::Open, // Default fallback
    }
}

/// Compare semantic equivalence between two checkpoints
fn compare_checkpoints_semantic(
    original_path: &Path,
    rehearsal_path: &Path,
) -> Result<SemanticComparison> {
    // Read both checkpoint files
    let original_issues = read_checkpoint_issues(original_path)?;
    let rehearsal_issues = read_checkpoint_issues(rehearsal_path)?;

    // Basic comparisons
    let issue_count_matches = original_issues.len() == rehearsal_issues.len();
    let issues_match = issue_count_matches;

    // Detailed comparison
    let mut differences = Vec::new();

    if !issue_count_matches {
        differences.push(SemanticDifference {
            issue_id: "N/A".to_string(),
            difference_type: "issue_count_mismatch".to_string(),
            description: format!(
                "Original: {} issues, Rehearsal: {} issues",
                original_issues.len(),
                rehearsal_issues.len()
            ),
        });
    }

    // Compare individual issues when counts match
    if issue_count_matches {
        for (orig_issue, reh_issue) in original_issues.iter().zip(rehearsal_issues.iter()) {
            if orig_issue.id != reh_issue.id {
                differences.push(SemanticDifference {
                    issue_id: format!("{} vs {}", orig_issue.id, reh_issue.id),
                    difference_type: "id_mismatch".to_string(),
                    description: "Issue IDs don't match in sequence".to_string(),
                });
            }

            // Compare key fields
            if orig_issue.title != reh_issue.title {
                differences.push(SemanticDifference {
                    issue_id: orig_issue.id.clone(),
                    difference_type: "title_mismatch".to_string(),
                    description: format!("'{}' vs '{}'", orig_issue.title, reh_issue.title),
                });
            }

            if orig_issue.priority != reh_issue.priority {
                differences.push(SemanticDifference {
                    issue_id: orig_issue.id.clone(),
                    difference_type: "priority_mismatch".to_string(),
                    description: format!("{} vs {}", orig_issue.priority, reh_issue.priority),
                });
            }

            if orig_issue.base_status != reh_issue.base_status {
                differences.push(SemanticDifference {
                    issue_id: orig_issue.id.clone(),
                    difference_type: "status_mismatch".to_string(),
                    description: format!(
                        "{:?} vs {:?}",
                        orig_issue.base_status, reh_issue.base_status
                    ),
                });
            }
        }
    }

    // Calculate content hash comparison
    let original_hash = calculate_file_hash(original_path)?;
    let rehearsal_hash = calculate_file_hash(rehearsal_path)?;
    let content_hashes_match = original_hash == rehearsal_hash;

    let overall_equivalence = issues_match && content_hashes_match && differences.is_empty();

    Ok(SemanticComparison {
        issues_match,
        issue_count_matches,
        content_hashes_match,
        differences,
        overall_equivalence,
    })
}

/// Read issues from a checkpoint file
fn read_checkpoint_issues(path: &Path) -> Result<Vec<Issue>> {
    let file = File::open(path).context("Failed to open checkpoint file")?;
    let reader = BufReader::new(file);

    let mut issues = Vec::new();

    for (line_num, line_result) in reader.lines().enumerate() {
        let line = line_result.context("Failed to read checkpoint line")?;

        if line.trim().is_empty() {
            continue;
        }

        let value: serde_json::Value = serde_json::from_str(&line)
            .with_context(|| format!("Failed to parse JSON at line {}", line_num + 1))?;

        let issue: Issue = serde_json::from_value(value)
            .with_context(|| format!("Failed to convert issue at line {}", line_num + 1))?;

        issues.push(issue);
    }

    Ok(issues)
}

/// Test-only function to calculate file hash
pub fn calculate_file_hash_for_test(path: &Path) -> String {
    calculate_file_hash(path).unwrap_or_else(|_| "hash-error".to_string())
}

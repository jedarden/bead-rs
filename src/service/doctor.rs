//! Doctor service for workspace diagnostics and repair
//!
//! This module provides read-only integrity checks and limited repair operations
//! for the bead workspace.

use crate::error::{Error, Result};
use crate::store::Store;
use std::path::Path;

/// Doctor diagnostic result
#[derive(Debug, Clone)]
pub struct DiagnosticCheck {
    pub name: String,
    pub status: DiagnosticStatus,
    pub message: String,
}

/// Status of a diagnostic check
#[derive(Debug, Clone, PartialEq)]
pub enum DiagnosticStatus {
    Ok,
    Warning,
    Error,
}

/// Doctor diagnostic result
#[derive(Debug, Clone)]
pub struct DoctorDiagnostics {
    pub checks: Vec<DiagnosticCheck>,
    pub has_errors: bool,
    #[allow(dead_code)]
    pub has_warnings: bool,
}

/// Run diagnostics on the workspace
pub fn run_diagnostics(store: &impl Store) -> Result<DoctorDiagnostics> {
    let mut checks = Vec::new();
    let mut has_errors = false;
    let mut has_warnings = false;

    // 1. Workspace/config parsing and permissions
    match check_workspace_config(store) {
        Ok(msg) => {
            checks.push(DiagnosticCheck {
                name: "workspace_config".to_string(),
                status: DiagnosticStatus::Ok,
                message: msg,
            });
        }
        Err(e) => {
            has_errors = true;
            checks.push(DiagnosticCheck {
                name: "workspace_config".to_string(),
                status: DiagnosticStatus::Error,
                message: format!("Workspace config error: {}", e),
            });
        }
    }

    // 2. Database open and schema checks
    match check_database_integrity(store) {
        Ok(msg) => {
            checks.push(DiagnosticCheck {
                name: "database_integrity".to_string(),
                status: DiagnosticStatus::Ok,
                message: msg,
            });
        }
        Err(e) => {
            has_errors = true;
            checks.push(DiagnosticCheck {
                name: "database_integrity".to_string(),
                status: DiagnosticStatus::Error,
                message: format!("Database integrity error: {}", e),
            });
        }
    }

    // 3. Checkpoint state validation
    match check_checkpoint_state(store) {
        Ok(msg) => {
            checks.push(DiagnosticCheck {
                name: "checkpoint_state".to_string(),
                status: DiagnosticStatus::Ok,
                message: msg,
            });
        }
        Err(e) => {
            has_warnings = true;
            checks.push(DiagnosticCheck {
                name: "checkpoint_state".to_string(),
                status: DiagnosticStatus::Warning,
                message: format!("Checkpoint state warning: {}", e),
            });
        }
    }

    // 4. Orphaned temporary files
    match check_temporary_files(store) {
        Ok(msg) => {
            checks.push(DiagnosticCheck {
                name: "temporary_files".to_string(),
                status: DiagnosticStatus::Ok,
                message: msg,
            });
        }
        Err(e) => {
            has_warnings = true;
            checks.push(DiagnosticCheck {
                name: "temporary_files".to_string(),
                status: DiagnosticStatus::Warning,
                message: format!("Temporary files warning: {}", e),
            });
        }
    }

    Ok(DoctorDiagnostics {
        checks,
        has_errors,
        has_warnings,
    })
}

/// Perform repairs on the workspace
pub fn run_repairs(store: &mut impl Store) -> Result<Vec<DiagnosticCheck>> {
    let mut repairs = Vec::new();

    // Check if there are orphaned temporary files that can be cleaned up
    let config = store.get_workspace_config()?;
    let beads_dir = config.root.join(".beads");

    // Look for .tmp files in the .beads directory
    if let Ok(entries) = std::fs::read_dir(&beads_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("tmp") {
                // Try to remove the temporary file
                if std::fs::remove_file(&path).is_ok() {
                    repairs.push(DiagnosticCheck {
                        name: "removed_temp_file".to_string(),
                        status: DiagnosticStatus::Ok,
                        message: format!("Removed temporary file: {}", path.display()),
                    });
                }
            }
        }
    }

    Ok(repairs)
}

/// Check workspace configuration
fn check_workspace_config(store: &impl Store) -> Result<String> {
    let config = store.get_workspace_config()?;

    // Check if config.json exists
    let config_path = config.root.join(".beads/config.json");
    if !config_path.exists() {
        return Err(Error::workspace("config.json not found".to_string()));
    }

    // Check if database exists
    let db_path = config.root.join(".beads/beads.db");
    if !db_path.exists() {
        return Err(Error::workspace("beads.db not found".to_string()));
    }

    // Check directory structure
    let checkpoint_dir = config.root.join(".beads/checkpoint");
    if !checkpoint_dir.exists() {
        return Err(Error::workspace(
            "checkpoint directory not found".to_string(),
        ));
    }

    let receipts_dir = config.root.join(".beads/receipts");
    if !receipts_dir.exists() {
        return Err(Error::workspace("receipts directory not found".to_string()));
    }

    Ok(format!(
        "Workspace config valid: UUID={}, prefix={}",
        config.uuid, config.prefix
    ))
}

/// Check database integrity
fn check_database_integrity(store: &impl Store) -> Result<String> {
    let config = store.get_workspace_config()?;
    let db_path = config.root.join(".beads/beads.db");

    // Try to open database
    let conn = rusqlite::Connection::open(&db_path)
        .map_err(|e| Error::Integrity(format!("Failed to open database: {}", e)))?;

    // Check SQLite integrity
    let mut stmt = conn
        .prepare("PRAGMA integrity_check")
        .map_err(|e| Error::Integrity(format!("Failed to check integrity: {}", e)))?;

    let result: Option<String> = stmt
        .query_row([], |row| row.get(0))
        .map_err(|e| Error::Integrity(format!("Failed to query integrity: {}", e)))?;

    // SQLite returns "ok" if integrity check passes
    match result {
        Some(msg) if msg == "ok" => {
            // Check foreign keys
            let fk_check: String = conn
                .query_row("PRAGMA foreign_key_check", [], |row| row.get(0))
                .unwrap_or_else(|_| "ok".to_string());

            if fk_check == "ok" {
                Ok("Database integrity check passed".to_string())
            } else {
                Err(Error::Integrity(format!(
                    "Foreign key constraint violations: {}",
                    fk_check
                )))
            }
        }
        Some(msg) => Err(Error::Integrity(format!(
            "Database integrity check failed: {}",
            msg
        ))),
        None => Err(Error::Integrity(
            "Integrity check returned no result".to_string(),
        )),
    }
}

/// Check checkpoint state (handles both pre-F017 and F017 formats)
fn check_checkpoint_state(store: &impl Store) -> Result<String> {
    let config = store.get_workspace_config()?;
    let db_path = config.root.join(".beads/beads.db");

    let conn = rusqlite::Connection::open(&db_path)
        .map_err(|e| Error::Integrity(format!("Failed to open database: {}", e)))?;

    // Get current event sequence
    let current_sequence: i64 = conn
        .query_row("SELECT COALESCE(MAX(sequence), 0) FROM events", [], |row| {
            row.get(0)
        })
        .unwrap_or(0);

    // Check for F017 forensic checkpoint first
    let current_json_path = config.root.join(".beads/checkpoint/current.json");
    if current_json_path.exists() {
        return check_forensic_checkpoint(&config, &conn, current_sequence);
    }

    // Fall back to pre-F017 issues.jsonl check
    check_pre_f017_checkpoint(&config, &conn, current_sequence)
}

/// Check F017 forensic checkpoint
fn check_forensic_checkpoint(
    config: &crate::store::WorkspaceConfig,
    conn: &rusqlite::Connection,
    current_sequence: i64,
) -> Result<String> {
    let current_json_path = config.root.join(".beads/checkpoint/current.json");

    // Read current.json pointer
    let pointer_content = std::fs::read_to_string(&current_json_path)
        .map_err(|e| Error::Integrity(format!("Failed to read current.json: {}", e)))?;

    let pointer: serde_json::Value = serde_json::from_str(&pointer_content)
        .map_err(|e| Error::Integrity(format!("Failed to parse current.json: {}", e)))?;

    // Get checkpoint state from database
    let covered_sequence: i64 = conn
        .query_row(
            "SELECT covered_event_sequence FROM checkpoint_state WHERE id = 1",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    let stored_generation: String = conn
        .query_row(
            "SELECT current_generation_id FROM checkpoint_state WHERE id = 1",
            [],
            |row| row.get(0),
        )
        .unwrap_or_else(|_| String::new());

    // Extract pointer values
    let pointer_generation = pointer
        .get("generation_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let pointer_sequence = pointer
        .get("snapshot_sequence")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    let root_hash = pointer
        .get("active_root")
        .and_then(|v| v.get("sha256"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let active_root = pointer
        .get("active_root")
        .and_then(|v| v.get("path"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let _issue_count = pointer
        .get("issue_count")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    let _event_count = pointer
        .get("event_count")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    let total_count = pointer
        .get("total_record_count")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    // Validate generation ID matches
    if !stored_generation.is_empty() && stored_generation != pointer_generation {
        return Err(Error::Integrity(format!(
            "Generation ID mismatch: database={}, pointer={}",
            stored_generation, pointer_generation
        )));
    }

    // Validate sequence numbers
    if pointer_sequence != covered_sequence {
        return Err(Error::Integrity(format!(
            "Sequence mismatch: pointer={}, database={}",
            pointer_sequence, covered_sequence
        )));
    }

    // Check if checkpoint is dirty
    if covered_sequence < current_sequence {
        return Err(Error::workspace(format!(
            "Checkpoint is dirty: covered={}, current={}",
            covered_sequence, current_sequence
        )));
    }

    // Verify root object file exists
    let root_path = config.root.join(".beads/checkpoint").join(active_root);
    if !root_path.exists() {
        return Err(Error::Integrity(format!(
            "Root object file missing: {}",
            active_root
        )));
    }

    // Verify root hash
    if let Ok(actual_hash) = calculate_file_hash(&root_path) {
        if root_hash != actual_hash {
            return Err(Error::Integrity(format!(
                "Root hash mismatch: pointer={}, actual={}",
                root_hash, actual_hash
            )));
        }
    }

    Ok(format!(
        "Forensic checkpoint valid: gen={}, covered={}, records={}, hash={}",
        pointer_generation,
        covered_sequence,
        total_count,
        root_hash.chars().take(8).collect::<String>()
    ))
}

/// Check pre-F017 checkpoint (issues.jsonl)
fn check_pre_f017_checkpoint(
    config: &crate::store::WorkspaceConfig,
    conn: &rusqlite::Connection,
    current_sequence: i64,
) -> Result<String> {
    // Get checkpoint state
    let covered_sequence: i64 = conn
        .query_row(
            "SELECT covered_event_sequence FROM checkpoint_state WHERE id = 1",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    // Check if issues.jsonl exists
    let jsonl_path = config.root.join(".beads/issues.jsonl");
    if !jsonl_path.exists() {
        return Ok(format!(
            "No checkpoint file found ({} events)",
            current_sequence
        ));
    }

    // Check if checkpoint is clean
    if covered_sequence < current_sequence {
        return Err(Error::workspace(format!(
            "Checkpoint is dirty: covered={}, current={}",
            covered_sequence, current_sequence
        )));
    }

    // Try to read first line to verify it's valid JSONL
    if let Ok(first_line) = std::fs::read_to_string(&jsonl_path) {
        if let Some(first_line) = first_line.lines().next() {
            if !first_line.trim().is_empty() {
                // Try to parse as JSON
                if serde_json::from_str::<serde_json::Value>(first_line.trim()).is_err() {
                    return Err(Error::Integrity(
                        "issues.jsonl contains invalid JSON".to_string(),
                    ));
                }
            }
        }
    }

    // Calculate hash and compare with checkpoint state
    let stored_hash: String = conn
        .query_row(
            "SELECT last_interchange_hash FROM checkpoint_state WHERE id = 1",
            [],
            |row| row.get(0),
        )
        .unwrap_or_else(|_| String::new());

    if !stored_hash.is_empty() {
        // Calculate actual hash
        if let Ok(actual_hash) = calculate_file_hash(&jsonl_path) {
            if stored_hash != actual_hash {
                return Err(Error::Integrity(format!(
                    "Checkpoint hash mismatch: stored={}, actual={}",
                    stored_hash, actual_hash
                )));
            }
        }

        Ok(format!(
            "Checkpoint state clean: covered={}, hash={}",
            covered_sequence,
            stored_hash.chars().take(8).collect::<String>()
        ))
    } else {
        Ok(format!(
            "Checkpoint state clean: covered={}, no hash",
            covered_sequence
        ))
    }
}

/// Check for orphaned temporary files
fn check_temporary_files(store: &impl Store) -> Result<String> {
    let config = store.get_workspace_config()?;
    let beads_dir = config.root.join(".beads");

    let mut temp_count = 0;

    if let Ok(entries) = std::fs::read_dir(&beads_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            // Check for .tmp files
            if path.extension().and_then(|s| s.to_str()) == Some("tmp") {
                temp_count += 1;
            }
        }
    }

    if temp_count == 0 {
        Ok("No orphaned temporary files found".to_string())
    } else {
        Err(Error::workspace(format!(
            "Found {} temporary file(s) that can be cleaned up with --repair",
            temp_count
        )))
    }
}

/// Calculate SHA-256 hash of a file
fn calculate_file_hash(path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};

    let contents = std::fs::read(path)
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to read file: {}", e)))?;

    let mut hasher = Sha256::new();
    hasher.update(&contents);
    let result = hasher.finalize();

    Ok(format!("{:x}", result))
}

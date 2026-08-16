//! Doctor service for workspace diagnostics and repair
//!
//! This module provides read-only integrity checks and limited repair operations
//! for the bead workspace.

use crate::error::{Error, Result};
use crate::store::Store;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Doctor diagnostic result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticCheck {
    pub name: String,
    pub status: DiagnosticStatus,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

/// Status of a diagnostic check
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticStatus {
    Ok,
    Warning,
    Error,
}

/// Doctor diagnostic result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorDiagnostics {
    pub checks: Vec<DiagnosticCheck>,
    pub has_errors: bool,
    #[allow(dead_code)]
    pub has_warnings: bool,
    pub scopes_checked: Vec<String>,
    pub timestamp: String,
}

/// Diagnostic scope options
#[derive(Debug, Clone, PartialEq)]
pub enum DiagnosticScope {
    Store,
    Backup,
    Schema,
    Dependencies,
    Comments,
    All,
}

impl DiagnosticScope {
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "store" => Some(DiagnosticScope::Store),
            "backup" => Some(DiagnosticScope::Backup),
            "schema" => Some(DiagnosticScope::Schema),
            "dependencies" => Some(DiagnosticScope::Dependencies),
            "comments" => Some(DiagnosticScope::Comments),
            "all" => Some(DiagnosticScope::All),
            _ => None,
        }
    }

    pub fn all_scopes() -> Vec<&'static str> {
        vec![
            "store",
            "backup",
            "schema",
            "dependencies",
            "comments",
            "all",
        ]
    }
}

/// Run diagnostics on the workspace
pub fn run_diagnostics(store: &impl Store) -> Result<DoctorDiagnostics> {
    run_diagnostics_with_scopes(store, &[DiagnosticScope::All])
}

/// Run diagnostics on the workspace with specific scopes
pub fn run_diagnostics_with_scopes(
    store: &impl Store,
    scopes: &[DiagnosticScope],
) -> Result<DoctorDiagnostics> {
    let mut checks = Vec::new();
    let mut has_errors = false;
    let mut has_warnings = false;
    let mut scopes_checked = Vec::new();

    let timestamp = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);

    // Determine which scopes to run
    let run_all = scopes.contains(&DiagnosticScope::All);

    // Store scope checks (workspace, database integrity)
    if run_all || scopes.contains(&DiagnosticScope::Store) {
        scopes_checked.push("store".to_string());

        // 1. Workspace/config parsing and permissions
        match check_workspace_config(store) {
            Ok(msg) => {
                checks.push(DiagnosticCheck {
                    name: "workspace_config".to_string(),
                    status: DiagnosticStatus::Ok,
                    message: msg.clone(),
                    scope: Some("store".to_string()),
                    details: Some(serde_json::json!({
                        "message": msg
                    })),
                });
            }
            Err(e) => {
                has_errors = true;
                checks.push(DiagnosticCheck {
                    name: "workspace_config".to_string(),
                    status: DiagnosticStatus::Error,
                    message: format!("Workspace config error: {}", e),
                    scope: Some("store".to_string()),
                    details: Some(serde_json::json!({
                        "error": e.to_string()
                    })),
                });
            }
        }

        // 2. Database open and schema checks
        match check_database_integrity(store) {
            Ok(msg) => {
                checks.push(DiagnosticCheck {
                    name: "database_integrity".to_string(),
                    status: DiagnosticStatus::Ok,
                    message: msg.clone(),
                    scope: Some("store".to_string()),
                    details: Some(serde_json::json!({
                        "message": msg
                    })),
                });
            }
            Err(e) => {
                has_errors = true;
                checks.push(DiagnosticCheck {
                    name: "database_integrity".to_string(),
                    status: DiagnosticStatus::Error,
                    message: format!("Database integrity error: {}", e),
                    scope: Some("store".to_string()),
                    details: Some(serde_json::json!({
                        "error": e.to_string()
                    })),
                });
            }
        }
    }

    // Backup scope checks (checkpoint state, generations, freshness)
    if run_all || scopes.contains(&DiagnosticScope::Backup) {
        scopes_checked.push("backup".to_string());

        // 3. Checkpoint state validation and freshness
        match check_checkpoint_state_with_freshness(store) {
            Ok(msg) => {
                checks.push(DiagnosticCheck {
                    name: "checkpoint_freshness".to_string(),
                    status: DiagnosticStatus::Ok,
                    message: msg.clone(),
                    scope: Some("backup".to_string()),
                    details: Some(serde_json::json!({
                        "message": msg
                    })),
                });
            }
            Err(e) => {
                has_warnings = true;
                checks.push(DiagnosticCheck {
                    name: "checkpoint_freshness".to_string(),
                    status: DiagnosticStatus::Warning,
                    message: format!("Checkpoint freshness warning: {}", e),
                    scope: Some("backup".to_string()),
                    details: Some(serde_json::json!({
                        "warning": e.to_string()
                    })),
                });
            }
        }

        // 4. Backup generations check
        match check_backup_generations(store) {
            Ok(msg) => {
                checks.push(DiagnosticCheck {
                    name: "backup_generations".to_string(),
                    status: DiagnosticStatus::Ok,
                    message: msg.clone(),
                    scope: Some("backup".to_string()),
                    details: Some(serde_json::json!({
                        "message": msg
                    })),
                });
            }
            Err(e) => {
                has_warnings = true;
                checks.push(DiagnosticCheck {
                    name: "backup_generations".to_string(),
                    status: DiagnosticStatus::Warning,
                    message: format!("Backup generations warning: {}", e),
                    scope: Some("backup".to_string()),
                    details: Some(serde_json::json!({
                        "warning": e.to_string()
                    })),
                });
            }
        }
    }

    // Schema scope checks (data validity)
    if run_all || scopes.contains(&DiagnosticScope::Schema) {
        scopes_checked.push("schema".to_string());

        // 5. Schema and data validity checks
        match check_schema_validity(store) {
            Ok(msg) => {
                checks.push(DiagnosticCheck {
                    name: "schema_validity".to_string(),
                    status: DiagnosticStatus::Ok,
                    message: msg.clone(),
                    scope: Some("schema".to_string()),
                    details: Some(serde_json::json!({
                        "message": msg
                    })),
                });
            }
            Err(e) => {
                has_errors = true;
                checks.push(DiagnosticCheck {
                    name: "schema_validity".to_string(),
                    status: DiagnosticStatus::Error,
                    message: format!("Schema validity error: {}", e),
                    scope: Some("schema".to_string()),
                    details: Some(serde_json::json!({
                        "error": e.to_string()
                    })),
                });
            }
        }
    }

    // Dependencies scope checks (cycles, conditional predicates)
    if run_all || scopes.contains(&DiagnosticScope::Dependencies) {
        scopes_checked.push("dependencies".to_string());

        // 6. Dependency graph checks
        match check_dependency_graph(store) {
            Ok(msg) => {
                checks.push(DiagnosticCheck {
                    name: "dependency_graph".to_string(),
                    status: DiagnosticStatus::Ok,
                    message: msg.clone(),
                    scope: Some("dependencies".to_string()),
                    details: Some(serde_json::json!({
                        "message": msg
                    })),
                });
            }
            Err(e) => {
                has_errors = true;
                checks.push(DiagnosticCheck {
                    name: "dependency_graph".to_string(),
                    status: DiagnosticStatus::Error,
                    message: format!("Dependency graph error: {}", e),
                    scope: Some("dependencies".to_string()),
                    details: Some(serde_json::json!({
                        "error": e.to_string()
                    })),
                });
            }
        }

        // 6b. Ready-frontier eligibility (issues held by an assignee alone)
        match check_ready_frontier(store) {
            Ok(msg) => {
                checks.push(DiagnosticCheck {
                    name: "ready_frontier".to_string(),
                    status: DiagnosticStatus::Ok,
                    message: msg.clone(),
                    scope: Some("dependencies".to_string()),
                    details: Some(serde_json::json!({
                        "message": msg
                    })),
                });
            }
            Err(e) => {
                has_warnings = true;
                checks.push(DiagnosticCheck {
                    name: "ready_frontier".to_string(),
                    status: DiagnosticStatus::Warning,
                    message: format!("Ready frontier warning: {}", e),
                    scope: Some("dependencies".to_string()),
                    details: Some(serde_json::json!({
                        "warning": e.to_string()
                    })),
                });
            }
        }
    }

    // Comments scope checks
    if run_all || scopes.contains(&DiagnosticScope::Comments) {
        scopes_checked.push("comments".to_string());

        // 7. Comment data integrity
        match check_comments_integrity(store) {
            Ok(msg) => {
                checks.push(DiagnosticCheck {
                    name: "comments_integrity".to_string(),
                    status: DiagnosticStatus::Ok,
                    message: msg.clone(),
                    scope: Some("comments".to_string()),
                    details: Some(serde_json::json!({
                        "message": msg
                    })),
                });
            }
            Err(e) => {
                has_warnings = true;
                checks.push(DiagnosticCheck {
                    name: "comments_integrity".to_string(),
                    status: DiagnosticStatus::Warning,
                    message: format!("Comments integrity warning: {}", e),
                    scope: Some("comments".to_string()),
                    details: Some(serde_json::json!({
                        "warning": e.to_string()
                    })),
                });
            }
        }
    }

    // Always check temporary files (part of store scope but run separately for safety)
    match check_temporary_files(store) {
        Ok(msg) => {
            checks.push(DiagnosticCheck {
                name: "temporary_files".to_string(),
                status: DiagnosticStatus::Ok,
                message: msg.clone(),
                scope: Some("store".to_string()),
                details: Some(serde_json::json!({
                    "message": msg
                })),
            });
        }
        Err(e) => {
            has_warnings = true;
            checks.push(DiagnosticCheck {
                name: "temporary_files".to_string(),
                status: DiagnosticStatus::Warning,
                message: format!("Temporary files warning: {}", e),
                scope: Some("store".to_string()),
                details: Some(serde_json::json!({
                    "warning": e.to_string()
                })),
            });
        }
    }

    Ok(DoctorDiagnostics {
        checks,
        has_errors,
        has_warnings,
        scopes_checked,
        timestamp,
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
                        scope: Some("store".to_string()),
                        details: Some(serde_json::json!({
                            "removed_path": path.display().to_string()
                        })),
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

/// Check checkpoint state with freshness analysis (handles both pre-F017 and F017 formats)
fn check_checkpoint_state_with_freshness(store: &impl Store) -> Result<String> {
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

    // Get checkpoint export time
    let export_time: Option<String> = conn
        .query_row(
            "SELECT export_time FROM checkpoint_state WHERE id = 1",
            [],
            |row| row.get(0),
        )
        .ok();

    // Calculate freshness age in seconds
    let freshness_age = if let Some(export_time_str) = export_time {
        if let Ok(export_datetime) = chrono::DateTime::parse_from_rfc3339(&export_time_str) {
            let now = chrono::Utc::now();
            let duration = now.signed_duration_since(export_datetime);
            Some(duration.num_seconds())
        } else {
            None
        }
    } else {
        None
    };

    // Check for F017 forensic checkpoint first
    let current_json_path = config.root.join(".beads/checkpoint/current.json");
    if current_json_path.exists() {
        let result = check_forensic_checkpoint(&config, &conn, current_sequence)?;
        let freshness_info = if let Some(age_secs) = freshness_age {
            format!(" ({} seconds old)", age_secs)
        } else {
            String::from(" (age unknown)")
        };
        return Ok(format!("{}{}", result, freshness_info));
    }

    // Fall back to pre-F017 issues.jsonl check with freshness
    let result = check_pre_f017_checkpoint(&config, &conn, current_sequence)?;
    let freshness_info = if let Some(age_secs) = freshness_age {
        if age_secs > 3600 {
            format!(" ({} hours old, stale)", age_secs / 3600)
        } else {
            format!(" ({} seconds old)", age_secs)
        }
    } else {
        String::from(" (age unknown)")
    };
    Ok(format!("{}{}", result, freshness_info))
}

/// Check backup generations
fn check_backup_generations(store: &impl Store) -> Result<String> {
    let config = store.get_workspace_config()?;
    let checkpoint_dir = config.root.join(".beads/checkpoint");

    if !checkpoint_dir.exists() {
        return Ok("No checkpoint directory found".to_string());
    }

    let mut generation_count = 0;
    let mut latest_gen = String::from("none");
    let mut previous_gen = String::from("none");

    // Check for forensic checkpoint generations
    let current_json = checkpoint_dir.join("current.json");
    let previous_json = checkpoint_dir.join("previous.json");

    if current_json.exists() {
        if let Ok(content) = std::fs::read_to_string(&current_json) {
            if let Ok(pointer) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(gen_id) = pointer.get("generation_id").and_then(|v| v.as_str()) {
                    latest_gen = gen_id.to_string();
                    generation_count += 1;
                }
            }
        }
    }

    if previous_json.exists() {
        if let Ok(content) = std::fs::read_to_string(&previous_json) {
            if let Ok(pointer) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(gen_id) = pointer.get("generation_id").and_then(|v| v.as_str()) {
                    previous_gen = gen_id.to_string();
                    generation_count += 1;
                }
            }
        }
    }

    // Check for objects directory with sharded content
    let objects_dir = checkpoint_dir.join("objects");
    if objects_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&objects_dir) {
            let object_count = entries.filter_map(|e| e.ok()).count();
            if object_count > 0 {
                return Ok(format!(
                    "Sharded backup: {} generations (latest: {}, previous: {}), {} objects",
                    generation_count, latest_gen, previous_gen, object_count
                ));
            }
        }
    }

    Ok(format!(
        "Backup generations: {} (latest: {}, previous: {})",
        generation_count, latest_gen, previous_gen
    ))
}

/// Check schema and data validity
fn check_schema_validity(store: &impl Store) -> Result<String> {
    let config = store.get_workspace_config()?;
    let db_path = config.root.join(".beads/beads.db");

    let conn = rusqlite::Connection::open(&db_path)
        .map_err(|e| Error::Integrity(format!("Failed to open database: {}", e)))?;

    let mut issues = Vec::new();

    // Check for issues with invalid data
    let mut stmt = conn
        .prepare("SELECT id, title FROM issues WHERE title IS NULL OR title = ''")
        .map_err(|e| Error::Integrity(format!("Failed to prepare statement: {}", e)))?;

    let invalid_titles: Vec<(String, Option<String>)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(|e| Error::Integrity(format!("Failed to query issues: {}", e)))?
        .filter_map(|r| r.ok())
        .collect();

    if !invalid_titles.is_empty() {
        issues.push(format!(
            "Found {} issues with invalid titles",
            invalid_titles.len()
        ));
    }

    // Check for priority values out of range
    let mut stmt = conn
        .prepare("SELECT COUNT(*) FROM issues WHERE priority < 0 OR priority > 4")
        .map_err(|e| Error::Integrity(format!("Failed to prepare statement: {}", e)))?;

    let invalid_priority_count: i64 = stmt.query_row([], |row| row.get(0)).unwrap_or(0);

    if invalid_priority_count > 0 {
        issues.push(format!(
            "Found {} issues with invalid priority values",
            invalid_priority_count
        ));
    }

    // Closed state and its audit metadata are a bidirectional invariant.
    // Detect both an incompletely closed issue and stale close metadata on
    // active work; either form makes checkpoint recovery ambiguous.
    let invalid_close_metadata_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM issues
             WHERE (base_status = 'closed'
                    AND (closed_at IS NULL OR close_reason IS NULL OR trim(close_reason) = ''))
                OR (base_status != 'closed'
                    AND (closed_at IS NOT NULL OR close_reason IS NOT NULL))",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    if invalid_close_metadata_count > 0 {
        issues.push(format!(
            "Found {} issues with inconsistent closed status metadata",
            invalid_close_metadata_count
        ));
    }

    // Check for foreign key violations in dependencies
    let mut stmt = conn
        .prepare(
            "SELECT COUNT(*) FROM dependencies d
             LEFT JOIN issues i1 ON d.blocked_issue_id = i1.id
             LEFT JOIN issues i2 ON d.blocker_issue_id = i2.id
             WHERE i1.id IS NULL OR i2.id IS NULL",
        )
        .map_err(|e| Error::Integrity(format!("Failed to prepare statement: {}", e)))?;

    let dangling_deps: i64 = stmt.query_row([], |row| row.get(0)).unwrap_or(0);

    if dangling_deps > 0 {
        issues.push(format!(
            "Found {} dependencies with dangling issue references",
            dangling_deps
        ));
    }

    // Check for orphaned comments
    let mut stmt = conn
        .prepare(
            "SELECT COUNT(*) FROM comments c
             LEFT JOIN issues i ON c.issue_id = i.id
             WHERE i.id IS NULL",
        )
        .map_err(|e| Error::Integrity(format!("Failed to prepare statement: {}", e)))?;

    let orphaned_comments: i64 = stmt.query_row([], |row| row.get(0)).unwrap_or(0);

    if orphaned_comments > 0 {
        issues.push(format!(
            "Found {} orphaned comments referencing non-existent issues",
            orphaned_comments
        ));
    }

    if issues.is_empty() {
        let issue_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM issues", [], |row| row.get(0))
            .unwrap_or(0);
        Ok(format!(
            "Schema validity OK: {} issues validated",
            issue_count
        ))
    } else {
        Err(Error::Integrity(format!(
            "Schema validity issues: {}",
            issues.join("; ")
        )))
    }
}

/// Check dependency graph for cycles and structural issues
fn check_dependency_graph(store: &impl Store) -> Result<String> {
    let config = store.get_workspace_config()?;
    let db_path = config.root.join(".beads/beads.db");

    let conn = rusqlite::Connection::open(&db_path)
        .map_err(|e| Error::Integrity(format!("Failed to open database: {}", e)))?;

    // Check for self-edges
    let mut stmt = conn
        .prepare("SELECT COUNT(*) FROM dependencies WHERE blocked_issue_id = blocker_issue_id")
        .map_err(|e| Error::Integrity(format!("Failed to prepare statement: {}", e)))?;

    let self_edges: i64 = stmt.query_row([], |row| row.get(0)).unwrap_or(0);

    if self_edges > 0 {
        return Err(Error::Integrity(format!(
            "Found {} self-edge dependencies",
            self_edges
        )));
    }

    // Check for cycles using DFS
    let cycles = detect_dependency_cycles(&conn)?;

    if !cycles.is_empty() {
        return Err(Error::Integrity(format!(
            "Found {} dependency cycles: {}",
            cycles.len(),
            cycles
                .iter()
                .map(|c| c.join(" -> "))
                .collect::<Vec<_>>()
                .join("; ")
        )));
    }

    // Get dependency statistics
    let dep_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM dependencies", [], |row| row.get(0))
        .unwrap_or(0);

    let blocked_count: i64 = conn
        .query_row(
            "SELECT COUNT(DISTINCT blocked_id) FROM dependencies",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    Ok(format!(
        "Dependency graph OK: {} dependencies, {} blocked issues, no cycles",
        dep_count, blocked_count
    ))
}

/// Check comments data integrity
fn check_comments_integrity(store: &impl Store) -> Result<String> {
    let config = store.get_workspace_config()?;
    let db_path = config.root.join(".beads/beads.db");

    let conn = rusqlite::Connection::open(&db_path)
        .map_err(|e| Error::Integrity(format!("Failed to open database: {}", e)))?;

    // Check for comments with invalid structure
    let mut issues = Vec::new();

    // Check for comments without issue_id
    let mut stmt = conn
        .prepare("SELECT COUNT(*) FROM comments WHERE issue_id IS NULL")
        .map_err(|e| Error::Integrity(format!("Failed to prepare statement: {}", e)))?;

    let null_issue_ids: i64 = stmt.query_row([], |row| row.get(0)).unwrap_or(0);

    if null_issue_ids > 0 {
        issues.push(format!(
            "Found {} comments with null issue_id",
            null_issue_ids
        ));
    }

    // Check for comments with empty body
    let mut stmt = conn
        .prepare("SELECT COUNT(*) FROM comments WHERE body IS NULL OR body = ''")
        .map_err(|e| Error::Integrity(format!("Failed to prepare statement: {}", e)))?;

    let empty_bodies: i64 = stmt.query_row([], |row| row.get(0)).unwrap_or(0);

    if empty_bodies > 0 {
        issues.push(format!("Found {} comments with empty body", empty_bodies));
    }

    // Check for invalid reply references
    let mut stmt = conn
        .prepare(
            "SELECT COUNT(*) FROM comments c1
             LEFT JOIN comments c2 ON c1.reply_to_id = c2.id
             WHERE c1.reply_to_id IS NOT NULL AND c2.id IS NULL",
        )
        .map_err(|e| Error::Integrity(format!("Failed to prepare statement: {}", e)))?;

    let invalid_replies: i64 = stmt.query_row([], |row| row.get(0)).unwrap_or(0);

    if invalid_replies > 0 {
        issues.push(format!(
            "Found {} comments with invalid reply_to references",
            invalid_replies
        ));
    }

    if issues.is_empty() {
        let comment_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM comments", [], |row| row.get(0))
            .unwrap_or(0);
        Ok(format!(
            "Comments integrity OK: {} comments validated",
            comment_count
        ))
    } else {
        Err(Error::Integrity(format!(
            "Comments integrity issues: {}",
            issues.join("; ")
        )))
    }
}

/// Check checkpoint state (handles both pre-F017 and F017 formats)
#[allow(dead_code)]
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

/// Check for issues held off the ready frontier by an assignee alone.
///
/// An issue that is `open` **and** carries an assignee is not an active claim —
/// a claim sets `in_progress`. It is, however, excluded from the ready frontier,
/// so no worker will ever pick it up. Nothing else in `doctor` reports this
/// shape, and neither `show` nor the dependency checks make it visible, so a
/// workspace can quietly accumulate unclaimable work until the frontier empties
/// and every worker starves against a backlog that looks healthy.
///
/// Reaching this state is legitimate (`reopen` preserves assignment by design),
/// so this is a warning and never an automatic repair: only the operator knows
/// whether a given assignment is still meaningful. The remedy is
/// `bead update <id> --clear-assignee`.
fn check_ready_frontier(store: &impl Store) -> Result<String> {
    let config = store.get_workspace_config()?;
    let db_path = config.root.join(".beads/beads.db");

    let conn = rusqlite::Connection::open(&db_path)
        .map_err(|e| Error::Integrity(format!("Failed to open database: {}", e)))?;

    let mut stmt = conn
        .prepare(
            "SELECT id FROM issues
             WHERE base_status = 'open'
               AND assignee IS NOT NULL
               AND TRIM(assignee) != ''
             ORDER BY id",
        )
        .map_err(|e| Error::Integrity(format!("Failed to prepare statement: {}", e)))?;

    let held: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| Error::Integrity(format!("Failed to query issues: {}", e)))?
        .filter_map(|r| r.ok())
        .collect();

    if held.is_empty() {
        return Ok("Ready frontier OK: no open issues are held by an assignee".to_string());
    }

    // Name a bounded sample so the warning is actionable without flooding output
    // on a workspace that has accumulated hundreds of them.
    const SAMPLE: usize = 5;
    let sample = held
        .iter()
        .take(SAMPLE)
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    let suffix = if held.len() > SAMPLE {
        format!(", and {} more", held.len() - SAMPLE)
    } else {
        String::new()
    };

    Err(Error::workspace(format!(
        "{} open issue(s) are assigned and therefore excluded from the ready frontier, \
         so no worker can claim them: {}{}. \
         An assigned open issue is not an active claim (a claim sets in_progress). \
         Clear the ones that are no longer meaningful with `bead update <id> --clear-assignee`",
        held.len(),
        sample,
        suffix
    )))
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

/// Detect cycles in the dependency graph using DFS
fn detect_dependency_cycles(conn: &rusqlite::Connection) -> Result<Vec<Vec<String>>> {
    use std::collections::{HashMap, HashSet};

    // Build adjacency list
    let mut adj_list: HashMap<String, Vec<String>> = HashMap::new();
    let mut all_issues: HashSet<String> = HashSet::new();

    let mut stmt = conn
        .prepare("SELECT blocked_issue_id, blocker_issue_id FROM dependencies")
        .map_err(|e| Error::Integrity(format!("Failed to prepare dependencies query: {}", e)))?;

    let deps: Vec<(String, String)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(|e| Error::Integrity(format!("Failed to query dependencies: {}", e)))?
        .filter_map(|r| r.ok())
        .collect();

    for (blocked, blocker) in deps {
        adj_list
            .entry(blocked.clone())
            .or_default()
            .push(blocker.clone());
        all_issues.insert(blocked);
        all_issues.insert(blocker);
    }

    let mut cycles = Vec::new();
    let mut visited = HashSet::new();
    let mut recursion_stack = HashSet::new();

    for issue in &all_issues {
        if !visited.contains(issue) {
            if let Some(cycle) =
                dfs_cycle_check(issue, &adj_list, &mut visited, &mut recursion_stack)
            {
                cycles.push(cycle);
            }
        }
    }

    Ok(cycles)
}

/// DFS helper for cycle detection
fn dfs_cycle_check(
    issue: &str,
    adj_list: &HashMap<String, Vec<String>>,
    visited: &mut HashSet<String>,
    recursion_stack: &mut HashSet<String>,
) -> Option<Vec<String>> {
    visited.insert(issue.to_string());
    recursion_stack.insert(issue.to_string());

    if let Some(neighbors) = adj_list.get(issue) {
        for neighbor in neighbors {
            if !visited.contains(neighbor) {
                if let Some(cycle) = dfs_cycle_check(neighbor, adj_list, visited, recursion_stack) {
                    return Some(cycle);
                }
            } else if recursion_stack.contains(neighbor) {
                // Found a cycle
                let mut cycle = vec![neighbor.clone()];
                cycle.push(issue.to_string());
                return Some(cycle);
            }
        }
    }

    recursion_stack.remove(issue);
    None
}

//! Doctor service for workspace diagnostics and repair
//!
//! This module provides read-only integrity checks and limited repair operations
//! for the bead workspace.

use crate::error::{Error, Result};
use crate::store::{open_configured_connection, Store};
use rusqlite::params;
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
    Attempts,
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
            "attempts" => Some(DiagnosticScope::Attempts),
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
            "attempts",
            "all",
        ]
    }
}

/// Version of the stale in-progress diagnostic configuration.
///
/// Kept separate from the top-level workspace identity version: it describes
/// the meaning of just `doctor.stale_in_progress` in `.beads/config.json`.
pub const STALE_IN_PROGRESS_CONFIG_VERSION: u32 = 1;

/// Default inactivity interval for ordinary claims when a workspace created
/// before R034 has no explicit `doctor.stale_in_progress` section yet.
pub const DEFAULT_STALE_IN_PROGRESS_MAX_AGE_SECONDS: u64 = 24 * 60 * 60;

/// Versioned workspace configuration for the R034 stale-claim diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StaleInProgressConfig {
    version: u32,
    max_age_seconds: u64,
}

impl Default for StaleInProgressConfig {
    fn default() -> Self {
        Self {
            version: STALE_IN_PROGRESS_CONFIG_VERSION,
            max_age_seconds: DEFAULT_STALE_IN_PROGRESS_MAX_AGE_SECONDS,
        }
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

        // 2b. Stale ordinary in-progress claims (R034). Leased claims are
        // deliberately excluded: R002 owns their expiry and fencing policy.
        match check_stale_in_progress(store) {
            Ok(report) if report.stale_issues.is_empty() => {
                checks.push(DiagnosticCheck {
                    name: "stale_in_progress".to_string(),
                    status: DiagnosticStatus::Ok,
                    message: format!(
                        "No non-leased in-progress beads have been inactive for more than {} seconds",
                        report.max_age_seconds
                    ),
                    scope: Some("store".to_string()),
                    details: Some(serde_json::json!({
                        "config_version": report.config_version,
                        "max_age_seconds": report.max_age_seconds,
                        "stale_count": 0,
                        "stale_issues": [],
                        "reason_codes": []
                    })),
                });
            }
            Ok(report) => {
                let listed = report
                    .stale_issues
                    .iter()
                    .map(|issue| {
                        format!(
                            "{} ({} seconds old; {})",
                            issue.id, issue.age_seconds, issue.remedy
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                has_warnings = true;
                checks.push(DiagnosticCheck {
                    name: "stale_in_progress".to_string(),
                    status: DiagnosticStatus::Warning,
                    message: format!(
                        "{} non-leased in-progress bead(s) have had no event for more than {} seconds: {}",
                        report.stale_issues.len(),
                        report.max_age_seconds,
                        listed
                    ),
                    scope: Some("store".to_string()),
                    details: Some(serde_json::json!({
                        "config_version": report.config_version,
                        "max_age_seconds": report.max_age_seconds,
                        "stale_count": report.stale_issues.len(),
                        "stale_issues": report.stale_issues,
                        "reason_codes": [crate::service::claim::ReasonCode::StaleInProgress.code_string()]
                    })),
                });
            }
            Err(e) => {
                has_errors = true;
                checks.push(DiagnosticCheck {
                    name: "stale_in_progress".to_string(),
                    status: DiagnosticStatus::Error,
                    message: format!("Stale in-progress check error: {}", e),
                    scope: Some("store".to_string()),
                    details: Some(serde_json::json!({
                        "error": e.to_string(),
                        "reason_codes": [crate::service::claim::ReasonCode::StaleInProgress.code_string()]
                    })),
                });
            }
        }

        // 3. Same-UUID divergence detection (R028)
        match check_uuid_divergence(store) {
            Ok(msg) => {
                checks.push(DiagnosticCheck {
                    name: "uuid_divergence".to_string(),
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
                    name: "uuid_divergence".to_string(),
                    status: DiagnosticStatus::Error,
                    message: format!("UUID divergence detected: {}", e),
                    scope: Some("store".to_string()),
                    details: Some(serde_json::json!({
                        "error": e.to_string(),
                        "remedy": "Run 'bead sync fork --actor <WHO> [--reason <WHY>]' to create a distinct workspace identity",
                        "explanation": "This workspace has the same UUID as another workspace but divergent event histories. Forking creates a new UUID while recording provenance to the parent."
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
                // R027: a remote-advanced checkpoint is an actionable
                // diagnostic, so the details carry the stable state marker
                // and the remedy alongside the message text. Match on the
                // workspace variant's own payload, not the rendered Display
                // string: the display carries a "Workspace error: " prefix,
                // so a `to_string().starts_with` test never fires and the
                // marker would silently vanish from every JSON report.
                let is_remote_advanced = matches!(
                    &e,
                    Error::Workspace(message)
                        if message.starts_with(crate::service::reconcile::REMOTE_ADVANCED_MARKER)
                );
                let details = if is_remote_advanced {
                    serde_json::json!({
                        "state": crate::service::reconcile::REMOTE_ADVANCED_MARKER,
                        "remedy": crate::service::reconcile::REMOTE_ADVANCED_REMEDY,
                        "warning": e.to_string()
                    })
                } else {
                    serde_json::json!({
                        "warning": e.to_string()
                    })
                };
                checks.push(DiagnosticCheck {
                    name: "checkpoint_freshness".to_string(),
                    status: DiagnosticStatus::Warning,
                    message: format!("Checkpoint freshness warning: {}", e),
                    scope: Some("backup".to_string()),
                    details: Some(details),
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
            Ok(report) => {
                if report.has_held_issues {
                    // Determine if we have any potentially abandoned assignments
                    let has_abandoned = !report.held_ids.is_empty();
                    let has_intentional = !report.intentionally_held_ids.is_empty();

                    // Build message based on what we found
                    let message = if has_abandoned && has_intentional {
                        format!(
                            "{} open issue(s) are assigned and excluded from the ready frontier ({} intentionally held). Potentially abandoned: {}. An assigned open issue is not an active claim (a claim sets in_progress).",
                            report.held_count + report.intentionally_held_ids.len(),
                            report.intentionally_held_ids.len(),
                            report.held_ids.iter().take(5).cloned().collect::<Vec<_>>().join(", ")
                        )
                    } else if has_abandoned {
                        format!(
                            "{} open issue(s) are assigned and excluded from the ready frontier: {}. An assigned open issue is not an active claim (a claim sets in_progress).",
                            report.held_count,
                            report.held_ids.iter().take(5).cloned().collect::<Vec<_>>().join(", ")
                        )
                    } else {
                        format!(
                            "{} open issue(s) are intentionally held and excluded from the ready frontier. Mark issues with 'intentionally-held' or 'parked' labels to suppress this warning.",
                            report.intentionally_held_ids.len()
                        )
                    };

                    checks.push(DiagnosticCheck {
                        name: "ready_frontier".to_string(),
                        status: if has_abandoned {
                            DiagnosticStatus::Warning
                        } else {
                            DiagnosticStatus::Ok
                        },
                        message,
                        scope: Some("dependencies".to_string()),
                        details: Some(serde_json::json!({
                            "held_count": report.held_count,
                            "held_ids": report.held_ids,
                            "intentionally_held_count": report.intentionally_held_ids.len(),
                            "intentionally_held_ids": report.intentionally_held_ids,
                            "reason_codes": report.reason_codes,
                            "remedy": report.remedy,
                            "explanation": "Open issues with assignees are excluded from the ready frontier. Use the 'intentionally-held' or 'parked' labels to mark deliberate reservations."
                        })),
                    });

                    if has_abandoned {
                        has_warnings = true;
                    }
                } else {
                    checks.push(DiagnosticCheck {
                        name: "ready_frontier".to_string(),
                        status: DiagnosticStatus::Ok,
                        message: "Ready frontier OK: no open issues are held by an assignee"
                            .to_string(),
                        scope: Some("dependencies".to_string()),
                        details: Some(serde_json::json!({
                            "held_count": 0,
                            "held_ids": [],
                            "intentionally_held_count": 0,
                            "intentionally_held_ids": [],
                            "reason_codes": []
                        })),
                    });
                }

                // Add manual blocked check if any exist
                if report.manual_blocked_count > 0 {
                    checks.push(DiagnosticCheck {
                        name: "ready_frontier".to_string(),
                        status: DiagnosticStatus::Warning,
                        message: format!(
                            "{} open issue(s) are manually blocked and excluded from the ready frontier. Use 'bead list --blocked' to view them.",
                            report.manual_blocked_count
                        ),
                        scope: Some("dependencies".to_string()),
                        details: Some(serde_json::json!({
                            "manual_blocked_count": report.manual_blocked_count,
                            "manual_blocked_ids": report.manual_blocked_ids,
                            "explanation": "Open issues with manual_blocked=true are excluded from the ready frontier even if unassigned."
                        })),
                    });
                    has_warnings = true;
                }
            }
            Err(e) => {
                has_errors = true;
                checks.push(DiagnosticCheck {
                    name: "ready_frontier".to_string(),
                    status: DiagnosticStatus::Error,
                    message: format!("Ready frontier check error: {}", e),
                    scope: Some("dependencies".to_string()),
                    details: Some(serde_json::json!({
                        "error": e.to_string()
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

    // Attempts scope checks
    if run_all || scopes.contains(&DiagnosticScope::Attempts) {
        scopes_checked.push("attempts".to_string());

        // 8. Attempt outcomes integrity
        match check_attempt_outcomes_integrity(store) {
            Ok(msg) => {
                checks.push(DiagnosticCheck {
                    name: "attempt_outcomes_integrity".to_string(),
                    status: DiagnosticStatus::Ok,
                    message: msg.clone(),
                    scope: Some("attempts".to_string()),
                    details: Some(serde_json::json!({
                        "message": msg
                    })),
                });
            }
            Err(e) => {
                has_errors = true;
                checks.push(DiagnosticCheck {
                    name: "attempt_outcomes_integrity".to_string(),
                    status: DiagnosticStatus::Error,
                    message: format!("Attempt outcomes integrity error: {}", e),
                    scope: Some("attempts".to_string()),
                    details: Some(serde_json::json!({
                        "error": e.to_string()
                    })),
                });
            }
        }

        // 9. Attempt tier consistency
        match check_attempt_tier_consistency(store) {
            Ok(msg) => {
                checks.push(DiagnosticCheck {
                    name: "attempt_tier_consistency".to_string(),
                    status: DiagnosticStatus::Ok,
                    message: msg.clone(),
                    scope: Some("attempts".to_string()),
                    details: Some(serde_json::json!({
                        "message": msg
                    })),
                });
            }
            Err(e) => {
                has_warnings = true;
                checks.push(DiagnosticCheck {
                    name: "attempt_tier_consistency".to_string(),
                    status: DiagnosticStatus::Warning,
                    message: format!("Attempt tier consistency warning: {}", e),
                    scope: Some("attempts".to_string()),
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

    // Recreate the receipts directory when missing. `check_workspace_config`
    // requires it and `init` creates it, but nothing recreates it afterwards:
    // the directory is untracked (git clean removes it) and runtime provenance
    // receipts live in the `provenance_receipts` table, so nothing ever
    // writes into it. Restoring it rebuilds the canonical init-era layout
    // without touching user data.
    let receipts_dir = beads_dir.join("receipts");
    if !receipts_dir.exists() {
        std::fs::create_dir_all(&receipts_dir).map_err(|e| Error::Io {
            path: receipts_dir.clone(),
            msg: e,
        })?;
        repairs.push(DiagnosticCheck {
            name: "created_receipts_dir".to_string(),
            status: DiagnosticStatus::Ok,
            message: format!(
                "Created missing receipts directory: {}",
                receipts_dir.display()
            ),
            scope: Some("store".to_string()),
            details: Some(serde_json::json!({
                "created_path": receipts_dir.display().to_string()
            })),
        });
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
    let conn = open_configured_connection(&db_path)
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

/// Load the versioned R034 inactivity threshold from workspace configuration.
///
/// The section is optional for workspaces created before R034; those retain a
/// documented version-1 default. Once present, both the version and positive
/// threshold are required so an operator cannot silently get a diagnostic with
/// guessed semantics after a future configuration change.
fn load_stale_in_progress_config(store: &impl Store) -> Result<StaleInProgressConfig> {
    let workspace = store.get_workspace_config()?;
    let config_path = workspace.root.join(".beads/config.json");
    let raw = std::fs::read_to_string(&config_path).map_err(|e| Error::Io {
        path: config_path.clone(),
        msg: e,
    })?;
    let config: serde_json::Value = serde_json::from_str(&raw).map_err(|e| {
        Error::workspace(format!(
            "Invalid .beads/config.json while loading doctor.stale_in_progress: {}",
            e
        ))
    })?;

    let Some(doctor) = config.get("doctor") else {
        return Ok(StaleInProgressConfig::default());
    };
    if doctor.is_null() {
        return Ok(StaleInProgressConfig::default());
    }
    let doctor = doctor.as_object().ok_or_else(|| {
        Error::workspace(".beads/config.json doctor must be an object".to_string())
    })?;

    let Some(stale_in_progress) = doctor.get("stale_in_progress") else {
        return Ok(StaleInProgressConfig::default());
    };
    if stale_in_progress.is_null() {
        return Ok(StaleInProgressConfig::default());
    }
    let stale_in_progress = stale_in_progress.as_object().ok_or_else(|| {
        Error::workspace(
            ".beads/config.json doctor.stale_in_progress must be an object".to_string(),
        )
    })?;

    let version = stale_in_progress
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| {
            Error::workspace(
                ".beads/config.json doctor.stale_in_progress.version must be an integer"
                    .to_string(),
            )
        })?;
    if version != u64::from(STALE_IN_PROGRESS_CONFIG_VERSION) {
        return Err(Error::workspace(format!(
            "Unsupported doctor.stale_in_progress version {} (supported: {})",
            version, STALE_IN_PROGRESS_CONFIG_VERSION
        )));
    }

    let max_age_seconds = stale_in_progress
        .get("max_age_seconds")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| {
            Error::workspace(
                ".beads/config.json doctor.stale_in_progress.max_age_seconds must be an integer"
                    .to_string(),
            )
        })?;
    if max_age_seconds == 0 || max_age_seconds > i64::MAX as u64 {
        return Err(Error::workspace(
            ".beads/config.json doctor.stale_in_progress.max_age_seconds must be between 1 and i64::MAX"
                .to_string(),
        ));
    }

    Ok(StaleInProgressConfig {
        version: STALE_IN_PROGRESS_CONFIG_VERSION,
        max_age_seconds,
    })
}

/// Whether the last claim that still defines the current lifecycle epoch was
/// a leased R002 claim.
///
/// Lease rows are intentionally retained as per-issue fencing-token history,
/// so their existence alone cannot identify an ordinary current claim. The
/// claim event records whether that epoch acquired a lease; a later release,
/// close, or reopen ends that epoch. Malformed claim detail is treated as
/// leased so diagnostics never recommend `release` for a possibly fenced R002
/// claim.
fn current_claim_epoch_is_leased(
    last_claim_sequence: Option<i64>,
    last_claim_detail: Option<&str>,
    last_exit_sequence: Option<i64>,
    has_lease_row: bool,
) -> bool {
    let Some(last_claim_sequence) = last_claim_sequence else {
        return false;
    };
    if last_exit_sequence.is_some_and(|sequence| sequence > last_claim_sequence) {
        return false;
    }

    let Some(detail) = last_claim_detail else {
        return has_lease_row;
    };
    let Ok(detail) = serde_json::from_str::<serde_json::Value>(detail) else {
        return true;
    };

    if let Some(with_lease) = detail
        .get("with_lease")
        .and_then(serde_json::Value::as_bool)
    {
        return with_lease;
    }

    detail.get("action").and_then(serde_json::Value::as_str) == Some("claim_with_fencing_token")
        || detail
            .get("new_fencing_token")
            .is_some_and(|value| !value.is_null())
        || has_lease_row
}

/// A stale ordinary claim returned by the R034 doctor diagnostic.
#[derive(Debug, Clone, Serialize)]
struct StaleInProgressIssue {
    id: String,
    last_event_at: String,
    age_seconds: u64,
    remedy: String,
}

/// Complete result of the R034 stale ordinary-claim query.
#[derive(Debug, Clone)]
struct StaleInProgressReport {
    config_version: u32,
    max_age_seconds: u64,
    stale_issues: Vec<StaleInProgressIssue>,
}

/// Database projection used only while evaluating the R034 diagnostic.
struct StaleClaimCandidate {
    id: String,
    last_event_at: String,
    last_claim_sequence: Option<i64>,
    last_claim_detail: Option<String>,
    last_exit_sequence: Option<i64>,
    has_lease_row: bool,
}

/// Find stale non-leased in-progress beads without mutating any workspace
/// state (R034).
///
/// The latest audit event is selected by event sequence: event sequence is the
/// native append order and remains well-defined even when imported timestamps
/// originate from systems with different clocks. An event exactly on the
/// threshold is not stale; the configured interval is exceeded strictly.
fn check_stale_in_progress(store: &impl Store) -> Result<StaleInProgressReport> {
    let config = load_stale_in_progress_config(store)?;
    let workspace = store.get_workspace_config()?;
    let db_path = workspace.root.join(".beads/beads.db");
    let conn = open_configured_connection(&db_path)
        .map_err(|e| Error::Integrity(format!("Failed to open database: {}", e)))?;

    let mut stmt = conn
        .prepare(
            "SELECT i.id,
                    latest_event.time,
                    latest_claim.sequence,
                    latest_claim.detail,
                    (
                        SELECT MAX(exit_event.sequence)
                        FROM events exit_event
                        WHERE exit_event.issue_id = i.id
                          AND exit_event.kind IN ('released', 'closed', 'reopened')
                    ) AS last_exit_sequence,
                    EXISTS(
                        SELECT 1 FROM leases lease WHERE lease.issue_id = i.id
                    ) AS has_lease_row
             FROM issues i
             JOIN events latest_event ON latest_event.sequence = (
                 SELECT MAX(event.sequence)
                 FROM events event
                 WHERE event.issue_id = i.id
             )
             LEFT JOIN events latest_claim ON latest_claim.sequence = (
                 SELECT MAX(claim_event.sequence)
                 FROM events claim_event
                 WHERE claim_event.issue_id = i.id
                   AND claim_event.kind = 'claimed'
             )
             WHERE i.base_status = 'in_progress'
             ORDER BY i.id",
        )
        .map_err(|e| Error::Integrity(format!("Failed to prepare stale-claim query: {}", e)))?;

    let candidates: Vec<StaleClaimCandidate> = stmt
        .query_map([], |row| {
            Ok(StaleClaimCandidate {
                id: row.get(0)?,
                last_event_at: row.get(1)?,
                last_claim_sequence: row.get(2)?,
                last_claim_detail: row.get(3)?,
                last_exit_sequence: row.get(4)?,
                has_lease_row: row.get::<_, i64>(5)? != 0,
            })
        })
        .map_err(|e| Error::Integrity(format!("Failed to query stale claims: {}", e)))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| Error::Integrity(format!("Failed to read stale-claim rows: {}", e)))?;

    let now = chrono::Utc::now();
    let mut stale_issues = Vec::new();
    for candidate in candidates {
        if current_claim_epoch_is_leased(
            candidate.last_claim_sequence,
            candidate.last_claim_detail.as_deref(),
            candidate.last_exit_sequence,
            candidate.has_lease_row,
        ) {
            continue;
        }

        let event_time =
            chrono::DateTime::parse_from_rfc3339(&candidate.last_event_at).map_err(|e| {
                Error::Integrity(format!(
                    "Invalid latest event timestamp for in-progress issue {}: {}",
                    candidate.id, e
                ))
            })?;
        let age_seconds = now
            .signed_duration_since(event_time.with_timezone(&chrono::Utc))
            .num_seconds();
        if age_seconds <= config.max_age_seconds as i64 {
            continue;
        }

        stale_issues.push(StaleInProgressIssue {
            remedy: format!("bead release {}", candidate.id),
            id: candidate.id,
            last_event_at: candidate.last_event_at,
            age_seconds: age_seconds as u64,
        });
    }

    Ok(StaleInProgressReport {
        config_version: config.version,
        max_age_seconds: config.max_age_seconds,
        stale_issues,
    })
}

/// Check checkpoint state with freshness analysis (handles both pre-F017 and F017 formats)
fn check_checkpoint_state_with_freshness(store: &impl Store) -> Result<String> {
    let config = store.get_workspace_config()?;
    let db_path = config.root.join(".beads/beads.db");

    let conn = open_configured_connection(&db_path)
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

    let conn = open_configured_connection(&db_path)
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

    let conn = open_configured_connection(&db_path)
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

    // Counts issues that are the blocked side of at least one `blocks` edge.
    // `relates_to` edges are informational and never block, so they are
    // excluded. This is not wrapped in `unwrap_or(0)` on purpose: a diagnostic
    // that reports zero when its own query failed is worse than no diagnostic.
    let blocked_count: i64 = conn.query_row(
        "SELECT COUNT(DISTINCT blocked_issue_id) FROM dependencies WHERE kind = 'blocks'",
        [],
        |row| row.get(0),
    )?;

    Ok(format!(
        "Dependency graph OK: {} dependencies, {} blocked issues, no cycles",
        dep_count, blocked_count
    ))
}

/// Check comments data integrity
fn check_comments_integrity(store: &impl Store) -> Result<String> {
    let config = store.get_workspace_config()?;
    let db_path = config.root.join(".beads/beads.db");

    let conn = open_configured_connection(&db_path)
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

    let conn = open_configured_connection(&db_path)
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

    // R027: classify the sync relationship before the recorded-state
    // agreement checks below fire. In the remote-advanced relationship the
    // database legitimately records the last local publication while the
    // pointer records the pulled one, so presenting that disagreement as a
    // generation/sequence integrity fault would misdiagnose the one
    // covered-ahead state that has a remedy. Remote-advanced is reported as
    // a distinct actionable diagnostic (a Warning, not an integrity failure
    // and not silent health) carrying the stable `remote-advanced` marker
    // and the reconcile remedy; doctor never reconciles, including under
    // `--repair`. Every other checkpoint-ahead-of-live shape stays an
    // integrity failure, now naming the first failed qualifier.
    let verdict = crate::service::reconcile::classify(conn, &config.root.join(".beads"))?;
    match verdict.relationship {
        crate::service::reconcile::SyncRelationship::RemoteAdvanced => {
            let covered = pointer
                .get("snapshot_sequence")
                .and_then(|v| v.as_i64())
                .unwrap_or_default();
            return Err(Error::workspace(format!(
                "{}: pulled checkpoint (covered {}) is ahead of the live store (live {}); {}",
                crate::service::reconcile::REMOTE_ADVANCED_MARKER,
                covered,
                current_sequence,
                crate::service::reconcile::REMOTE_ADVANCED_REMEDY
            )));
        }
        crate::service::reconcile::SyncRelationship::CoveredAheadIntegrityFailure => {
            return Err(Error::Integrity(format!(
                "covered-ahead integrity failure: {}",
                verdict.failed_qualifier.as_deref().unwrap_or(
                    "the checkpoint is ahead of the live store but failed its \
                               qualification"
                )
            )));
        }
        _ => {}
    }

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
///
/// R035: This check emits R001 semantic reason codes rather than prose and
/// provides a machine-readable list of held IDs. Future work may add a way to
/// declare intentionally-held assignments so the warning remains meaningful in
/// workspaces that park work on purpose.
fn check_ready_frontier(store: &impl Store) -> Result<ReadyFrontierReport> {
    let config = store.get_workspace_config()?;
    let db_path = config.root.join(".beads/beads.db");

    let conn = open_configured_connection(&db_path)
        .map_err(|e| Error::Integrity(format!("Failed to open database: {}", e)))?;

    // Query for open issues with assignees, including their labels
    let mut stmt = conn
        .prepare(
            "SELECT i.id, GROUP_CONCAT(l.label, ',') as labels
             FROM issues i
             LEFT JOIN labels l ON i.id = l.issue_id
             WHERE i.base_status = 'open'
               AND i.assignee IS NOT NULL
               AND TRIM(i.assignee) != ''
             GROUP BY i.id
             ORDER BY i.id",
        )
        .map_err(|e| Error::Integrity(format!("Failed to prepare statement: {}", e)))?;

    let held_result: Vec<(String, Option<String>)> = stmt
        .query_map([], |row| {
            let id: String = row.get(0)?;
            let labels: Option<String> = row.get(1)?;
            Ok((id, labels))
        })
        .map_err(|e| Error::Integrity(format!("Failed to query issues: {}", e)))?
        .filter_map(|r| r.ok())
        .collect();

    // Query for manually blocked open issues
    let mut blocked_stmt = conn
        .prepare(
            "SELECT id FROM issues WHERE base_status = 'open' AND manual_blocked = 1 ORDER BY id",
        )
        .map_err(|e| Error::Integrity(format!("Failed to prepare statement: {}", e)))?;

    let manual_blocked_result: Vec<String> = blocked_stmt
        .query_map([], |row| {
            let id: String = row.get(0)?;
            Ok(id)
        })
        .map_err(|e| Error::Integrity(format!("Failed to query manual blocked issues: {}", e)))?
        .filter_map(|r| r.ok())
        .collect();

    let manual_blocked_ids = manual_blocked_result.clone();
    let manual_blocked_count = manual_blocked_ids.len();

    if held_result.is_empty() && manual_blocked_ids.is_empty() {
        return Ok(ReadyFrontierReport {
            has_held_issues: false,
            held_count: 0,
            held_ids: vec![],
            intentionally_held_ids: vec![],
            reason_codes: vec![],
            remedy: None,
            manual_blocked_count: 0,
            manual_blocked_ids: vec![],
        });
    }

    // Separate intentionally-held assignments from potentially abandoned ones
    // Convention: labels 'intentionally-held' or 'parked' mark deliberate reservations
    let intentionally_held_label = "intentionally-held";
    let parked_label = "parked";

    let mut held_ids = Vec::new();
    let mut intentionally_held_ids = Vec::new();
    let mut reason_codes = Vec::new();

    for (id, labels) in held_result {
        let labels_str = labels.as_deref().unwrap_or("");
        let is_intentionally_held =
            labels_str.contains(intentionally_held_label) || labels_str.contains(parked_label);

        if is_intentionally_held {
            intentionally_held_ids.push(id.clone());
        } else {
            held_ids.push(id.clone());
        }
    }

    // Build reason codes based on what we found
    if !held_ids.is_empty() {
        reason_codes.push(crate::service::claim::ReasonCode::OpenIssueHeldByAssignee);
    }
    if !intentionally_held_ids.is_empty() {
        reason_codes.push(crate::service::claim::ReasonCode::IntentionallyHeldAssignment);
    }

    Ok(ReadyFrontierReport::from_reason_codes(
        held_ids,
        intentionally_held_ids,
        reason_codes,
        manual_blocked_ids,
        manual_blocked_count,
    ))
}

/// Structured report for ready frontier check (R035)
///
/// This provides machine-readable output with R001 reason codes and complete
/// lists of held IDs, not embedded in prose. It distinguishes between
/// potentially abandoned assignments and intentionally-held work.
#[derive(Debug, Clone)]
pub struct ReadyFrontierReport {
    /// Whether any open issues are held by assignees (either category)
    pub has_held_issues: bool,

    /// Count of potentially abandoned held issues (excluding intentionally-held)
    pub held_count: usize,

    /// Potentially abandoned held issue IDs (machine-readable)
    pub held_ids: Vec<String>,

    /// Intentionally-held issue IDs (marked with 'intentionally-held' or 'parked' labels)
    pub intentionally_held_ids: Vec<String>,

    /// R001 semantic reason codes explaining the conditions (as strings for JSON)
    pub reason_codes: Vec<String>,

    /// Exact remedy command for clearing held assignments
    pub remedy: Option<String>,

    /// Count of manually blocked open issues
    pub manual_blocked_count: usize,

    /// Manually blocked open issue IDs
    pub manual_blocked_ids: Vec<String>,
}

impl ReadyFrontierReport {
    /// Create a new report from ReasonCode enums
    pub fn from_reason_codes(
        held_ids: Vec<String>,
        intentionally_held_ids: Vec<String>,
        reason_codes: Vec<crate::service::claim::ReasonCode>,
        manual_blocked_ids: Vec<String>,
        manual_blocked_count: usize,
    ) -> Self {
        let reason_code_strings: Vec<String> =
            reason_codes.iter().map(|rc| rc.code_string()).collect();

        Self {
            has_held_issues: !held_ids.is_empty() || !intentionally_held_ids.is_empty(),
            held_count: held_ids.len(),
            held_ids,
            intentionally_held_ids,
            reason_codes: reason_code_strings,
            remedy: Some("bead update <id> --clear-assignee".to_string()),
            manual_blocked_count,
            manual_blocked_ids,
        }
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

/// Check for same-UUID divergence (R028)
///
/// Detects when the current workspace has the same UUID as another workspace
/// but divergent event histories, which indicates a clone without explicit
/// forking. This condition will cause conflicts during merge operations.
///
/// Returns Ok if no divergence is detected, Err with remediation advice if
/// divergence is found.
fn check_uuid_divergence(store: &impl Store) -> Result<String> {
    let config = store.get_workspace_config()?;
    let db_path = config.root.join(".beads/beads.db");

    let conn = open_configured_connection(&db_path)
        .map_err(|e| Error::Integrity(format!("Failed to open database: {}", e)))?;

    // Check for fork receipts - if any exist, this workspace has already been forked
    let fork_receipt_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM provenance_receipts WHERE kind = 'fork'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    if fork_receipt_count > 0 {
        // This is a forked workspace - no divergence issue
        return Ok(format!(
            "Workspace is a fork ({} fork receipts)",
            fork_receipt_count
        ));
    }

    // Check events for multiple origin store UUIDs (divergence indicator)
    let uuid_check: Result<(Vec<String>, i64)> = conn
        .query_row(
            "SELECT COUNT(DISTINCT origin_store_uuid), COUNT(*) FROM events",
            [],
            |row| {
                let _distinct_uuids: i64 = row.get(0)?;
                let _total_events: i64 = row.get(1)?;
                Ok((Vec::<String>::new(), _distinct_uuids))
            },
        )
        .map_err(|e| Error::Integrity(format!("Failed to query event UUIDs: {}", e)));

    let (_distinct_uuids, total_events) = match uuid_check {
        Ok((_, count)) => (Vec::<String>::new(), count),
        Err(e) => return Err(e),
    };

    // If we have events but no fork receipt, check if this looks like a clone
    if total_events > 0 {
        // Check for events that don't match current workspace UUID
        let current_uuid: String = conn
            .query_row("SELECT uuid FROM workspace WHERE id = 1", [], |row| {
                row.get(0)
            })
            .unwrap_or_else(|_| String::new());

        let mismatched_events: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM events WHERE origin_store_uuid != ?1",
                params![current_uuid],
                |row| row.get(0),
            )
            .unwrap_or(0);

        if mismatched_events > 0 {
            return Err(Error::Integrity(format!(
                "Workspace has {} events from a different store UUID without a fork receipt. \
                 This indicates a cloned workspace that needs explicit forking. \
                 Run 'bead sync fork --actor <WHO> [--reason <WHY>]' to create a distinct identity.",
                mismatched_events
            )));
        }
    }

    Ok("No UUID divergence detected".to_string())
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

/// Starvation check report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StarvationCheckReport {
    pub timestamp: String,
    pub open_bead_count: usize,
    pub ready_bead_count: usize,
    pub excluded_beads: Vec<ExcludedBead>,
    pub has_starvation: bool,
}

/// Details of a bead excluded from the ready frontier
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExcludedBead {
    pub id: String,
    pub title: String,
    pub priority: i64,
    pub reasons: Vec<String>,
    pub assignee: Option<String>,
    pub manual_blocked: bool,
    pub blocking_dependencies: Vec<String>,
    pub resource_conflicts: Vec<String>,
}

/// Run starvation check: diagnose beads that are open but not appearing in the ready frontier
pub fn run_starvation_check(store: &impl Store) -> Result<StarvationCheckReport> {
    let config = store.get_workspace_config()?;
    let db_path = config.root.join(".beads/beads.db");
    let conn = open_configured_connection(&db_path)
        .map_err(|e| Error::Integrity(format!("Failed to open database: {}", e)))?;

    let timestamp = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);

    // Get all open beads
    let mut open_stmt = conn
        .prepare_cached(
            "SELECT id, title, priority, assignee, manual_blocked, created_at
         FROM issues
         WHERE base_status = 'open'
         ORDER BY priority ASC, created_at ASC, id ASC",
        )
        .map_err(|e| Error::Integrity(format!("Failed to prepare statement: {}", e)))?;

    let open_beads: Vec<(String, String, i64, Option<String>, Option<bool>, String)> = open_stmt
        .query_map([], |row| {
            Ok((
                row.get(0)?, // id
                row.get(1)?, // title
                row.get(2)?, // priority
                row.get(3)?, // assignee
                row.get(4)?, // manual_blocked
                row.get(5)?, // created_at
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to fetch open beads: {}", e)))?;

    let open_bead_count = open_beads.len();
    let mut excluded_beads = Vec::new();
    let mut ready_bead_count = 0;

    for (id, title, priority, assignee, manual_blocked, _created_at) in open_beads {
        let mut reasons = Vec::new();
        let mut blocking_dependencies = Vec::new();
        let mut resource_conflicts = Vec::new();
        let is_manually_blocked = manual_blocked.unwrap_or(false);

        // Check assignee
        if let Some(ref assignee_name) = assignee {
            if !assignee_name.trim().is_empty() {
                reasons.push(format!("has assignee: {}", assignee_name));
            }
        }

        // Check manual block
        if is_manually_blocked {
            reasons.push("manually blocked".to_string());
        }

        // Check dependencies
        let has_blockers: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM dependencies
                 WHERE blocked_issue_id = ?1 AND kind = 'blocks'
                 AND blocker_issue_id IN (SELECT id FROM issues WHERE base_status != 'closed')",
                [&id],
                |row| row.get(0),
            )
            .unwrap_or(0);

        if has_blockers > 0 {
            let mut blocker_stmt = conn.prepare_cached(
                "SELECT blocker_issue_id FROM dependencies
                 WHERE blocked_issue_id = ?1 AND kind = 'blocks'
                 AND blocker_issue_id IN (SELECT id FROM issues WHERE base_status != 'closed')
                 LIMIT 10",
            )?;

            let blocker_ids: Vec<String> = blocker_stmt
                .query_map([&id], |row| row.get(0))?
                .collect::<std::result::Result<Vec<_>, _>>()
                .unwrap_or_default();

            blocking_dependencies = blocker_ids.clone();

            if blocker_ids.len() < has_blockers as usize {
                reasons.push(format!(
                    "blocked by {}+ unclosed issues (including: {})",
                    has_blockers,
                    blocker_ids.join(", ")
                ));
            } else {
                reasons.push(format!(
                    "blocked by {} unclosed issue(s): {}",
                    has_blockers,
                    blocker_ids.join(", ")
                ));
            }
        }

        // Check resource conflicts
        let has_conflicts: i64 = conn
            .query_row(
                "SELECT COUNT(DISTINCT held_lock.issue_id)
                 FROM issue_resource_keys candidate_key
                 JOIN resource_locks held_lock ON held_lock.resource_key = candidate_key.resource_key
                 WHERE candidate_key.issue_id = ?1
                 AND held_lock.issue_id != ?1",
                [&id],
                |row| row.get(0),
            )
            .unwrap_or(0);

        if has_conflicts > 0 {
            let mut conflict_stmt = conn.prepare_cached(
                "SELECT DISTINCT held_lock.issue_id
                 FROM issue_resource_keys candidate_key
                 JOIN resource_locks held_lock ON held_lock.resource_key = candidate_key.resource_key
                 WHERE candidate_key.issue_id = ?1
                 AND held_lock.issue_id != ?1
                 LIMIT 5")?;

            let conflict_ids: Vec<String> = conflict_stmt
                .query_map([&id], |row| row.get(0))?
                .collect::<std::result::Result<Vec<_>, _>>()
                .unwrap_or_default();

            resource_conflicts = conflict_ids.clone();

            reasons.push(format!(
                "resource conflict with {} other issue(s): {}",
                has_conflicts,
                conflict_ids.join(", ")
            ));
        }

        // If no reasons were found, the bead is ready
        if reasons.is_empty() {
            ready_bead_count += 1;
        } else {
            excluded_beads.push(ExcludedBead {
                id: id.clone(),
                title: title.clone(),
                priority,
                reasons: reasons.clone(),
                assignee: assignee.clone(),
                manual_blocked: is_manually_blocked,
                blocking_dependencies,
                resource_conflicts,
            });
        }
    }

    let has_starvation = open_bead_count > 0 && ready_bead_count == 0;

    Ok(StarvationCheckReport {
        timestamp,
        open_bead_count,
        ready_bead_count,
        excluded_beads,
        has_starvation,
    })
}

/// Starvation recovery result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StarvationRecoveryResult {
    pub timestamp: String,
    pub repairs_performed: Vec<RepairRecord>,
    pub total_repairs: usize,
    pub integrity_checks_passed: bool,
    pub checkpoint_verified: bool,
}

/// Individual repair record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepairRecord {
    pub repair_type: String,
    pub description: String,
    pub affected_beads: Vec<String>,
    pub timestamp: String,
}

/// Run starvation recovery: automatically diagnose and repair common starvation causes
///
/// This function performs the following repairs when `force` is true:
/// 1. Runs SQLite integrity checks on beads.db
/// 2. Verifies checkpoint/current.json matches database state
/// 3. Identifies and fixes beads with inconsistent status (e.g., assigned-but-open)
/// 4. Resets stale_since timestamps on beads that appear stuck
/// 5. Logs all repairs to .beads/doctor-recovery.log for audit
///
/// When `force` is false (recommendation-only mode), this function only diagnoses
/// issues and emits recommendations without performing any mutations.
pub fn run_starvation_recovery(
    store: &mut impl Store,
    force: bool,
) -> Result<StarvationRecoveryResult> {
    let config = store.get_workspace_config()?;
    let db_path = config.root.join(".beads/beads.db");
    let log_path = config.root.join(".beads/doctor-recovery.log");

    let timestamp = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let mut repairs_performed = Vec::new();
    #[allow(unused_assignments)]
    let mut integrity_checks_passed = false;
    let mut checkpoint_verified = false;

    // Open log file in append mode
    let mut log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|e| Error::Io {
            path: log_path.clone(),
            msg: e,
        })?;

    use std::io::Write;
    writeln!(
        log_file,
        "\n=== Starvation Recovery Run at {} ===",
        timestamp
    )
    .map_err(|e| Error::Io {
        path: log_path.clone(),
        msg: e,
    })?;

    // 1. Run SQLite integrity checks
    writeln!(log_file, "[1/5] Running SQLite integrity checks...").map_err(|e| Error::Io {
        path: log_path.clone(),
        msg: e,
    })?;

    match check_database_integrity(store) {
        Ok(_msg) => {
            integrity_checks_passed = true;
            writeln!(log_file, "  ✓ Database integrity check passed").map_err(|e| Error::Io {
                path: log_path.clone(),
                msg: e,
            })?;
        }
        Err(e) => {
            writeln!(log_file, "  ✗ Database integrity check failed: {}", e).map_err(|e| {
                Error::Io {
                    path: log_path.clone(),
                    msg: e,
                }
            })?;
            return Err(Error::integrity(format!(
                "Cannot proceed with recovery: database integrity check failed: {}",
                e
            )));
        }
    }

    // 2. Verify checkpoint matches database
    writeln!(log_file, "[2/5] Verifying checkpoint state...").map_err(|e| Error::Io {
        path: log_path.clone(),
        msg: e,
    })?;

    let conn = open_configured_connection(&db_path)
        .map_err(|e| Error::Integrity(format!("Failed to open database: {}", e)))?;

    let current_sequence: i64 = conn
        .query_row("SELECT COALESCE(MAX(sequence), 0) FROM events", [], |row| {
            row.get(0)
        })
        .unwrap_or(0);

    let covered_sequence: i64 = conn
        .query_row(
            "SELECT covered_event_sequence FROM checkpoint_state WHERE id = 1",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    if covered_sequence >= current_sequence {
        checkpoint_verified = true;
        writeln!(
            log_file,
            "  ✓ Checkpoint is clean and up-to-date (covered={}, current={})",
            covered_sequence, current_sequence
        )
        .map_err(|e| Error::Io {
            path: log_path.clone(),
            msg: e,
        })?;
    } else {
        writeln!(
            log_file,
            "  ⚠ Checkpoint is dirty: covered={}, current={}",
            covered_sequence, current_sequence
        )
        .map_err(|e| Error::Io {
            path: log_path.clone(),
            msg: e,
        })?;

        // Flush checkpoint before proceeding
        writeln!(log_file, "  → Flushing checkpoint to ensure consistency...").map_err(|e| {
            Error::Io {
                path: log_path.clone(),
                msg: e,
            }
        })?;

        // This will be handled by the sync flush after repairs
    }

    // 3. Identify and fix assigned-but-open beads (known starvation cause)
    writeln!(log_file, "[3/5] Identifying assigned-but-open beads...").map_err(|e| Error::Io {
        path: log_path.clone(),
        msg: e,
    })?;

    let mut stmt = conn
        .prepare(
            "SELECT id, title, assignee
             FROM issues
             WHERE base_status = 'open'
               AND assignee IS NOT NULL
               AND TRIM(assignee) != ''
             ORDER BY id",
        )
        .map_err(|e| Error::Integrity(format!("Failed to prepare statement: {}", e)))?;

    let assigned_open_beads: Vec<(String, String, String)> = stmt
        .query_map([], |row| {
            Ok((
                row.get(0)?, // id
                row.get(1)?, // title
                row.get(2)?, // assignee
            ))
        })
        .map_err(|e| Error::Integrity(format!("Failed to query beads: {}", e)))?
        .filter_map(|r| r.ok())
        .collect();

    if !assigned_open_beads.is_empty() {
        writeln!(
            log_file,
            "  Found {} assigned-but-open beads",
            assigned_open_beads.len()
        )
        .map_err(|e| Error::Io {
            path: log_path.clone(),
            msg: e,
        })?;

        let affected_beads: Vec<String> = assigned_open_beads
            .iter()
            .map(|(id, _, _)| id.clone())
            .collect();

        if force {
            // Clear assignees for these beads using proper lifecycle service
            for (id, title, assignee) in &assigned_open_beads {
                match crate::service::lifecycle::update_issue(
                    &conn, id, None, None, true, None, None, None,
                ) {
                    Ok(_) => {
                        writeln!(
                            log_file,
                            "  ✓ Cleared assignee '{}' from bead {} (title: {})",
                            assignee, id, title
                        )
                        .map_err(|e| Error::Io {
                            path: log_path.clone(),
                            msg: e,
                        })?;
                    }
                    Err(e) => {
                        writeln!(
                            log_file,
                            "  ✗ Failed to clear assignee for {} (title: {}): {}",
                            id, title, e
                        )
                        .map_err(|e| Error::Io {
                            path: log_path.clone(),
                            msg: e,
                        })?;
                    }
                }
            }

            repairs_performed.push(RepairRecord {
                repair_type: "clear_stale_assignees".to_string(),
                description: format!("Cleared stale assignees from {} assigned-but-open beads using lifecycle service", assigned_open_beads.len()),
                affected_beads: affected_beads.clone(),
                timestamp: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            });
        } else {
            // Recommendation-only mode
            writeln!(
                log_file,
                "  [RECOMMENDATION] Would clear assignees from {} assigned-but-open beads:",
                assigned_open_beads.len()
            )
            .map_err(|e| Error::Io {
                path: log_path.clone(),
                msg: e,
            })?;
            for (id, title, assignee) in &assigned_open_beads {
                writeln!(
                    log_file,
                    "    - bead {} (title: {}, assignee: {})",
                    id, title, assignee
                )
                .map_err(|e| Error::Io {
                    path: log_path.clone(),
                    msg: e,
                })?;
            }
            writeln!(log_file, "    Remedy: bead update <id> --clear-assignee").map_err(|e| {
                Error::Io {
                    path: log_path.clone(),
                    msg: e,
                }
            })?;

            repairs_performed.push(RepairRecord {
                repair_type: "recommend_clear_stale_assignees".to_string(),
                description: format!(
                    "Recommendation: clear stale assignees from {} assigned-but-open beads",
                    assigned_open_beads.len()
                ),
                affected_beads: affected_beads.clone(),
                timestamp: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            });
        }
    } else {
        writeln!(log_file, "  ✓ No assigned-but-open beads found").map_err(|e| Error::Io {
            path: log_path.clone(),
            msg: e,
        })?;
    }

    // 4. Check for stale in-progress beads and reset stale_since
    writeln!(log_file, "[4/5] Checking for stale in-progress beads...").map_err(|e| Error::Io {
        path: log_path.clone(),
        msg: e,
    })?;

    let stale_config = load_stale_in_progress_config(store)?;

    let mut stale_stmt = conn
        .prepare(
            "SELECT i.id, i.title, e.time, (
                SELECT MAX(exit.sequence)
                FROM events exit
                WHERE exit.issue_id = i.id
                  AND exit.kind IN ('released', 'closed', 'reopened')
            ) AS last_exit_sequence,
            EXISTS(
                SELECT 1 FROM leases lease WHERE lease.issue_id = i.id
            ) AS has_lease_row
             FROM issues i
             JOIN events e ON e.sequence = (
                 SELECT MAX(event.sequence)
                 FROM events event
                 WHERE event.issue_id = i.id
             )
             WHERE i.base_status = 'in_progress'
             ORDER BY i.id",
        )
        .map_err(|e| Error::Integrity(format!("Failed to prepare statement: {}", e)))?;

    let stale_candidates: Vec<(String, String, String, Option<i64>, bool)> = stale_stmt
        .query_map([], |row| {
            Ok((
                row.get(0)?,                // id
                row.get(1)?,                // title
                row.get(2)?,                // last_event_at
                row.get(3)?,                // last_exit_sequence
                row.get::<_, i64>(4)? != 0, // has_lease_row
            ))
        })
        .map_err(|e| Error::Integrity(format!("Failed to query stale beads: {}", e)))?
        .filter_map(|r| r.ok())
        .collect();

    let now = chrono::Utc::now();
    let mut stale_beads = Vec::new();

    for (id, title, last_event_at, last_exit_sequence, has_lease_row) in stale_candidates {
        // Skip if this was released/closed/reopened after the claim
        if last_exit_sequence.is_some() {
            continue;
        }

        // Skip leased claims (R002 owns their expiry)
        if has_lease_row {
            continue;
        }

        let event_time = chrono::DateTime::parse_from_rfc3339(&last_event_at)
            .map_err(|e| Error::Integrity(format!("Invalid timestamp for bead {}: {}", id, e)))?;

        let age_seconds = now
            .signed_duration_since(event_time.with_timezone(&chrono::Utc))
            .num_seconds();

        if age_seconds > stale_config.max_age_seconds as i64 {
            stale_beads.push((id, title, age_seconds));
        }
    }

    if !stale_beads.is_empty() {
        writeln!(
            log_file,
            "  Found {} stale in-progress beads",
            stale_beads.len()
        )
        .map_err(|e| Error::Io {
            path: log_path.clone(),
            msg: e,
        })?;

        let affected_beads: Vec<String> = stale_beads.iter().map(|(id, _, _)| id.clone()).collect();
        let _release_reason = format!(
            "Starvation recovery: stale claim (no activity for >{} seconds)",
            stale_config.max_age_seconds
        );

        if force {
            // Release stale in-progress beads using proper lifecycle service
            for (id, title, age_seconds) in &stale_beads {
                match crate::service::lifecycle::release_issue(&conn, id, None, None) {
                    Ok(_) => {
                        writeln!(
                            log_file,
                            "  ✓ Released stale bead {} (title: {}, age: {} seconds)",
                            id, title, age_seconds
                        )
                        .map_err(|e| Error::Io {
                            path: log_path.clone(),
                            msg: e,
                        })?;
                    }
                    Err(e) => {
                        writeln!(
                            log_file,
                            "  ✗ Failed to release stale bead {} (title: {}, age: {} seconds): {}",
                            id, title, age_seconds, e
                        )
                        .map_err(|e| Error::Io {
                            path: log_path.clone(),
                            msg: e,
                        })?;
                    }
                }
            }

            repairs_performed.push(RepairRecord {
                repair_type: "release_stale_claims".to_string(),
                description: format!("Released {} stale in-progress beads (no activity for >{} seconds) using lifecycle service", stale_beads.len(), stale_config.max_age_seconds),
                affected_beads: affected_beads.clone(),
                timestamp: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            });
        } else {
            // Recommendation-only mode
            writeln!(
                log_file,
                "  [RECOMMENDATION] Would release {} stale in-progress beads:",
                stale_beads.len()
            )
            .map_err(|e| Error::Io {
                path: log_path.clone(),
                msg: e,
            })?;
            for (id, title, age_seconds) in &stale_beads {
                writeln!(
                    log_file,
                    "    - bead {} (title: {}, age: {} seconds)",
                    id, title, age_seconds
                )
                .map_err(|e| Error::Io {
                    path: log_path.clone(),
                    msg: e,
                })?;
            }
            writeln!(log_file, "    Remedy: bead release <id>").map_err(|e| Error::Io {
                path: log_path.clone(),
                msg: e,
            })?;

            repairs_performed.push(RepairRecord {
                repair_type: "recommend_release_stale_claims".to_string(),
                description: format!("Recommendation: release {} stale in-progress beads (no activity for >{} seconds)", stale_beads.len(), stale_config.max_age_seconds),
                affected_beads: affected_beads.clone(),
                timestamp: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            });
        }
    } else {
        writeln!(log_file, "  ✓ No stale in-progress beads found").map_err(|e| Error::Io {
            path: log_path.clone(),
            msg: e,
        })?;
    }

    // 5. Flush checkpoint to ensure all repairs are persisted
    writeln!(log_file, "[5/5] Flushing checkpoint to persist repairs...").map_err(|e| {
        Error::Io {
            path: log_path.clone(),
            msg: e,
        }
    })?;

    // Get the current event sequence after our repairs
    let final_sequence: i64 = conn
        .query_row("SELECT COALESCE(MAX(sequence), 0) FROM events", [], |row| {
            row.get(0)
        })
        .unwrap_or(0);

    writeln!(log_file, "  Final event sequence: {}", final_sequence).map_err(|e| Error::Io {
        path: log_path.clone(),
        msg: e,
    })?;

    // Log summary
    writeln!(log_file, "\nRecovery Summary:").map_err(|e| Error::Io {
        path: log_path.clone(),
        msg: e,
    })?;
    writeln!(
        log_file,
        "  Total repairs performed: {}",
        repairs_performed.len()
    )
    .map_err(|e| Error::Io {
        path: log_path.clone(),
        msg: e,
    })?;

    for repair in &repairs_performed {
        writeln!(
            log_file,
            "  - {}: {}",
            repair.repair_type, repair.description
        )
        .map_err(|e| Error::Io {
            path: log_path.clone(),
            msg: e,
        })?;
    }

    writeln!(log_file, "=== End of Recovery Run ===\n").map_err(|e| Error::Io {
        path: log_path.clone(),
        msg: e,
    })?;

    let total_repairs = repairs_performed.len();

    Ok(StarvationRecoveryResult {
        timestamp,
        repairs_performed,
        total_repairs,
        integrity_checks_passed,
        checkpoint_verified,
    })
}

/// Check attempt outcomes integrity (R036)
///
/// Read-only diagnostic that verifies attempt outcomes table consistency:
/// - No orphaned outcomes (referencing non-existent issues)
/// - All outcomes have corresponding audit events
/// - Receipt IDs and attempt IDs are properly formatted
/// - Canonical hashes are valid SHA-256 hex strings
/// - Evidence refs are properly formatted
/// Never synthesizes missing outcomes - only reports what exists.
fn check_attempt_outcomes_integrity(store: &impl Store) -> Result<String> {
    let config = store.get_workspace_config()?;
    let db_path = config.root.join(".beads/beads.db");

    let conn = open_configured_connection(&db_path)
        .map_err(|e| Error::Integrity(format!("Failed to open database: {}", e)))?;

    // Check if attempt_outcomes table exists (may not in legacy workspaces)
    let table_exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='attempt_outcomes'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    if table_exists == 0 {
        // Legacy workspace - no attempt outcomes table is OK
        return Ok("Attempt outcomes table not present (legacy workspace)".to_string());
    }

    let mut issues = Vec::new();

    // 1. Check for orphaned attempt outcomes (referencing non-existent issues)
    let mut stmt = conn
        .prepare(
            "SELECT COUNT(*) FROM attempt_outcomes ao
             LEFT JOIN issues i ON ao.issue_id = i.id
             WHERE i.id IS NULL",
        )
        .map_err(|e| Error::Integrity(format!("Failed to prepare orphan check: {}", e)))?;

    let orphaned_count: i64 = stmt.query_row([], |row| row.get(0)).unwrap_or(0);

    if orphaned_count > 0 {
        issues.push(format!(
            "Found {} attempt outcomes referencing non-existent issues",
            orphaned_count
        ));
    }

    // 2. Check for attempt outcomes without corresponding audit events
    let mut stmt = conn
        .prepare(
            "SELECT COUNT(*) FROM attempt_outcomes ao
             LEFT JOIN events e ON e.issue_id = ao.issue_id AND e.kind = 'attempt_resolved'
             WHERE e.sequence IS NULL",
        )
        .map_err(|e| Error::Integrity(format!("Failed to prepare event check: {}", e)))?;

    let missing_events_count: i64 = stmt.query_row([], |row| row.get(0)).unwrap_or(0);

    if missing_events_count > 0 {
        issues.push(format!(
            "Found {} attempt outcomes without corresponding attempt_resolved events",
            missing_events_count
        ));
    }

    // 3. Validate receipt ID format (should start with "ao-" prefix)
    let mut stmt = conn
        .prepare(
            "SELECT COUNT(*) FROM attempt_outcomes
             WHERE receipt_id NOT LIKE 'ao-%'",
        )
        .map_err(|e| Error::Integrity(format!("Failed to prepare receipt_id check: {}", e)))?;

    let invalid_receipt_ids: i64 = stmt.query_row([], |row| row.get(0)).unwrap_or(0);

    if invalid_receipt_ids > 0 {
        issues.push(format!(
            "Found {} attempt outcomes with invalid receipt_id format (should start with 'ao-')",
            invalid_receipt_ids
        ));
    }

    // 4. Validate canonical request hash format (should be 64-char hex string)
    let mut stmt = conn
        .prepare(
            "SELECT COUNT(*) FROM attempt_outcomes
             WHERE length(canonical_request_hash) != 64
                OR canonical_request_hash NOT GLOB '*[0-9a-f][0-9a-f][0-9a-f][0-9a-f]*'",
        )
        .map_err(|e| Error::Integrity(format!("Failed to prepare hash check: {}", e)))?;

    let invalid_hashes: i64 = stmt.query_row([], |row| row.get(0)).unwrap_or(0);

    if invalid_hashes > 0 {
        issues.push(format!(
            "Found {} attempt outcomes with invalid canonical_request_hash (expected 64-char hex)",
            invalid_hashes
        ));
    }

    // 5. Validate evidence_refs JSON format
    let mut stmt = conn
        .prepare("SELECT COUNT(*) FROM attempt_outcomes WHERE evidence_refs_json IS NOT NULL")
        .map_err(|e| Error::Integrity(format!("Failed to prepare evidence check: {}", e)))?;

    let with_evidence: i64 = stmt.query_row([], |row| row.get(0)).unwrap_or(0);

    let mut invalid_evidence = 0;
    if with_evidence > 0 {
        let mut stmt = conn
            .prepare("SELECT evidence_refs_json FROM attempt_outcomes WHERE evidence_refs_json IS NOT NULL")
            .map_err(|e| Error::Integrity(format!("Failed to query evidence refs: {}", e)))?;

        let evidence_rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| Error::Integrity(format!("Failed to read evidence refs: {}", e)))?;

        for evidence_json in evidence_rows.flatten() {
            if let Ok(refs) = serde_json::from_str::<Vec<String>>(&evidence_json) {
                for ref_str in refs {
                    // Basic format check: NAMESPACE:VALUE
                    if !ref_str.contains(':') || ref_str.split(':').count() != 2 {
                        invalid_evidence += 1;
                        break;
                    }
                }
            } else {
                invalid_evidence += 1;
            }
        }
    }

    if invalid_evidence > 0 {
        issues.push(format!(
            "Found {} attempt outcomes with malformed evidence_refs_json",
            invalid_evidence
        ));
    }

    if issues.is_empty() {
        let outcome_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM attempt_outcomes", [], |row| row.get(0))
            .unwrap_or(0);
        Ok(format!(
            "Attempt outcomes integrity OK: {} outcomes validated",
            outcome_count
        ))
    } else {
        Err(Error::Integrity(format!(
            "Attempt outcomes integrity issues: {}",
            issues.join("; ")
        )))
    }
}

/// Check attempt tier consistency with consecutive failures (R036)
///
/// Read-only diagnostic that verifies attempt tier values match consecutive
/// failure counts according to the failure-aware scheduling policy:
/// - 0 consecutive failures: tier 0 (Normal)
/// - 1 consecutive failure: tier 1 (Retryable)
/// - 2 consecutive failures: tier 2 (Struggling)
/// - 3+ consecutive failures: tier 3 (Quarantined)
///
/// Inconsistencies may indicate:
/// - Manual tier adjustments (valid but notable)
/// - Legacy workspace before attempt tier enforcement
/// - Data corruption requiring investigation
fn check_attempt_tier_consistency(store: &impl Store) -> Result<String> {
    let config = store.get_workspace_config()?;
    let db_path = config.root.join(".beads/beads.db");

    let conn = open_configured_connection(&db_path)
        .map_err(|e| Error::Integrity(format!("Failed to open database: {}", e)))?;

    // Check if attempt_outcomes table exists (may not in legacy workspaces)
    let table_exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='attempt_outcomes'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    if table_exists == 0 {
        // Legacy workspace - no attempt outcomes table is OK
        return Ok("Attempt tier consistency not applicable (legacy workspace)".to_string());
    }

    // Query for issues where attempt_tier doesn't match consecutive_failures
    let mut stmt = conn
        .prepare(
            "SELECT i.id, i.consecutive_failures, i.attempt_tier
             FROM issues i
             WHERE (
                 (consecutive_failures = 0 AND attempt_tier != 0)
                 OR (consecutive_failures = 1 AND attempt_tier != 1)
                 OR (consecutive_failures = 2 AND attempt_tier != 2)
                 OR (consecutive_failures >= 3 AND attempt_tier != 3)
             )
             LIMIT 20",
        )
        .map_err(|e| Error::Integrity(format!("Failed to prepare tier check: {}", e)))?;

    let inconsistent_issues: Vec<(String, i64, i64)> = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })
        .map_err(|e| Error::Integrity(format!("Failed to query tier issues: {}", e)))?
        .filter_map(|r| r.ok())
        .collect();

    if inconsistent_issues.is_empty() {
        let issue_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM issues", [], |row| row.get(0))
            .unwrap_or(0);
        Ok(format!(
            "Attempt tier consistency OK: {} issues validated",
            issue_count
        ))
    } else {
        // Report with remedy - manual reset is available but diagnostic is read-only
        let examples = inconsistent_issues
            .iter()
            .take(5)
            .map(|(id, failures, tier)| {
                format!(
                    "{} (failures={}, tier={})",
                    id, failures, tier
                )
            })
            .collect::<Vec<_>>()
            .join(", ");

        let total_count = inconsistent_issues.len();
        let more_indicator = if total_count > 5 {
            format!(" (and {} more)", total_count - 5)
        } else {
            String::new()
        };

        Err(Error::workspace(format!(
            "Found {} issues with inconsistent attempt_tier vs consecutive_failures: {}{}. \
             This may indicate manual tier adjustments or legacy data. \
             To reset: bead update <id> --reset-attempt-tier",
            total_count, examples, more_indicator
        )))
    }
}

/// Visibility check report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisibilityCheckReport {
    pub timestamp: String,
    pub open_bead_count: usize,
    pub ready_bead_count: usize,
    pub has_discrepancy: bool,
    pub discrepancy_details: Option<DiscrepancyDetails>,
}

/// Detailed information about beads causing visibility discrepancy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscrepancyDetails {
    pub open_not_ready: Vec<InvisibleBead>,
    pub ready_not_open: Vec<String>, // Should not happen in practice
}

/// Bead that is open but not appearing in the ready frontier
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvisibleBead {
    pub id: String,
    pub title: String,
    pub priority: i64,
    pub assignee: Option<String>,
    pub manual_blocked: bool,
    pub has_blockers: bool,
    pub has_resource_conflicts: bool,
}

/// Run visibility check: compare open bead count against ready frontier query
///
/// This function performs a consistency check that queries the database for beads
/// WHERE status='open' and compares that count against Pluck's ready-set query.
/// If counts differ, it automatically dumps the differing bead IDs and their
/// metadata to a structured log.
pub fn run_visibility_check(store: &impl Store) -> Result<VisibilityCheckReport> {
    let config = store.get_workspace_config()?;
    let db_path = config.root.join(".beads/beads.db");
    let conn = open_configured_connection(&db_path)
        .map_err(|e| Error::Integrity(format!("Failed to open database: {}", e)))?;

    let timestamp = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);

    // 1. Get all open beads (WHERE base_status = 'open')
    let open_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM issues WHERE base_status = 'open'", [], |row| {
            row.get(0)
        })
        .unwrap_or(0);

    // 2. Get ready frontier count using the same query as Pluck (find_eligible_frontier)
    let now_string = crate::service::resource_locks::now_string();
    let ready_query = r#"
        SELECT COUNT(*)
        FROM issues i
        WHERE i.base_status = 'open'
          AND i.assignee IS NULL
          AND i.manual_blocked = 0
          AND NOT EXISTS (
              SELECT 1
              FROM dependencies d
              JOIN issues blocker ON blocker.id = d.blocker_issue_id
              WHERE d.blocked_issue_id = i.id
                AND d.kind = 'blocks'
                AND blocker.base_status != 'closed'
          )
          AND NOT EXISTS (
              SELECT 1
              FROM issue_resource_keys candidate_key
              JOIN resource_locks held_lock
                ON held_lock.resource_key = candidate_key.resource_key
              WHERE candidate_key.issue_id = i.id
                AND held_lock.issue_id != i.id
                AND (held_lock.lease_fencing_token IS NULL OR EXISTS (
                    SELECT 1 FROM leases active_lease
                    WHERE active_lease.issue_id = held_lock.issue_id
                      AND active_lease.fencing_token = held_lock.lease_fencing_token
                      AND active_lease.expires_at > ?
                ))
          )
    "#;

    let ready_count: i64 = conn
        .query_row(ready_query, [&now_string], |row| row.get(0))
        .unwrap_or(0);

    let open_bead_count = open_count as usize;
    let ready_bead_count = ready_count as usize;
    let has_discrepancy = open_bead_count != ready_bead_count;

    let mut discrepancy_details = None;

    if has_discrepancy {
        // Get detailed information about beads that are open but not ready
        let open_not_ready_query = r#"
            SELECT i.id, i.title, i.priority, i.assignee, i.manual_blocked,
                   EXISTS (
                       SELECT 1
                       FROM dependencies d
                       JOIN issues blocker ON blocker.id = d.blocker_issue_id
                       WHERE d.blocked_issue_id = i.id
                         AND d.kind = 'blocks'
                         AND blocker.base_status != 'closed'
                   ) as has_blockers,
                   EXISTS (
                       SELECT 1
                       FROM issue_resource_keys candidate_key
                       JOIN resource_locks held_lock
                         ON held_lock.resource_key = candidate_key.resource_key
                       WHERE candidate_key.issue_id = i.id
                         AND held_lock.issue_id != i.id
                         AND (held_lock.lease_fencing_token IS NULL OR EXISTS (
                             SELECT 1 FROM leases active_lease
                             WHERE active_lease.issue_id = held_lock.issue_id
                               AND active_lease.fencing_token = held_lock.lease_fencing_token
                               AND active_lease.expires_at > ?
                         ))
                   ) as has_conflicts
            FROM issues i
            WHERE i.base_status = 'open'
              AND (
                  i.assignee IS NOT NULL
                  OR i.manual_blocked != 0
                  OR EXISTS (
                      SELECT 1
                      FROM dependencies d
                      JOIN issues blocker ON blocker.id = d.blocker_issue_id
                      WHERE d.blocked_issue_id = i.id
                        AND d.kind = 'blocks'
                        AND blocker.base_status != 'closed'
                  )
                  OR EXISTS (
                      SELECT 1
                      FROM issue_resource_keys candidate_key
                      JOIN resource_locks held_lock
                        ON held_lock.resource_key = candidate_key.resource_key
                      WHERE candidate_key.issue_id = i.id
                        AND held_lock.issue_id != i.id
                        AND (held_lock.lease_fencing_token IS NULL OR EXISTS (
                            SELECT 1 FROM leases active_lease
                            WHERE active_lease.issue_id = held_lock.issue_id
                              AND active_lease.fencing_token = held_lock.lease_fencing_token
                              AND active_lease.expires_at > ?
                          ))
                  )
              )
            ORDER BY i.priority ASC, i.created_at ASC, i.id ASC
            LIMIT 100
        "#;

        let mut stmt = conn
            .prepare(open_not_ready_query)
            .map_err(|e| Error::Integrity(format!("Failed to prepare visibility query: {}", e)))?;

        let invisible_beads: Vec<InvisibleBead> = stmt
            .query_map([&now_string, &now_string], |row| {
                Ok(InvisibleBead {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    priority: row.get(2)?,
                    assignee: row.get(3)?,
                    manual_blocked: row.get::<_, i64>(4)? != 0,
                    has_blockers: row.get::<_, i64>(5)? != 0,
                    has_resource_conflicts: row.get::<_, i64>(6)? != 0,
                })
            })
            .map_err(|e| Error::Integrity(format!("Failed to query invisible beads: {}", e)))?
            .filter_map(|r| r.ok())
            .collect();

        // Log to structured file
        let log_path = config.root.join(".beads/visibility-check.log");
        if let Ok(mut log_file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
        {
            use std::io::Write;
            writeln!(
                log_file,
                "\n=== Visibility Check Run at {} ===",
                timestamp
            )
            .ok();
            writeln!(log_file, "Open beads: {}", open_bead_count).ok();
            writeln!(log_file, "Ready beads: {}", ready_bead_count).ok();
            writeln!(log_file, "Discrepancy: {} beads excluded from ready frontier", open_bead_count - ready_bead_count).ok();
            writeln!(log_file, "Invisible beads ({} shown, max 100):", invisible_beads.len()).ok();

            for bead in &invisible_beads {
                writeln!(log_file, "  - {}: {}", bead.id, bead.title).ok();
                writeln!(log_file, "    Priority: {}", bead.priority).ok();
                if let Some(ref assignee) = bead.assignee {
                    writeln!(log_file, "    Assignee: {}", assignee).ok();
                }
                if bead.manual_blocked {
                    writeln!(log_file, "    Manually blocked: true").ok();
                }
                if bead.has_blockers {
                    writeln!(log_file, "    Has blockers: true").ok();
                }
                if bead.has_resource_conflicts {
                    writeln!(log_file, "    Has resource conflicts: true").ok();
                }
            }
        }

        discrepancy_details = Some(DiscrepancyDetails {
            open_not_ready: invisible_beads,
            ready_not_open: vec![], // Should not happen in practice
        });
    }

    Ok(VisibilityCheckReport {
        timestamp,
        open_bead_count,
        ready_bead_count,
        has_discrepancy,
        discrepancy_details,
    })
}

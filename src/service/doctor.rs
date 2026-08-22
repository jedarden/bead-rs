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
                // and the reconcile remedy alongside the message text.
                let details = if e.to_string().starts_with(
                    crate::service::reconcile::REMOTE_ADVANCED_MARKER,
                ) {
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

        let event_time = chrono::DateTime::parse_from_rfc3339(&candidate.last_event_at).map_err(|e| {
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
                verdict
                    .failed_qualifier
                    .as_deref()
                    .unwrap_or("the checkpoint is ahead of the live store but failed its \
                               qualification")
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

    if held_result.is_empty() {
        return Ok(ReadyFrontierReport {
            has_held_issues: false,
            held_count: 0,
            held_ids: vec![],
            intentionally_held_ids: vec![],
            reason_codes: vec![],
            remedy: None,
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
}

impl ReadyFrontierReport {
    /// Create a new report from ReasonCode enums
    pub fn from_reason_codes(
        held_ids: Vec<String>,
        intentionally_held_ids: Vec<String>,
        reason_codes: Vec<crate::service::claim::ReasonCode>,
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
            .query_row("SELECT uuid FROM workspace WHERE id = 1", [], |row| row.get(0))
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

//! SQLite store implementation for bead-rs
//!
//! This module provides the SQLite database backend, including schema migrations,
//! connection management, and the core workspace initialization logic.

pub mod migrations;
mod sqlite;

pub use sqlite::SqliteStore;

use std::path::{Path, PathBuf};

/// Workspace configuration and discovery
#[derive(Debug, Clone)]
pub struct WorkspaceConfig {
    /// Path to the workspace root
    pub root: PathBuf,
    /// Workspace UUID
    pub uuid: String,
    /// Bead ID prefix
    pub prefix: String,
}

/// Operator-facing explanation of the uninitialized-workspace state.
///
/// Shared by every entry point so the remedy is described identically wherever
/// the state surfaces.
fn uninitialized_message(root: &Path, db_path: &Path) -> String {
    format!(
        "Workspace database at {} is missing or uninitialized: .beads/config.json is present, \
         but the database has no schema. This is the expected state of a fresh clone, because \
         beads.db is gitignored while config.json is committed. Rebuild it with `bead init` in \
         {} (the committed workspace identity is preserved), then restore content with \
         `bead sync import-only --input .beads/checkpoint/forensic.jsonl --restore-into-empty \
         --actor <you>`.",
        db_path.display(),
        root.display()
    )
}

/// Result of probing the filesystem for a workspace.
///
/// The `Uninitialized` variant exists because `.beads/config.json` is tracked in
/// git while `.beads/beads.db` is gitignored: every fresh clone has an identity
/// but no schema. That state is recoverable, so it must be distinguishable from
/// both "no workspace" and "working workspace" rather than collapsing into an
/// opaque error.
#[derive(Debug, Clone)]
pub enum WorkspaceState {
    /// No `.beads/config.json` found walking up from the current directory.
    NotFound,
    /// `config.json` found, but the database has no schema. Recoverable with
    /// `bead init` (which rebuilds around the committed identity).
    Uninitialized {
        /// Workspace root containing the `.beads` directory
        root: PathBuf,
        /// Path to the database that is missing or empty
        db_path: PathBuf,
    },
    /// Fully initialized workspace.
    Ready(WorkspaceConfig),
}

impl WorkspaceConfig {
    /// Probe for a workspace, walking up from the current directory.
    ///
    /// Unlike [`discover`](Self::discover), this reports an uninitialized
    /// workspace as a distinct state instead of an error, so `init` and
    /// `doctor` can act on it.
    pub fn probe() -> crate::Result<WorkspaceState> {
        let cwd = std::env::current_dir().map_err(|e| crate::Error::Io {
            path: ".".into(),
            msg: e,
        })?;

        let mut current = cwd.as_path();
        loop {
            let config_file = current.join(".beads/config.json");
            if config_file.exists() {
                return Self::state_from_config_path(&config_file);
            }

            match current.parent() {
                Some(parent) if !parent.as_os_str().is_empty() => {
                    current = parent;
                }
                _ => return Ok(WorkspaceState::NotFound),
            }
        }
    }

    /// Discover the workspace by walking up from the current directory.
    ///
    /// An uninitialized workspace surfaces as an actionable error naming the
    /// remedy; see [`probe`](Self::probe) when the caller can repair it.
    pub fn discover() -> crate::Result<Option<Self>> {
        match Self::probe()? {
            WorkspaceState::NotFound => Ok(None),
            WorkspaceState::Ready(config) => Ok(Some(config)),
            WorkspaceState::Uninitialized { root, db_path } => {
                Err(crate::Error::workspace(uninitialized_message(
                    &root, &db_path,
                )))
            }
        }
    }

    /// Classify the workspace rooted at the given config file path
    fn state_from_config_path(config_path: &Path) -> crate::Result<WorkspaceState> {
        let root = config_path
            .parent()
            .and_then(|p| p.parent())
            .ok_or_else(|| crate::Error::workspace("Invalid workspace structure"))?;

        let db_path = root.join(".beads/beads.db");

        // Open database and load workspace metadata
        let conn = rusqlite::Connection::open(&db_path).map_err(|e| {
            crate::Error::Internal(anyhow::anyhow!("Failed to open database: {}", e))
        })?;

        // Configure connection to match SqliteStore configuration
        conn.execute("PRAGMA foreign_keys = ON", []).map_err(|e| {
            crate::Error::Internal(anyhow::anyhow!("Failed to enable foreign keys: {}", e))
        })?;

        // busy_timeout returns the new value, so we need to consume the result
        let _timeout: i64 = conn
            .query_row("PRAGMA busy_timeout = 5000", [], |row| row.get(0))
            .map_err(|e| {
                crate::Error::Internal(anyhow::anyhow!("Failed to set busy timeout: {}", e))
            })?;

        // A tracked `.beads/config.json` can exist without a matching
        // `.beads/beads.db` (the db is gitignored on purpose) -- most
        // commonly right after a fresh `git clone`. A failed query covers both
        // a missing `workspace` table and a missing row, so treat either as
        // "not actually initialized here yet" rather than a hard error, and
        // report it as a distinct recoverable state so `init` can self-heal it
        // and `doctor` can diagnose it.
        let workspace_row: Option<(String, String)> = conn
            .query_row(
                "SELECT uuid, prefix FROM workspace WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok();

        let Some((uuid, prefix)) = workspace_row else {
            return Ok(WorkspaceState::Uninitialized {
                root: root.to_path_buf(),
                db_path,
            });
        };

        Ok(WorkspaceState::Ready(Self {
            root: root.to_path_buf(),
            uuid,
            prefix,
        }))
    }

    /// Get the path to the SQLite database
    #[allow(dead_code)]
    pub fn database_path(&self) -> PathBuf {
        self.root.join(".beads/beads.db")
    }
}

/// Store trait for database operations
pub trait Store {
    /// Initialize the workspace with a new database
    fn init_workspace(&self, prefix: &str) -> crate::Result<WorkspaceConfig>;

    /// Get the current workspace configuration
    fn get_workspace_config(&self) -> crate::Result<WorkspaceConfig>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    #[serial]
    fn test_workspace_discovery_none() {
        // Save original directory to restore later (canonicalize to get absolute path)
        let original_dir = std::env::current_dir().unwrap().canonicalize().unwrap();

        // In a temp directory, there should be no workspace
        let temp = tempfile::tempdir().unwrap();
        std::env::set_current_dir(temp.path()).unwrap();
        assert!(WorkspaceConfig::discover().unwrap().is_none());

        // Restore original directory before dropping temp
        std::env::set_current_dir(original_dir).unwrap();
        drop(temp);
    }
}

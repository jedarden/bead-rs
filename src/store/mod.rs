//! SQLite store implementation for bead-rs
//!
//! This module provides the SQLite database backend, including schema migrations,
//! connection management, and the core workspace initialization logic.

mod migrations;
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

impl WorkspaceConfig {
    /// Discover the workspace by walking up from the current directory
    pub fn discover() -> crate::Result<Option<Self>> {
        let cwd = std::env::current_dir().map_err(|e| crate::Error::Io {
            path: ".".into(),
            msg: e,
        })?;

        let mut current = cwd.as_path();
        loop {
            let config_file = current.join(".beads/config.json");
            if config_file.exists() {
                return Self::from_config_path(&config_file);
            }

            match current.parent() {
                Some(parent) if !parent.as_os_str().is_empty() => {
                    current = parent;
                }
                _ => return Ok(None),
            }
        }
    }

    /// Load workspace configuration from a specific config file path
    fn from_config_path(config_path: &Path) -> crate::Result<Option<Self>> {
        let root = config_path
            .parent()
            .and_then(|p| p.parent())
            .ok_or_else(|| crate::Error::workspace("Invalid workspace structure"))?;

        let db_path = root.join(".beads/beads.db");

        // Open database and load workspace metadata
        let conn = rusqlite::Connection::open(&db_path).map_err(|e| {
            crate::Error::Internal(anyhow::anyhow!("Failed to open database: {}", e))
        })?;

        let uuid: String = conn
            .query_row("SELECT uuid FROM workspace WHERE id = 1", [], |row| {
                row.get(0)
            })
            .map_err(|e| {
                crate::Error::workspace(format!("Failed to load workspace UUID: {}", e))
            })?;

        let prefix: String = conn
            .query_row("SELECT prefix FROM workspace WHERE id = 1", [], |row| {
                row.get(0)
            })
            .map_err(|e| {
                crate::Error::workspace(format!("Failed to load workspace prefix: {}", e))
            })?;

        Ok(Some(Self {
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
pub trait Store: Send + Sync {
    /// Initialize the workspace with a new database
    fn init_workspace(&self, prefix: &str) -> crate::Result<WorkspaceConfig>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workspace_discovery_none() {
        // In a temp directory, there should be no workspace
        let temp = tempfile::tempdir().unwrap();
        std::env::set_current_dir(temp.path()).unwrap();
        assert!(WorkspaceConfig::discover().unwrap().is_none());
    }
}

//! SQLite store implementation

use super::{migrations, Store, WorkspaceConfig};
use crate::error::{Error, Result};
use rusqlite::{Connection, Result as SqliteResult};
use std::path::Path;

/// SQLite store implementation
pub struct SqliteStore {
    // For now, this is a placeholder. The full implementation will include
    // connection management, transaction handling, etc.
}

impl SqliteStore {
    /// Create a new SQLite store
    pub fn new() -> Self {
        Self {}
    }

    /// Open a database connection at the specified path
    fn open_connection(path: &Path) -> SqliteResult<Connection> {
        let conn = Connection::open(path)?;

        // Configure connection
        conn.execute("PRAGMA foreign_keys = ON", [])?;
        // busy_timeout returns the new value, so we need to consume the result
        let _timeout: i64 = conn.query_row("PRAGMA busy_timeout = 5000", [], |row| row.get(0))?;

        // journal_mode returns a result string, so use query_row
        let journal_mode: String =
            conn.query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))?;
        if journal_mode != "wal" && journal_mode != "wal (deleted)" {
            return Err(rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_MISUSE),
                format!("Failed to set journal_mode to WAL, got: {}", journal_mode).into(),
            ));
        }

        conn.execute("PRAGMA synchronous = NORMAL", [])?;

        Ok(conn)
    }

    /// Ensure the workspace directory structure exists
    fn ensure_workspace_structure(root: &Path) -> Result<()> {
        let beads_dir = root.join(".beads");
        std::fs::create_dir_all(&beads_dir).map_err(|e| Error::Io {
            path: beads_dir.clone(),
            msg: e,
        })?;

        // Create checkpoint directory
        let checkpoint_dir = beads_dir.join("checkpoint");
        std::fs::create_dir_all(&checkpoint_dir).map_err(|e| Error::Io {
            path: checkpoint_dir.clone(),
            msg: e,
        })?;

        // Create receipts directory
        let receipts_dir = beads_dir.join("receipts");
        std::fs::create_dir_all(&receipts_dir).map_err(|e| Error::Io {
            path: receipts_dir.clone(),
            msg: e,
        })?;

        Ok(())
    }

    /// Generate a workspace UUID
    fn generate_uuid() -> String {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let bytes: [u8; 16] = rng.gen();
        format!(
            "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            bytes[0], bytes[1], bytes[2], bytes[3],
            bytes[4], bytes[5],
            bytes[6], bytes[7],
            bytes[8], bytes[9],
            bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]
        )
    }

    /// Validate workspace prefix
    fn validate_prefix(prefix: &str) -> Result<()> {
        if prefix.is_empty() {
            return Err(Error::cli_usage("Prefix cannot be empty"));
        }

        if prefix.len() > 32 {
            return Err(Error::cli_usage("Prefix cannot exceed 32 characters"));
        }

        // Must match [a-z][a-z0-9-]*
        if !prefix
            .chars()
            .next()
            .map(|c| c.is_ascii_lowercase())
            .unwrap_or(false)
        {
            return Err(Error::cli_usage(
                "Prefix must start with a lowercase letter",
            ));
        }

        if !prefix
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            return Err(Error::cli_usage(
                "Prefix must contain only lowercase letters, digits, and hyphens",
            ));
        }

        Ok(())
    }

    /// Create a .gitignore file in the .beads directory
    fn create_gitignore(beads_dir: &Path) -> Result<()> {
        let gitignore_path = beads_dir.join(".gitignore");
        let content = r#"# SQLite database files
*.db
*.db-shm
*.db-wal

# Lock files
*.lock

# Temporary files
*.tmp
*.temp

# Journals
*.journal
"#;

        std::fs::write(&gitignore_path, content).map_err(|e| Error::Io {
            path: gitignore_path,
            msg: e,
        })?;

        Ok(())
    }
}

impl Store for SqliteStore {
    fn init_workspace(&self, prefix: &str) -> Result<WorkspaceConfig> {
        // Validate prefix
        Self::validate_prefix(prefix)?;

        // Get current directory as workspace root
        let root = std::env::current_dir().map_err(|e| Error::Io {
            path: ".".into(),
            msg: e,
        })?;

        // Check if workspace already exists
        let config_path = root.join(".beads/config.json");
        if config_path.exists() {
            // Load existing workspace configuration from database
            let db_path = root.join(".beads/beads.db");
            let conn = Self::open_connection(&db_path)
                .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to open database: {}", e)))?;

            let uuid: String = conn
                .query_row("SELECT uuid FROM workspace WHERE id = 1", [], |row| {
                    row.get(0)
                })
                .map_err(|e| Error::workspace(format!("Failed to load workspace UUID: {}", e)))?;

            return Ok(WorkspaceConfig {
                root,
                uuid,
                prefix: prefix.to_string(),
            });
        }

        // Create directory structure
        Self::ensure_workspace_structure(&root)?;

        // Create .gitignore
        let beads_dir = root.join(".beads");
        Self::create_gitignore(&beads_dir)?;

        // Initialize database
        let db_path = root.join(".beads/beads.db");
        let conn = Self::open_connection(&db_path)
            .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to create database: {}", e)))?;

        // Apply migrations
        if let Err(e) = migrations::apply_migrations(&conn) {
            return Err(Error::Internal(anyhow::anyhow!(
                "Failed to apply migrations: {}",
                e
            )));
        }

        // Initialize workspace row if it doesn't exist
        let workspace_exists: i64 = conn
            .query_row("SELECT COUNT(*) FROM workspace WHERE id = 1", [], |row| {
                row.get(0)
            })
            .unwrap_or(0);

        if workspace_exists == 0 {
            let uuid = Self::generate_uuid();
            let prefix_string = prefix.to_string();
            let created_at = time::OffsetDateTime::now_utc()
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_else(|_| "unknown".to_string());

            conn.execute(
                "INSERT INTO workspace (id, uuid, prefix, layout_version, created_at) VALUES (1, ?1, ?2, 1, ?3)",
                [&uuid, &prefix_string, &created_at],
            )?;
        }

        // Initialize checkpoint_state row if it doesn't exist
        let checkpoint_exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM checkpoint_state WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        if checkpoint_exists == 0 {
            conn.execute(
                "INSERT INTO checkpoint_state (id, last_interchange_hash, covered_event_sequence) VALUES (1, '', 0)",
                [],
            )?;
        }

        // Get workspace UUID
        let uuid: String =
            conn.query_row("SELECT uuid FROM workspace WHERE id = 1", [], |row| {
                row.get(0)
            })?;

        // Update workspace with generated UUID and prefix
        conn.execute(
            "UPDATE workspace SET uuid = ?1, prefix = ?2",
            [&uuid, prefix],
        )
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to update workspace: {}", e)))?;

        // Create initial config.json (placeholder for now)
        let config_content = serde_json::json!({
            "version": 1,
            "uuid": uuid,
            "prefix": prefix,
            "created_at": time::OffsetDateTime::now_utc()
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_else(|_| "unknown".to_string())
        });

        std::fs::write(&config_path, config_content.to_string()).map_err(|e| Error::Io {
            path: config_path.clone(),
            msg: e,
        })?;

        Ok(WorkspaceConfig {
            root,
            uuid,
            prefix: prefix.to_string(),
        })
    }
}

impl Default for SqliteStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_prefix_valid() {
        assert!(SqliteStore::validate_prefix("bead").is_ok());
        assert!(SqliteStore::validate_prefix("my-workspace").is_ok());
        assert!(SqliteStore::validate_prefix("abc123").is_ok());
    }

    #[test]
    fn test_validate_prefix_invalid() {
        assert!(SqliteStore::validate_prefix("").is_err());
        assert!(SqliteStore::validate_prefix("Bead").is_err()); // uppercase
        assert!(SqliteStore::validate_prefix("1bead").is_err()); // starts with number
        assert!(SqliteStore::validate_prefix("bead_").is_err()); // underscore
        assert!(SqliteStore::validate_prefix("bead.workspace").is_err()); // dot
    }

    #[test]
    fn test_generate_uuid_format() {
        let uuid = SqliteStore::generate_uuid();
        assert_eq!(uuid.len(), 36); // 8-4-4-4-12 format
        assert!(uuid.chars().all(|c| c.is_ascii_hexdigit() || c == '-'));
    }

    #[test]
    fn test_init_workspace() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();

        std::env::set_current_dir(root).unwrap();

        let store = SqliteStore::new();
        let result = store.init_workspace("test").unwrap();

        assert_eq!(result.prefix, "test");
        assert!(result.root == root);
        assert!(!result.uuid.is_empty());

        // Check that .beads directory was created
        let beads_dir = root.join(".beads");
        assert!(beads_dir.exists());

        // Check checkpoint directory
        let checkpoint_dir = beads_dir.join("checkpoint");
        assert!(checkpoint_dir.exists());

        // Check receipts directory
        let receipts_dir = beads_dir.join("receipts");
        assert!(receipts_dir.exists());

        // Check database was created
        let db_path = root.join(".beads/beads.db");
        assert!(db_path.exists());

        // Check .gitignore was created
        let gitignore_path = beads_dir.join(".gitignore");
        assert!(gitignore_path.exists());

        // Check config.json was created
        let config_path = root.join(".beads/config.json");
        assert!(config_path.exists());
    }

    #[test]
    fn test_init_workspace_idempotent() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();

        std::env::set_current_dir(root).unwrap();

        let store = SqliteStore::new();

        // First initialization
        let result1 = store.init_workspace("test").unwrap();
        let uuid1 = result1.uuid.clone();

        // Second initialization (should be idempotent)
        let result2 = store.init_workspace("test").unwrap();

        assert_eq!(uuid1, result2.uuid);
        assert_eq!(result1.prefix, result2.prefix);
    }
}

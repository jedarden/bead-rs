//! SQLite store implementation

use super::{migrations, Store, WorkspaceConfig};
use crate::error::{Error, Result};
use rusqlite::{Connection, Result as SqliteResult};
use std::path::Path;

/// SQLite store implementation
pub struct SqliteStore {
    conn: Option<Connection>,
}

impl SqliteStore {
    /// Create a new SQLite store
    pub fn new() -> Self {
        Self { conn: None }
    }

    /// Create a new SQLite store with a database path
    #[allow(dead_code)]
    pub fn with_path(path: &Path) -> Result<Self> {
        let conn = Self::open_connection(path)?;
        Ok(Self { conn: Some(conn) })
    }

    /// Create a SQLite store from an existing connection
    pub fn from_conn(conn: Connection) -> Self {
        Self { conn: Some(conn) }
    }

    /// Get the connection
    pub fn conn(&mut self) -> &Connection {
        self.conn.as_ref().expect("Connection not initialized")
    }

    /// Get a mutable reference to the connection
    #[allow(dead_code)]
    pub fn conn_mut(&mut self) -> &mut Connection {
        self.conn.as_mut().expect("Connection not initialized")
    }

    /// Create a new store at the specified path
    #[allow(dead_code)]
    pub fn new_at(path: &Path) -> Result<Self> {
        Self::with_path(path)
    }

    /// Apply migrations to the database
    #[allow(dead_code)]
    pub fn apply_migrations(&mut self) -> Result<()> {
        let conn = self.conn.as_ref().expect("Connection not initialized");
        migrations::apply_migrations(conn)?;
        Ok(())
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

    /// Report whether the database has been initialized with the native schema.
    ///
    /// Presence of `.beads/config.json` is NOT proof that the database exists:
    /// `config.json` is tracked in git while `beads.db` is gitignored, so every
    /// fresh clone starts with a config and no database. Callers must probe the
    /// schema itself rather than inferring it from the config file.
    fn schema_initialized(conn: &Connection) -> bool {
        conn.query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'workspace'",
            [],
            |_| Ok(()),
        )
        .is_ok()
            && conn
                .query_row("SELECT 1 FROM workspace WHERE id = 1", [], |_| Ok(()))
                .is_ok()
    }

    /// Read the workspace identity recorded in `.beads/config.json`, if present.
    ///
    /// Returns `(uuid, prefix)`. This is the durable identity of the workspace:
    /// it is committed to git and is what checkpoint records key their
    /// `origin_store_uuid` against, so a rebuilt database MUST reuse it rather
    /// than minting a fresh UUID.
    fn identity_from_config(config_path: &Path) -> Option<(String, String)> {
        let raw = std::fs::read_to_string(config_path).ok()?;
        let parsed: serde_json::Value = serde_json::from_str(&raw).ok()?;
        let uuid = parsed.get("uuid")?.as_str()?.to_string();
        let prefix = parsed.get("prefix")?.as_str()?.to_string();
        if uuid.is_empty() || prefix.is_empty() {
            return None;
        }
        Some((uuid, prefix))
    }

    /// Generate a workspace UUID
    fn generate_uuid() -> String {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let bytes: [u8; 16] = rng.r#gen();
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

        // R030: never initialize into a `.beads` directory this tool does not
        // recognize. A `.beads` without `config.json` (the bead-rs workspace
        // fingerprint) is not ours to write into; laying the native schema
        // alongside foreign files is exactly the mixed-store corruption this
        // guard exists to prevent. Unconditional: the discovery override only
        // widens the upward *search* (a `--skip-foreign-workspace init` with
        // a bead-rs workspace above reports that workspace via `probe` and
        // never reaches this point), so by the time we are here the current
        // directory is the intended root and its `.beads` must be ours or
        // absent.
        let beads_dir = root.join(".beads");
        if beads_dir.exists() && !root.join(".beads/config.json").exists() {
            return Err(Error::workspace(super::foreign_workspace_message(
                &beads_dir,
            )));
        }

        // An existing config.json means this workspace already has an identity,
        // but NOT necessarily a database — `beads.db` is gitignored while
        // `config.json` is tracked, so a fresh clone lands here with a config
        // and no schema. Probe the schema; only short-circuit if it is really
        // initialized, otherwise fall through and rebuild around the recorded
        // identity.
        let config_path = root.join(".beads/config.json");
        let recorded_identity = if config_path.exists() {
            let db_path = root.join(".beads/beads.db");
            let conn = Self::open_connection(&db_path)
                .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to open database: {}", e)))?;

            if Self::schema_initialized(&conn) {
                let uuid: String = conn
                    .query_row("SELECT uuid FROM workspace WHERE id = 1", [], |row| {
                        row.get(0)
                    })
                    .map_err(|e| {
                        Error::workspace(format!("Failed to load workspace UUID: {}", e))
                    })?;

                let existing_prefix: String = conn
                    .query_row("SELECT prefix FROM workspace WHERE id = 1", [], |row| {
                        row.get(0)
                    })
                    .map_err(|e| {
                        Error::workspace(format!("Failed to load workspace prefix: {}", e))
                    })?;

                return Ok(WorkspaceConfig {
                    root,
                    uuid,
                    prefix: existing_prefix,
                });
            }

            // Config present but schema absent: rebuild, preserving identity.
            Self::identity_from_config(&config_path)
        } else {
            None
        };

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

        // When rebuilding around a committed config.json, reuse its identity:
        // the UUID is what checkpoint records reference as `origin_store_uuid`,
        // and the prefix is already baked into every bead ID ever minted here.
        let (effective_uuid, effective_prefix) = match &recorded_identity {
            Some((uuid, recorded_prefix)) => (uuid.clone(), recorded_prefix.clone()),
            None => (Self::generate_uuid(), prefix.to_string()),
        };

        if workspace_exists == 0 {
            let created_at = time::OffsetDateTime::now_utc()
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_else(|_| "unknown".to_string());

            conn.execute(
                "INSERT INTO workspace (id, uuid, prefix, layout_version, created_at) VALUES (1, ?1, ?2, 1, ?3)",
                [&effective_uuid, &effective_prefix, &created_at],
            )?;
        }

        // Note: checkpoint_state row is created by migration 2, no need to initialize here

        // Get workspace UUID
        let uuid: String =
            conn.query_row("SELECT uuid FROM workspace WHERE id = 1", [], |row| {
                row.get(0)
            })?;

        // Update workspace with generated UUID and prefix
        conn.execute(
            "UPDATE workspace SET uuid = ?1, prefix = ?2",
            [&uuid, &effective_prefix],
        )
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to update workspace: {}", e)))?;

        // Only write config.json when it does not already exist. Rewriting it
        // during a rebuild would reset `created_at` and could silently rewrite
        // the committed identity.
        if !config_path.exists() {
            let config_content = serde_json::json!({
                "version": 1,
                "uuid": uuid,
                "prefix": effective_prefix,
                "created_at": time::OffsetDateTime::now_utc()
                    .format(&time::format_description::well_known::Rfc3339)
                    .unwrap_or_else(|_| "unknown".to_string())
            });

            std::fs::write(&config_path, config_content.to_string()).map_err(|e| Error::Io {
                path: config_path.clone(),
                msg: e,
            })?;
        }

        Ok(WorkspaceConfig {
            root,
            uuid,
            prefix: effective_prefix,
        })
    }

    fn get_workspace_config(&self) -> Result<WorkspaceConfig> {
        // R030: resolve the workspace through the same discovery walk every
        // command uses -- stop at the first `.beads` directory and fail closed
        // when it is not ours, continuing past one only under the explicit
        // override. The cwd-only lookup this replaced answered a run from a
        // subdirectory with a generic "no workspace" error and never applied
        // the first-`.beads` rule at all, which both lost the R030 diagnostic
        // and made `doctor` from inside a workspace depend on where it was
        // invoked from.
        let discovered = WorkspaceConfig::discover()?
            .ok_or_else(|| Error::workspace("No workspace found in current directory"))?;
        let root = discovered.root;

        // Load existing workspace configuration from database
        let db_path = root.join(".beads/beads.db");
        let conn = Self::open_connection(&db_path)
            .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to open database: {}", e)))?;

        let uuid: String = conn
            .query_row("SELECT uuid FROM workspace WHERE id = 1", [], |row| {
                row.get(0)
            })
            .map_err(|e| Error::workspace(format!("Failed to load workspace UUID: {}", e)))?;

        let prefix: String = conn
            .query_row("SELECT prefix FROM workspace WHERE id = 1", [], |row| {
                row.get(0)
            })
            .map_err(|e| Error::workspace(format!("Failed to load workspace prefix: {}", e)))?;

        Ok(WorkspaceConfig { root, uuid, prefix })
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
    use serial_test::serial;

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
    #[serial]
    fn test_init_workspace() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();

        // Save original directory to restore later (canonicalize to get absolute path)
        let original_dir = std::env::current_dir().unwrap().canonicalize().unwrap();

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

        // Restore original directory before dropping temp
        std::env::set_current_dir(original_dir).unwrap();
        drop(temp);
    }

    #[test]
    #[serial]
    fn test_init_workspace_idempotent() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();

        // Save original directory to restore later (canonicalize to get absolute path)
        let original_dir = std::env::current_dir().unwrap().canonicalize().unwrap();

        std::env::set_current_dir(root).unwrap();

        let store = SqliteStore::new();

        // First initialization
        let result1 = store.init_workspace("test").unwrap();
        let uuid1 = result1.uuid.clone();

        // Second initialization (should be idempotent)
        let result2 = store.init_workspace("test").unwrap();

        assert_eq!(uuid1, result2.uuid);
        assert_eq!(result1.prefix, result2.prefix);

        // Restore original directory before dropping temp
        std::env::set_current_dir(original_dir).unwrap();
        drop(temp);
    }
}

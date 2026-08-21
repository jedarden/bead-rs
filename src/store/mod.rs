//! SQLite store implementation for bead-rs
//!
//! This module provides the SQLite database backend, including schema migrations,
//! connection management, and the core workspace initialization logic.

pub mod migrations;
mod sqlite;

pub use sqlite::{open_configured_connection, SqliteStore};

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// Process-wide discovery override (R030): when set, the upward walk may
/// continue past `.beads` directories that lack the bead-rs fingerprint
/// instead of stopping at the first one.
///
/// Discovery already depends on process-wide state — it starts from the
/// current directory — so this is the same dependency class, not a new one.
/// It exists so the `--skip-foreign-workspace` CLI flag, parsed once in
/// `main`, can reach every `probe`/`discover` call site without threading a
/// parameter through every command. The compiled default is `false`
/// (fail closed): library consumers get the guarding behavior unless they
/// explicitly opt in for their process, and the CLI sets it exactly once,
/// before any command dispatches. The override only widens the *search*; it
/// never authorizes writing into the skipped directory — `init_workspace`
/// refuses a `.beads` it does not recognize regardless of this setting.
static SKIP_FOREIGN_WORKSPACES: AtomicBool = AtomicBool::new(false);

/// Set whether workspace discovery may continue past `.beads` directories
/// lacking the bead-rs fingerprint (see [`SKIP_FOREIGN_WORKSPACES`]).
///
/// The binary sets it exactly once, before the first command dispatches.
/// Tests that toggle it must be `#[serial]`, since discovery already reads
/// process-global state (the current directory).
pub fn set_skip_foreign_workspaces(on: bool) {
    SKIP_FOREIGN_WORKSPACES.store(on, Ordering::SeqCst);
}

/// Whether the discovery walk currently skips past unrecognized `.beads`
/// directories instead of stopping at the first one.
fn skip_foreign_workspaces() -> bool {
    SKIP_FOREIGN_WORKSPACES.load(Ordering::SeqCst)
}

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
         beads.db is gitignored while config.json is committed. Recover it in one verified \
         operation by running `bead restore --source .beads/checkpoint --generation \
         <generation_id-from-current.json> --actor <you>` in {}; the committed identity is \
         preserved and doctor never performs this restore automatically.",
        db_path.display(),
        root.display()
    )
}

/// Operator-facing explanation of the first-`.beads`-is-not-ours state (R030).
///
/// The message opens with "No workspace found" — the same contract every
/// no-workspace error already uses — then names the directory and claims only
/// that it is not a bead-rs workspace. Identifying which foreign format
/// occupies it stays prohibited by the clean-room boundary: the diagnostic is
/// derived from the absence of `.beads/config.json` (the bead-rs workspace
/// fingerprint) alone, never from inspection of the directory's contents.
pub(crate) fn foreign_workspace_message(beads_path: &Path) -> String {
    format!(
        "No workspace found: discovery stopped at {}, which is not a bead-rs workspace \
         (.beads/config.json, the bead-rs workspace fingerprint, is absent). Discovery does not \
         continue past the first .beads directory, because silently skipping a store here and \
         operating on an unrelated parent workspace has destroyed a real workspace before. If \
         this nesting is intentional and a bead-rs workspace exists farther up, rerun with \
         --skip-foreign-workspace; to use bead-rs in this directory instead, move or remove the \
         existing .beads directory first.",
        beads_path.display()
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
    /// No `.beads` directory found walking up from the current directory.
    NotFound,
    /// The first `.beads` directory on the upward walk lacks the bead-rs
    /// fingerprint (`.beads/config.json`). Discovery stopped there and did
    /// not continue to any parent workspace (R030). Not repairable by
    /// `init`: that would write into the unrecognized directory.
    NotBeadRs {
        /// The `.beads` directory that stopped the walk
        beads_path: PathBuf,
    },
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
    /// The walk stops at the FIRST `.beads` directory it encounters (R030):
    /// if that directory carries the bead-rs fingerprint
    /// (`.beads/config.json`) it is classified as [`WorkspaceState::Ready`]
    /// or [`WorkspaceState::Uninitialized`]; if it does not, discovery fails
    /// closed as [`WorkspaceState::NotBeadRs`] rather than continuing to an
    /// unrelated parent workspace. Walking past an unrecognized `.beads` is
    /// what the `--skip-foreign-workspace` override (see
    /// [`set_skip_foreign_workspaces`]) exists to permit, for legitimately
    /// nested layouts.
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
            let beads_dir = current.join(".beads");
            if beads_dir.exists() {
                let config_file = beads_dir.join("config.json");
                if config_file.exists() {
                    return Self::state_from_config_path(&config_file);
                }

                // First `.beads` on the walk, and it is not ours. Stop here
                // and say so, unless the override explicitly widens the
                // search past it.
                if !skip_foreign_workspaces() {
                    return Ok(WorkspaceState::NotBeadRs {
                        beads_path: beads_dir,
                    });
                }
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
    /// remedy, and an unrecognized first `.beads` directory fails closed
    /// naming that path (never the foreign format); see
    /// [`probe`](Self::probe) when the caller can repair or diagnose it.
    pub fn discover() -> crate::Result<Option<Self>> {
        match Self::probe()? {
            WorkspaceState::NotFound => Ok(None),
            WorkspaceState::Ready(config) => Ok(Some(config)),
            WorkspaceState::NotBeadRs { beads_path } => Err(crate::Error::workspace(
                foreign_workspace_message(&beads_path),
            )),
            WorkspaceState::Uninitialized { root, db_path } => Err(crate::Error::workspace(
                uninitialized_message(&root, &db_path),
            )),
        }
    }

    /// Classify the workspace rooted at the given config file path
    fn state_from_config_path(config_path: &Path) -> crate::Result<WorkspaceState> {
        let root = config_path
            .parent()
            .and_then(|p| p.parent())
            .ok_or_else(|| crate::Error::workspace("Invalid workspace structure"))?;

        let db_path = root.join(".beads/beads.db");

        // Open database and load workspace metadata through the shared
        // pragma-configured opener, so discovery connections carry the same
        // configuration as every other connection to the workspace.
        let conn = open_configured_connection(&db_path).map_err(|e| {
            crate::Error::Internal(anyhow::anyhow!("Failed to open database: {}", e))
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

    /// Restore the cwd and the discovery override around a probe test.
    struct DiscoveryGuard {
        original_dir: PathBuf,
    }

    impl DiscoveryGuard {
        fn new() -> Self {
            // canonicalize to get absolute path
            let original_dir = std::env::current_dir().unwrap().canonicalize().unwrap();
            set_skip_foreign_workspaces(false);
            Self { original_dir }
        }
    }

    impl Drop for DiscoveryGuard {
        fn drop(&mut self) {
            std::env::set_current_dir(&self.original_dir).unwrap();
            set_skip_foreign_workspaces(false);
        }
    }

    #[test]
    #[serial]
    fn test_workspace_discovery_none() {
        // Save original directory to restore later (canonicalize to get absolute path)
        let original_dir = std::env::current_dir().unwrap().canonicalize().unwrap();

        // In a temp directory, there is no workspace *in it*. A `.beads`
        // above the temp directory (this machine keeps unrelated debris in
        // $HOME, above $TMPDIR) legitimately stops the walk instead — the
        // R030 outcome — so the assertion is that no workspace is found IN
        // the temp directory, never Ready or Uninitialized.
        let temp = tempfile::tempdir().unwrap();
        std::env::set_current_dir(temp.path()).unwrap();
        match WorkspaceConfig::probe().unwrap() {
            WorkspaceState::NotFound => assert!(WorkspaceConfig::discover().unwrap().is_none()),
            WorkspaceState::NotBeadRs { beads_path } => {
                assert!(
                    !beads_path.starts_with(temp.path()),
                    "a .beads inside the bare temp dir must not exist: {beads_path:?}"
                );
                assert!(WorkspaceConfig::discover().is_err());
            }
            other => panic!("bare temp dir must not resolve a workspace: {other:?}"),
        }

        // Restore original directory before dropping temp
        std::env::set_current_dir(original_dir).unwrap();
        drop(temp);
    }

    #[test]
    #[serial]
    fn test_discovery_stops_at_first_beads_without_fingerprint() {
        let _guard = DiscoveryGuard::new();

        // A beaded parent (fingerprint present) with an unrecognized `.beads`
        // closer to the start: the walk must stop at the child and never
        // reach the parent.
        let temp = tempfile::tempdir().unwrap();
        let parent = temp.path().join("parent");
        let child = parent.join("child");
        std::fs::create_dir_all(child.join(".beads")).unwrap();
        std::fs::create_dir_all(parent.join(".beads")).unwrap();
        std::fs::write(parent.join(".beads/config.json"), "{}").unwrap();

        std::env::set_current_dir(&child).unwrap();
        match WorkspaceConfig::probe().unwrap() {
            WorkspaceState::NotBeadRs { beads_path } => {
                assert_eq!(beads_path, child.join(".beads"));
            }
            other => panic!("expected NotBeadRs, got {other:?}"),
        }

        // discover() fails closed with the diagnostic naming that path.
        let err = WorkspaceConfig::discover().unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains(child.join(".beads").display().to_string().as_str()),
            "{msg}"
        );
        assert!(msg.contains("not a bead-rs workspace"), "{msg}");
    }

    #[test]
    #[serial]
    fn test_discovery_override_continues_past_unrecognized_beads() {
        let _guard = DiscoveryGuard::new();

        let temp = tempfile::tempdir().unwrap();
        let parent = temp.path().join("parent");
        let child = parent.join("child");
        std::fs::create_dir_all(child.join(".beads")).unwrap();
        std::fs::create_dir_all(parent.join(".beads")).unwrap();
        // Parent has the fingerprint but no database: the walk having
        // continued past the child is what this asserts, so Uninitialized
        // (not NotBeadRs) is the expected classification.
        std::fs::write(parent.join(".beads/config.json"), "{}").unwrap();

        std::env::set_current_dir(&child).unwrap();
        set_skip_foreign_workspaces(true);
        match WorkspaceConfig::probe().unwrap() {
            WorkspaceState::Uninitialized { root, .. } => assert_eq!(root, parent),
            other => panic!("expected Uninitialized, got {other:?}"),
        }
    }

    #[test]
    #[serial]
    fn test_discovery_sandwiched_workspace_still_wins() {
        let _guard = DiscoveryGuard::new();

        // The working directory sits inside a bead-rs workspace whose own
        // `.beads` comes first on the walk: an unrecognized `.beads` above
        // it must be irrelevant, with or without the override.
        let temp = tempfile::tempdir().unwrap();
        let outer = temp.path().join("outer");
        let workspace = outer.join("workspace");
        let deep = workspace.join("deep");
        std::fs::create_dir_all(outer.join(".beads")).unwrap();
        std::fs::create_dir_all(workspace.join(".beads")).unwrap();
        std::fs::write(workspace.join(".beads/config.json"), "{}").unwrap();
        std::fs::create_dir_all(&deep).unwrap();

        std::env::set_current_dir(&deep).unwrap();
        for skip in [false, true] {
            set_skip_foreign_workspaces(skip);
            match WorkspaceConfig::probe().unwrap() {
                WorkspaceState::Uninitialized { root, .. } => assert_eq!(root, workspace),
                other => panic!("skip={skip}: expected Uninitialized, got {other:?}"),
            }
        }
    }
}

//! CLI command definitions for bead-rs
//!
//! This module uses clap derive to define all command-line interface commands.

use clap::{Parser, Subcommand};

/// Main CLI structure for bead-rs
#[derive(Parser, Debug)]
#[command(name = "bead")]
#[command(
    author = "Jed Arden <github@jedarden.com>",
    version = "0.1.0",
    about = "Clean-room task coordination for agent fleets",
    long_about = "bead-rs is an independent Rust task-coordination system.

The intended workflow is:
  init workspace -> create/import beads -> add blocking relationships
  -> inspect ready work -> claim -> update/release -> close -> flush JSONL backup

The ready frontier can be inspected with `bead list --ready --json --limit N`,
which uses claim order but does not reserve the displayed beads. Use `bead claim`
to atomically assign work.

SQLite is the authoritative live state between flushes. The JSONL checkpoint is
the portable backup and should be flushed with `bead sync --flush-only` before
committing the repository.

Lifecycle transitions:
  - open beads may be ready if unassigned and not manually blocked
  - unfinished `blocks` edges remove beads from the ready frontier
  - claim atomically assigns one ready bead and moves it to in_progress
  - release returns claimed work to open/unassigned
  - close requires a reason and may expose dependents
  - reopen restores a closed bead to open

Use `bead --help` to see all available commands."
)]
#[command(propagate_version = true)]
pub struct Cli {
    /// Subcommand to execute
    #[command(subcommand)]
    pub command: Command,
}

/// Available commands
#[derive(Subcommand, Debug)]
pub enum Command {
    /// Initialize a new workspace
    Init(InitOptions),

    /// Create a new issue
    Create(CreateOptions),

    /// List issues
    List(ListOptions),

    /// Show a single issue
    Show(ShowOptions),

    /// Not yet implemented
    #[command(subcommand)]
    #[allow(clippy::enum_variant_names)]
    Unimplemented(UnimplementedCommand),
}

/// Options for workspace initialization
#[derive(Parser, Debug)]
pub struct InitOptions {
    /// Custom prefix for bead IDs (default: bead)
    #[arg(long, default_value = "bead")]
    pub prefix: String,
}

/// Options for creating a new issue
#[derive(Parser, Debug)]
pub struct CreateOptions {
    /// Issue title (required)
    #[arg(long)]
    pub title: String,

    /// Issue description (optional, defaults to empty)
    #[arg(long)]
    pub description: Option<String>,

    /// Issue priority (0-4, default: 2)
    #[arg(long, default_value = "2")]
    pub priority: i64,

    /// Issue type (default: task)
    #[arg(long)]
    pub issue_type: Option<String>,

    /// Assignee (optional)
    #[arg(long)]
    pub assignee: Option<String>,

    /// Labels to add (can be specified multiple times)
    #[arg(long)]
    pub label: Vec<String>,
}

/// Options for listing issues
#[derive(Parser, Debug)]
pub struct ListOptions {
    /// Output in JSON format
    #[arg(long)]
    pub json: bool,

    /// Filter by status
    #[arg(long)]
    pub status: Option<String>,

    /// Filter by assignee
    #[arg(long)]
    pub assignee: Option<String>,

    /// Show only ready frontier issues
    #[arg(long)]
    pub ready: bool,

    /// Comment projection: none, unresolved, or all (default: none)
    #[arg(long, default_value = "none")]
    pub comments: String,

    /// Maximum number of issues to return (0-999999)
    #[arg(long, default_value = "100")]
    pub limit: i64,
}

/// Options for showing a single issue
#[derive(Parser, Debug)]
pub struct ShowOptions {
    /// Issue ID
    pub id: String,

    /// Output in JSON format
    #[arg(long)]
    pub json: bool,

    /// Comment projection: none, unresolved, or all (default: none)
    #[arg(long, default_value = "none")]
    pub comments: String,
}

/// Placeholder for unimplemented commands
#[derive(Subcommand, Debug)]
pub enum UnimplementedCommand {
    Claim,
    Update,
    Release,
    Close,
    Reopen,
    Label,
    Dep,
    Sync,
    Doctor,
    Capabilities,
    Schema,
    Migrate,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_parsing() {
        // Test init command
        let cli = Cli::try_parse_from(["bead", "init"]).unwrap();
        matches!(cli.command, Command::Init(_));

        // Test init with custom prefix
        let cli = Cli::try_parse_from(["bead", "init", "--prefix", "custom"]).unwrap();
        if let Command::Init(opts) = cli.command {
            assert_eq!(opts.prefix, "custom");
        } else {
            panic!("Expected Init command");
        }
    }

    #[test]
    fn test_cli_help() {
        // Just ensure help doesn't panic
        let _ = Cli::try_parse_from(["bead", "--help"]);
    }
}

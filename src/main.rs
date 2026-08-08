#![forbid(unsafe_code)]

mod cli;
mod error;
mod store;

use crate::cli::{Cli, Command};
use crate::error::{Error, Result};
use crate::store::Store;
use clap::Parser;
use std::process::ExitCode;

fn main() -> ExitCode {
    // Parse CLI arguments
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(err) => {
            // Clap will display the error message
            eprintln!("{err}");
            return ExitCode::from(2u8);
        }
    };

    // Execute command
    let result = execute_command(cli);

    match result {
        Ok(_) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("bead: {err}");
            ExitCode::from(err.exit_code() as u8)
        }
    }
}

fn execute_command(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Init(opts) => cmd_init(opts),
        Command::Unimplemented(_) => Err(Error::cli_usage(
            "This command is not yet implemented. See `bead --help` for available commands.",
        )),
    }
}

fn cmd_init(opts: cli::InitOptions) -> Result<()> {
    let store = store::SqliteStore::new();

    // Check if workspace already exists
    if let Some(existing_config) = store::WorkspaceConfig::discover()? {
        eprintln!(
            "Workspace already exists at: {}",
            existing_config.root.display()
        );
        eprintln!("Prefix: {}", existing_config.prefix);
        eprintln!("UUID: {}", existing_config.uuid);
        return Ok(());
    }

    // Initialize new workspace
    let config = store.init_workspace(&opts.prefix)?;

    eprintln!("Initialized workspace:");
    eprintln!("  Root: {}", config.root.display());
    eprintln!("  UUID: {}", config.uuid);
    eprintln!("  Prefix: {}", config.prefix);

    Ok(())
}

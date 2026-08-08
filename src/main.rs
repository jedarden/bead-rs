#![forbid(unsafe_code)]

mod cli;
mod error;
mod model;
mod service;
mod store;

use crate::cli::{Cli, Command};
use crate::error::{Error, Result};
use crate::store::Store;
use clap::Parser;
use std::process::ExitCode;

fn main() -> ExitCode {
    // Parse CLI arguments
    // Use parse() instead of try_parse() so --help and --version are handled automatically
    let cli = Cli::parse();

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
        Command::Create(opts) => cmd_create(opts),
        Command::Claim(opts) => cmd_claim(opts),
        Command::List(opts) => cmd_list(opts),
        Command::Show(opts) => cmd_show(opts),
        Command::Update(opts) => cmd_update(opts),
        Command::Release(opts) => cmd_release(opts),
        Command::Close(opts) => cmd_close(opts),
        Command::Reopen(opts) => cmd_reopen(opts),
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

fn cmd_claim(opts: cli::ClaimOptions) -> Result<()> {
    // Discover workspace
    let config = store::WorkspaceConfig::discover()?
        .ok_or_else(|| Error::workspace("No workspace found. Run `bead init` first."))?;

    // Open database connection
    let db_path = config.database_path();
    let conn = rusqlite::Connection::open(&db_path)
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to open database: {}", e)))?;

    // Use an immediate transaction for atomicity
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to start transaction: {}", e)))?;

    // Claim an issue
    let result = service::claim_issue(&tx, &opts.assignee, None, None, None)?;

    // Commit transaction
    tx.commit()
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to commit transaction: {}", e)))?;

    // Output result
    if opts.json {
        let output = serde_json::to_string(&result).map_err(|e| {
            Error::Internal(anyhow::anyhow!("Failed to serialize claim result: {}", e))
        })?;
        println!("{}", output);
    } else {
        if let Some(bead_id) = result.bead_id {
            println!("Claimed: {}", bead_id);
            println!("Assignee: {}", result.assignee);
        } else {
            println!("No eligible work found.");
        }
    }

    Ok(())
}

fn cmd_create(opts: cli::CreateOptions) -> Result<()> {
    // Discover workspace
    let config = store::WorkspaceConfig::discover()?
        .ok_or_else(|| Error::workspace("No workspace found. Run `bead init` first."))?;

    // Open database connection
    let db_path = config.database_path();
    let conn = rusqlite::Connection::open(&db_path)
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to open database: {}", e)))?;

    // Use a transaction for atomicity
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to start transaction: {}", e)))?;

    // Create the issue
    let issue = service::create_issue(
        &tx,
        &config,
        opts.title,
        opts.description,
        opts.priority,
        opts.issue_type,
        opts.assignee,
        opts.label,
    )?;

    // Commit transaction
    tx.commit()
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to commit transaction: {}", e)))?;

    // Print only the ID on success
    println!("{}", issue.id);

    Ok(())
}

fn cmd_list(opts: cli::ListOptions) -> Result<()> {
    // Discover workspace
    let config = store::WorkspaceConfig::discover()?
        .ok_or_else(|| Error::workspace("No workspace found. Run `bead init` first."))?;

    // Open database connection
    let db_path = config.database_path();
    let conn = rusqlite::Connection::open(&db_path)
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to open database: {}", e)))?;

    // Validate limit
    if opts.limit < 0 || opts.limit > 999999 {
        return Err(Error::validation("Limit must be between 0 and 999999"));
    }

    // Validate comments option
    if opts.comments != "none" && opts.comments != "unresolved" && opts.comments != "all" {
        return Err(Error::validation(
            "Comments must be one of: none, unresolved, all",
        ));
    }

    // Get issues
    let issues = service::list_issues(
        &conn,
        opts.status.as_deref(),
        opts.assignee.as_deref(),
        opts.ready,
        opts.limit,
    )?;

    // Output results
    if opts.json {
        // Emit one compact object per line
        for issue in issues {
            let output = serde_json::to_string(&to_needle_json(&issue)).map_err(|e| {
                Error::Internal(anyhow::anyhow!("Failed to serialize issue: {}", e))
            })?;
            println!("{}", output);
        }
    } else {
        // Human-readable output
        if issues.is_empty() {
            println!("No issues found.");
        } else {
            for issue in issues {
                println!("ID: {}", issue.id);
                println!("  Title: {}", issue.title);
                println!("  Status: {:?}", issue.base_status);
                println!("  Priority: P{}", issue.priority);
                if let Some(assignee) = &issue.assignee {
                    println!("  Assignee: {}", assignee);
                }
                println!();
            }
        }
    }

    Ok(())
}

fn cmd_show(opts: cli::ShowOptions) -> Result<()> {
    // Discover workspace
    let config = store::WorkspaceConfig::discover()?
        .ok_or_else(|| Error::workspace("No workspace found. Run `bead init` first."))?;

    // Open database connection
    let db_path = config.database_path();
    let conn = rusqlite::Connection::open(&db_path)
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to open database: {}", e)))?;

    // Validate comments option
    if opts.comments != "none" && opts.comments != "unresolved" && opts.comments != "all" {
        return Err(Error::validation(
            "Comments must be one of: none, unresolved, all",
        ));
    }

    // Get the issue
    let issue = service::get_issue_by_id(&conn, &opts.id)?
        .ok_or_else(|| Error::not_found(format!("Issue not found: {}", opts.id)))?;

    // Output results
    if opts.json {
        // Emit as one-element array for NEEDLE v1 compatibility
        let output = serde_json::to_string(&vec![to_needle_json(&issue)])
            .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to serialize issue: {}", e)))?;
        println!("{}", output);
    } else {
        // Human-readable output
        println!("ID: {}", issue.id);
        println!("Title: {}", issue.title);
        println!("Status: {:?}", issue.base_status);
        println!("Priority: P{}", issue.priority);
        println!("Created: {}", issue.created_at);
        println!("Updated: {}", issue.updated_at);

        if let Some(description) = &issue.description {
            println!("Description: {}", description);
        }

        if let Some(assignee) = &issue.assignee {
            println!("Assignee: {}", assignee);
        }

        if let Some(issue_type) = &issue.issue_type {
            println!("Type: {}", issue_type);
        }
    }

    Ok(())
}

fn cmd_update(opts: cli::UpdateOptions) -> Result<()> {
    // Discover workspace
    let config = store::WorkspaceConfig::discover()?
        .ok_or_else(|| Error::workspace("No workspace found. Run `bead init` first."))?;

    // Open database connection
    let db_path = config.database_path();
    let conn = rusqlite::Connection::open(&db_path)
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to open database: {}", e)))?;

    // Update the issue
    let id = service::update_issue(
        &conn,
        &opts.id,
        opts.status.as_deref(),
        opts.assignee.as_deref(),
        opts.clear_assignee,
        opts.notes.as_deref(),
    )?;

    // Print only the ID on success
    println!("{}", id);

    Ok(())
}

fn cmd_release(opts: cli::ReleaseOptions) -> Result<()> {
    // Discover workspace
    let config = store::WorkspaceConfig::discover()?
        .ok_or_else(|| Error::workspace("No workspace found. Run `bead init` first."))?;

    // Open database connection
    let db_path = config.database_path();
    let conn = rusqlite::Connection::open(&db_path)
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to open database: {}", e)))?;

    // Release the issue
    let id = service::release_issue(&conn, &opts.id)?;

    // Print only the ID on success
    println!("{}", id);

    Ok(())
}

fn cmd_close(opts: cli::CloseOptions) -> Result<()> {
    // Discover workspace
    let config = store::WorkspaceConfig::discover()?
        .ok_or_else(|| Error::workspace("No workspace found. Run `bead init` first."))?;

    // Open database connection
    let db_path = config.database_path();
    let conn = rusqlite::Connection::open(&db_path)
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to open database: {}", e)))?;

    // Close the issue
    let id = service::close_issue(&conn, &opts.id, &opts.reason)?;

    // Print only the ID on success
    println!("{}", id);

    Ok(())
}

fn cmd_reopen(opts: cli::ReopenOptions) -> Result<()> {
    // Discover workspace
    let config = store::WorkspaceConfig::discover()?
        .ok_or_else(|| Error::workspace("No workspace found. Run `bead init` first."))?;

    // Open database connection
    let db_path = config.database_path();
    let conn = rusqlite::Connection::open(&db_path)
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to open database: {}", e)))?;

    // Reopen the issue
    let id = service::reopen_issue(&conn, &opts.id)?;

    // Print only the ID on success
    println!("{}", id);

    Ok(())
}

/// Convert an Issue to NEEDLE-compatible JSON format
fn to_needle_json(issue: &model::Issue) -> serde_json::Value {
    let status_str = match issue.base_status {
        model::BaseStatus::Open => "open",
        model::BaseStatus::InProgress => "in_progress",
        model::BaseStatus::Deferred => "deferred",
        model::BaseStatus::Closed => "closed",
    };

    serde_json::json!({
        "id": issue.id,
        "title": issue.title,
        "description": issue.description.as_ref().unwrap_or(&String::new()),
        "priority": issue.priority,
        "status": status_str,
        "assignee": issue.assignee,
        "dependencies": [],
        "created_at": issue.created_at,
        "updated_at": issue.updated_at,
        "labels": []
    })
}

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
        Command::Label(opts) => cmd_label(opts),
        Command::Dep(opts) => cmd_dep(opts),
        Command::Sync(opts) => cmd_sync(opts),
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
        if issues.is_empty() {
            println!("[]");
        } else {
            for issue in issues {
                // Load dependencies and labels for each issue
                let dependencies = load_dependencies(&conn, &issue.id)?;
                let labels = load_labels(&conn, &issue.id)?;
                let output = serde_json::to_string(&to_needle_json(&issue, &dependencies, &labels))
                    .map_err(|e| {
                        Error::Internal(anyhow::anyhow!("Failed to serialize issue: {}", e))
                    })?;
                println!("{}", output);
            }
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

    // Load dependencies for this issue
    let dependencies = load_dependencies(&conn, &opts.id)?;

    // Load labels for this issue
    let labels = load_labels(&conn, &opts.id)?;

    // Output results
    if opts.json {
        // Emit as one-element array for NEEDLE v1 compatibility
        let output = serde_json::to_string(&vec![to_needle_json(&issue, &dependencies, &labels)])
            .map_err(|e| {
            Error::Internal(anyhow::anyhow!("Failed to serialize issue: {}", e))
        })?;
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

fn cmd_label(cmd: cli::LabelCommand) -> Result<()> {
    match cmd {
        cli::LabelCommand::Add(opts) => cmd_label_add(opts),
        cli::LabelCommand::Remove(opts) => cmd_label_remove(opts),
    }
}

fn cmd_label_add(opts: cli::LabelAddOptions) -> Result<()> {
    // Discover workspace
    let config = store::WorkspaceConfig::discover()?
        .ok_or_else(|| Error::workspace("No workspace found. Run `bead init` first."))?;

    // Open database connection
    let db_path = config.database_path();
    let conn = rusqlite::Connection::open(&db_path)
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to open database: {}", e)))?;

    // Create store wrapper
    let mut store = store::SqliteStore::from_conn(conn);

    // Add the label
    service::add_label(&mut store, &opts.id, &opts.label)?;

    // Print success message
    println!("Added label '{}' to {}", opts.label, opts.id);

    Ok(())
}

fn cmd_label_remove(opts: cli::LabelRemoveOptions) -> Result<()> {
    // Discover workspace
    let config = store::WorkspaceConfig::discover()?
        .ok_or_else(|| Error::workspace("No workspace found. Run `bead init` first."))?;

    // Open database connection
    let db_path = config.database_path();
    let conn = rusqlite::Connection::open(&db_path)
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to open database: {}", e)))?;

    // Create store wrapper
    let mut store = store::SqliteStore::from_conn(conn);

    // Remove the label
    service::remove_label(&mut store, &opts.id, &opts.label)?;

    // Print success message
    println!("Removed label '{}' from {}", opts.label, opts.id);

    Ok(())
}

fn cmd_dep(cmd: cli::DepCommand) -> Result<()> {
    match cmd {
        cli::DepCommand::Add(opts) => cmd_dep_add(opts),
        cli::DepCommand::Remove(opts) => cmd_dep_remove(opts),
    }
}

fn cmd_dep_add(opts: cli::DepAddOptions) -> Result<()> {
    // Discover workspace
    let config = store::WorkspaceConfig::discover()?
        .ok_or_else(|| Error::workspace("No workspace found. Run `bead init` first."))?;

    // Open database connection
    let db_path = config.database_path();
    let conn = rusqlite::Connection::open(&db_path)
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to open database: {}", e)))?;

    // Create store wrapper
    let mut store = store::SqliteStore::from_conn(conn);

    // Add the dependency
    service::add_dependency(&mut store, &opts.blocked, &opts.blocker, &opts.kind)?;

    // Print success message
    println!(
        "Added dependency: {} {} {}",
        opts.blocked,
        if opts.kind == "blocks" {
            "blocked by"
        } else {
            "related to"
        },
        opts.blocker
    );

    Ok(())
}

fn cmd_dep_remove(opts: cli::DepRemoveOptions) -> Result<()> {
    // Discover workspace
    let config = store::WorkspaceConfig::discover()?
        .ok_or_else(|| Error::workspace("No workspace found. Run `bead init` first."))?;

    // Open database connection
    let db_path = config.database_path();
    let conn = rusqlite::Connection::open(&db_path)
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to open database: {}", e)))?;

    // Create store wrapper
    let mut store = store::SqliteStore::from_conn(conn);

    // Remove the dependency
    service::remove_dependency(
        &mut store,
        &opts.blocked,
        &opts.blocker,
        opts.kind.as_deref(),
    )?;

    // Print success message
    println!("Removed dependency: {} <- {}", opts.blocked, opts.blocker);

    Ok(())
}

fn cmd_sync(cmd: cli::SyncCommand) -> Result<()> {
    match cmd {
        cli::SyncCommand::FlushOnly(opts) => cmd_sync_flush_only(opts),
        cli::SyncCommand::ImportOnly(opts) => cmd_sync_import_only(opts),
    }
}

fn cmd_sync_flush_only(opts: cli::SyncFlushOptions) -> Result<()> {
    // Validate profile (only native-v1 allowed before F017)
    if opts.profile != "native-v1" {
        return Err(Error::validation(format!(
            "Profile '{}' is not supported. Only 'native-v1' is available before F017.",
            opts.profile
        )));
    }

    // Discover workspace
    let config = store::WorkspaceConfig::discover()?
        .ok_or_else(|| Error::workspace("No workspace found. Run `bead init` first."))?;

    // Determine output path
    let output_path = if let Some(ref output) = opts.output {
        // Explicit output path provided
        config.root.join(output)
    } else {
        // Default to .beads/issues.jsonl
        config.root.join(".beads").join("issues.jsonl")
    };

    // Validate output path doesn't point into .beads/checkpoint (reserved for F017)
    let checkpoint_dir = config.root.join(".beads").join("checkpoint");
    if output_path.starts_with(&checkpoint_dir) {
        return Err(Error::validation(
            "Output path cannot be in .beads/checkpoint (reserved for F017 forensic checkpoints)",
        ));
    }

    // Open database connection
    let db_path = config.database_path();
    let conn = rusqlite::Connection::open(&db_path)
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to open database: {}", e)))?;

    // Create store wrapper
    let mut store = store::SqliteStore::from_conn(conn);

    // Flush checkpoint
    let result = service::flush_checkpoint(&mut store, &output_path)?;

    // Print success message
    eprintln!("Flushed checkpoint:");
    eprintln!("  Path: {}", output_path.display());
    eprintln!("  Issues: {}", result.issue_count);
    eprintln!("  Hash: {}", result.hash);
    eprintln!("  Covered sequence: {}", result.covered_sequence);
    eprintln!("  Export time: {}", result.export_time);

    Ok(())
}

fn cmd_sync_import_only(opts: cli::SyncImportOptions) -> Result<()> {
    // Discover workspace
    let config = store::WorkspaceConfig::discover()?
        .ok_or_else(|| Error::workspace("No workspace found. Run `bead init` first."))?;

    // Resolve input path relative to workspace root if not absolute
    let input_path = if std::path::Path::new(&opts.input).is_absolute() {
        std::path::PathBuf::from(&opts.input)
    } else {
        config.root.join(&opts.input)
    };

    // Validate input file exists
    if !input_path.exists() {
        return Err(Error::not_found(format!(
            "Input file not found: {}",
            input_path.display()
        )));
    }

    // Open database connection
    let db_path = config.database_path();
    let conn = rusqlite::Connection::open(&db_path)
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to open database: {}", e)))?;

    // Create store wrapper
    let mut store = store::SqliteStore::from_conn(conn);

    // Import checkpoint
    let result = service::import_checkpoint(&mut store, &input_path, &opts.profile, opts.dry_run)?;

    // Print result
    if opts.dry_run {
        eprintln!("Dry-run import analysis:");
    } else {
        eprintln!("Imported checkpoint:");
    }
    eprintln!("  Profile: {}", result.profile);
    eprintln!("  Input hash: {}", result.input_hash);
    eprintln!("  Inserted: {}", result.inserted);
    eprintln!("  Updated: {}", result.updated);
    eprintln!("  Retained: {}", result.retained);
    eprintln!("  Conflicted: {}", result.conflicted);
    eprintln!("  Activation sequence: {}", result.activation_sequence);
    eprintln!("  Covered sequence: {}", result.covered_sequence);
    eprintln!("  Dry run: {}", result.dry_run);
    eprintln!("  Prospective: {}", result.prospective);

    Ok(())
}

/// Convert an Issue to NEEDLE-compatible JSON format
fn to_needle_json(
    issue: &model::Issue,
    dependencies: &[serde_json::Value],
    labels: &[String],
) -> serde_json::Value {
    let status_str = match issue.base_status {
        model::BaseStatus::Open => "open",
        model::BaseStatus::InProgress => "in_progress",
        model::BaseStatus::Deferred => "deferred",
        model::BaseStatus::Closed => "closed",
    };

    // Include all issue fields for complete representation
    serde_json::json!({
        "id": issue.id,
        "title": issue.title,
        "description": issue.description.as_ref().unwrap_or(&String::new()),
        "priority": issue.priority,
        "status": status_str,
        "assignee": issue.assignee,
        "dependencies": dependencies,
        "created_at": issue.created_at,
        "updated_at": issue.updated_at,
        "labels": labels
    })
}

/// Load dependencies for an issue from the database
fn load_dependencies(
    conn: &rusqlite::Connection,
    issue_id: &str,
) -> Result<Vec<serde_json::Value>> {
    let mut stmt = conn.prepare_cached(
        "SELECT blocker_issue_id, kind FROM dependencies WHERE blocked_issue_id = ?",
    )?;

    let deps = stmt
        .query_map([issue_id], |row| {
            let blocker: String = row.get(0)?;
            let kind: String = row.get(1)?;
            Ok(serde_json::json!({
                "blocker": blocker,
                "kind": kind
            }))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to load dependencies: {}", e)))?;

    Ok(deps)
}

/// Load labels for an issue from the database
fn load_labels(conn: &rusqlite::Connection, issue_id: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare_cached("SELECT label FROM labels WHERE issue_id = ?")?;

    let labels = stmt
        .query_map([issue_id], |row| row.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to load labels: {}", e)))?;

    Ok(labels)
}

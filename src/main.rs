#![forbid(unsafe_code)]

mod cli;
mod error;
mod model;
mod service;
mod store;

use crate::cli::{Cli, Command};
use crate::error::{Error, Result};
use crate::service::checkpoint::CheckpointMode;
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
        Command::Doctor(opts) => cmd_doctor(opts),
        Command::Capabilities(opts) => cmd_capabilities(opts),
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

    // Claim an issue with optional lease support
    let enhanced_result = service::claim_issue_with_lease(
        &tx,
        &opts.assignee,
        opts.lease_ttl,
        opts.renew_lease,
        opts.fencing_token,
    )?;

    // Get decision trace if requested (backward compatibility with R001)
    let trace = if opts.why {
        let (_, trace_data) = service::claim_issue_with_trace(&tx, &opts.assignee, None, None, None, true)?;
        trace_data
    } else {
        None
    };

    // Commit transaction
    tx.commit()
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to commit transaction: {}", e)))?;

    // Output result
    if opts.json {
        let output = if let Some(trace_data) = trace {
            // When --why is set, output enriched result with decision trace and lease info
            serde_json::to_string(&serde_json::json!({
                "bead_id": enhanced_result.bead_id,
                "assignee": enhanced_result.assignee,
                "lease": enhanced_result.lease,
                "decision_trace": trace_data
            }))
            .map_err(|e| {
                Error::Internal(anyhow::anyhow!(
                    "Failed to serialize claim result with trace: {}",
                    e
                ))
            })?
        } else {
            // Standard claim result with lease information
            serde_json::to_string(&enhanced_result).map_err(|e| {
                Error::Internal(anyhow::anyhow!("Failed to serialize claim result: {}", e))
            })?
        };
        println!("{}", output);
    } else {
        if let Some(bead_id) = &enhanced_result.bead_id {
            println!("Claimed: {}", bead_id);
            println!("Assignee: {}", enhanced_result.assignee);

            // Display lease information if present
            if let Some(lease_info) = &enhanced_result.lease {
                println!("Lease expires at: {}", lease_info.expires_at);
                println!("Fencing token: {}", lease_info.fencing_token);
            }
        } else {
            println!("No eligible work found.");
        }

        // Output decision trace in human-readable format if requested
        if let Some(trace_data) = trace {
            println!("\n=== Decision Trace ===");
            println!("Version: {}", trace_data.version);
            println!("Policy: {}", trace_data.policy);
            println!("Assignee: {}", trace_data.assignee);
            println!(
                "Selection: {}",
                if trace_data.has_selection {
                    "Yes"
                } else {
                    "No"
                }
            );

            if let Some(selected_id) = &trace_data.selected_issue_id {
                println!("Selected Issue: {}", selected_id);
            }

            println!("\nReasons:");
            for reason in &trace_data.reasons {
                println!("  - {:?}", reason);
            }

            println!("\nEligibility Summary:");
            println!(
                "  Total Issues: {}",
                trace_data.eligibility_summary.total_issues
            );
            println!(
                "  Eligible: {}",
                trace_data.eligibility_summary.eligible_count
            );
            println!(
                "  Ineligible: {}",
                trace_data.eligibility_summary.ineligible_count
            );

            if !trace_data
                .eligibility_summary
                .ineligibility_reasons
                .is_empty()
            {
                println!("  Ineligibility Reasons:");
                for (reason, count) in &trace_data.eligibility_summary.ineligibility_reasons {
                    println!("    {}: {}", reason, count);
                }
            }

            if let Some(factors) = &trace_data.selected_factors {
                println!("\nSelected Issue Factors:");
                println!("  Priority: {}", factors.priority);
                println!("  Status: {}", factors.base_status);
                println!("  Assigned: {}", factors.is_assigned);
                println!("  Manually Blocked: {}", factors.is_manually_blocked);
                println!(
                    "  Unfinished Blockers: {}",
                    factors.unfinished_blocker_count
                );
            }
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
                println!("  Revision: {}", issue.revision.unwrap_or(1));
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
        println!("Revision: {}", issue.revision.unwrap_or(1));
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
        opts.if_revision,
        opts.fencing_token,
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
    let id = service::release_issue(&conn, &opts.id, opts.if_revision, opts.fencing_token)?;

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
    let id = service::close_issue(&conn, &opts.id, &opts.reason, opts.if_revision, opts.fencing_token)?;

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
    let id = service::reopen_issue(&conn, &opts.id, opts.if_revision, opts.fencing_token)?;

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
    // Discover workspace
    let config = store::WorkspaceConfig::discover()?
        .ok_or_else(|| Error::workspace("No workspace found. Run `bead init` first."))?;

    // Open database connection
    let db_path = config.database_path();
    let conn = rusqlite::Connection::open(&db_path)
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to open database: {}", e)))?;

    // Create store wrapper
    let mut store = store::SqliteStore::from_conn(conn);

    // If explicit output path provided, use pre-F017 issue-only export
    if let Some(ref output) = opts.output {
        let output_path = config.root.join(output);

        // Validate output path doesn't point into .beads/checkpoint
        let checkpoint_dir = config.root.join(".beads").join("checkpoint");
        if output_path.starts_with(&checkpoint_dir) {
            return Err(Error::validation(
                "Explicit output path cannot be in .beads/checkpoint (use default for forensic checkpoints)",
            ));
        }

        // Validate profile for export
        if opts.profile != "native-v1" {
            return Err(Error::validation(format!(
                "Profile '{}' is not supported for export. Only 'native-v1' is available.",
                opts.profile
            )));
        }

        // Flush issue-only checkpoint for export
        let result = service::flush_checkpoint(&mut store, &output_path)?;

        // Print success message
        eprintln!("Exported issue-only checkpoint:");
        eprintln!("  Path: {}", output_path.display());
        eprintln!("  Issues: {}", result.issue_count);
        eprintln!("  Hash: {}", result.hash);
        eprintln!("  Covered sequence: {}", result.covered_sequence);
        eprintln!("  Export time: {}", result.export_time);
    } else {
        // No explicit output - use F017 forensic checkpoint
        let checkpoint_base = config.root.join(".beads");

        // Determine checkpoint mode (default to monolithic for now)
        let mode = CheckpointMode::Monolithic;

        // Publish forensic checkpoint
        let result = service::publish_forensic_checkpoint(&mut store, mode, &checkpoint_base)?;

        // Print success message
        eprintln!("Flushed forensic checkpoint:");
        eprintln!("  Mode: {}", mode.as_str());
        eprintln!("  Generation: {}", result.generation_id);
        eprintln!("  Issues: {}", result.issue_count);
        eprintln!("  Events: {}", result.event_count);
        eprintln!("  Receipts: {}", result.receipt_count);
        eprintln!("  Total records: {}", result.total_record_count);
        eprintln!("  Root hash: {}", result.root_hash);
        eprintln!("  Covered sequence: {}", result.covered_sequence);
        eprintln!("  Changed paths: {}", result.changed_paths.len());

        for path in &result.changed_paths {
            eprintln!("    {}", path);
        }
    }

    Ok(())
}

fn cmd_sync_import_only(opts: cli::SyncImportOptions) -> Result<()> {
    // Validate that exactly one mode is selected
    let mode = if opts.restore_into_empty {
        Some(cli::ImportMode::RestoreIntoEmpty)
    } else if opts.merge {
        Some(cli::ImportMode::Merge)
    } else {
        None
    };

    let mode = mode.ok_or_else(|| {
        Error::cli_usage("Exactly one of --restore-into-empty or --merge must be specified")
    })?;

    // Validate that actor is provided
    let actor = opts
        .actor
        .ok_or_else(|| Error::cli_usage("--actor is required for import operations"))?;

    // Validate actor format
    if actor.trim().is_empty() {
        return Err(Error::cli_usage("Actor cannot be empty"));
    }
    if actor.len() > 255 {
        return Err(Error::cli_usage("Actor cannot exceed 255 bytes"));
    }
    if actor.contains(char::is_control) {
        return Err(Error::cli_usage("Actor cannot contain control characters"));
    }

    // Discover workspace
    let config = store::WorkspaceConfig::discover()?
        .ok_or_else(|| Error::workspace("No workspace found. Run `bead init` first."))?;

    // Resolve input path relative to workspace root if not absolute
    let input_path = if std::path::Path::new(&opts.input).is_absolute() {
        std::path::PathBuf::from(&opts.input)
    } else {
        config.root.join(&opts.input)
    };

    // Validate input path exists
    if !input_path.exists() {
        return Err(Error::not_found(format!(
            "Input not found: {}",
            input_path.display()
        )));
    }

    // Open database connection
    let db_path = config.database_path();
    let conn = rusqlite::Connection::open(&db_path)
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to open database: {}", e)))?;

    // Create store wrapper
    let mut store = store::SqliteStore::from_conn(conn);

    // Import checkpoint with forensic mode
    let result = service::import_forensic_checkpoint(
        &mut store,
        &input_path,
        &opts.profile,
        mode,
        &actor,
        opts.dry_run,
    )?;

    // Print result
    if opts.dry_run {
        eprintln!("Dry-run forensic import analysis:");
    } else {
        eprintln!("Forensic import completed:");
    }
    eprintln!(
        "  Mode: {}",
        match mode {
            cli::ImportMode::RestoreIntoEmpty => "restore-into-empty",
            cli::ImportMode::Merge => "merge",
        }
    );
    eprintln!("  Profile: {}", result.profile);
    eprintln!("  Input hash: {}", result.input_hash);
    eprintln!(
        "  Issues: {} inserted, {} updated, {} retained, {} conflicted",
        result.inserted, result.updated, result.retained, result.conflicted
    );
    eprintln!("  Events: {} imported", result.events_imported);
    eprintln!("  Receipts: {} processed", result.receipts_processed);
    eprintln!("  Dry run: {}", result.dry_run);
    eprintln!("  Prospective: {}", result.prospective);

    // Print receipt information
    if let Some(receipt_preview) = result.receipt_preview {
        eprintln!("  Receipt preview:");
        eprintln!("    Kind: {}", receipt_preview.kind);
        eprintln!("    Source UUID: {}", receipt_preview.source_store_uuid);
        eprintln!("    Target UUID: {}", receipt_preview.target_store_uuid);
        eprintln!(
            "    Source root hash: {}",
            receipt_preview.source_root_sha256
        );
        eprintln!("    Actor: {}", receipt_preview.actor);
        eprintln!(
            "    Counts: {}",
            serde_json::to_string(&receipt_preview.counts).unwrap_or_default()
        );
        eprintln!("    Result: {}", receipt_preview.result);
    }

    // Print activation information for non-dry-run
    if !result.dry_run {
        if let Some(receipt) = result.receipt {
            eprintln!("  Receipt ID: {}", receipt.receipt_id);
            eprintln!("  Receipt hash: {}", receipt.receipt_sha256);
            if let Some(seq) = result.summary_event_sequence {
                eprintln!("  Summary event sequence: {}", seq);
            }
        }
    }

    Ok(())
}

fn cmd_doctor(opts: cli::DoctorOptions) -> Result<()> {
    // Discover workspace
    let _config = store::WorkspaceConfig::discover()?
        .ok_or_else(|| Error::workspace("No workspace found. Run `bead init` first."))?;

    if opts.repair {
        // Run repairs
        eprintln!("Attempting repairs...");
        let mut store_wrapper = store::SqliteStore::new();

        let repairs = service::run_repairs(&mut store_wrapper)?;

        if repairs.is_empty() {
            eprintln!("No repairs needed.");
        } else {
            for repair in repairs {
                let prefix = match repair.status {
                    service::DiagnosticStatus::Ok => "FIXED",
                    service::DiagnosticStatus::Warning => "WARN",
                    service::DiagnosticStatus::Error => "ERROR",
                };
                eprintln!("{} {}: {}", prefix, repair.name, repair.message);
            }
        }
    } else {
        // Run diagnostics
        let diagnostics = service::run_diagnostics(&store::SqliteStore::new())?;

        for check in diagnostics.checks {
            let prefix = match check.status {
                service::DiagnosticStatus::Ok => "OK",
                service::DiagnosticStatus::Warning => "WARN",
                service::DiagnosticStatus::Error => "ERROR",
            };
            eprintln!("{} {}: {}", prefix, check.name, check.message);
        }

        // Exit with error code if there are errors
        if diagnostics.has_errors {
            return Err(Error::integrity("Diagnostics found errors"));
        }
    }

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
        "labels": labels,
        "revision": issue.revision.unwrap_or(1)
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

fn cmd_capabilities(opts: cli::CapabilitiesOptions) -> Result<()> {
    // Generate capabilities
    let capabilities = service::generate_capabilities(&opts.profile)?;

    // Output as JSON
    let output = serde_json::to_string_pretty(&capabilities)
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to serialize capabilities: {}", e)))?;

    println!("{}", output);

    Ok(())
}

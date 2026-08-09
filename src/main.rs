#![forbid(unsafe_code)]

mod cli;
mod error;
mod model;
mod service;
mod store;

use crate::cli::{Cli, Command};
use crate::error::{Error, Result};
use crate::service::checkpoint::CheckpointMode;
use crate::service::claim::ClaimResult;
use crate::service::policy::{validate_workspace_policy, WorkspaceConfig};
use crate::service::scheduling::SchedulingPolicy;
use crate::store::Store;
use anyhow::Context;
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
        Command::Ref(opts) => cmd_ref(opts),
        Command::Sync(opts) => cmd_sync(opts),
        Command::Doctor(opts) => cmd_doctor(opts),
        Command::Capabilities(opts) => cmd_capabilities(opts),
        Command::Query(opts) => cmd_query(opts),
        Command::Changes(opts) => cmd_changes(opts),
        Command::Data(opts) => cmd_data(opts),
        Command::Why(opts) => cmd_why(opts),
        Command::Recurrence(opts) => cmd_recurrence(opts),
        Command::Policy(opts) => cmd_policy(opts),
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

    // Parse scheduling policy
    let policy = SchedulingPolicy::from_string(&opts.policy)?;

    // Perform claim based on policy
    let (enhanced_result, claim_result) = if matches!(policy, SchedulingPolicy::FifoV1) {
        // Use existing FIFO claim for backward compatibility
        let enhanced = service::claim_issue_with_lease(
            &tx,
            &opts.assignee,
            opts.lease_ttl,
            opts.renew_lease,
            opts.fencing_token,
        )?;

        let claim = ClaimResult {
            bead_id: enhanced.bead_id.clone(),
            assignee: enhanced.assignee.clone(),
        };
        (enhanced, claim)
    } else {
        // Use intelligent scheduling for R019 policies
        let claim = service::claim_issue_with_policy(
            &tx,
            &opts.assignee,
            &policy,
            None, // model
            None, // harness
            None, // harness_version
        )?;

        // Create enhanced result without lease for intelligent policies
        let enhanced = service::EnhancedClaimResult {
            bead_id: claim.bead_id.clone(),
            assignee: claim.assignee.clone(),
            lease: None, // Intelligent policies don't include lease info in basic result
        };
        (enhanced, claim)
    };

    // Get decision trace if requested (backward compatibility with R001)
    let trace = if opts.why {
        let (_, trace_data) =
            service::claim_issue_with_trace(&tx, &opts.assignee, None, None, None, true)?;
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
            // When --why is set, output enriched result with decision trace
            serde_json::to_string(&serde_json::json!({
                "claim_result": enhanced_result,
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
        if let Some(bead_id) = &claim_result.bead_id {
            println!("Claimed: {}", bead_id);
            println!("Assignee: {}", claim_result.assignee);
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

    // Handle dry-run mode
    if opts.dry_run {
        let result = service::update_issue_dryrun(
            &conn,
            &opts.id,
            opts.status.as_deref(),
            opts.assignee.as_deref(),
            opts.clear_assignee,
            opts.notes.as_deref(),
        )?;

        // Output JSON result
        let json = serde_json::to_string_pretty(&result)?;
        println!("{}", json);
        return Ok(());
    }

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

    // Handle dry-run mode
    if opts.dry_run {
        let result = service::release_issue_dryrun(&conn, &opts.id)?;

        // Output JSON result
        let json = serde_json::to_string_pretty(&result)?;
        println!("{}", json);
        return Ok(());
    }

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

    // Handle dry-run mode
    if opts.dry_run {
        let result = service::close_issue_dryrun(&conn, &opts.id, &opts.reason)?;

        // Output JSON result
        let json = serde_json::to_string_pretty(&result)?;
        println!("{}", json);
        return Ok(());
    }

    // Close the issue
    let id = service::close_issue(
        &conn,
        &opts.id,
        &opts.reason,
        opts.if_revision,
        opts.fencing_token,
    )?;

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

    // Handle dry-run mode
    if opts.dry_run {
        let result = service::reopen_issue_dryrun(&conn, &opts.id)?;

        // Output JSON result
        let json = serde_json::to_string_pretty(&result)?;
        println!("{}", json);
        return Ok(());
    }

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

    // Handle dry-run mode
    if opts.dry_run {
        let result = service::add_dependency_dryrun(
            &conn,
            &opts.blocked,
            &opts.blocker,
            &opts.kind,
            opts.condition.as_deref(),
        )?;

        // Output JSON result
        let json = serde_json::to_string_pretty(&result)?;
        println!("{}", json);
        return Ok(());
    }

    // Create store wrapper
    let mut store = store::SqliteStore::from_conn(conn);

    // Parse condition if provided
    let condition = if let Some(ref condition_json) = opts.condition {
        let cond = service::ConditionExpr::from_json(condition_json)
            .map_err(|e| Error::validation(format!("Invalid condition JSON: {}", e)))?;
        Some(cond)
    } else {
        None
    };

    // Add the dependency
    service::add_dependency(
        &mut store,
        &opts.blocked,
        &opts.blocker,
        &opts.kind,
        condition.as_ref(),
    )?;

    // Print success message
    if opts.condition.is_some() {
        println!(
            "Added conditional dependency: {} {} {} (when condition met)",
            opts.blocked,
            if opts.kind == "blocks" {
                "blocked by"
            } else {
                "related to"
            },
            opts.blocker
        );
    } else {
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
    }

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

    // Handle dry-run mode
    if opts.dry_run {
        let result = service::remove_dependency_dryrun(
            &conn,
            &opts.blocked,
            &opts.blocker,
            opts.kind.as_deref(),
        )?;

        // Output JSON result
        let json = serde_json::to_string_pretty(&result)?;
        println!("{}", json);
        return Ok(());
    }

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

fn cmd_ref(cmd: cli::RefCommand) -> Result<()> {
    match cmd {
        cli::RefCommand::Add(opts) => cmd_ref_add(opts),
        cli::RefCommand::Remove(opts) => cmd_ref_remove(opts),
        cli::RefCommand::List(opts) => cmd_ref_list(opts),
        cli::RefCommand::Find(opts) => cmd_ref_find(opts),
    }
}

fn cmd_ref_add(opts: cli::RefAddOptions) -> Result<()> {
    // Discover workspace
    let config = store::WorkspaceConfig::discover()?
        .ok_or_else(|| Error::workspace("No workspace found. Run `bead init` first."))?;

    // Open database connection
    let db_path = config.database_path();
    let conn = rusqlite::Connection::open(&db_path)
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to open database: {}", e)))?;

    // Create store wrapper
    let mut store = store::SqliteStore::from_conn(conn);

    // Add the external reference
    service::add_external_reference(
        &mut store,
        &opts.id,
        &opts.namespace,
        &opts.key,
        &opts.value,
    )?;

    // Print success message
    println!(
        "Added reference: {} -> {}/{}/{}",
        opts.id, opts.namespace, opts.key, opts.value
    );

    Ok(())
}

fn cmd_ref_remove(opts: cli::RefRemoveOptions) -> Result<()> {
    // Discover workspace
    let config = store::WorkspaceConfig::discover()?
        .ok_or_else(|| Error::workspace("No workspace found. Run `bead init` first."))?;

    // Open database connection
    let db_path = config.database_path();
    let conn = rusqlite::Connection::open(&db_path)
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to open database: {}", e)))?;

    // Create store wrapper
    let mut store = store::SqliteStore::from_conn(conn);

    // Remove the external reference
    service::remove_external_reference(&mut store, &opts.id, &opts.namespace, &opts.key)?;

    // Print success message
    println!(
        "Removed reference: {} -> {}/{}",
        opts.id, opts.namespace, opts.key
    );

    Ok(())
}

fn cmd_ref_list(opts: cli::RefListOptions) -> Result<()> {
    // Discover workspace
    let config = store::WorkspaceConfig::discover()?
        .ok_or_else(|| Error::workspace("No workspace found. Run `bead init` first."))?;

    // Open database connection
    let db_path = config.database_path();
    let conn = rusqlite::Connection::open(&db_path)
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to open database: {}", e)))?;

    // Create store wrapper
    let mut store = store::SqliteStore::from_conn(conn);

    // List external references
    let references = service::list_external_references(&mut store, &opts.id)?;

    if opts.json {
        // Output JSON format
        for reference in references {
            let json = serde_json::to_string(&reference).unwrap();
            println!("{}", json);
        }
    } else {
        // Output human-readable format
        if references.is_empty() {
            println!("No external references found for {}", opts.id);
        } else {
            println!("External references for {}:", opts.id);
            for reference in references {
                println!(
                    "  {}/{}: {}",
                    reference.namespace, reference.key, reference.value
                );
            }
        }
    }

    Ok(())
}

fn cmd_ref_find(opts: cli::RefFindOptions) -> Result<()> {
    // Discover workspace
    let config = store::WorkspaceConfig::discover()?
        .ok_or_else(|| Error::workspace("No workspace found. Run `bead init` first."))?;

    // Open database connection
    let db_path = config.database_path();
    let conn = rusqlite::Connection::open(&db_path)
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to open database: {}", e)))?;

    // Create store wrapper
    let mut store = store::SqliteStore::from_conn(conn);

    // Find issues by reference
    let issue_ids = service::find_issues_by_reference(&mut store, &opts.namespace, &opts.value)?;

    if opts.json {
        // Output JSON format
        let json = serde_json::to_string(&issue_ids).unwrap();
        println!("{}", json);
    } else {
        // Output human-readable format
        if issue_ids.is_empty() {
            println!(
                "No issues found with reference {}/{}",
                opts.namespace, opts.value
            );
        } else {
            println!("Issues with reference {}/{}:", opts.namespace, opts.value);
            for issue_id in issue_ids {
                println!("  {}", issue_id);
            }
        }
    }

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

    if opts.rehearse {
        // Run disposable recovery rehearsal
        eprintln!("Running disposable recovery rehearsal (R015)...");
        eprintln!(
            "This will create a temporary workspace, run diagnostics, and verify recovery.\n"
        );

        let report = service::run_recovery_rehearsal().context("Recovery rehearsal failed")?;

        // Determine overall success
        let success = report.diagnostics.overall_status != "FAILED"
            && report.semantic_comparison.overall_equivalence;

        if !success {
            return Err(Error::integrity("Recovery rehearsal found issues"));
        }

        eprintln!("\n✅ Recovery rehearsal completed successfully!");
        return Ok(());
    }

    // Parse scopes
    let scopes = if let Some(scope_values) = opts.scope {
        let mut parsed_scopes = Vec::new();
        for scope_str in scope_values {
            match service::doctor::DiagnosticScope::from_str(&scope_str) {
                Some(scope) => parsed_scopes.push(scope),
                None => {
                    return Err(Error::validation(format!(
                        "Invalid scope: {}. Valid scopes: {}",
                        scope_str,
                        service::doctor::DiagnosticScope::all_scopes().join(", ")
                    )))
                }
            }
        }
        parsed_scopes
    } else {
        vec![service::doctor::DiagnosticScope::All] // Default to all scopes
    };

    if opts.repair {
        // Run repairs - maintain narrow allowlist (only temp files)
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

        // Note: Repairs stay narrowly allowlisted and never rewrite user semantic data
        eprintln!("Repairs completed. Only operation-owned temporary files are removed.");
    } else {
        // Run diagnostics with specified scopes
        let diagnostics = if scopes.len() == 1 && scopes[0] == service::doctor::DiagnosticScope::All
        {
            service::run_diagnostics(&store::SqliteStore::new())?
        } else {
            service::run_diagnostics_with_scopes(&store::SqliteStore::new(), &scopes)?
        };

        if opts.json {
            // Output stable JSON diagnostics
            let json_output = serde_json::to_string_pretty(&diagnostics).map_err(|e| {
                Error::Internal(anyhow::anyhow!("Failed to serialize diagnostics: {}", e))
            })?;
            println!("{}", json_output);
        } else {
            // Human-readable output
            eprintln!(
                "Running diagnostics with scopes: {}",
                scopes
                    .iter()
                    .map(|s| format!("{:?}", s))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            eprintln!();

            for check in &diagnostics.checks {
                let prefix = match check.status {
                    service::DiagnosticStatus::Ok => "OK",
                    service::DiagnosticStatus::Warning => "WARN",
                    service::DiagnosticStatus::Error => "ERROR",
                };
                eprintln!("{} {}: {}", prefix, check.name, check.message);
            }

            eprintln!();
            eprintln!("Scopes checked: {}", diagnostics.scopes_checked.join(", "));
            eprintln!("Timestamp: {}", diagnostics.timestamp);

            if diagnostics.has_warnings {
                eprintln!(
                    "Warnings found: {}",
                    diagnostics
                        .checks
                        .iter()
                        .filter(|c| c.status == service::DiagnosticStatus::Warning)
                        .count()
                );
            }
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

fn cmd_query(opts: cli::QueryOptions) -> Result<()> {
    // Discover workspace
    let config = store::WorkspaceConfig::discover()?
        .ok_or_else(|| Error::cli_usage("No bead workspace found. Run 'bead init' first."))?;

    // Open database connection
    let db_path = config.database_path();
    let conn = rusqlite::Connection::open(&db_path)
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to open database: {}", e)))?;

    // Handle list views
    if opts.list_views {
        return cmd_list_views(&conn);
    }

    // Handle delete view
    if let Some(view_name) = opts.delete_view {
        return cmd_delete_view(&conn, &view_name);
    }

    // Handle execute saved view
    if let Some(view_name) = opts.view {
        return cmd_execute_view(&conn, &view_name, opts.output_json);
    }

    // Load query from file or inline JSON
    let query_json = if let Some(file_path) = opts.file {
        std::fs::read_to_string(&file_path)
            .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to read query file: {}", e)))?
    } else if let Some(inline_json) = opts.json {
        inline_json
    } else {
        return Err(Error::cli_usage(
            "Query specification required. Use --file <path> or --json '<query>'",
        ));
    };

    // Parse and validate query
    let query = service::parse_query(&query_json)?;

    // Execute query
    let issues = service::execute_query(&conn, &query)?;

    // Apply projection if specified
    let results: Vec<serde_json::Value> = if let Some(ref projection) = query.projection {
        issues
            .iter()
            .map(|issue| service::project_issue(issue, projection).unwrap())
            .collect()
    } else {
        issues
            .iter()
            .map(|issue| serde_json::to_value(issue).unwrap())
            .collect()
    };

    // Save as view if requested
    if let Some(view_name) = opts.save_as {
        cmd_save_view(&conn, &view_name, &query, &query_json)?;
        println!("Saved view: {}", view_name);
    }

    // Output results
    if opts.output_json {
        let output = serde_json::to_string_pretty(&results)
            .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to serialize results: {}", e)))?;
        println!("{}", output);
    } else {
        // Human-readable output
        println!("Found {} issues", results.len());
        for result in &results {
            println!("  - {}", result);
        }
    }

    Ok(())
}

fn cmd_list_views(conn: &rusqlite::Connection) -> Result<()> {
    let views = service::list_views(conn)?;

    if views.is_empty() {
        println!("No saved views found.");
        return Ok(());
    }

    println!("Saved views ({}):", views.len());
    for view in views {
        println!("  - {} ({})", view.name, view.description);
        println!("    Created: {}", view.created_at);
        println!("    Updated: {}", view.updated_at);
    }

    Ok(())
}

fn cmd_delete_view(conn: &rusqlite::Connection, view_name: &str) -> Result<()> {
    service::delete_view(conn, view_name)?;
    println!("Deleted view: {}", view_name);
    Ok(())
}

fn cmd_execute_view(conn: &rusqlite::Connection, view_name: &str, output_json: bool) -> Result<()> {
    let view = service::get_view(conn, view_name)?;
    let query = service::parse_query(&view.query_json)?;

    // Execute query
    let issues = service::execute_query(conn, &query)?;

    // Apply projection if specified
    let results: Vec<serde_json::Value> = if let Some(ref projection) = query.projection {
        issues
            .iter()
            .map(|issue| service::project_issue(issue, projection).unwrap())
            .collect()
    } else {
        issues
            .iter()
            .map(|issue| serde_json::to_value(issue).unwrap())
            .collect()
    };

    // Output results
    if output_json {
        let output = serde_json::to_string_pretty(&results)
            .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to serialize results: {}", e)))?;
        println!("{}", output);
    } else {
        // Human-readable output
        println!("Found {} issues", results.len());
        for result in &results {
            println!("  - {}", result);
        }
    }

    Ok(())
}

fn cmd_save_view(
    conn: &rusqlite::Connection,
    view_name: &str,
    query: &service::Query,
    query_json: &str,
) -> Result<()> {
    let description = format!("Query with {} predicates", query.predicates.len());

    service::save_view(conn, view_name, &description, query_json)?;

    Ok(())
}

fn cmd_changes(opts: cli::ChangesOptions) -> Result<()> {
    // Discover workspace
    let config = store::WorkspaceConfig::discover()?
        .ok_or_else(|| Error::workspace("No workspace found. Run `bead init` first."))?;

    // Open database connection
    let db_path = config.database_path();
    let conn = rusqlite::Connection::open(&db_path)
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to open database: {}", e)))?;

    // Handle different modes
    if opts.latest {
        // Get latest cursor position
        let snapshot = service::get_snapshot_identity(&conn)?;
        if opts.json {
            let output = serde_json::to_string_pretty(&snapshot).map_err(|e| {
                Error::Internal(anyhow::anyhow!("Failed to serialize snapshot: {}", e))
            })?;
            println!("{}", output);
        } else {
            println!("Latest cursor: {}", snapshot.max_sequence);
            println!("Workspace UUID: {}", snapshot.workspace_uuid);
            println!("Checksum: {}", snapshot.checksum);
            println!("Timestamp: {}", snapshot.timestamp);
        }
        return Ok(());
    }

    if opts.snapshot {
        // Get current snapshot identity
        let snapshot = service::get_snapshot_identity(&conn)?;
        if opts.json {
            let output = serde_json::to_string_pretty(&snapshot).map_err(|e| {
                Error::Internal(anyhow::anyhow!("Failed to serialize snapshot: {}", e))
            })?;
            println!("{}", output);
        } else {
            println!("Current snapshot identity:");
            println!("  Workspace UUID: {}", snapshot.workspace_uuid);
            println!("  Max sequence: {}", snapshot.max_sequence);
            println!("  Checksum: {}", snapshot.checksum);
            println!("  Timestamp: {}", snapshot.timestamp);
        }
        return Ok(());
    }

    if let Some(cursor_str) = opts.validate {
        // Validate cursor and check for gaps
        let cursor = service::Cursor::from_string(&cursor_str)?;
        let is_valid = service::validate_cursor(&conn, &cursor)?;

        if opts.json {
            let result = serde_json::json!({
                "cursor": cursor_str,
                "valid": is_valid,
            });
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        } else {
            if is_valid {
                println!("Cursor '{}' is valid - no gaps detected", cursor_str);
            } else {
                println!(
                    "Cursor '{}' is INVALID - gaps detected, resynchronization required",
                    cursor_str
                );
            }

            // Show gap details if available
            if let Some(gap_info) = service::get_gap_info(&conn, &cursor)? {
                println!("  Gap details:");
                println!("    Expected sequence: {}", gap_info.expected);
                println!("    Actual first sequence: {}", gap_info.actual);
                println!("    Gap size: {} events", gap_info.gap_size);
            }
        }

        return if is_valid {
            Ok(())
        } else {
            Err(Error::validation("Gap detected in event sequence"))
        };
    }

    if opts.since.is_none() {
        // Default: show current snapshot info
        let snapshot = service::get_snapshot_identity(&conn)?;
        if opts.json {
            let output = serde_json::to_string_pretty(&snapshot).map_err(|e| {
                Error::Internal(anyhow::anyhow!("Failed to serialize snapshot: {}", e))
            })?;
            println!("{}", output);
        } else {
            println!("Current workspace state:");
            println!("  Workspace UUID: {}", snapshot.workspace_uuid);
            println!("  Max sequence: {}", snapshot.max_sequence);
            println!("  Checksum: {}", snapshot.checksum);
            println!("  Timestamp: {}", snapshot.timestamp);
            println!();
            println!("Use --since <cursor> to get changes since a specific position");
            println!("Use --latest to get the latest cursor position");
            println!("Use --validate <cursor> to check for gaps");
        }
        return Ok(());
    }

    // Get changes since cursor
    let cursor_str = opts.since.unwrap();
    let cursor = service::Cursor::from_string(&cursor_str)?;
    let change_feed = service::get_changes_since(&conn, &cursor)?;

    if opts.json {
        let output = serde_json::to_string_pretty(&change_feed).map_err(|e| {
            Error::Internal(anyhow::anyhow!("Failed to serialize change feed: {}", e))
        })?;
        println!("{}", output);
    } else {
        println!("Change feed since cursor position {}:", cursor.sequence);
        println!(
            "  Snapshot: {} (seq: {})",
            change_feed.snapshot.workspace_uuid, change_feed.snapshot.max_sequence
        );
        println!("  Total available: {} events", change_feed.total_available);
        println!("  Returned: {} events", change_feed.returned_count);
        println!("  Has gaps: {}", change_feed.has_gaps);
        println!();

        if change_feed.has_gaps {
            println!("WARNING: Gaps detected in event sequence!");
            println!("Consumers should resynchronize from full checkpoint.");
            println!();

            if let Some(gap_info) = service::get_gap_info(&conn, &cursor)? {
                println!("Gap details:");
                println!("  Expected sequence: {}", gap_info.expected);
                println!("  Actual first sequence: {}", gap_info.actual);
                println!("  Gap size: {} events", gap_info.gap_size);
                println!();
            }
        }

        if change_feed.mutations.is_empty() {
            println!("No new mutations since cursor position.");
        } else {
            println!("Mutations:");
            for mutation in &change_feed.mutations {
                println!(
                    "  [{}] {} - {}",
                    mutation.sequence,
                    mutation.kind,
                    mutation
                        .issue_id
                        .as_ref()
                        .unwrap_or(&"(workspace)".to_string())
                );
                println!("    Time: {}", mutation.time);
                if let Some(actor) = &mutation.actor {
                    println!("    Actor: {}", actor);
                }
            }
        }

        println!();
        println!("Next cursor: {}", change_feed.snapshot.max_sequence);
    }

    Ok(())
}

fn cmd_data(opts: cli::DataCommand) -> Result<()> {
    match opts {
        cli::DataCommand::Set(data_opts) => cmd_data_set(data_opts),
        cli::DataCommand::Get(data_opts) => cmd_data_get(data_opts),
        cli::DataCommand::List(data_opts) => cmd_data_list(data_opts),
        cli::DataCommand::Remove(data_opts) => cmd_data_remove(data_opts),
    }
}

fn cmd_data_set(opts: cli::DataSetOptions) -> Result<()> {
    let config = store::WorkspaceConfig::discover()?
        .ok_or_else(|| Error::workspace("No workspace found. Run `bead init` first."))?;

    let db_path = config.database_path();
    let mut store = store::SqliteStore::with_path(&db_path)
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to open database: {}", e)))?;

    // Parse JSON value
    let value: serde_json::Value = serde_json::from_str(&opts.value)
        .map_err(|e| Error::validation(format!("Invalid JSON value: {}", e)))?;

    // Set the data
    service::set_data(
        &mut store,
        &opts.id,
        &opts.namespace,
        &opts.schema_ref,
        &value,
    )?;

    eprintln!("Set structured data:");
    eprintln!("  Issue: {}", opts.id);
    eprintln!("  Namespace: {}", opts.namespace);
    eprintln!("  Schema: {}", opts.schema_ref);

    Ok(())
}

fn cmd_data_get(opts: cli::DataGetOptions) -> Result<()> {
    let config = store::WorkspaceConfig::discover()?
        .ok_or_else(|| Error::workspace("No workspace found. Run `bead init` first."))?;

    let db_path = config.database_path();
    let mut store = store::SqliteStore::with_path(&db_path)
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to open database: {}", e)))?;

    let result = service::get_data(&mut store, &opts.id, &opts.namespace)?;

    match result {
        Some((schema_ref, value)) => {
            if opts.json {
                let output = serde_json::json!({
                    "issue_id": opts.id,
                    "namespace": opts.namespace,
                    "schema_ref": schema_ref,
                    "value": value
                });
                println!("{}", serde_json::to_string(&output).unwrap());
            } else {
                eprintln!("Structured data:");
                eprintln!("  Issue: {}", opts.id);
                eprintln!("  Namespace: {}", opts.namespace);
                eprintln!("  Schema: {}", schema_ref);
                eprintln!("  Value: {}", serde_json::to_string_pretty(&value).unwrap());
            }
        }
        None => {
            return Err(Error::not_found(format!(
                "No structured data found for namespace '{}' on issue '{}'",
                opts.namespace, opts.id
            )));
        }
    }

    Ok(())
}

fn cmd_data_list(opts: cli::DataListOptions) -> Result<()> {
    let config = store::WorkspaceConfig::discover()?
        .ok_or_else(|| Error::workspace("No workspace found. Run `bead init` first."))?;

    let db_path = config.database_path();
    let mut store = store::SqliteStore::with_path(&db_path)
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to open database: {}", e)))?;

    let namespaces = service::list_data(&mut store, &opts.id)?;

    if opts.json {
        let output: Vec<serde_json::Value> = namespaces
            .into_iter()
            .map(|(namespace, schema_ref)| {
                serde_json::json!({
                    "namespace": namespace,
                    "schema_ref": schema_ref
                })
            })
            .collect();
        println!("{}", serde_json::to_string(&output).unwrap());
    } else {
        if namespaces.is_empty() {
            eprintln!("No structured data found for issue '{}'", opts.id);
        } else {
            eprintln!("Structured data for issue '{}':", opts.id);
            for (namespace, schema_ref) in namespaces {
                eprintln!("  {} (schema: {})", namespace, schema_ref);
            }
        }
    }

    Ok(())
}

fn cmd_data_remove(opts: cli::DataRemoveOptions) -> Result<()> {
    let config = store::WorkspaceConfig::discover()?
        .ok_or_else(|| Error::workspace("No workspace found. Run `bead init` first."))?;

    let db_path = config.database_path();
    let mut store = store::SqliteStore::with_path(&db_path)
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to open database: {}", e)))?;

    service::remove_data(&mut store, &opts.id, &opts.namespace)?;

    eprintln!("Removed structured data:");
    eprintln!("  Issue: {}", opts.id);
    eprintln!("  Namespace: {}", opts.namespace);

    Ok(())
}

fn cmd_recurrence(opts: cli::RecurrenceCommand) -> Result<()> {
    match opts {
        cli::RecurrenceCommand::Create(opts) => cmd_recurrence_create(opts),
        cli::RecurrenceCommand::Show(opts) => cmd_recurrence_show(opts),
        cli::RecurrenceCommand::List(opts) => cmd_recurrence_list(opts),
        cli::RecurrenceCommand::Delete(opts) => cmd_recurrence_delete(opts),
        cli::RecurrenceCommand::Materialize(opts) => cmd_recurrence_materialize(opts),
        cli::RecurrenceCommand::History(opts) => cmd_recurrence_history(opts),
    }
}

fn cmd_recurrence_create(opts: cli::RecurrenceCreateOptions) -> Result<()> {
    let config = store::WorkspaceConfig::discover()?
        .ok_or_else(|| Error::workspace("No workspace found. Run `bead init` first."))?;

    let db_path = config.database_path();
    let mut store = store::SqliteStore::with_path(&db_path)
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to open database: {}", e)))?;

    // Parse labels if provided
    let labels = if let Some(ref labels_str) = opts.labels {
        if labels_str.is_empty() {
            None
        } else {
            Some(
                labels_str
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect(),
            )
        }
    } else {
        None
    };

    let request = crate::model::recurrence::CreateTemplateRequest {
        id: opts.id.clone(),
        title: opts.title.clone(),
        description: opts.description.clone(),
        base_title_template: opts.base_title_template.clone(),
        base_description: opts.base_description.clone(),
        priority: opts.priority,
        issue_type: opts.issue_type,
        labels,
    };

    let template = service::create_template(store.conn_mut(), request)?;

    eprintln!("Created recurrence template:");
    eprintln!("  ID: {}", template.id);
    eprintln!("  Title: {}", template.title);
    eprintln!("  Title Template: {}", template.base_title_template);
    eprintln!("  Priority: {}", template.priority);
    eprintln!("  Issue Type: {}", template.issue_type);
    if let Some(ref description) = template.description {
        eprintln!("  Description: {}", description);
    }
    if let Ok(labels_vec) = template.get_labels() {
        if !labels_vec.is_empty() {
            eprintln!("  Labels: {}", labels_vec.join(", "));
        }
    }

    Ok(())
}

fn cmd_recurrence_show(opts: cli::RecurrenceShowOptions) -> Result<()> {
    let config = store::WorkspaceConfig::discover()?
        .ok_or_else(|| Error::workspace("No workspace found. Run `bead init` first."))?;

    let db_path = config.database_path();
    let mut store = store::SqliteStore::with_path(&db_path)
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to open database: {}", e)))?;

    let template = service::get_template(store.conn(), &opts.id)?;
    let history = service::get_materialization_history(store.conn(), &opts.id)?;

    if opts.json {
        let output = serde_json::json!({
            "template": template,
            "history": history
        });
        println!("{}", output);
    } else {
        eprintln!("Recurrence Template:");
        eprintln!("  ID: {}", template.id);
        eprintln!("  Title: {}", template.title);
        if let Some(ref description) = template.description {
            eprintln!("  Description: {}", description);
        }
        eprintln!("  Title Template: {}", template.base_title_template);
        if let Some(ref base_description) = template.base_description {
            eprintln!("  Description Template: {}", base_description);
        }
        eprintln!("  Priority: {}", template.priority);
        eprintln!("  Issue Type: {}", template.issue_type);
        if let Ok(labels_vec) = template.get_labels() {
            if !labels_vec.is_empty() {
                eprintln!("  Labels: {}", labels_vec.join(", "));
            }
        }
        eprintln!("  Created At: {}", template.created_at);

        eprintln!("\nMaterialization History:");
        if history.is_empty() {
            eprintln!("  No occurrences materialized yet");
        } else {
            for mat in &history {
                eprintln!(
                    "  Sequence {}: Issue {} (materialized {})",
                    mat.series_sequence, mat.occurrence_id, mat.materialized_at
                );
                if let Some(ref actor) = mat.actor {
                    eprintln!("    Actor: {}", actor);
                }
            }
        }
    }

    Ok(())
}

fn cmd_recurrence_list(opts: cli::RecurrenceListOptions) -> Result<()> {
    let config = store::WorkspaceConfig::discover()?
        .ok_or_else(|| Error::workspace("No workspace found. Run `bead init` first."))?;

    let db_path = config.database_path();
    let mut store = store::SqliteStore::with_path(&db_path)
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to open database: {}", e)))?;

    let templates = service::list_templates(store.conn())?;

    if opts.json {
        let output = serde_json::to_string_pretty(&templates).unwrap();
        println!("{}", output);
    } else {
        if templates.is_empty() {
            eprintln!("No recurrence templates found");
        } else {
            eprintln!("Recurrence Templates:");
            for template in &templates {
                let history = service::get_materialization_history(store.conn(), &template.id)
                    .unwrap_or_default();
                let count = history.len();

                eprintln!(
                    "  {} - {} ({} occurrence(s))",
                    template.id, template.title, count
                );
            }
        }
    }

    Ok(())
}

fn cmd_recurrence_delete(opts: cli::RecurrenceDeleteOptions) -> Result<()> {
    let config = store::WorkspaceConfig::discover()?
        .ok_or_else(|| Error::workspace("No workspace found. Run `bead init` first."))?;

    let db_path = config.database_path();
    let mut store = store::SqliteStore::with_path(&db_path)
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to open database: {}", e)))?;

    service::delete_template(store.conn_mut(), &opts.id)?;

    eprintln!("Deleted recurrence template: {}", opts.id);

    Ok(())
}

fn cmd_recurrence_materialize(opts: cli::RecurrenceMaterializeOptions) -> Result<()> {
    let config = store::WorkspaceConfig::discover()?
        .ok_or_else(|| Error::workspace("No workspace found. Run `bead init` first."))?;

    let db_path = config.database_path();
    let mut store = store::SqliteStore::with_path(&db_path)
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to open database: {}", e)))?;

    let (_issue_id, materialization) =
        service::materialize_next_occurrence(store.conn_mut(), &opts.id, opts.actor.as_deref())?;

    eprintln!("Materialized next occurrence:");
    eprintln!("  Template: {}", materialization.template_id);
    eprintln!("  Sequence: {}", materialization.series_sequence);
    eprintln!("  Issue ID: {}", materialization.occurrence_id);
    if let Some(ref actor) = materialization.actor {
        eprintln!("  Actor: {}", actor);
    }
    eprintln!("  Materialized At: {}", materialization.materialized_at);

    Ok(())
}

fn cmd_why(opts: cli::WhyOptions) -> Result<()> {
    // Discover workspace
    let config = store::WorkspaceConfig::discover()?
        .ok_or_else(|| Error::workspace("No workspace found. Run `bead init` first."))?;

    let db_path = config.database_path();
    let conn = rusqlite::Connection::open(&db_path)
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to open database: {}", e)))?;

    // Generate comprehensive why explanation
    let explanation = service::explain_why(&conn, &opts.id)?;

    if opts.json {
        // Output machine-readable JSON
        let output = serde_json::to_string_pretty(&explanation).map_err(|e| {
            Error::Internal(anyhow::anyhow!(
                "Failed to serialize why explanation: {}",
                e
            ))
        })?;
        println!("{}", output);
    } else {
        // Human-readable explanation
        print_human_readable_why(&explanation);
    }

    Ok(())
}

fn print_human_readable_why(why: &service::WhyExplanation) {
    println!("Why explanation for: {}", why.issue_id);

    // Status section
    println!("\n=== Status ===");
    println!("Base Status: {}", why.base_status);
    println!("Effective Status: {}", why.effective_status);
    println!("Ready: {}", if why.is_ready { "Yes" } else { "No" });
    println!(
        "Assigned: {}",
        why.assignee.as_ref().unwrap_or(&"None".to_string())
    );
    println!("Manual Blocked: {}", why.manual_blocked);
    println!("Priority: P{}", why.priority);
    println!("Issue Type: {}", why.issue_type);

    // Timing information
    println!("\n=== Timing ===");
    println!("Created: {}", why.created_at);
    println!("Updated: {}", why.updated_at);
    if let Some(closed_at) = &why.closed_at {
        println!("Closed: {}", closed_at);
    }

    // Blockers section
    println!("\n=== Blockers ===");
    if why.blockers.active_blocker_count > 0 {
        println!("Active Blockers: {}", why.blockers.active_blocker_count);
        for blocker in &why.blockers.active_blockers {
            println!("  - {} ({})", blocker.issue_id, blocker.title);
            println!("    Status: {}", blocker.status);
            if blocker.is_conditional {
                println!(
                    "    Conditional: {}",
                    blocker
                        .condition_explanation
                        .as_ref()
                        .unwrap_or(&"No condition".to_string())
                );
            }
        }
    } else if why.blockers.total_dependency_count > 0 {
        println!(
            "No active blockers (has {} inactive dependencies)",
            why.blockers.total_dependency_count
        );
    } else {
        println!("No dependencies");
    }

    // Ranking factors section
    println!("\n=== Ranking Factors ===");
    println!(
        "Declared Priority: P{}",
        why.ranking_factors.declared_priority
    );
    println!(
        "Effective Priority: P{}",
        why.ranking_factors.effective_priority
    );
    if let Some(age_seconds) = why.ranking_factors.ready_age_seconds {
        let age_minutes = age_seconds / 60;
        println!(
            "Ready Age: {} seconds ({} minutes)",
            age_seconds, age_minutes
        );
    } else {
        println!("Ready Age: Not currently ready");
    }
    println!(
        "Attempt Tier: {}",
        tier_name(why.ranking_factors.attempt_tier)
    );
    println!(
        "Consecutive Failures: {}",
        why.ranking_factors.consecutive_failures
    );
    if let Some(last_claim) = why.ranking_factors.last_claim_sequence {
        println!("Last Claim Sequence: {}", last_claim);
    } else {
        println!("Last Claim Sequence: Never claimed");
    }

    if let Some(impact) = &why.ranking_factors.graph_impact {
        println!("\n=== Graph Impact ===");
        println!("Immediate Unlock Count: {}", impact.immediate_unlock_count);
        println!("Downstream Reach: {}", impact.downstream_reach);
        println!(
            "Critical Path Reduction: {}",
            impact.critical_path_reduction
        );
        if !impact.unlocked_priorities.is_empty() {
            println!("Unlocked Priorities: {:?}", impact.unlocked_priorities);
        }
    }

    // Legal operations section
    println!("\n=== Legal Operations ===");
    println!("Current State: {}", why.base_status);
    for operation in &why.legal_operations {
        let status = if operation.is_valid { "✓" } else { "✗" };
        println!(
            "{} {} - {}",
            status,
            operation.operation,
            operation
                .command_example
                .as_ref()
                .unwrap_or(&"".to_string())
        );
        if let Some(reason) = &operation.invalid_reason {
            println!("    Reason: {}", reason);
        }
    }

    // Reason codes section
    if !why.reasons.is_empty() {
        println!("\n=== Reason Codes ===");
        for reason in &why.reasons {
            println!("  - {:?}", reason);
        }
    }
}

fn cmd_policy(opts: cli::PolicyCommand) -> Result<()> {
    match opts {
        cli::PolicyCommand::Check(check_opts) => cmd_policy_check(check_opts),
    }
}

fn cmd_policy_check(opts: cli::PolicyCheckOptions) -> Result<()> {
    // Discover workspace
    let config = store::WorkspaceConfig::discover()?
        .ok_or_else(|| Error::workspace("No workspace found. Run `bead init` first."))?;

    let db_path = config.database_path();
    let conn = rusqlite::Connection::open(&db_path)
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to open database: {}", e)))?;

    // Build workspace configuration for validation
    let workspace_config = build_workspace_config(&conn, &config, &opts)?;

    // Run policy validation
    let diagnostics = validate_workspace_policy(&workspace_config)?;

    // Output results
    if opts.format == "json" {
        let output = serde_json::to_string_pretty(&diagnostics).map_err(|e| {
            Error::Internal(anyhow::anyhow!(
                "Failed to serialize policy diagnostics: {}",
                e
            ))
        })?;
        println!("{}", output);
    } else {
        print_human_readable_policy_diagnostics(&diagnostics);
    }

    Ok(())
}

/// Build workspace configuration for policy validation
fn build_workspace_config(
    _conn: &rusqlite::Connection,
    _config: &store::WorkspaceConfig,
    opts: &cli::PolicyCheckOptions,
) -> Result<WorkspaceConfig> {
    // Use provided policy or default to fifo-v1
    let scheduling_policy = opts.policy.clone().unwrap_or_else(|| "fifo-v1".to_string());

    // Use provided version or default to v1
    let scheduling_policy_version = opts
        .policy_version
        .clone()
        .unwrap_or_else(|| "v1".to_string());

    let config_schema_version = "v1".to_string();

    // Build empty parameters map (could be populated from database in future)
    let scheduling_params = std::collections::HashMap::new();

    Ok(WorkspaceConfig {
        scheduling_policy,
        scheduling_policy_version,
        config_schema_version,
        scheduling_params,
    })
}

/// Print human-readable policy diagnostics
fn print_human_readable_policy_diagnostics(diagnostics: &service::PolicyDiagnostics) {
    println!("Workspace Policy Validation");
    println!("\nConfiguration:");
    println!("  Schema Version: {}", diagnostics.config_schema_version);
    println!("  Policy Version: {}", diagnostics.policy_version);

    println!("\nOverall Status: {}", format_status(&diagnostics.status));

    if !diagnostics.validation_success {
        println!("\n⚠️  Validation failed: Unknown configuration version");
        println!("The workspace configuration uses schema or policy versions that are not");
        println!("recognized by this version of bead-rs. Policy validation cannot continue.");
        return;
    }

    if diagnostics.findings.is_empty() {
        println!("\n✅ No policy issues found");
        return;
    }

    println!("\nFindings ({} total):", diagnostics.summary.total_findings);
    println!(
        "  Critical: {} | Error: {} | Warning: {} | Info: {}",
        diagnostics.summary.critical_count,
        diagnostics.summary.error_count,
        diagnostics.summary.warning_count,
        diagnostics.summary.info_count
    );

    for finding in &diagnostics.findings {
        println!("\n{}", format_severity(&finding.severity));
        println!("  Category: {}", format_category(&finding.category));
        println!("  {}", finding.message);

        if let Some(ref location) = finding.location {
            println!("  Location: {}", location);
        }

        if let Some(ref key) = finding.config_key {
            println!("  Config Key: {}", key);
        }

        if let Some(ref recommendation) = finding.recommendation {
            println!("  Recommendation: {}", recommendation);
        }
    }
}

/// Format diagnostic status
fn format_status(status: &service::PolicyDiagnosticStatus) -> String {
    match status {
        service::PolicyDiagnosticStatus::Healthy => "✅ Healthy".to_string(),
        service::PolicyDiagnosticStatus::Warning => "⚠️  Warning".to_string(),
        service::PolicyDiagnosticStatus::Error => "❌ Error".to_string(),
        service::PolicyDiagnosticStatus::UnknownVersion => "❓ Unknown Version".to_string(),
    }
}

/// Format finding severity
fn format_severity(severity: &service::FindingSeverity) -> String {
    match severity {
        service::FindingSeverity::Critical => "🔴 CRITICAL".to_string(),
        service::FindingSeverity::Error => "❌ ERROR".to_string(),
        service::FindingSeverity::Warning => "⚠️  WARNING".to_string(),
        service::FindingSeverity::Info => "ℹ️  INFO".to_string(),
    }
}

/// Format finding category
fn format_category(category: &service::FindingCategory) -> String {
    match category {
        service::FindingCategory::Contradictory => "Contradictory".to_string(),
        service::FindingCategory::Unreachable => "Unreachable".to_string(),
        service::FindingCategory::Redundant => "Redundant".to_string(),
        service::FindingCategory::InvalidValue => "Invalid Value".to_string(),
        service::FindingCategory::MissingRequired => "Missing Required".to_string(),
        service::FindingCategory::Deprecated => "Deprecated".to_string(),
        service::FindingCategory::VersionCompatibility => "Version Compatibility".to_string(),
        service::FindingCategory::Ineffective => "Ineffective".to_string(),
        service::FindingCategory::Info => "Info".to_string(),
    }
}

/// Convert attempt tier to human-readable name
fn tier_name(tier: i64) -> &'static str {
    match tier {
        0 => "Unproven",
        1 => "Retryable",
        2 => "Struggling",
        3 => "Quarantined",
        _ => "Unknown",
    }
}

fn cmd_recurrence_history(opts: cli::RecurrenceHistoryOptions) -> Result<()> {
    let config = store::WorkspaceConfig::discover()?
        .ok_or_else(|| Error::workspace("No workspace found. Run `bead init` first."))?;

    let db_path = config.database_path();
    let mut store = store::SqliteStore::with_path(&db_path)
        .map_err(|e| Error::Internal(anyhow::anyhow!("Failed to open database: {}", e)))?;

    let template = service::get_template(store.conn(), &opts.id)?;
    let history = service::get_materialization_history(store.conn(), &opts.id)?;

    if opts.json {
        let output = serde_json::to_string_pretty(&history).unwrap();
        println!("{}", output);
    } else {
        eprintln!("Materialization History for: {}", template.title);
        if history.is_empty() {
            eprintln!("  No occurrences materialized yet");
        } else {
            for mat in &history {
                eprintln!(
                    "  Sequence {}: Issue {} (materialized {})",
                    mat.series_sequence, mat.occurrence_id, mat.materialized_at
                );
                if let Some(ref actor) = mat.actor {
                    eprintln!("    Actor: {}", actor);
                }
            }
        }
    }

    Ok(())
}

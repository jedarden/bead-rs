//! Documentation and man page generation for bead-rs
//!
//! This module handles generation of man pages and structured documentation
//! from the clap command tree.

use std::fs;
use std::path::Path;

/// Generate a man page for a single command
fn generate_man_page(cmd: &clap::Command, out_dir: &Path) -> Result<(), std::io::Error> {
    use clap_mangen::Man;

    let mut buffer = Vec::new();
    let man = Man::new(cmd.clone());
    man.render(&mut buffer)?;

    // Write to file
    let man_path = out_dir.join(format!("{}.1", cmd.get_name()));
    fs::write(&man_path, buffer)?;

    // Generate man pages for subcommands
    for subcommand in cmd.get_subcommands() {
        // Skip hidden commands
        if subcommand.is_hide_set() {
            continue;
        }

        // Generate man page for subcommand
        generate_man_page(subcommand, out_dir)?;
    }

    Ok(())
}

/// Generate all man pages from the clap command tree
pub fn generate_man_pages(out_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    use clap::CommandFactory;

    // Create the output directory if it doesn't exist
    fs::create_dir_all(out_dir)?;

    // Build the command from the CLI struct
    let cmd = crate::cli::Cli::command();

    // Generate man pages recursively
    generate_man_page(&cmd, out_dir)?;

    Ok(())
}

/// Validate that all public commands have help text
pub fn validate_help_coverage(cmd: &clap::Command) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();

    validate_command_help(cmd, &mut errors, "", true);

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Recursively validate help coverage for a command
fn validate_command_help(
    cmd: &clap::Command,
    errors: &mut Vec<String>,
    path: &str,
    check_unimplemented: bool,
) {
    let current_path = if path.is_empty() {
        cmd.get_name().to_string()
    } else {
        format!("{} {}", path, cmd.get_name())
    };

    // Skip unimplemented commands when requested
    let is_unimplemented = current_path.contains("unimplemented");
    if is_unimplemented && !check_unimplemented {
        return;
    }

    // Check if command has about text
    if cmd.get_about().is_none() && !cmd.is_hide_set() && !is_unimplemented {
        errors.push(format!("Command '{}' missing about text", current_path));
    }

    // Check if command has long_about text for root commands
    if path.is_empty() && cmd.get_long_about().is_none() && !cmd.is_hide_set() && !is_unimplemented
    {
        errors.push(format!(
            "Root command '{}' missing long_about text",
            current_path
        ));
    }

    // Check options have help text using clap 4.x API
    if !is_unimplemented {
        for opt in cmd.get_arguments() {
            if let Some(long) = opt.get_long() {
                if opt.get_help().is_none() && opt.get_long_help().is_none() {
                    errors.push(format!(
                        "Option '--{}' in command '{}' missing help text",
                        long, current_path
                    ));
                }
            }
        }
    }

    // Recursively check subcommands
    for subcommand in cmd.get_subcommands() {
        if !subcommand.is_hide_set() {
            validate_command_help(subcommand, errors, &current_path, check_unimplemented);
        }
    }
}

/// Get all public command paths for testing
pub fn get_public_command_paths() -> Vec<String> {
    use clap::CommandFactory;

    let cmd = crate::cli::Cli::command();
    let mut paths = Vec::new();
    collect_command_paths(&cmd, &mut paths, "");
    paths
}

/// Collect command paths recursively
fn collect_command_paths(cmd: &clap::Command, paths: &mut Vec<String>, base: &str) {
    let current_path = if base.is_empty() {
        cmd.get_name().to_string()
    } else {
        format!("{} {}", base, cmd.get_name())
    };

    // Skip hidden commands
    if cmd.is_hide_set() {
        return;
    }

    paths.push(current_path.clone());

    // Collect subcommand paths
    for subcommand in cmd.get_subcommands() {
        collect_command_paths(subcommand, paths, &current_path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_help_coverage() {
        use clap::CommandFactory;

        let cmd = crate::cli::Cli::command();
        if let Err(errors) = validate_help_coverage(&cmd) {
            panic!("Help coverage validation failed:\n{}", errors.join("\n"));
        }
    }

    #[test]
    fn test_public_commands_exist() {
        let paths = get_public_command_paths();

        // Expected root commands from the command list
        let expected_commands = vec![
            "bead".to_string(),
            "bead init".to_string(),
            "bead create".to_string(),
            "bead list".to_string(),
            "bead show".to_string(),
            "bead update".to_string(),
            "bead release".to_string(),
            "bead close".to_string(),
            "bead reopen".to_string(),
            "bead claim".to_string(),
            "bead label".to_string(),
            "bead label add".to_string(),
            "bead label remove".to_string(),
            "bead dep".to_string(),
            "bead dep add".to_string(),
            "bead dep remove".to_string(),
            "bead sync".to_string(),
            "bead sync flush-only".to_string(),
            "bead sync import-only".to_string(),
            "bead doctor".to_string(),
            "bead capabilities".to_string(),
        ];

        for expected in expected_commands {
            assert!(
                paths.contains(&expected),
                "Expected command '{}' not found in command paths: {:?}",
                expected,
                paths
            );
        }

        // Verify unimplemented commands are present but not required for help coverage
        assert!(paths.contains(&"bead unimplemented".to_string()));
        assert!(paths.contains(&"bead unimplemented schema".to_string()));
        assert!(paths.contains(&"bead unimplemented migrate".to_string()));
    }

    #[test]
    fn test_man_page_generation() {
        let temp_dir = tempfile::tempdir().unwrap();
        let out_dir = temp_dir.path();

        let result = generate_man_pages(out_dir);
        assert!(
            result.is_ok(),
            "Man page generation failed: {:?}",
            result.err()
        );

        // Check that some expected man pages were created
        // Note: clap_mangen generates pages with just the command name, not bead- prefix
        let expected_pages = vec!["bead.1", "init.1", "create.1", "claim.1", "list.1"];
        for page in expected_pages {
            let man_path = out_dir.join(page);
            assert!(man_path.exists(), "Expected man page '{}' not found", page);
        }
    }
}

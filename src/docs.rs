//! Documentation and man page generation for bead-rs
//!
//! This module handles generation of man pages and structured documentation
//! from the clap command tree.

use std::fs;
use std::path::Path;

/// Generate a man page for a single command and, recursively, its subcommands.
///
/// Pages are named by their full command path with hyphens -- `bead.1`,
/// `bead-create.1`, `bead-recurrence-create.1` -- following the same convention
/// as `git-commit.1`.
///
/// Naming by the bare leaf name instead is not merely untidy, it loses pages:
/// `create`, `list`, `show`, `add`, `remove`, `set`, `get`, `delete`, `find`,
/// `check`, `history`, and `materialize` each occur under more than one parent,
/// so whichever subtree was walked last silently overwrote the others. That is
/// how `bead create`'s page came to contain the description of
/// `bead recurrence create`.
fn generate_man_page(
    cmd: &clap::Command,
    out_dir: &Path,
    path: &str,
) -> Result<(), std::io::Error> {
    use clap_mangen::Man;

    let full_name = if path.is_empty() {
        cmd.get_name().to_string()
    } else {
        format!("{} {}", path, cmd.get_name())
    };
    let file_stem = full_name.replace(' ', "-");

    // Render under the fully-qualified name so the NAME and SYNOPSIS sections
    // read `bead recurrence create`, not a bare, ambiguous `create`.
    //
    // clap 4.x's `Command::name` takes `impl Into<Str>`, which this version
    // implements only for `&'static str`. The leak is bounded by the number of
    // commands in the tree and happens once per generation run.
    let static_name: &'static str = Box::leak(full_name.clone().into_boxed_str());
    let renamed = cmd.clone().name(static_name).display_name(static_name);

    let mut buffer = Vec::new();
    Man::new(renamed).render(&mut buffer)?;
    fs::write(out_dir.join(format!("{file_stem}.1")), buffer)?;

    for subcommand in cmd.get_subcommands() {
        // Skip hidden commands and clap's built-in `help` pseudo-command.
        if subcommand.is_hide_set() || subcommand.get_name() == "help" {
            continue;
        }
        generate_man_page(subcommand, out_dir, &full_name)?;
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
    generate_man_page(&cmd, out_dir, "")?;

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

    // Every non-hidden command must carry long_about, not just the root.
    //
    // This check used to apply only when `path.is_empty()`. That let 24 leaf
    // commands silently lose their long help: clap emits `.long_about(None)`
    // from a subcommand variant's doc comment, which overwrites whatever the
    // payload struct's `#[command(long_about = ...)]` set, and nothing noticed.
    if cmd.get_long_about().is_none() && !cmd.is_hide_set() && !is_unimplemented {
        errors.push(format!(
            "Command '{}' missing long_about text (a variant doc comment may be \
             shadowing the payload struct's #[command(long_about = ...)])",
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

/// A `bead ...` invocation quoted as an example inside some command's help text.
#[derive(Debug, Clone)]
pub struct HelpExample {
    /// Command path the example was found under, e.g. "bead sync flush-only".
    pub source_command: String,
    /// The example line as written in the help text.
    pub line: String,
    /// The example split into argv, with surrounding quotes resolved.
    pub argv: Vec<String>,
}

/// Extract every `bead ...` example line from every command's help text.
///
/// Help text is only useful if the commands it shows actually parse. Scraping
/// the examples back out of the built command tree lets a test feed each one to
/// the real parser, so a documented invocation cannot drift from the interface
/// it documents.
pub fn collect_help_examples() -> Vec<HelpExample> {
    use clap::CommandFactory;

    let cmd = crate::cli::Cli::command();
    let mut examples = Vec::new();
    collect_examples_from(&cmd, "", &mut examples);
    examples
}

fn collect_examples_from(cmd: &clap::Command, base: &str, out: &mut Vec<HelpExample>) {
    let current_path = if base.is_empty() {
        cmd.get_name().to_string()
    } else {
        format!("{} {}", base, cmd.get_name())
    };

    if let Some(long_about) = cmd.get_long_about() {
        for raw in long_about.to_string().lines() {
            let line = raw.trim();
            if !line.starts_with("bead ") {
                continue;
            }
            // Drop trailing `# comment` annotations used to caption examples.
            let command_part = match line.find(" #") {
                Some(idx) => line[..idx].trim(),
                None => line,
            };
            // Synopsis forms such as `bead dep add <BLOCKED> <BLOCKER>` or
            // `bead policy check [--format text|json]` document shape rather
            // than a runnable invocation, so they are not parsed as examples.
            if command_part.contains(['<', '[', '|']) {
                continue;
            }
            if let Some(argv) = split_example(command_part) {
                out.push(HelpExample {
                    source_command: current_path.clone(),
                    line: command_part.to_string(),
                    argv,
                });
            }
        }
    }

    for sub in cmd.get_subcommands() {
        collect_examples_from(sub, &current_path, out);
    }
}

/// Split an example into argv, honouring single and double quotes.
///
/// Returns `None` for a line with an unterminated quote, which is itself a
/// defect worth surfacing rather than silently skipping.
fn split_example(line: &str) -> Option<Vec<String>> {
    let mut argv = Vec::new();
    let mut current = String::new();
    let mut started = false;
    let mut quote: Option<char> = None;

    for ch in line.chars() {
        match quote {
            Some(q) if ch == q => quote = None,
            Some(_) => current.push(ch),
            None if ch == '\'' || ch == '"' => {
                quote = Some(ch);
                started = true;
            }
            None if ch.is_whitespace() => {
                if started || !current.is_empty() {
                    argv.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            None => current.push(ch),
        }
    }

    if quote.is_some() {
        return None;
    }
    if started || !current.is_empty() {
        argv.push(current);
    }
    Some(argv)
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

    /// Every `bead ...` example quoted in help text must parse.
    ///
    /// Before this test existed, nine documented examples were rejected outright
    /// by the parser they claimed to demonstrate -- wrong argument forms
    /// (`bead why <id>` for a `--id` flag), flags that never existed
    /// (`bead capabilities --format json`), and a `bead sync --flush-only` spelling
    /// of what is really the `flush-only` subcommand.
    #[test]
    fn test_help_examples_parse() {
        use clap::Parser;

        let examples = collect_help_examples();
        assert!(
            examples.len() > 30,
            "expected the help text to carry examples; found {}",
            examples.len()
        );

        let mut failures = Vec::new();
        for example in &examples {
            if let Err(err) = crate::cli::Cli::try_parse_from(&example.argv) {
                // A parse *error* is a broken example. `DisplayHelp` and
                // `DisplayVersion` are successful parses that clap reports as Err.
                match err.kind() {
                    clap::error::ErrorKind::DisplayHelp
                    | clap::error::ErrorKind::DisplayVersion
                    | clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand => {}
                    _ => failures.push(format!(
                        "  in `{}` help:\n    {}\n    -> {}",
                        example.source_command,
                        example.line,
                        err.to_string().lines().next().unwrap_or("parse error")
                    )),
                }
            }
        }

        assert!(
            failures.is_empty(),
            "{} documented example(s) do not parse:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }

    /// `-h` and `--help` must differ wherever long help was written.
    ///
    /// When a subcommand variant's doc comment shadows the payload struct's
    /// `#[command(long_about = ...)]`, clap falls back to the short about for
    /// both, and the long help becomes unreachable without any build failure.
    #[test]
    fn test_long_help_is_reachable() {
        use clap::CommandFactory;

        fn walk(cmd: &clap::Command, path: &str, out: &mut Vec<String>) {
            let current = if path.is_empty() {
                cmd.get_name().to_string()
            } else {
                format!("{} {}", path, cmd.get_name())
            };

            if !cmd.is_hide_set() && !current.contains("unimplemented") && current != "bead help" {
                match (cmd.get_about(), cmd.get_long_about()) {
                    (Some(about), Some(long)) if about.to_string() == long.to_string() => {
                        out.push(current.clone())
                    }
                    (_, None) => out.push(current.clone()),
                    _ => {}
                }
            }

            for sub in cmd.get_subcommands() {
                walk(sub, &current, out);
            }
        }

        let mut shadowed = Vec::new();
        walk(&crate::cli::Cli::command(), "", &mut shadowed);
        assert!(
            shadowed.is_empty(),
            "these commands have no distinct long help; a variant doc comment is \
             probably shadowing the payload struct's #[command(long_about = ...)]:\n  {}",
            shadowed.join("\n  ")
        );
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

        // ADR-002 removed `migrate` entirely; it must not appear anywhere in
        // the command tree, implemented or unimplemented.
        assert!(!paths.contains(&"bead migrate".to_string()));
        assert!(!paths.contains(&"bead unimplemented migrate".to_string()));
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

        // Pages are named by full command path, git-style.
        let expected_pages = vec![
            "bead.1",
            "bead-init.1",
            "bead-create.1",
            "bead-claim.1",
            "bead-list.1",
            "bead-sync-flush-only.1",
            "bead-recurrence-create.1",
        ];
        for page in expected_pages {
            let man_path = out_dir.join(page);
            assert!(man_path.exists(), "Expected man page '{}' not found", page);
        }
    }

    /// One man page per public command, with no two commands sharing a file.
    ///
    /// `create`, `list`, `show`, `add`, `remove`, `set`, `get`, `delete`, `find`,
    /// `check`, `history`, and `materialize` all occur under more than one
    /// parent. When pages were named by the bare leaf name, those collisions
    /// silently overwrote each other and 13 of 35 pages were lost.
    #[test]
    fn test_man_pages_do_not_collide() {
        let temp_dir = tempfile::tempdir().unwrap();
        let out_dir = temp_dir.path();
        generate_man_pages(out_dir).unwrap();

        let generated = std::fs::read_dir(out_dir).unwrap().count();
        let expected = get_public_command_paths()
            .into_iter()
            .filter(|p| !p.ends_with(" help"))
            .count();

        assert_eq!(
            generated, expected,
            "expected one man page per public command ({expected}), found {generated}; \
             a name collision is overwriting pages"
        );
    }

    /// `bead create`'s page must describe `bead create`.
    ///
    /// Regression guard for the collision above, which left `create.1`
    /// containing the description of `bead recurrence create`.
    #[test]
    fn test_man_page_content_matches_its_command() {
        let temp_dir = tempfile::tempdir().unwrap();
        let out_dir = temp_dir.path();
        generate_man_pages(out_dir).unwrap();

        let create = std::fs::read_to_string(out_dir.join("bead-create.1")).unwrap();
        assert!(
            create.contains("Create a new issue in the workspace"),
            "bead-create.1 does not describe `bead create`"
        );
        assert!(
            !create.contains("recurrence template"),
            "bead-create.1 was overwritten by another command's page"
        );

        let recurrence = std::fs::read_to_string(out_dir.join("bead-recurrence-create.1")).unwrap();
        assert!(
            recurrence.contains("recurrence template"),
            "bead-recurrence-create.1 does not describe `bead recurrence create`"
        );
    }
}

# Man Page Installation

`bead-rs` includes comprehensive man pages for all commands. This document describes how to install and use them.

## Generating Man Pages

Man pages are generated from the clap command tree using the `generate-man-pages` binary:

```bash
# Generate man pages to the default location (man/man1/)
cargo run --bin generate-man-pages

# Generate to a custom location
cargo run --bin generate-man-pages -- /custom/path
```

Pages are named by their full command path with hyphens, the same convention
`git` uses for `git-commit.1`:

- Root command: `bead.1`
- Subcommands: `bead-init.1`, `bead-create.1`, `bead-claim.1`
- Nested subcommands: `bead-sync-flush-only.1`, `bead-recurrence-create.1`

Naming pages by the bare leaf name instead loses them to collisions: `create`,
`list`, `show`, `add`, `remove`, `set`, `get`, `delete`, `find`, `check`,
`history`, and `materialize` each appear under more than one parent.

## Installing Man Pages

### System-wide Installation

To install man pages system-wide (requires write permissions to man directory):

```bash
# Generate man pages
cargo run --bin generate-man-pages

# Copy to system man directory
sudo cp man/man1/* /usr/share/man/man1/

# Update man database (if needed)
sudo mandb
```

### User-local Installation

To install man pages for the current user only:

```bash
# Create user man directory
mkdir -p ~/.local/share/man/man1

# Generate and copy man pages
cargo run --bin generate-man-pages
cp man/man1/* ~/.local/share/man/man1/

# Update man database
mandb ~/.local/share/man
```

### Installation from Package

When `bead` is installed via `cargo install`, the man pages are included in the crate but need to be manually installed:

```bash
# Find the installed package location
cargo install bead --root ~/.local

# Extract man pages from the crate (they're in man/man1/)
# The package will include them automatically
```

## Using Man Pages

Once installed, you can access man pages using the `man` command:

```bash
# Root command
man bead

# Specific commands
man bead-init
man bead-create
man bead-claim
man bead-sync
```

For nested subcommands, join the path with hyphens:

```bash
man bead-sync-flush-only
man bead-recurrence-materialize
```

## Man Page Contents

Each man page includes:
- **NAME**: Command name and brief description
- **SYNOPSIS**: Usage syntax with all options
- **DESCRIPTION**: Detailed command description with examples
- **OPTIONS**: Complete list of options with descriptions
- **EXAMPLES**: Practical usage examples (where applicable)
- **EXIT STATUS**: Exit codes and their meanings (where applicable)
- **SEE ALSO**: Related commands (where applicable)

## Packaging Integration

The man pages are automatically included in the crate distribution. When building a release package:

```bash
# The .crate file includes man pages automatically
cargo package

# When installing the package, man pages can be extracted
# from the included man/man1/ directory
```

## Verification

Verify man page installation:

```bash
# Test that man pages are accessible
man -k bead

# View specific man page
man bead
```

If man pages are not found, check:
1. Man page installation directory is in `MANPATH`
2. Man database is up to date (`mandb`)
3. Man page files have correct permissions (644)

## Development

Man pages are generated from the clap command tree, so they inherit whatever
`src/cli.rs` declares. The `generate-man-pages` binary:

1. Parses the clap command tree
2. Generates a page for every non-hidden command, named by its full path
3. Renders each page under its fully-qualified name, so `NAME` reads
   `bead recurrence create` rather than an ambiguous `create`
4. Recurses into subcommands, skipping clap's built-in `help` pseudo-command

They are **not** regenerated automatically. Run the generation step after any
CLI change and commit the result:

```bash
cargo run --bin generate-man-pages
```

Because the pages come from the command tree, help text defects propagate into
them. Two unit tests in `src/docs.rs` guard the source:

- `test_long_help_is_reachable` fails if a subcommand's long help is shadowed by
  a variant doc comment, which would otherwise reduce its page to a one-line
  description.
- `test_man_pages_do_not_collide` fails if two commands would write the same
  file.

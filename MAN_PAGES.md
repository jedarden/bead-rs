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

The generated man pages follow the naming convention:
- Root command: `bead.1`
- Subcommands: `init.1`, `create.1`, `claim.1`, etc.

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
man init
man create
man claim
man sync
```

For subcommands like `sync flush-only`, use:

```bash
man flush-only
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

When adding new commands to `src/cli.rs`, the man pages are automatically regenerated. The `generate-man-pages` binary:

1. Parses the clap command tree
2. Generates man pages for all non-hidden commands
3. Creates proper man page formatting with sections
4. Handles subcommands recursively

Run the generation step after CLI changes to ensure man pages stay in sync with the command interface.

# Changelog

All notable changes to bead-rs are documented in this file.

## [0.2.2] - 2026-08-29

### Added
- Install contract: `install.sh` and architecture-suffixed release binaries (`bead-x86_64-unknown-linux-gnu`, `bead-aarch64-unknown-linux-gnu`) with `checksums.txt` for verification
- README one-liner installer: `curl -fsSL https://github.com/jedarden/bead-rs/releases/latest/download/install.sh | bash`

### Fixed
- Sync flag fixes (integration tests added)
- Capabilities command now includes 'schema' in commands array
- Starvation recovery fallback query implemented
- Removed blocked from stored statuses inventory

### CI/CD
- Release workflow now generates and uploads arch-suffixed binaries, install.sh, and checksums.txt
- Enhanced checksums.txt validation to prevent incomplete or corrupted releases

## [0.2.1] - 2026-08-26

### Added
- Initial bead-rs release as the canonical bead CLI for agent fleets
- SQLite-based task coordination with atomic claim operations
- Checkpoint system for workspace state persistence

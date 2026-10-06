# Changelog

All notable changes to bead-rs are documented in this file.

## [0.3.0] - Unreleased (candidate)

- Controlled atomic secret scrubbing, including metadata rekeying with
  preserved references, one-epoch batches and resumable local-byte cleanup.
- Service-level secret rejection, managed enforcement without acknowledgment
  bypasses, and recovery quarantine that withholds unsafe checkpoints.
- Ruleset 4 provider/structured credentials, mapped normalized and decoded
  views, complete JWT headers, exact raw-range fingerprints and bounded
  decoded-view coverage diagnostics.
- Counted value-free advisory summaries without changing scalar/array output
  shapes; complete authorization values and contextual assignment detection.
- Digest-pinned Argo verification, checksummed default/managed candidates,
  fail-closed evidence approval and promotion of the exact tested payload.

Publication and lab/codinghome installation remain gated on exact-source,
candidate and fleet evidence. This entry is not a release receipt. Historical
Git copies are not rewritten; real exposed credentials require rotation.

## [0.2.4] - 2026-08-30

Same content as the 0.2.2 entry below plus the test-suite update for the
initial checkpoint published by `bead init`. 0.2.2 and 0.2.3 were tagged on
Forgejo before a release could be published (0.2.2: CI never green; 0.2.3: the
release step failed to link aarch64, and main moved past the tag before the
fixed pipeline ran). 0.2.4 is the first release that carries the install
contract.

### Fixed
- Tests follow the initial-checkpoint-on-init contract (checkpoint_tombstones,
  cli_sync, r015/r027/r028); clippy `unnecessary_unwrap` in main.rs
- `bead init` publishes an initial (empty) generation so a fresh workspace is
  never a dangling checkpoint pointer (`--no-auto-flush` suppresses it)

## [0.2.2] - 2026-08-29 (tagged, never released)

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

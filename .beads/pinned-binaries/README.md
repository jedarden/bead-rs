# Pinned Binaries for Capability Testing

This directory contains pinned bead-rs binaries for testing capability differences between pre-feature and feature-enabled versions.

## Binaries

### Feature-Enabled Binary (attempt-resolution-complete)
- **Binary:** `bead-feature-enabled`
- **Commit:** `5bb28bf7b853be7ba244adf3ce4c76b8d1bd01e5`
- **Date:** 2026-09-01 15:45:58 -0400
- **Message:** `docs(boundaries): document attempt-resolution feature boundary commits`
- **Hash:** `e6a8ffb8b9d6b6cbba2d98f0458e62c3e211c1590d7abacd178419299a41a318`
- **Capabilities:** Full attempt-resolution support with atomic idempotent outcome recording

### Pre-Feature Binary
**Status:** Requires manual build from `attempt-resolution-pre` tag
- **Tag:** `attempt-resolution-pre`
- **Commit:** `53dade07ff2b9afda87e67459a825ec7e138dafa`
- **Date:** 2026-08-31 09:24:28 -0400
- **Message:** `feat(recovery): replace heuristic starvation mutations with recommendation-only diagnostics`
- **Note:** This commit has minor compilation issues due to later API changes. Building requires fixing field name references in watchdog code.

## Building Pre-Feature Binary

To build the pre-feature binary from the boundary commit:

```bash
# Checkout the pre-feature state
git checkout attempt-resolution-pre

# The code needs minor fixes for compilation:
# - src/main.rs: Replace `lease_valid_but_stale` with `alive_but_stale` (3 occurrences)
# - These are WatchdogResult field renames that happened after this commit

# Build
cargo build --release

# Pin the binary
cp target/release/bead .beads/pinned-binaries/bead-pre-feature
sha256sum .beads/pinned-binaries/bead-pre-feature > .beads/pinned-binaries/bead-pre-feature.sha256
```

## Capability Testing Framework

The test framework validates:
1. **Capability Detection:** `bead capabilities` shows attempt-outcome support
2. **Resolve Command:** `bead resolve` is available/absent
3. **Why Command:** Attempt information in `bead why` output
4. **Checkpoint Persistence:** Attempt outcomes survive checkpoint round-trips
5. **NEEDLE Fallback:** Worker starvation detection and recommendation behavior

## Usage

```bash
# Test feature-enabled binary
./bead-feature-enabled capabilities --format json | jq '.attempt_outcome.supported'
./bead-feature-enabled why bead-123abc

# Test pre-feature binary (after building)
./bead-pre-feature capabilities --format json | jq '.attempt_outcome.supported'  # Should be false or missing
./bead-pre-feature why bead-123abc  # Should not show attempt info
```

## Verification

See `tests/pinned_binary_capability.rs` for automated capability testing.

---

**Created:** 2026-09-01  
**Bead:** `beadrs-78ced0f1`

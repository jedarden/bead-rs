# bead-rs Build Procedure with Attempt-Resolution Feature

## Binary Information

- **Binary Name**: `bead`
- **Version**: 0.2.6
- **Commit SHA**: `6561869ba87fa1391967abf6877e51ef6425301b`
- **Binary SHA256**: `0690918612738e0ff4717e9a9d54434a6bc734b67890fe99d767c5de780d02be`
- **Build Date**: 2026-09-01
- **Build Profile**: release (optimized)

## Attempt-Resolution Feature

The attempt-resolution functionality is **always enabled** in the current bead-rs codebase. No special feature flag is required. The `bead resolve` command is available by default in the release binary.

### Verify Attempt-Resolution Functionality

```bash
# Check that resolve command is available
/home/coding/target/release/bead --help | grep resolve
# Output: resolve            Record an execution attempt outcome atomically
```

## Build Rule

Pinned binaries are built from a git-archive extraction in scratch via
`scripts/build-from-archive.sh <sha>` — never by stashing, resetting, or
checking out commits inside this shared checkout. `/home/coding/bead-rs` is a
single shared NEEDLE workspace: moving its HEAD or index rewires every other
worker's tree and can erase uncommitted work, so the script is the only
sanctioned way to produce a pinned binary.

## Build Procedure

### Prerequisites
- Rust 1.85 or later
- Standard build tools (cargo, make, etc.)

### Step 1: Build the pinned commit from a git-archive extraction

```bash
cd /home/coding/bead-rs
scripts/build-from-archive.sh 6561869ba87fa1391967abf6877e51ef6425301b
```

The script runs `git archive <sha> | tar -x` into a fresh scratch directory
under `~/scratch`, builds there with `CARGO_TARGET_DIR` inside that same
directory, prints the binary path and its sha256, and copies the binary and
its metadata (sha, sha256) to the pinned location. The scratch directory is
removed on success and left in place on failure for diagnosis. The shared
checkout's HEAD, index, stash, and working tree are never touched.

Reachability caveat: the script can only build commits that still resolve in
this repo — check with `git cat-file -t <sha>` first. The source commits of
the existing pins are unreachable after the 2026-09-02 twin-lineage
force-push, so the invocation above records the sanctioned form rather than a
currently runnable rebuild.

### Step 2: Verify Binary

```bash
# Check version (use the binary path the script printed)
<binary-path-from-script> --version
# Should show a 6561869 build

# Verify resolve command is available
<binary-path-from-script> resolve --help
```

### Step 3: Calculate Binary Hash (for verification)

```bash
sha256sum <binary-path-from-script>
```

## Installation

### System-wide Installation

```bash
cargo install --path .
# Or copy the built binary:
sudo cp /home/coding/target/release/bead /usr/local/bin/bead
```

### User-local Installation

```bash
mkdir -p ~/.local/bin
cp /home/coding/target/release/bead ~/.local/bin/bead
# Ensure ~/.local/bin is in your PATH
```

## Attempt-Resolution Commands

The binary includes the following attempt-resolution functionality:

- `bead resolve` - Record an execution attempt outcome atomically
- Support for outcome types: verified_success, work_failure, infrastructure_failure, cancelled, indeterminate
- Schema URNs: `urn:bead-rs:schema:attempt-outcome:native-v1`, `urn:bead-rs:schema:resolve-receipt:native-v1`

## Test Fixtures

The repository includes test fixtures for attempt-resolution at:
- `tests/fixtures/attempts/new/` - New format fixtures
- `tests/fixtures/attempts/old/` - Legacy format fixtures
- `tests/attempt.rs` - Attempt outcome model tests
- `tests/test_migration_on_open.rs` - Migration tests

## Verification

To verify the binary is working correctly with attempt-resolution:

```bash
# Initialize a test workspace
mkdir -p /tmp/bead-test
cd /tmp/bead-test
/home/coding/target/release/bead init --prefix test

# Create a test bead
TEST_BEAD=$(/home/coding/target/release/bead create --title "Test resolve functionality" --priority 1)

# Verify resolve command works
/home/coding/target/release/bead resolve --help
```

## Commit Pinning

This build is permanently pinned to commit:
```
6561869ba87fa1391967abf6877e51ef6425301b
```

To reproduce this build, go through the archive script:

```bash
scripts/build-from-archive.sh 6561869ba87fa1391967abf6877e51ef6425301b
```

The script extracts the pinned commit's tree into a scratch directory and
builds there; the shared checkout is never moved to the pinned commit to do
it. That pin's source commit is itself unreachable in this repo since the
2026-09-02 twin-lineage force-push, so the invocation cannot currently run —
it records the sanctioned form for any commit that does resolve.

**Byte-exact reproduction caveat:** `build.rs` embeds a wall-clock build
timestamp, so a fresh build of this pre-determinism pin yields a different
sha256 than the `06909186…` value recorded above. Treat the recorded hash as
the identity of the pinned artifact (compare it against the committed bytes in
`pinned-binaries/`), and treat a script run as the proof that a new build came
from the pinned tree. Deterministic rebuilds (SOURCE_DATE_EPOCH) are tracked
as separate work; until they land, do not expect a fresh build to match the
recorded hash.

## Notes

- The attempt-resolution feature is integrated into the main binary, not a separate feature flag
- The build process produces some warnings about unused fields, but these do not affect functionality
- SQLite is bundled, so there are no runtime dependencies
- The binary supports the full bead-rs lifecycle plus the new attempt-resolution commands
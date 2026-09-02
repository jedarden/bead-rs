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

## Build Procedure

### Prerequisites
- Rust 1.85 or later
- Standard build tools (cargo, make, etc.)

### Step 1: Clone and Navigate to Repository

```bash
cd /home/coding/bead-rs
```

### Step 2: Verify Commit

```bash
git log --oneline -1
# Should show: 6561869 feat(pluck): update pre-attempt-resolution binary to commit 946a727
```

### Step 3: Build Release Binary

```bash
cargo build --release
```

Build output will be placed at: `/home/coding/target/release/bead`

### Step 4: Verify Binary

```bash
# Check version
/home/coding/target/release/bead --version
# Should show: bead 0.2.6 (6561869-dirty 2026-09-02T01:43:04Z)

# Verify resolve command is available
/home/coding/target/release/bead resolve --help
```

### Step 5: Calculate Binary Hash (for verification)

```bash
sha256sum /home/coding/target/release/bead
# Expected output: 0690918612738e0ff4717e9a9d54434a6bc734b67890fe99d767c5de780d02be  /home/coding/target/release/bead
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

To reproduce this exact build:
```bash
git clone https://github.com/jedarden/bead-rs.git
cd bead-rs
git checkout 6561869ba87fa1391967abf6877e51ef6425301b
cargo build --release
sha256sum target/release/bead
# Should match: 0690918612738e0ff4717e9a9d54434a6bc734b67890fe99d767c5de780d02be
```

## Notes

- The attempt-resolution feature is integrated into the main binary, not a separate feature flag
- The build process produces some warnings about unused fields, but these do not affect functionality
- SQLite is bundled, so there are no runtime dependencies
- The binary supports the full bead-rs lifecycle plus the new attempt-resolution commands
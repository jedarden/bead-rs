# Attempts Binary Build Process and Verification

This document describes the reproducible build process for the bead-rs attempts binary and verifies that the feature-enabled binary is cryptographically distinct from the standard build.

## Overview

The attempts binary is a version of bead-rs built with the `attempt-resolution` feature enabled. The functionality it refers to — atomic attempt outcome recording and lifecycle transitions for NEEDLE fleet coordination (`bead resolve`) — is always compiled in; the cargo feature is currently an empty marker that gates no code (see "Interpreting the difference" below and `pinned-binaries/BINARY_VERIFICATION.md`).

## Feature Flag Configuration

### Cargo.toml Features
```toml
[features]
default = []
attempt-resolution = []
```

The `attempt-resolution` feature flag is defined in `Cargo.toml` and can be enabled during build.

## Build Process

### Prerequisites

- Rust 1.85+
- Cargo
- Clean git checkout at specific commit
- Target directory: `/home/coding/target/release/`

### Standard Build (Without Feature)

**Purpose:** Baseline binary without attempt-resolution feature

```bash
# From clean git state
cd /home/coding/bead-rs
git clean -fdx
git checkout <commit-sha>

# Build without features
cargo build --release --no-default-features

# Binary location
/home/coding/target/release/bead
```

### Feature-Enabled Build (With Attempt-Resolution)

**Purpose:** Production binary with attempt-resolution functionality

```bash
# From clean git state
cd /home/coding/bead-rs
git clean -fdx
git checkout <commit-sha>

# Build with feature enabled
cargo build --release --features attempt-resolution

# Binary location
/home/coding/target/release/bead
```

## Metadata Capture Process

After building, capture the following metadata:

### 1. Version Information
```bash
./target/release/bead --version
# Expected output: bead 0.2.6 (<commit-sha> <timestamp>)
```

### 2. Binary Hash
```bash
sha256sum target/release/bead
# Outputs: <hash>  target/release/bead
```

### 3. Binary Size
```bash
ls -lh target/release/bead
# Outputs size in human-readable format (e.g., 7.0M)
```

### 4. Build Timestamp
```bash
date -u +"%Y-%m-%dT%H:%M:%SZ"
# Current UTC timestamp
```

### 5. Git Commit Information
```bash
git rev-parse HEAD
# Current commit SHA

git log -1 --format="%H %s"
# Commit hash and message
```

## Binary Uniqueness Verification

### Current Pinned Binaries

| Binary | Size | SHA256 Hash | Build Date | Commit |
|--------|------|-------------|------------|--------|
| bead-pre-feature | 6.5M | `7e0e73defebb75fc987ddf8b6fb959f47c73ccbbcd7e066e2af302a6a43db6b5` | 2026-09-01T19:14:12Z (embedded) | af023ad (release 0.2.4) |
| bead-pre-attempt-resolution | 7.0M | `d0da42bbf59b721bc64bc3d55610844efe3f1f06e37c2d9494c0b3dda6e29ac6` | 2026-09-02T01:35:01Z | 946a727 |
| bead-attempt-resolution-e115609 | 7.0M | `68fe8d534721be4ba4147312364d8f0b216b62f3093e85e7c91f0a0db695a645` | 2026-09-02T07:23:55Z | e115609 |
| bead-attempt-resolution-f25ab5c | 7.0M | `9a8455f25bacf5bc961bd740442fdc1b30a67fb6e38d304c23c97a57cf57b04e` | 2026-09-02T10:52:25Z (embedded) | f25ab5c |

### Hash Comparison

**Pre-Feature vs Feature-Enabled Build:**
- Pre-feature (6.5M): `7e0e73defebb75fc987ddf8b6fb959f47c73ccbbcd7e066e2af302a6a43db6b5`
- Attempt-resolution enabled, HEAD pin (7.0M): `9a8455f25bacf5bc961bd740442fdc1b30a67fb6e38d304c23c97a57cf57b04e`

✅ **DISTINCT**: Different hashes prove the feature-enabled binary is cryptographically unique

**Size Comparison:**
- Pre-feature: 6.5M (6,788,016 bytes)
- Feature-enabled: 7.0M (7,305,184 bytes)

✅ **SIZE DIFFERENCE**: HEAD binary is ~517KB larger

⚠️ **Interpreting the difference:** `attempt-resolution` is an empty marker feature (see Cargo.toml and BINARY_VERIFICATION.md) — no `#[cfg]` gates anywhere in `src/`, and builds made with and without the flag (`bead-pre-attempt-resolution`, `bead-attempt-resolution-e115609`) both expose `bead resolve`. The pre-feature binary is release 0.2.4, which predates the feature's existence in Cargo.toml *and* the resolve work itself — it has no `bead resolve` subcommand and no `attempt_outcome` capability — so the hash/size delta measures all development between 0.2.4 and HEAD (including the arrival of `resolve`). It confirms the builds are distinct artifacts, not that the flag itself adds code.

### Verification Commands

To verify binary uniqueness:

```bash
# Compare hashes
sha256sum pinned-binaries/bead-pre-feature
sha256sum pinned-binaries/bead-attempt-resolution-e115609
sha256sum pinned-binaries/bead-attempt-resolution-f25ab5c

# Compare sizes
ls -lh pinned-binaries/bead-pre-feature pinned-binaries/bead-attempt-resolution-e115609 pinned-binaries/bead-attempt-resolution-f25ab5c

# Compare versions
./pinned-binaries/bead-pre-feature --version
./pinned-binaries/bead-attempt-resolution-e115609 --version
./pinned-binaries/bead-attempt-resolution-f25ab5c --version
```

## Pinned Binary Storage

Once built and verified, store binaries with metadata:

### Directory Structure
```
pinned-binaries/
├── bead-attempt-resolution-e115609
├── bead-attempt-resolution-e115609.metadata.json
├── bead-attempt-resolution-f25ab5c
├── bead-attempt-resolution-f25ab5c.metadata.json
├── bead-pre-attempt-resolution
├── bead-pre-attempt-resolution.metadata.json
├── bead-pre-feature
├── bead-pre-feature.metadata.json
├── bead-metadata.json
├── bead-release-metadata.json
├── BINARY_VERIFICATION.md
├── COMMITS.md
├── README.md
└── commits.json
```

### Metadata File Format

Field names below follow the newest metadata file, `bead-attempt-resolution-f25ab5c.metadata.json` (older files carry a subset; see the files themselves for each binary's record):

```json
{
  "binary_name": "bead-attempt-resolution-f25ab5c",
  "description": "bead-rs release binary built with --features attempt-resolution at HEAD, pinned byte-exact from the build bead's staging",
  "embedded_version_string": "bead 0.2.6 (f25ab5c-dirty 2026-09-02T10:52:25Z)",
  "embedded_build_timestamp": "2026-09-02T10:52:25Z",
  "git_commit_sha": "f25ab5c91c09a3408f23b9cdf2f3e95e81abc060",
  "git_commit_short": "f25ab5c",
  "git_commit_message": "docs(attempts-binary): add comprehensive build process and verification documentation",
  "binary_sha256": "9a8455f25bacf5bc961bd740442fdc1b30a67fb6e38d304c23c97a57cf57b04e",
  "binary_size_bytes": 7305184,
  "binary_size_human": "7.0M",
  "cargo_package_version": "0.2.6",
  "build_features": "attempt-resolution",
  "build_profile": "release",
  "rustc_version": "1.97.1",
  "build_command": "cargo build --release --features attempt-resolution"
}
```

## Build Recipe

> **A rebuild does not reproduce a pinned hash.** `build.rs` embeds `BEAD_BUILD_TIMESTAMP` at compile time and re-runs whenever `.git/index` changes, so two builds of identical source produce different bytes unless the build is pinned with `SOURCE_DATE_EPOCH` (embedded timestamp) and `BEAD_COMMIT_SHA` (commit, for trees with no `.git`) — with both set, builds of the same tree are byte-identical (`tests/reproducible_build.rs` asserts this; see `pinned-binaries/BINARY_VERIFICATION.md`, "Reproducibility caveat"). The recipe below therefore **records the provenance of a fresh build** — it does not, and cannot, check a new build against a previously pinned hash. To verify a pinned binary, compare its sha256 against its `*.metadata.json`; never rebuild for that purpose.

```bash
#!/bin/bash
set -e

TARGET_COMMIT="e1156098b01264bb998797047115521261443c13"
FEATURE="attempt-resolution"
BUILD_DIR="/home/coding/bead-rs"
OUTPUT_DIR="/home/coding/target/release"

echo "Building attempts binary and recording provenance..."

# Clean and checkout
cd "$BUILD_DIR"
git clean -fdx
git checkout "$TARGET_COMMIT"

# Build with feature
cargo build --release --features "$FEATURE"

# Record provenance for the new artifact
BINARY="$OUTPUT_DIR/bead"
echo "Binary: $BINARY"
echo "Version: $($BINARY --version)"
echo "SHA256: $(sha256sum $BINARY | cut -d' ' -f1)"
echo "Size: $(stat -c %s $BINARY)"
echo "Commit: $(git rev-parse HEAD)"

# To pin this build: copy it to pinned-binaries/bead-attempt-resolution-<short-sha>
# and write a matching .metadata.json with the values printed above.
```

## Integration Testing

The feature-enabled binary is used for:

1. **Integration Testing**: Binary variant integration test suite validates capability detection
2. **NEEDLE Coordination**: Production deployments requiring atomic attempt resolution
3. **Feature Development**: Testing attempt-resolution functionality without breaking existing workflows

### Capability Detection

Verify the binary advertises attempt-resolution support. Note `capabilities` already emits JSON — there is no `--json` flag:

```bash
./pinned-binaries/bead-attempt-resolution-e115609 capabilities | jq '.attempt_outcome'
```

Expected output (identical for `bead-pre-attempt-resolution`, `bead-attempt-resolution-e115609`, and `bead-attempt-resolution-f25ab5c`; `bead-pre-feature` has no `attempt_outcome` key at all):
```json
{
  "supported": true,
  "outcomes": [
    "verified_success",
    "work_failure",
    "infrastructure_failure",
    "cancelled",
    "indeterminate"
  ],
  "actions": [
    "close",
    "release",
    "quarantine",
    "block",
    "none"
  ],
  "replay_detection": true,
  "revision_guard": true,
  "fencing_token": true,
  "evidence_refs": true,
  "resolve_receipt_schema": "urn:bead-rs:schema:resolve-receipt:native-v1",
  "resolve_request_schema": "urn:bead-rs:schema:resolve-request:native-v1"
}
```

## Troubleshooting

### Build Fails with "unknown feature"

**Issue**: Cargo doesn't recognize the feature flag
**Solution**: Verify `attempt-resolution = []` exists in `[features]` section of Cargo.toml

### Hash Mismatch After Build

**Issue**: Reproduced binary has a different hash than the pinned version
**Expected behavior**: This always happens by default. `build.rs` embeds `BEAD_BUILD_TIMESTAMP` at compile time and re-runs whenever `.git/index` changes, so unpinned builds — even of identical source — are never byte-identical. That is why pinned binaries are copied byte-exact rather than rebuilt. (Pinning both `SOURCE_DATE_EPOCH` and `BEAD_COMMIT_SHA` does make same-tree builds byte-identical; see `tests/reproducible_build.rs`. A matching hash after a pinned rebuild is then the expected outcome, not a coincidence.)

**Solution**: Treat the rebuild as a new artifact:
```bash
# Record the new artifact's provenance (see Build Recipe above)
sha256sum target/release/bead
./target/release/bead --version

# Verify a PINNED binary by comparing bytes to its metadata — never by rebuilding
sha256sum pinned-binaries/bead-attempt-resolution-f25ab5c
# Must equal binary_sha256 in bead-attempt-resolution-f25ab5c.metadata.json
```

If you also need assurance the rebuilt source is functionally equivalent to the pin, compare `git rev-parse HEAD` against the pin's `git_commit_sha` and run the test suite — but expect the hashes to differ.

### Binary Size Unexpectedly Different

**Issue**: Binary size differs significantly from expected
**Causes**:
- Different optimization levels
- Debug assertions enabled
- Different target triple

**Solution**: Verify release build:
```bash
# Ensure release profile
cargo build --release --features attempt-resolution

# Check binary type
file target/release/bead
# Should show: ELF 64-bit LSB executable, x86-64
```

## Maintenance

When updating to a new commit:

1. Update this document with new commit SHA and hash
2. Rebuild using the process above
3. Run full test suite
4. Verify backward compatibility
5. Update pinned binaries in `pinned-binaries/`
6. Update `pinned-binaries/README.md` with new metadata
7. Update `pinned-binaries/BINARY_VERIFICATION.md` with new verification results

## References

- [ADR-011: Atomic Idempotent Attempt Resolution](adr/011-atomic-idempotent-attempt-resolution.md)
- [ADR-012: Capability-Gated Attempt Contract Rollout](adr/012-capability-gated-attempt-contract-rollout.md)
- [Binary Verification Report](../pinned-binaries/BINARY_VERIFICATION.md)
- [Pinned Binaries Documentation](../pinned-binaries/README.md)
- [BUILD_PROCEDURE.md](../BUILD_PROCEDURE.md)

---

**Document Version**: 1.0  
**Last Updated**: 2026-09-02  
**Status**: ✅ Complete - All acceptance criteria met
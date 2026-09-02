# Pinned bead-rs Binaries Documentation

This document provides comprehensive information about the pinned bead-rs binaries used for compatibility testing and feature development validation.

## Overview

The `pinned-binaries/` directory contains two baseline binaries that represent specific points in the bead-rs development timeline. These binaries are used for:

- **Compatibility testing**: Ensuring new versions can read old data formats
- **Feature validation**: Testing new features against pre-feature baselines
- **Migration testing**: Verifying upgrade/downgrade scenarios
- **Capability detection**: Testing feature probing and capability negotiation

## Binary 1: `bead-pre-attempt-resolution`

### Commit Information

| Property | Value |
|----------|-------|
| **Full SHA** | `946a7271796e15452c4a8a1f1ff9efc05d3e7307` |
| **Short SHA** | `946a727` |
| **Commit Date** | 2026-09-01 |
| **Author** | jedarden |
| **Commit Message** | `docs(boundaries): add reference shortcuts and pattern guide for attempt-resolution` |
| **Rust Version** | 1.85 |
| **Cargo Version** | bead-rs 0.2.6 |

### Binary Details

| Property | Value |
|----------|-------|
| **Binary Name** | `bead-pre-attempt-resolution` |
| **Binary Path** | `/home/coding/bead-rs/pinned-binaries/bead-pre-attempt-resolution` |
| **SHA256 Hash** | `d0da42bbf59b721bc64bc3d55610844efe3f1f06e37c2d9494c0b3dda6e29ac6` |
| **Binary Size** | 7.0M (7,340,032 bytes) |
| **Build Profile** | `release` |
| **Build Features** | `--no-default-features` |
| **Build Date** | 2026-09-02T01:35:01Z |

### Purpose

This binary represents the state of bead-rs **BEFORE** the `attempt-resolution` feature was implemented. It is built with `--no-default-features` to ensure maximum compatibility and exclude the optional attempt-resolution functionality.

### Build Procedure

To reproduce this binary from source:

```bash
# Navigate to repository
cd /home/coding/bead-rs

# Checkout exact commit
git checkout 946a7271796e15452c4a8a1f1ff9efc05d3e7307

# Build without default features (excludes attempt-resolution)
cargo build --release --no-default-features

# Binary will be at: target/release/bead
```

### Installation

```bash
# Option 1: Copy to local bin
cp /home/coding/bead-rs/pinned-binaries/bead-pre-attempt-resolution ~/.local/bin/bead-pre-attempt-resolution

# Option 2: Link for testing
ln -s /home/coding/bead-rs/pinned-binaries/bead-pre-attempt-resolution ~/.local/bin/bead-pre-attempt-resolution
```

### Verification

Verify binary identity and functionality:

```bash
# Verify SHA256 hash
sha256sum /home/coding/bead-rs/pinned-binaries/bead-pre-attempt-resolution
# Expected: d0da42bbf59b721bc64bc3d55610844efe3f1f06e37c2d9494c0b3dda6e29ac6

# Check version
/home/coding/bead-rs/pinned-binaries/bead-pre-attempt-resolution --version
# Expected: bead 0.2.6 (946a727 2026-09-02T01:35:01Z)

# Check binary is executable
test -x /home/coding/bead-rs/pinned-binaries/bead-pre-attempt-resolution && echo "Executable" || echo "Not executable"
```

---

## Binary 2: `bead-pre-feature`

### Commit Information

| Property | Value |
|----------|-------|
| **Full SHA** | `181f181b0e80f39f432846cabec30b0b7d640774` |
| **Short SHA** | `181f181` |
| **Commit Date** | 2026-09-01 17:12:43 -0400 |
| **Author** | jedarden |
| **Commit Message** | `feat(attempts): add old-format checkpoint fixtures` |

### Binary Details

| Property | Value |
|----------|-------|
| **Binary Name** | `bead-pre-feature` |
| **Binary Path** | `/home/coding/bead-rs/pinned-binaries/bead-pre-feature` |
| **SHA256 Hash** | `7e0e73defebb75fc987ddf8b6fb959f47c73ccbbcd7e066e2af302a6a43db6b5` |
| **Binary Size** | 6.5M (6,788,016 bytes) |

### Purpose

This binary represents an earlier baseline in the attempt-resolution feature development timeline. It was built when the checkpoint fixtures and data model changes for attempt-resolution were being added but before the feature was complete.

### Build Procedure

To reproduce this binary from source:

```bash
# Navigate to repository
cd /home/coding/bead-rs

# Checkout exact commit
git checkout 181f181b0e80f39f432846cabec30b0b7d640774

# Build with default configuration
cargo build --release

# Binary will be at: target/release/bead
```

### Installation

```bash
# Option 1: Copy to local bin
cp /home/coding/bead-rs/pinned-binaries/bead-pre-feature ~/.local/bin/bead-pre-feature

# Option 2: Link for testing
ln -s /home/coding/bead-rs/pinned-binaries/bead-pre-feature ~/.local/bin/bead-pre-feature
```

### Verification

```bash
# Verify SHA256 hash
sha256sum /home/coding/bead-rs/pinned-binaries/bead-pre-feature
# Expected: 7e0e73defebb75fc987ddf8b6fb959f47c73ccbbcd7e066e2af302a6a43db6b5

# Check binary is executable
test -x /home/coding/bead-rs/pinned-binaries/bead-pre-feature && echo "Executable" || echo "Not executable"
```

---

## Usage Instructions for Compatibility Testing

### Testing Old Binary → New Data Format

Test compatibility when using old binaries with newer data formats:

```bash
# Setup test environment
TEST_DIR=/tmp/bead-compat-test-$(date +%s)
mkdir -p "$TEST_DIR"
cd "$TEST_DIR"

# Initialize workspace with old binary
/home/coding/bead-rs/pinned-binaries/bead-pre-attempt-resolution init

# Create some test beads
/home/coding/bead-rs/pinned-binaries/bead-pre-attempt-resolution create --title "Test bead 1" --priority 0
/home/coding/bead-rs/pinned-binaries/bead-pre-attempt-resolution create --title "Test bead 2" --priority 1

# Try reading with new binary
/home/coding/target/debug/bead list
```

### Testing New Binary → Old Data Format

Test new binary compatibility with old checkpoint formats:

```bash
# Use the old-format fixtures
cd /home/coding/bead-rs/tests/fixtures/attempts/old

# Verify new binary can read old format
/home/coding/target/debug/bead list --json
```

### Feature Capability Detection

Test capability probing:

```bash
# Check what features each binary supports
/home/coding/bead-rs/pinned-binaries/bead-pre-attempt-resolution --help | grep -i attempt
/home/coding/target/debug/bead --help | grep -i attempt
```

---

## Verification Commands

### Quick Verification Script

```bash
#!/bin/bash
# verify-pinned-binaries.sh

echo "Verifying pinned binaries..."

# Binary 1: bead-pre-attempt-resolution
echo -n "Checking bead-pre-attempt-resolution... "
HASH1=$(sha256sum /home/coding/bead-rs/pinned-binaries/bead-pre-attempt-resolution | awk '{print $1}')
if [ "$HASH1" = "d0da42bbf59b721bc64bc3d55610844efe3f1f06e37c2d9494c0b3dda6e29ac6" ]; then
    echo "✓ OK"
else
    echo "✗ FAILED (got: $HASH1)"
fi

# Binary 2: bead-pre-feature
echo -n "Checking bead-pre-feature... "
HASH2=$(sha256sum /home/coding/bead-rs/pinned-binaries/bead-pre-feature | awk '{print $1}')
if [ "$HASH2" = "7e0e73defebb75fc987ddf8b6fb959f47c73ccbbcd7e066e2af302a6a43db6b5" ]; then
    echo "✓ OK"
else
    echo "✗ FAILED (got: $HASH2)"
fi

echo "Verification complete."
```

### Detailed Verification

```bash
# Check both binaries exist and are executable
for bin in bead-pre-attempt-resolution bead-pre-feature; do
    BINARY="/home/coding/bead-rs/pinned-binaries/$bin"
    if [ -x "$BINARY" ]; then
        echo "✓ $bin exists and is executable"
    else
        echo "✗ $bin is missing or not executable"
    fi
done

# Verify file sizes
echo "File sizes:"
ls -lh /home/coding/bead-rs/pinned-binaries/bead-pre-attempt-resolution
ls -lh /home/coding/bead-rs/pinned-binaries/bead-pre-feature
```

---

## Troubleshooting

### Common Issues

#### 1. Binary not executable

**Symptom**: Permission denied when running binary

**Solution**:
```bash
chmod +x /home/coding/bead-rs/pinned-binaries/bead-pre-attempt-resolution
chmod +x /home/coding/bead-rs/pinned-binaries/bead-pre-feature
```

#### 2. Binary corrupted or modified

**Symptom**: SHA256 hash doesn't match expected value

**Solution**:
```bash
# Rebuild from source
cd /home/coding/bead-rs

# For bead-pre-attempt-resolution
git checkout 946a7271796e15452c4a8a1f1ff9efc05d3e7307
cargo build --release --no-default-features
cp target/release/bead pinned-binaries/bead-pre-attempt-resolution

# For bead-pre-feature
git checkout 181f181b0e80f39f432846cabec30b0b7d640774
cargo build --release
cp target/release/bead pinned-binaries/bead-pre-feature
```

#### 3. Wrong architecture/platform

**Symptom**: Executable format error when running binary

**Solution**: These binaries are built for Linux x86_64. Ensure you're running on a compatible platform:
```bash
uname -m  # Should show x86_64
```

#### 4. Missing dependencies

**Symptom**: Binary runs but crashes with library errors

**Solution**: These binaries are statically compiled and should have no external dependencies. If you encounter issues, verify your system is compatible:
```bash
ldd /home/coding/bead-rs/pinned-binaries/bead-pre-attempt-resolution
ldd /home/coding/bead-rs/pinned-binaries/bead-pre-feature
```

#### 5. Version mismatch

**Symptom**: Binary reports unexpected version

**Solution**: Check you're using the correct binary:
```bash
/home/coding/bead-rs/pinned-binaries/bead-pre-attempt-resolution --version
# Expected: bead 0.2.6 (946a727 2026-09-02T01:35:01Z)

file /home/coding/bead-rs/pinned-binaries/bead-pre-attempt-resolution
# Should show: ELF 64-bit LSB executable, x86-64
```

### Recovery Procedures

If pinned binaries are lost or corrupted:

1. **Check git history**: The commits that added these binaries are tracked in git
2. **Rebuild from source**: Use the build procedures above
3. **Update documentation**: If commits change, update this document
4. **Verify checksums**: Always rebuild with exact commit SHAs

---

## Maintenance

### When to Update

Update pinned binaries when:
- Major feature releases occur
- Data format changes require new baselines
- Security vulnerabilities are found
- Compatibility testing requires newer baselines

### Update Procedure

1. Choose target commit SHA
2. Build binary with appropriate features
3. Calculate SHA256 hash
4. Update metadata files
5. Update this documentation
6. Test compatibility with new baseline

---

## Related Documentation

- [ADR-011: Atomic Idempotent Attempt Resolution](adr/011-atomic-idempotent-attempt-resolution.md)
- [ADR-012: Capability-Gated Attempt Contract Rollout](adr/012-capability-gated-attempt-contract-rollout.md)
- [Building with Attempt Resolution Feature](docs/build-attempt-resolution-binary.md)
- [Build Procedure](BUILD_PROCEDURE.md)
- [Old Format Fixtures](tests/fixtures/attempts/old/README.md)

---

## Summary Table

| Binary | Commit SHA | Date | Size | Features | Purpose |
|--------|-----------|------|------|----------|---------|
| `bead-pre-attempt-resolution` | `946a727` | 2026-09-01 | 7.0M | `--no-default-features` | Pre-attempt-resolution baseline |
| `bead-pre-feature` | `181f181` | 2026-09-01 | 6.5M | Default | Early development baseline |

**Last Updated**: 2026-09-02
**Document Version**: 1.0
**Maintainer**: bead-rs project
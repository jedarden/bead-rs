# Pinned Binaries

This directory contains pinned, reproducible builds of bead-rs binaries for compatibility testing.

## bead-pre-attempt-resolution

**Purpose**: Baseline binary built WITHOUT the `attempt-resolution` feature. This serves as the pre-feature reference for testing compatibility with the new attempt resolution feature.

**Built**: 2026-09-01

**Metadata**: See `bead-pre-attempt-resolution.metadata.json` for exact build details.

### Reproducing This Build

To reproduce this exact binary:

```bash
# 1. Checkout the exact commit
cd /home/coding/bead-rs
git checkout 181f181b0e80f39f432846cabec30b0b7d640774

# 2. Build with no default features
cargo build --release --no-default-features

# 3. Verify the hash matches
sha256sum /home/coding/target/release/bead
# Expected: 0b2ccfc74ad99aac18d2688b6f27786239ddfdb167642776f5ee83eebe6445ce
```

### Using the Pinned Binary

```bash
# Direct execution
./pinned-binaries/bead-pre-attempt-resolution --version

# For testing compatibility with new features
./pinned-binaries/bead-pre-attempt-resolution list --ready
```

### Verification

```bash
# Verify the binary hash
sha256sum pinned-binaries/bead-pre-attempt-resolution

# Verify the metadata
cat pinned-binaries/bead-pre-attempt-resolution.metadata.json
```

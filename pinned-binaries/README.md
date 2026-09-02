# Pinned bead-rs Binaries

This directory contains pinned binaries of bead-rs for compatibility testing and feature development.

## bead-pre-attempt-resolution

**Purpose:** Pre-feature baseline binary for attempt-resolution feature testing

**Build Date:** 2026-09-02

**Git Commit:** `946a7271796e15452c4a8a1f1ff9efc05d3e7307`

**Commit Message:** `docs(boundaries): add reference shortcuts and pattern guide for attempt-resolution`

**Binary Version:** `bead 0.2.6 (946a727 2026-09-02T01:35:01Z)`

**SHA256 Hash:** `d0da42bbf59b721bc64bc3d55610844efe3f1f06e37c2d9494c0b3dda6e29ac6`

**Binary Size:** 7.0M

### Build Procedure

```bash
# From clean git state at commit 946a7271796e15452c4a8a1f1ff9efc05d3e7307
cd /home/coding/bead-rs
cargo build --release --no-default-features
```

**Feature Flag Used:** `--no-default-features`

**Rationale:** This binary was built WITHOUT the `attempt-resolution` feature enabled. It serves as a pre-feature baseline for compatibility testing when the attempt-resolution feature is being developed. Building with `--no-default-features` ensures that no optional features are included, providing a clean baseline for comparison.

### Verification

To verify the binary:

```bash
sha256sum pinned-binaries/bead-pre-attempt-resolution
# Should output: d0da42bbf59b721bc64bc3d55610844efe3f1f06e37c2d9494c0b3dda6e29ac6

./pinned-binaries/bead-pre-attempt-resolution --version
# Should output: bead 0.2.6 (946a727 2026-09-02T01:35:01Z)
```

### Usage

This binary is intended for:
- Compatibility testing during attempt-resolution feature development
- Baseline comparison for feature functionality
- Test fixtures that require a pre-attempt-resolution binary

### Important Notes

- This binary represents the state BEFORE attempt-resolution feature was merged
- When attempt-resolution feature is completed, a new post-feature binary should be added
- The `--no-default-features` flag ensures maximum compatibility by excluding optional features
- This binary should remain unchanged once pinned to maintain reproducibility

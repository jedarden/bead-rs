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

## bead-attempt-resolution-e115609

**Purpose:** Post-feature binary with attempt-resolution feature enabled for integration testing

**Build Date:** 2026-09-02

**Git Commit:** `e1156098b01264bb998797047115521261443c13`

**Commit Message:** `feat(tests): add binary variant integration test suite for capability detection`

**Binary Version:** `bead 0.2.6 (e115609 2026-09-02T07:23:55Z)`

**SHA256 Hash:** `68fe8d534721be4ba4147312364d8f0b216b62f3093e85e7c91f0a0db695a645`

**Binary Size:** 7.0M

### Build Procedure

```bash
# From clean git state at commit e1156098b01264bb998797047115521261443c13
cd /home/coding/bead-rs
cargo build --release
```

**Feature Flag Used:** Default features (attempt-resolution included)

**Rationale:** This binary was built WITH the `attempt-resolution` feature enabled by default. It serves as the post-feature baseline for compatibility testing and integration test suites that validate the attempt-resolution functionality.

### Verification

To verify the binary:

```bash
sha256sum pinned-binaries/bead-attempt-resolution-e115609
# Should output: 68fe8d534721be4ba4147312364d8f0b216b62f3093e85e7c91f0a0db695a645

./pinned-binaries/bead-attempt-resolution-e115609 --version
# Should output: bead 0.2.6 (e115609 2026-09-02T07:23:55Z)
```

### Usage

This binary is intended for:
- Integration testing of attempt-resolution feature
- Capability detection test fixtures
- Post-feature baseline comparisons

### Important Notes

- This binary represents the state AFTER attempt-resolution feature was fully implemented
- Used by binary variant integration test suite for capability detection
- This binary should remain unchanged once pinned to maintain reproducibility

---

### Important Notes (All Binaries)

- Pinned binaries represent specific commit states for reproducible testing
- Each binary should remain unchanged once pinned to maintain reproducibility
- Pre-feature binary built without attempt-resolution for baseline comparisons
- Post-feature binary built with attempt-resolution for integration testing

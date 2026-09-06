# Pinned bead-rs Binaries

This directory contains pinned binaries of bead-rs for compatibility testing and feature development.

## Pin inventory (this directory is the pin location)

`pinned-binaries/` at the repo root (`/home/coding/bead-rs/pinned-binaries/`) is the pin location of record. Exactly four binaries are pinned here, each with a `*.metadata.json` recording its hash, size, and provenance:

| Pin | Source commit | Metadata file |
|-----|---------------|---------------|
| `bead-pre-feature` | `af023ad` (release 0.2.4) | `bead-pre-feature.metadata.json` |
| `bead-pre-attempt-resolution` | `946a727` | `bead-pre-attempt-resolution.metadata.json` |
| `bead-attempt-resolution-e115609` | `e115609` | `bead-attempt-resolution-e115609.metadata.json` |
| `bead-attempt-resolution-f25ab5c` | `f25ab5c` (HEAD pin) | `bead-attempt-resolution-f25ab5c.metadata.json` |

**Naming scheme:** `<name>-<shaslice>`, where `<shaslice>` is the first 7 hex characters of the source commit (`bead-attempt-resolution-f25ab5c` → `f25ab5c`). The two baseline pins predate this convention and keep role-only names; their source commit is recorded in their metadata files and in `COMMITS.md`.

**Rebuilding from this table:** the `Source commit` column is built-from provenance, not a rebuild input — all four of those commits are lost-lineage objects (force-pushed away 2026-09-02) and do not resolve here (verified 2026-09-03). To rebuild a pin, use the restored-lineage twin recorded as `restored_lineage_twin_sha` in the pin's `*.metadata.json` (also listed as the rebuild target in `COMMITS.md`) through the sanctioned archive path: `scripts/build-from-archive.sh <twin-sha>` (see `../BUILD_PROCEDURE.md`, "Build Rule").

**Everything else in this directory is documentation or metadata, not a pin:** `README.md`, `COMMITS.md`, `BINARY_VERIFICATION.md`, `commits.json` (machine-readable commit/binary registry), and `bead-metadata.json` / `bead-release-metadata.json` (provenance records for the working debug and release builds in `/home/coding/target/`, not pins). This table is maintained against `ls pinned-binaries/` — a binary not in the table is not a pin, and a pin missing from the table means this section is stale.

## bead-pre-feature

**Purpose:** Earliest baseline, built before the attempt-resolution work began (the feature does not exist in this tree)

**Build Date:** 2026-09-01 (embedded build timestamp `2026-09-01T19:14:12Z`)

**Git Commit:** `af023ad47740cf5458f52398e70937b2cc1c18df` (release 0.2.4)

**Binary Version:** `bead 0.2.4 (af023ad 2026-09-01T19:14:12Z)`

**SHA256 Hash:** `7e0e73defebb75fc987ddf8b6fb959f47c73ccbbcd7e066e2af302a6a43db6b5`

**Binary Size:** 6.5M (6,788,016 bytes)

**Metadata File:** `bead-pre-feature.metadata.json` — commit attribution here was reconstructed from the binary's own embedded version string (verified 2026-09-02, beadrs-b6441e82); earlier docs wrongly attributed this binary to `181f181`.

### Build Procedure

```bash
# From git state at commit af023ad47740cf5458f52398e70937b2cc1c18df
cd /home/coding/bead-rs
cargo build --release
```

**Feature Flag Used:** default features (`attempt-resolution` did not yet exist in Cargo.toml)

**Rationale:** this is the "before any of the feature work" comparison point. A rebuild will not reproduce the pinned hash (build.rs embeds the build timestamp); verify by hash comparison against the pinned bytes.

### Verification

```bash
sha256sum pinned-binaries/bead-pre-feature
# Should output: 7e0e73defebb75fc987ddf8b6fb959f47c73ccbbcd7e066e2af302a6a43db6b5

./pinned-binaries/bead-pre-feature --version
# Should output: bead 0.2.4 (af023ad 2026-09-01T19:14:12Z)
```

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

**Rationale:** This binary was built without the `attempt-resolution` cargo flag enabled. It serves as a pre-flag baseline for compatibility testing. Because the flag is an empty marker that gates no code, this binary is functionally identical to a flag-enabled build of the same commit — `bead resolve` works and `capabilities` advertises `attempt_outcome`.

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

**Binary Version:** `bead 0.2.6 (e115609-dirty 2026-09-02T07:23:55Z)`

**SHA256 Hash:** `68fe8d534721be4ba4147312364d8f0b216b62f3093e85e7c91f0a0db695a645`

**Binary Size:** 7.0M

### Build Procedure

```bash
# From clean git state at commit e1156098b01264bb998797047115521261443c13
cd /home/coding/bead-rs
cargo build --release
```

**Feature Flag Used:** default features (plain `cargo build --release`; `default = []` at this commit, so the `attempt-resolution` flag was not explicitly enabled — irrelevant in practice, since the flag gates no code)

**Rationale:** This binary was built at the commit that added the binary-variant integration test suite. It serves as the post-feature baseline for compatibility testing and integration test suites that validate the attempt-resolution functionality.

### Verification

To verify the binary:

```bash
sha256sum pinned-binaries/bead-attempt-resolution-e115609
# Should output: 68fe8d534721be4ba4147312364d8f0b216b62f3093e85e7c91f0a0db695a645

./pinned-binaries/bead-attempt-resolution-e115609 --version
# Should output: bead 0.2.6 (e115609-dirty 2026-09-02T07:23:55Z)
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

## bead-attempt-resolution-f25ab5c

**Purpose:** Current HEAD binary with attempt-resolution feature, pinned byte-exact from the build bead's staging area

**Build Date:** 2026-09-02

**Git Commit:** `f25ab5c91c09a3408f23b9cdf2f3e95e81abc060`

**Commit Message:** `docs(attempts-binary): add comprehensive build process and verification documentation`

**Binary Version:** `bead 0.2.6 (f25ab5c-dirty 2026-09-02T10:52:25Z)`

**SHA256 Hash:** `9a8455f25bacf5bc961bd740442fdc1b30a67fb6e38d304c23c97a57cf57b04e`

**Binary Size:** 7.0M (7305184 bytes)

### Build Procedure

```bash
# From git state at commit f25ab5c91c09a3408f23b9cdf2f3e95e81abc060
cd /home/coding/bead-rs
cargo build --release --features attempt-resolution
```

**Feature Flag Used:** `--features attempt-resolution`

**Rationale:** This is the HEAD build captured by the build-bead (`beadrs-efb89f33`) and pinned byte-exact by the pinning bead (`beadrs-759601d4`). **Do not rebuild and expect this hash**: build.rs re-embeds `BEAD_BUILD_TIMESTAMP` whenever `.git/index` changes, so two rebuilds of identical source hash differently. The pinned bytes are the artifact of record.

**On the `-dirty` version marker:** the marker comes from tracked `.beads/*` checkpoint files modified by the bead CLI's post-claim auto-flush at build time. The compiled tracked source (`src/`, `Cargo.toml`, `Cargo.lock`, `build.rs`) was exactly HEAD, verified via `git status --porcelain` on those paths — see build.rs: "untracked files are excluded: they do not alter what was compiled."

**On the pin's SHA of record (informational):** the commit that recorded this pin (63c2ee8) says *"pin HEAD attempt-resolution binary at b0d7840"* in its message, but `b0d7840` is not this pin's SHA. The pin's name, its metadata file (`git_commit_sha: f25ab5c91c09a3408f23b9cdf2f3e95e81abc060`), and the binary's own embedded version string (`bead 0.2.6 (f25ab5c-dirty …)`) all say `f25ab5c` and agree with each other — that recorded agreement, not the pinning commit's message, is the pin's authority. The pinning commit was made on the force-pushed lineage, where this same change is `b0d7840` (it is 63c2ee8's direct parent), while the binary was built on the twin lineage where it is `f25ab5c`; merge b057d2768a859270b2d9e8855f1467bfb3521a84 later restored that lineage to `main`, so `b0d7840` is reachable today as the content-identical twin of `f25ab5c` (the `f25ab5c` object itself no longer exists here or on any origin ref, so tree identity rests on the matching commit message and the metadata record — `git diff b0d7840 f25ab5c` cannot be run). The message's `b0d7840` stays informational history, never authority: reference this pin as `f25ab5c`, and use `b0d7840` only as the rebuild target (`scripts/build-from-archive.sh b0d7840f6c96cd45e16ea05b7babdb42ef0d2654 --features attempt-resolution`, per the metadata's `restored_lineage_twin_sha`).

### Verification

```bash
sha256sum pinned-binaries/bead-attempt-resolution-f25ab5c
# Should output: 9a8455f25bacf5bc961bd740442fdc1b30a67fb6e38d304c23c97a57cf57b04e

./pinned-binaries/bead-attempt-resolution-f25ab5c --version
# Should output: bead 0.2.6 (f25ab5c-dirty 2026-09-02T10:52:25Z)

./pinned-binaries/bead-attempt-resolution-f25ab5c capabilities | grep attempt_outcome
# Should show attempt_outcome supported with native-v1 receipt schema
```

### Usage

This binary is intended for:
- Current-HEAD reference for capability detection tests
- Reproducibility anchor: exact bytes that were verified to advertise `attempt_outcome` support (5 outcomes / 5 actions, `native-v1` receipt/request schema URNs)
- Compatibility comparisons against the earlier pinned baselines

### Important Notes

- `attempt-resolution` is an empty marker cargo feature: no `#[cfg(feature)]` gates exist in `src/`, and `bead capabilities` advertises `attempt_outcome` regardless of the flag
- This binary should remain unchanged once pinned to maintain reproducibility

### Important Notes (All Binaries)

- Pinned binaries represent specific commit states for reproducible testing
- Each binary should remain unchanged once pinned to maintain reproducibility
- The `attempt-resolution` cargo flag is an empty marker that gates no code: builds made with and without it are functionally identical (see `BINARY_VERIFICATION.md`). The pins differ by **commit**, not by flag.
- Verify a pin by comparing its sha256 against its `*.metadata.json`. Never verify by rebuilding — `build.rs` re-embeds the build timestamp, so no rebuild reproduces a pinned hash

---

## Build Documentation

For comprehensive build instructions, metadata capture procedures, and reproducible build recipes, see:

- **[docs/attempts-binary-build.md](../docs/attempts-binary-build.md)** - Complete build process and verification guide

This document includes:
- Feature flag configuration and usage
- Standard vs feature-enabled build procedures
- Metadata capture steps (version, hash, size, timestamps)
- Binary uniqueness verification
- Reproducible build recipes
- Integration testing guidance
- Troubleshooting guide

---

## Binary Uniqueness Verification

### Hash Comparison Results

All pinned binaries are cryptographically distinct:

| Binary | SHA256 Hash | Size | Build Type |
|--------|-------------|------|------------|
| bead-pre-feature | `7e0e73defebb75fc987ddf8b6fb959f47c73ccbbcd7e066e2af302a6a43db6b5` | 6.5M | Standard (no features) |
| bead-pre-attempt-resolution | `d0da42bbf59b721bc64bc3d55610844efe3f1f06e37c2d9494c0b3dda6e29ac6` | 7.0M | No default features |
| bead-attempt-resolution-e115609 | `68fe8d534721be4ba4147312364d8f0b216b62f3093e85e7c91f0a0db695a645` | 7.0M | Feature-enabled |
| bead-attempt-resolution-f25ab5c | `9a8455f25bacf5bc961bd740442fdc1b30a67fb6e38d304c23c97a57cf57b04e` | 7.0M | Feature-enabled (HEAD pin) |

✅ **VERIFIED**: Different hashes prove each build configuration produces a unique binary

### Size Analysis

- **Standard build (pre-feature)**: 6.5M - built at release 0.2.4 (commit `af023ad`), before the attempt-resolution work began
- **Feature-enabled builds**: 7.0M - ~517KB larger than the 0.2.4 baseline

⚠️ The size difference reflects all development between 0.2.4 and HEAD, **not** the `attempt-resolution` flag itself: the feature is an empty marker (no `#[cfg]` gates, and `bead resolve` is present in `bead-pre-attempt-resolution`/`bead-attempt-resolution-*` regardless of how the flag was set — see BINARY_VERIFICATION.md). The 0.2.4 baseline has no `bead resolve` subcommand at all. Distinctness between the pins is a hash-level fact; it is not evidence of what the flag contributes.

---

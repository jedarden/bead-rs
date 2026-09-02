# Pinned Binary Commits

This file documents the exact git commits for pinned bead-rs binaries used in compatibility testing and feature validation.

## Pre-Feature Commit

**Commit SHA**: `af023ad47740cf5458f52398e70937b2cc1c18df`  
**Short SHA**: `af023ad`  
**Message**: `chore(beadrs-4fcead71): release 0.2.4 — v0.2.3 tag landed behind a checkpoint commit`  
**Date**: 2026-08-29  
**Purpose**: Earliest baseline binary, built before the attempt-resolution work began

This is the release 0.2.4 commit. The `attempt-resolution` cargo feature does not exist in this tree, so `bead-pre-feature` is the true "before any feature work" comparison point. Attribution was reconstructed from the binary's embedded version string (`bead 0.2.4 (af023ad 2026-09-01T19:14:12Z)`) and verified against this commit by beadrs-b6441e82 — earlier documentation wrongly attributed this binary to `181f181`.

## Pre-Attempt-Resolution Commit

**Commit SHA**: `946a7271796e15452c4a8a1f1ff9efc05d3e7307`  
**Short SHA**: `946a727`  
**Message**: `docs(boundaries): add reference shortcuts and pattern guide for attempt-resolution`  
**Date**: 2026-09-01  
**Purpose**: Baseline binary built WITHOUT the attempt-resolution feature

This commit represents the state of bead-rs just **before** the `attempt-resolution` cargo flag was added (the flag landed in 9efbc92). The attempt-resolution *functionality* is already fully present here — `bead resolve` works and `capabilities` advertises `attempt_outcome`. The binary was built with `--no-default-features`, which is functionally identical to a flag-enabled build of this commit because the flag gates no code.

## Post-Attempt-Resolution Commit (flag introduced; no binary pinned)

**Commit SHA**: `1ee45e39f518a1e47a26fd312bbadbc36b1af00c`  
**Short SHA**: `1ee45e3`  
**Message**: `docs(build): add reproducible build instructions to README`  
**Date**: 2026-09-02  
**Purpose**: First commit carrying the `attempt-resolution` cargo flag plus build documentation

⚠️ **No pinned binary exists for this commit.** An earlier revision of the tracking docs referenced a binary named `bead-feature-enabled` at this commit; no such binary was ever committed. The feature-enabled binaries of record are `bead-attempt-resolution-e115609` and `bead-attempt-resolution-f25ab5c` (below).

## Usage in Build Steps

When referencing these commits in build scripts or CI/CD pipelines:

```bash
# Pre-attempt-resolution-flag build (functionally identical to a flag-enabled build)
PRE_ATTEMPT_COMMIT="946a7271796e15452c4a8a1f1ff9efc05d3e7307"
git checkout $PRE_ATTEMPT_COMMIT
cargo build --release --no-default-features

# Feature-enabled build at a pinned commit
FEATURE_COMMIT="f25ab5c91c09a3408f23b9cdf2f3e95e81abc060"
git checkout $FEATURE_COMMIT
cargo build --release --features attempt-resolution
```

## Verification

To verify these commits are reachable in your repository:

```bash
# Verify pre-attempt-resolution commit
git log --oneline | grep 946a727

# Verify post-attempt-resolution commit  
git log --oneline | grep 1ee45e3

# Show full commit details
git show 946a7271796e15452c4a8a1f1ff9efc05d3e7307
git show 1ee45e39f518a1e47a26fd312bbadbc36b1af00c
```

## Integration Test Binary

**Commit SHA**: `e1156098b01264bb998797047115521261443c13`  
**Short SHA**: `e115609`  
**Message**: `feat(tests): add binary variant integration test suite for capability detection`  
**Date**: 2026-09-02  
**Purpose**: Binary built WITH attempt-resolution feature for integration testing

This commit represents the current state of bead-rs with the attempt-resolution feature fully implemented and integrated into the test suite. The binary built from this commit includes the complete attempt-resolution functionality and is used for capability detection tests.

## Current HEAD Binary (Pinned)

**Commit SHA**: `f25ab5c91c09a3408f23b9cdf2f3e95e81abc060`  
**Short SHA**: `f25ab5c`  
**Message**: `docs(attempts-binary): add comprehensive build process and verification documentation`  
**Date**: 2026-09-02  
**Purpose**: HEAD binary built WITH attempt-resolution feature, pinned byte-exact

The compiled tracked source at this commit is identical to `e115609` (the commits in between touch only tests, docs, and pinned-binary artifacts), but the pinned bytes are unique because build.rs re-embeds the build timestamp. This is the binary of record for the current HEAD state — see its metadata file for the `-dirty` version-marker explanation and the rebuild-non-reproducibility caveat.

**Note on this pin's SHA (informational only):** the pinning commit 63c2ee8's message names `b0d7840`, which was not the pin's commit — at pin time `b0d7840` had been force-pushed out of `main` and did not exist in the tree. The authoritative SHA is the one recorded in the pin's metadata file (`git_commit_sha`) and printed by the binary's embedded version string — `f25ab5c` — and the two agree. Merge 7d08577 later restored the force-pushed lineage, making `b0d7840` reachable again as a content-identical twin of `f25ab5c`, so nothing was lost; `f25ab5c` remains the SHA of record, and no documentation treats `b0d7840` as authoritative.

## Binary Metadata

Pins follow the `<name>-<shaslice>` naming scheme (`<shaslice>` = first 7 hex characters of the source commit); the two baseline pins predate this convention — see `pinned-binaries/README.md`, "Pin inventory".

For complete binary metadata (SHA256 hashes, build timestamps, etc.), see:
- `pinned-binaries/commits.json` - Machine-readable commit/binary registry (this file's data source)
- `pinned-binaries/bead-pre-attempt-resolution.metadata.json` - Pre-attempt-resolution binary details
- `pinned-binaries/bead-attempt-resolution-e115609.metadata.json` - Post-attempt-resolution binary details
- `pinned-binaries/bead-attempt-resolution-f25ab5c.metadata.json` - Current HEAD binary details (pinned)
- `pinned-binaries/bead-pre-feature.metadata.json` - Early development baseline details
- `docs/pinned-binaries.md` - Comprehensive documentation

## Binary Verification

For complete verification of binary distinctness, capability comparison, and build reproducibility, see:
- `pinned-binaries/BINARY_VERIFICATION.md` - Comprehensive verification report with SHA256 hash validation, functional capability testing, and reproducible verification steps

**Last Updated**: 2026-09-02
**Document Version**: 1.4
**Verification Status**: ✅ Complete - All acceptance criteria met

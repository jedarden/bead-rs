# Pinned Binary Commits

This file documents the exact git commits for pinned bead-rs binaries used in compatibility testing and feature validation.

## Pre-Attempt-Resolution Commit

**Commit SHA**: `946a7271796e15452c4a8a1f1ff9efc05d3e7307`  
**Short SHA**: `946a727`  
**Message**: `docs(boundaries): add reference shortcuts and pattern guide for attempt-resolution`  
**Date**: 2026-09-01  
**Purpose**: Baseline binary built WITHOUT the attempt-resolution feature

This commit represents the state of bead-rs **before** the attempt-resolution feature was implemented. The binary built from this commit uses `--no-default-features` to exclude the attempt-resolution functionality.

## Post-Attempt-Resolution Commit (Feature-Enabled)

**Commit SHA**: `1ee45e39f518a1e47a26fd312bbadbc36b1af00c`  
**Short SHA**: `1ee45e3`  
**Message**: `docs(build): add reproducible build instructions to README`  
**Date**: 2026-09-01  
**Purpose**: Baseline binary built WITH the attempt-resolution feature enabled

This commit represents the state of bead-rs **after** the attempt-resolution feature was fully implemented and documented. The binary built from this commit includes the complete attempt-resolution functionality.

## Usage in Build Steps

When referencing these commits in build scripts or CI/CD pipelines:

```bash
# Pre-attempt-resolution build (without feature)
PRE_ATTEMPT_COMMIT="946a7271796e15452c4a1f1ff9efc05d3e7307"
git checkout $PRE_ATTEMPT_COMMIT
cargo build --release --no-default-features

# Post-attempt-resolution build (with feature)
POST_ATTEMPT_COMMIT="1ee45e39f518a1e47a26fd312bbadbc36b1af00c"
git checkout $POST_ATTEMPT_COMMIT
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

## Binary Metadata

For complete binary metadata (SHA256 hashes, build timestamps, etc.), see:
- `pinned-binaries/bead-pre-attempt-resolution.metadata.json` - Pre-attempt-resolution binary details
- `pinned-binaries/bead-attempt-resolution-e115609.metadata.json` - Post-attempt-resolution binary details
- `pinned-binaries/bead-attempt-resolution-f25ab5c.metadata.json` - Current HEAD binary details (pinned)
- `pinned-binaries/bead-pre-feature.metadata.json` - Early development baseline details
- `docs/pinned-binaries.md` - Comprehensive documentation

## Binary Verification

For complete verification of binary distinctness, capability comparison, and build reproducibility, see:
- `pinned-binaries/BINARY_VERIFICATION.md` - Comprehensive verification report with SHA256 hash validation, functional capability testing, and reproducible verification steps

**Last Updated**: 2026-09-02
**Document Version**: 1.3
**Verification Status**: ✅ Complete - All acceptance criteria met

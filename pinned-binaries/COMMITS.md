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

## Binary Metadata

For complete binary metadata (SHA256 hashes, build timestamps, etc.), see:
- `pinned-binaries/bead-pre-attempt-resolution.metadata.json` - Pre-attempt-resolution binary details
- `pinned-binaries/bead-pre-feature.metadata.json` - Early development baseline details
- `docs/pinned-binaries.md` - Comprehensive documentation

**Last Updated**: 2026-09-02  
**Document Version**: 1.0

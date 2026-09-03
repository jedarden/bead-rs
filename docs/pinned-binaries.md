# Pinned bead-rs Binaries Documentation

This document provides comprehensive information about the pinned bead-rs binaries used for compatibility testing and feature development validation.

## Overview

The `pinned-binaries/` directory contains four pinned binaries representing specific points in the bead-rs development timeline: `bead-pre-feature` (release 0.2.4, earliest baseline), `bead-pre-attempt-resolution` (946a727), `bead-attempt-resolution-e115609`, and `bead-attempt-resolution-f25ab5c` (HEAD pin). Each ships with a `*.metadata.json` recording its hash, size, and provenance; `pinned-binaries/README.md` and `pinned-binaries/BINARY_VERIFICATION.md` cover all four. These binaries are used for:

- **Compatibility testing**: Ensuring new versions can read old data formats
- **Feature validation**: Testing new features against pre-feature baselines
- **Migration testing**: Verifying upgrade/downgrade scenarios
- **Capability detection**: Testing feature probing and capability negotiation

Rebuilds of any pinned commit go through `scripts/build-from-archive.sh <sha>`: pinned binaries are built from a git-archive extraction in scratch, never by stashing, resetting, or checking out commits inside the shared checkout at `/home/coding/bead-rs` (see `../BUILD_PROCEDURE.md`, "Build Rule"). The script can only build commits that still resolve in this repo — check `git cat-file -t <sha>` first. The source commits the existing pins were built from are unreachable after the 2026-09-02 twin-lineage force-push, so they are recorded as built-from provenance only (see `pinned-binaries/README.md`, "Pin inventory", and `pinned-binaries/COMMITS.md`, "SHA lineage and provenance"); the per-binary invocations below are runnable today — each builds the pin's restored-lineage content twin (same subject, author date, and tree content), recorded as `restored_lineage_twin_sha` in the pin's `*.metadata.json`.

## Binary 1: `bead-pre-attempt-resolution`

### Commit Information

| Property | Value |
|----------|-------|
| **Source commit (built-from provenance, unresolvable)** | `946a7271796e15452c4a8a1f1ff9efc05d3e7307` |
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
| **Binary Size** | 7.0M (7,305,144 bytes) |
| **Build Profile** | `release` |
| **Build Features** | `--no-default-features` |
| **Build Date** | 2026-09-02T01:35:01Z |

### Purpose

This binary was built just before the `attempt-resolution` cargo **feature flag** was added to Cargo.toml (the flag landed in `0c7bab9`, the restored-lineage twin of the originally-recorded `9efbc92`, which the 2026-09-02 force-push removed — see `pinned-binaries/COMMITS.md`, "SHA lineage and provenance"). The attempt-resolution *functionality* was already fully present in this tree — `bead resolve` works and `capabilities` advertises `attempt_outcome` — because the feature is an empty marker that gates no code (see `pinned-binaries/BINARY_VERIFICATION.md`). It is built with `--no-default-features` for maximum compatibility, which changes nothing functionally.

### Build Procedure

To reproduce this binary from source, build its restored-lineage twin through the sanctioned archive-build path:

```bash
cd /home/coding/bead-rs
# bf10936 = restored-lineage content twin of 946a727 (the built-from commit
# recorded above): same subject, author date, and tree content. The original
# object was removed by the 2026-09-02 force-push and no longer resolves.
scripts/build-from-archive.sh bf1093601dbb6367378a09c813700b4664115a51
```

The script extracts this commit's tree into a scratch directory and builds there; the shared checkout is never moved to the pinned commit. This pin was recorded with `--no-default-features`, which is functionally identical to the script's default-feature build because the attempt-resolution flag gates no code (see `pinned-binaries/BINARY_VERIFICATION.md`).

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
| **Source commit (built-from provenance, unresolvable)** | `af023ad47740cf5458f52398e70937b2cc1c18df` |
| **Short SHA** | `af023ad` |
| **Commit Date** | 2026-08-29 22:45:04 -0400 |
| **Author** | jedarden |
| **Commit Message** | `chore(beadrs-4fcead71): release 0.2.4 — v0.2.3 tag landed behind a checkpoint commit` |

> **Attribution note (corrected 2026-09-02, beadrs-b6441e82):** this binary was previously documented as built at `181f181`. That is wrong: the binary's own embedded version string reads `bead 0.2.4 (af023ad 2026-09-01T19:14:12Z)`, and `181f181`'s Cargo.toml declares 0.2.6 while the pinned binary embeds 0.2.4. The commit recorded above is taken from the binary itself, which build.rs derives from git at compile time.

### Binary Details

| Property | Value |
|----------|-------|
| **Binary Name** | `bead-pre-feature` |
| **Binary Path** | `/home/coding/bead-rs/pinned-binaries/bead-pre-feature` |
| **Binary Version** | `bead 0.2.4 (af023ad 2026-09-01T19:14:12Z)` |
| **SHA256 Hash** | `7e0e73defebb75fc987ddf8b6fb959f47c73ccbbcd7e066e2af302a6a43db6b5` |
| **Binary Size** | 6.5M (6,788,016 bytes) |
| **Metadata File** | `pinned-binaries/bead-pre-feature.metadata.json` |

### Purpose

This binary is the earliest baseline in the attempt-resolution feature development timeline: it is the release 0.2.4 build, which predates the attempt-resolution work entirely — the `attempt-resolution` cargo feature does not exist in this tree. It is the correct comparison point for "the binary before the feature work began"; `bead-pre-attempt-resolution` (946a727) is the later baseline built just before the capability was advertised.

### Build Procedure

To reproduce this binary from source, build its restored-lineage twin through the sanctioned archive-build path:

```bash
cd /home/coding/bead-rs
# ea4e317 = restored-lineage content twin of af023ad (the built-from commit
# recorded above): same subject, author date, and tree content. The original
# object was removed by the 2026-09-02 force-push and no longer resolves.
scripts/build-from-archive.sh ea4e317e697306275aa1a781497a133f472c0df5
```

The script extracts this commit's tree into a scratch directory and builds there; the shared checkout is never moved to the pinned commit.

> A rebuild will **not** reproduce the pinned hash: build.rs re-embeds `BEAD_BUILD_TIMESTAMP` whenever `.git/index` changes. Verify the pin by comparing its sha256 against the `binary_sha256` in its `*.metadata.json`, never by rebuilding.
>
> (Builds are not currently reproducible on demand: the committed `build.rs` embeds a wall-clock timestamp and honors no override — deterministic rebuilds via `SOURCE_DATE_EPOCH` / `BEAD_COMMIT_SHA` are tracked as separate work. Once they land they will reproduce a build recipe, not a pin; verification remains byte comparison.)

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

**Solution**: Restore the pinned bytes from git — the binaries are committed to the repo, so the correct copy is the committed one. **Do not rebuild to restore a pin**: `build.rs` re-embeds `BEAD_BUILD_TIMESTAMP` on every build, so a rebuild hashes differently and copying it over the pin would replace the pinned bytes with a different artifact. Restore with `git show`, which writes only the named file — this shared checkout must never have its HEAD moved or its index touched by a restore, since other workers hold in-flight edits here.

```bash
cd /home/coding/bead-rs

# Restore any pinned binary from the committed bytes — only after sha256sum proves the pin is corrupt
git show HEAD:pinned-binaries/bead-pre-feature > pinned-binaries/bead-pre-feature
git show HEAD:pinned-binaries/bead-pre-attempt-resolution > pinned-binaries/bead-pre-attempt-resolution
git show HEAD:pinned-binaries/bead-attempt-resolution-f25ab5c > pinned-binaries/bead-attempt-resolution-f25ab5c

# Confirm against the metadata files
sha256sum pinned-binaries/bead-pre-feature
# must equal binary_sha256 in pinned-binaries/bead-pre-feature.metadata.json
```

Rebuilding from source is only for producing a *new* artifact (record its provenance; see `docs/attempts-binary-build.md`), and it goes through the archive script:

```bash
# bead-pre-feature: release 0.2.4 (feature did not yet exist)
# (ea4e317 = restored-lineage twin of the built-from commit af023ad)
scripts/build-from-archive.sh ea4e317e697306275aa1a781497a133f472c0df5

# bead-pre-attempt-resolution (recorded with --no-default-features; the flag gates no code)
# (bf10936 = restored-lineage twin of the built-from commit 946a727)
scripts/build-from-archive.sh bf1093601dbb6367378a09c813700b4664115a51
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
2. Build it with `scripts/build-from-archive.sh <sha>` (the only sanctioned build path)
3. Calculate SHA256 hash
4. Update metadata files
5. Update this documentation
6. Test compatibility with new baseline

---

## Related Documentation

- [ADR-011: Atomic Idempotent Attempt Resolution](adr/011-atomic-idempotent-attempt-resolution.md)
- [ADR-012: Capability-Gated Attempt Contract Rollout](adr/012-capability-gated-attempt-contract-rollout.md)
- [Building with Attempt Resolution Feature](build-attempt-resolution-binary.md)
- [Build Process and Verification](attempts-binary-build.md)
- [Build Procedure](../BUILD_PROCEDURE.md)
- [Old Format Fixtures](../tests/fixtures/attempts/old/README.md)

---

## Summary Table

| Binary | Source commit | Date | Size | Features | Purpose |
|--------|-----------|------|------|----------|---------|
| `bead-pre-feature` | `af023ad` | 2026-08-29 | 6.5M | Default (feature did not yet exist) | Early development baseline (release 0.2.4) |
| `bead-pre-attempt-resolution` | `946a727` | 2026-09-01 | 7.0M | `--no-default-features` | Pre-attempt-resolution-flag baseline (functionality already present) |
| `bead-attempt-resolution-e115609` | `e115609` | 2026-09-02 | 7.0M | default (plain build; flag not explicitly enabled) | Post-feature test binary (flag gates no code) |
| `bead-attempt-resolution-f25ab5c` | `f25ab5c` | 2026-09-02 | 7.0M | `--features attempt-resolution` | HEAD pin, byte-exact from staged build |

**Naming scheme:** pins follow `<name>-<shaslice>`, where `<shaslice>` is the first 7 hex characters of the binary's source (built-from) commit — `bead-attempt-resolution-f25ab5c` → `f25ab5c`, matching the `Source commit` column above (built-from provenance — those commits no longer resolve; rebuilds go through the restored-lineage twins, see "Overview"). The two baseline pins predate this convention and keep role-only names: `bead-pre-feature` and `bead-pre-attempt-resolution` are the `af023ad` / `946a727` rows above. See `pinned-binaries/README.md`, "Pin inventory", for the authoritative table.

**Last Updated**: 2026-09-03
**Document Version**: 1.1
**Maintainer**: bead-rs project
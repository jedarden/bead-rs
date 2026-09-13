# bead-rs Build Procedure with Attempt-Resolution Feature

## Binary Information

- **Binary of record**: `pinned-binaries/bead-attempt-resolution-f25ab5c`
- **Version**: 0.2.6
- **Commit SHA (of record, rebuild target)**: `b0d7840f6c96cd45e16ea05b7babdb42ef0d2654`
- **Binary SHA256**: `9a8455f25bacf5bc961bd740442fdc1b30a67fb6e38d304c23c97a57cf57b04e`
- **Build Date**: 2026-09-02
- **Build Profile**: release (`--features attempt-resolution`)

The commit the pinned binary was actually built from is recorded as built-from
provenance in `pinned-binaries/bead-attempt-resolution-f25ab5c.metadata.json`;
it belongs to the force-pushed-away lineage and no longer exists in any clone
(see `pinned-binaries/COMMITS.md`, "SHA lineage and provenance"). The SHA
above is its restored-lineage content twin — the commit to rebuild from.

## Attempt-Resolution Feature

The attempt-resolution functionality is **always enabled** in the current bead-rs codebase. No special feature flag is required. The `bead resolve` command is available by default in the release binary.

### Verify Attempt-Resolution Functionality

```bash
# Check that resolve command is available
/home/coding/target/release/bead --help | grep resolve
# Output: resolve            Record an execution attempt outcome atomically
```

## Build Rule

Pinned binaries are built from a git-archive extraction in scratch via
`scripts/build-from-archive.sh <sha>` — never by stashing, resetting, or
checking out commits inside this shared checkout. `/home/coding/bead-rs` is a
single shared NEEDLE workspace: moving its HEAD or index rewires every other
worker's tree and can erase uncommitted work, so the script is the only
sanctioned way to produce a pinned binary.

## Build Procedure

### Prerequisites
- Rust 1.85 or later
- Standard build tools (cargo, make, etc.)

### Step 1: Build the pinned commit from a git-archive extraction

```bash
cd /home/coding/bead-rs
scripts/build-from-archive.sh b0d7840f6c96cd45e16ea05b7babdb42ef0d2654 --features attempt-resolution
```

The script runs `git archive <sha> | tar -x` into a fresh scratch directory
under `~/scratch`, builds there with `CARGO_TARGET_DIR` inside that same
directory, prints the binary path and its sha256, and copies the binary and
its metadata (sha, sha256) to the pinned location. The scratch directory is
removed on success and left in place on failure for diagnosis. The shared
checkout's HEAD, index, stash, and working tree are never touched.

Reachability caveat: the script can only build commits that still resolve in
this repo — check with `git cat-file -t <sha>` first. The example commit above
resolves. The pins' *original* build commits (named inside the metadata files)
do not: they were lost with the 2026-09-02 twin-lineage force-push, which is
why every pin carries a resolvable restored-lineage twin as its SHA of record
(`pinned-binaries/COMMITS.md`).

### Step 2: Verify Binary

```bash
# Check version (use the binary path the script printed)
<binary-path-from-script> --version
# Archive builds embed `unknown` as the commit (a git-archive extraction has
# no .git for build.rs to read), e.g.: bead 0.2.6 (unknown 2026-09-03T…Z).
# The source commit is the one you passed to the script.

# Verify resolve command is available
<binary-path-from-script> resolve --help
```

### Step 3: Calculate Binary Hash (for verification)

```bash
sha256sum <binary-path-from-script>
```

## Installation

### System-wide Installation

```bash
cargo install --path .
# Or copy the built binary:
sudo cp /home/coding/target/release/bead /usr/local/bin/bead
```

### User-local Installation

```bash
mkdir -p ~/.local/bin
cp /home/coding/target/release/bead ~/.local/bin/bead
# Ensure ~/.local/bin is in your PATH
```

## Attempt-Resolution Commands

The binary includes the following attempt-resolution functionality:

- `bead resolve` - Record an execution attempt outcome atomically
- Support for outcome types: verified_success, work_failure, infrastructure_failure, cancelled, indeterminate
- Schema URNs: `urn:bead-rs:schema:attempt-outcome:native-v1`, `urn:bead-rs:schema:resolve-receipt:native-v1`

## Test Fixtures

The repository includes test fixtures for attempt-resolution at:
- `tests/fixtures/attempts/new/` - New format fixtures
- `tests/fixtures/attempts/old/` - Legacy format fixtures
- `tests/attempt.rs` - Attempt outcome model tests
- `tests/test_migration_on_open.rs` - Migration tests

## Verification

To verify the binary is working correctly with attempt-resolution:

```bash
# Initialize a test workspace
mkdir -p /tmp/bead-test
cd /tmp/bead-test
/home/coding/target/release/bead init --prefix test

# Create a test bead
TEST_BEAD=$(/home/coding/target/release/bead create --title "Test resolve functionality" --priority 1)

# Verify resolve command works
/home/coding/target/release/bead resolve --help
```

## Commit Pinning

The pinned artifact of record is `pinned-binaries/bead-attempt-resolution-f25ab5c`,
and the commit to rebuild it from is:

```
b0d7840f6c96cd45e16ea05b7babdb42ef0d2654
```

To reproduce the build, go through the archive script:

```bash
scripts/build-from-archive.sh b0d7840f6c96cd45e16ea05b7babdb42ef0d2654 --features attempt-resolution
```

The script extracts the pinned commit's tree into a scratch directory and
builds there; the shared checkout is never moved to the pinned commit to do
it. The commit the original binary was built from is itself unreachable in
this repo and every clone (lost with the 2026-09-02 twin-lineage force-push);
`b0d7840` above is its restored-lineage content twin, verified by identical
subject, author date, and tree content. The original full SHA survives only
as built-from provenance in the pin's metadata file.

**Recorded hash is hash-only, not rebuild-verifiable.** `build.rs` embeds a
wall-clock build timestamp, so a fresh build yields a different sha256 than
the `9a8455f2…` value recorded above. Treat the recorded hash as the identity
of the pinned artifact (compare it against the committed bytes in
`pinned-binaries/`), and treat a script run as the proof that a new build came
from the pinned tree. Deterministic rebuilds (`SOURCE_DATE_EPOCH`) are tracked
as beadrs-dc295092 / beadrs-baba38b8; until they land, do not expect a fresh
build to match any recorded hash.

An earlier revision of this document pinned a different build by a commit +
sha256 pair that corresponded to no commit in any clone and no committed
artifact anywhere; that pair was removed on 2026-09-03 (beadrs-e030cc56).

## Notes

- The attempt-resolution feature is integrated into the main binary, not a separate feature flag
- The build process produces some warnings about unused fields, but these do not affect functionality
- SQLite is bundled, so there are no runtime dependencies
- The binary supports the full bead-rs lifecycle plus the new attempt-resolution commands

---

**Last Updated**: 2026-09-13 — the binary of record is re-pointed at its
restored-lineage twin (`b0d7840`); every recorded binary sha256 in
`pinned-binaries/` is hash-only, not rebuild-verifiable, until deterministic
builds land (see `pinned-binaries/COMMITS.md`, "SHA lineage and provenance").

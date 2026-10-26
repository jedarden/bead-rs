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
/build/bead-rs/release/bead --help | grep resolve
# Output: resolve            Record an execution attempt outcome atomically
```

On fleet build hosts, the cargo wrapper enforces
`CARGO_TARGET_DIR=/build/<repo>`, where `<repo>` is the origin URL's basename.
For this repository, a direct release build therefore produces
`/build/bead-rs/release/bead`. Hosts without the wrapper's `/build` directory
use Cargo's default `target/release/bead` path instead.

## Building from source

*(Moved verbatim from `README.md` on 2026-10-26, beadrs-5e2144e8, so the
README's Quick start sits directly under Install there.)*

### Environment requirements

- **Rust version:** 1.85 or newer (minimum MSRV: 1.85)
- **Tested with:** rustc 1.97.1 (8bab26f4f 2026-07-14)
- **Edition:** Rust 2024
- **Cargo:** 1.85 or newer

### Build commands

```bash
# Clone and build the current main tip
git clone https://github.com/jedarden/bead-rs.git
cd bead-rs

# Build the main binary
cargo build --release --bin bead

# (Optional) Build man page generator
cargo build --release --bin generate-man-pages

# (Optional) Build with attempt-resolution feature enabled
cargo build --release --bin bead --features attempt-resolution
```

### Building a pinned commit

Never `git checkout` an older commit in a shared checkout to build it — that
has destroyed another worker's uncommitted work here. Use the archive script,
which extracts the commit read-only and builds in a scratch directory:

```bash
scripts/build-from-archive.sh <commit-sha> --features attempt-resolution
```

See [BUILD_PROCEDURE.md](BUILD_PROCEDURE.md) ("Build Rule") and
[docs/build-attempt-resolution-binary.md](docs/build-attempt-resolution-binary.md).

### Note on byte-reproducibility

`build.rs` embeds a wall-clock build timestamp and reads no environment
override, so two builds of the same source are never byte-identical and **no
fresh build reproduces a recorded pin's hash** — pinned binaries are verified
by comparing their sha256 against the `binary_sha256` in their
`*.metadata.json`, never by rebuilding. Deterministic rebuilds
(`SOURCE_DATE_EPOCH` / `BEAD_COMMIT_SHA`) are tracked as separate work and are
not in the committed build script yet. Details:
[docs/build-attempt-resolution-binary.md](docs/build-attempt-resolution-binary.md),
"What 'reproduce' means here".

### Build artifacts

The fleet build hosts run a cargo wrapper (`~/.local/bin/cargo`) that enforces
one cargo target directory per repository — `CARGO_TARGET_DIR=/build/<repo>`,
where `<repo>` is the origin URL's basename (needle-d6b685b4). A
`--target-dir` outside that directory is refused, and a foreign
`CARGO_TARGET_DIR` is overridden. On such a host the binaries land at:

- `bead` binary: `/build/bead-rs/release/bead`
- Man page generator: `/build/bead-rs/release/generate-man-pages`

On a host without `/build` the wrapper leaves cargo's default in place, and
the same binaries sit at `target/release/bead` and
`target/release/generate-man-pages` relative to the repository root.

### Feature flags

- **default** (empty): No features enabled by default
- **attempt-resolution**: marker feature for the attempt-resolution contract; it currently gates no code — the functionality is always compiled in (see [docs/build-attempt-resolution-binary.md](docs/build-attempt-resolution-binary.md), "Distinctness")

### Current version information

- **Version:** 0.2.6
- **Feature-enabled build SHA (declared rebuild target):** `861cdcbfebeb70a9ebc6a2e33ee98cef97274fec`
- **Pinned binaries of record:** [`pinned-binaries/`](pinned-binaries/) — see [pinned-binaries/COMMITS.md](pinned-binaries/COMMITS.md) for the SHA lineage and each pin's built-from provenance
- **Git repository:** https://github.com/jedarden/bead-rs

### Verifying the build

After building, verify the installation:

```bash
# Hosts with /build (enforced per-repo target directory):
/build/bead-rs/release/bead --version
/build/bead-rs/release/bead capabilities

# Hosts without /build (cargo's default target directory):
target/release/bead --version
target/release/bead capabilities
```

The `--version` output should show version 0.2.6, and `capabilities` should emit a machine-readable feature contract document.

### Comprehensive build documentation

For detailed build procedures, metadata capture, and binary uniqueness verification, see:

- **[docs/attempts-binary-build.md](docs/attempts-binary-build.md)** - Complete attempts binary build process and verification guide
- **[docs/build-attempt-resolution-binary.md](docs/build-attempt-resolution-binary.md)** - Attempt-resolution build process and binary distinctness
- **[pinned-binaries/README.md](pinned-binaries/README.md)** - Pinned binary documentation and hash comparisons
- **[BUILD_PROCEDURE.md](BUILD_PROCEDURE.md)** - Step-by-step build instructions

### Pinned feature-enabled build (attempt-resolution)

The feature-enabled pins of record live in [`pinned-binaries/`](pinned-binaries/),
built with `cargo build --release --locked --features attempt-resolution` via
`scripts/build-from-archive.sh`:

| Pin | sha256 | Built from (provenance) | Rebuild target |
|---|---|---|---|
| `pinned-binaries/bead-attempt-resolution-f25ab5c` | `9a8455f25bacf5bc961bd740442fdc1b30a67fb6e38d304c23c97a57cf57b04e` | `f25ab5c91c09…` (lost lineage) | `b0d7840f6c96cd45e16ea05b7babdb42ef0d2654` |
| `pinned-binaries/bead-attempt-resolution-e115609` | `68fe8d534721be4ba4147312364d8f0b216b62f3093e85e7c91f0a0db695a645` | `e1156098b01…` (lost lineage) | `861cdcbfebeb70a9ebc6a2e33ee98cef97274fec` |

`~/.local/bin/bead-77db95e` (sha256
`e9d44131f7cfab3bf43d6a9dc0040e7759ed64f278808a354576f418429150b4`) is an
older install-copy of a build whose commit the 2026-09-02 force-push removed;
the hash is true of that file, but its commit no longer resolves and
`~/.local/bin` is not the pin location of record.

To verify a pin — by hash comparison, never by rebuilding (see "Note on
byte-reproducibility"):

```bash
sha256sum pinned-binaries/bead-attempt-resolution-f25ab5c
# Compare against binary_sha256 in pinned-binaries/bead-attempt-resolution-f25ab5c.metadata.json

./pinned-binaries/bead-attempt-resolution-f25ab5c --version
# bead 0.2.6 (f25ab5c-dirty 2026-09-02T10:52:25Z)

./pinned-binaries/bead-attempt-resolution-f25ab5c capabilities | jq '.attempt_outcome.supported'
# true
```

To build a fresh feature-enabled binary, see "Building a pinned commit" above.

### Development build

For faster iteration during development:

```bash
# Build without optimizations (faster compilation)
cargo build --bin bead

# Run tests
cargo test

# Check code formatting and linting
cargo fmt --check
cargo clippy -- -D warnings
```

### Installing locally

To install the built binary to your local cargo bin directory:

```bash
cargo install --path .
```

This installs `bead` to `~/.cargo/bin/bead` (or your configured CARGO_HOME).

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
sudo cp /build/bead-rs/release/bead /usr/local/bin/bead
```

### User-local Installation

```bash
mkdir -p ~/.local/bin
cp /build/bead-rs/release/bead ~/.local/bin/bead
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
/build/bead-rs/release/bead init --prefix test

# Create a test bead
TEST_BEAD=$(/build/bead-rs/release/bead create --title "Test resolve functionality" --priority 1)

# Verify resolve command works
/build/bead-rs/release/bead resolve --help
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

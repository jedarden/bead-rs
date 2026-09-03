# Pinned Binary Commits

This file documents the exact git commits for pinned bead-rs binaries used in compatibility testing and feature validation.

The pinned binaries themselves live in `pinned-binaries/` at the repo root — that directory is the pin location of record (`/home/coding/bead-rs/pinned-binaries/`); see its `README.md`, "Pin inventory", for the authoritative pin table.

## SHA lineage and provenance (read this first)

On 2026-09-02 the repository's `main` history was force-pushed and later
reconciled by merge b057d2768a859270b2d9e8855f1467bfb3521a84, which restored
the pushed-away lineage as a set of **content-twin commits**: same subject,
same author and date, identical tree content, different SHAs. The pinned
binaries were built from commits on the pre-force-push lineage, and those
commit objects no longer exist in any clone (verified 2026-09-03: absent from
this clone's object store, zero unreachable commits under `git fsck`, and no
ref on origin holds them), so the pins were **re-pointed** at their
restored-lineage twins by beadrs-e030cc56.

Consequences, applied consistently across the pin records:

- **The commit SHA in this file is the restored-lineage twin** — a resolvable
  object, and the commit a rebuild must use
  (`scripts/build-from-archive.sh <sha>`). It is the rebuild target, never a
  pin's provenance: what each binary was actually built from stays in the
  metadata (`git_commit_sha`).
- The commit each binary was **actually built from** is recorded as built-from
  provenance in the pin's `*.metadata.json` (`git_commit_sha`); it is an
  object of the lost lineage and is **expected to fail**
  `git cat-file -e <sha>^{commit}`. That is documented reality, not breakage.
- **Pin filenames keep the original build commit's 7-hex slice**
  (`bead-attempt-resolution-e115609` etc.) for artifact-identity continuity;
  the slice is not itself a resolvable ref — it names the built-from
  provenance recorded in the metadata, not the rebuild target.
- **Every recorded binary sha256 is hash-only.** `build.rs` embeds a
  wall-clock build timestamp, so no fresh build reproduces any recorded
  pin's hash. Verify a pin by comparing its sha256 against the
  `binary_sha256` in its `*.metadata.json`, never by rebuilding.
  (`SOURCE_DATE_EPOCH` + `BEAD_COMMIT_SHA` make two fresh builds of the
  same tree byte-identical — see `docs/pinned-binaries.md`, "Build
  Procedure" — but that reproduces a build recipe, never a recorded pin:
  every pin was built without them.)

## Pre-Feature Commit

**Commit SHA (rebuild target)**: `ea4e317e697306275aa1a781497a133f472c0df5`  
**Short SHA**: `ea4e317`  
**Message**: `chore(beadrs-4fcead71): release 0.2.4 — v0.2.3 tag landed behind a checkpoint commit`  
**Date**: 2026-08-29  
**Purpose**: Earliest baseline binary, built before the attempt-resolution work began

This is the release 0.2.4 commit. The `attempt-resolution` cargo feature does not exist in this tree, so `bead-pre-feature` is the true "before any feature work" comparison point. Attribution was reconstructed from the binary's embedded version string (`bead 0.2.4 (af023ad 2026-09-01T19:14:12Z)`) and verified by beadrs-b6441e82 — earlier documentation wrongly attributed the binary to a later commit whose tree already declares 0.2.6. The binary was actually built from this commit's lost-lineage twin, named by the shaslice in the pin's filename and recorded in `bead-pre-feature.metadata.json`.

## Pre-Attempt-Resolution Commit

**Commit SHA (rebuild target)**: `bf1093601dbb6367378a09c813700b4664115a51`  
**Short SHA**: `bf10936`  
**Message**: `docs(boundaries): add reference shortcuts and pattern guide for attempt-resolution`  
**Date**: 2026-09-01  
**Purpose**: Baseline binary built WITHOUT the attempt-resolution feature

This commit represents the state of bead-rs just **before** the `attempt-resolution` cargo flag was added (the flag landed in 0c7bab961616fd077cf46b2ac7b7b6f379528d64). The attempt-resolution *functionality* is already fully present here — `bead resolve` works and `capabilities` advertises `attempt_outcome`. The binary was built with `--no-default-features`, which is functionally identical to a flag-enabled build of this commit because the flag gates no code and is not in default features.

## Post-Attempt-Resolution Commit (flag introduced; no binary pinned)

**Commit SHA (rebuild target)**: `c8836e0138c4b305739dd7e33d0fa10efe1242f9`  
**Short SHA**: `c8836e0`  
**Message**: `docs(build): add reproducible build instructions to README`  
**Date**: 2026-09-02  
**Purpose**: First commit carrying the `attempt-resolution` cargo flag plus build documentation

⚠️ **No pinned binary exists for this commit.** An earlier revision of the tracking docs referenced a binary named `bead-feature-enabled` at this commit; no such binary was ever committed. The feature-enabled binaries of record are `bead-attempt-resolution-e115609` and `bead-attempt-resolution-f25ab5c` (below).

## Usage in Build Steps

When referencing these commits in build scripts or CI/CD pipelines, build them through the sanctioned archive script — pinned binaries are built from a git-archive extraction in scratch, never by moving the shared checkout to the pinned commit:

```bash
# Pre-attempt-resolution-flag build (a plain build here is functionally
# identical to the recorded --no-default-features build: the flag gates no code)
scripts/build-from-archive.sh bf1093601dbb6367378a09c813700b4664115a51

# Feature-enabled build at a pinned commit
scripts/build-from-archive.sh b0d7840f6c96cd45e16ea05b7babdb42ef0d2654 --features attempt-resolution
```

The script extracts the commit's tree via `git archive` into a scratch directory and builds there, so the shared checkout's HEAD, index, stash, and working tree are untouched (see `../BUILD_PROCEDURE.md`, "Build Rule"). Both example commits resolve today; check `git cat-file -t <sha>` if you are unsure about any other.

## Verification

To verify the rebuild-target twins are reachable in your repository:

```bash
for sha in ea4e317 bf10936 c8836e0 861cdcb b0d7840; do
  git cat-file -e "$sha"^{commit} && echo "OK $sha" || echo "MISSING $sha"
done
# Expected: all five OK, from any fresh clone of origin
```

The built-from SHAs inside the `*.metadata.json` files name lost-lineage
objects and are expected to fail this check — see "SHA lineage and
provenance" above.

## Integration Test Binary — Declared Feature-Enabled Build SHA

**Commit SHA (declared build target)**: `861cdcbfebeb70a9ebc6a2e33ee98cef97274fec`  
**Short SHA**: `861cdcb`  
**Message**: `feat(tests): add binary variant integration test suite for capability detection`  
**Date**: 2026-09-02  
**Purpose**: Binary built WITH attempt-resolution feature for integration testing

**This section declares the single canonical feature-enabled build SHA** — the
exact commit an attempt-resolution binary must be built from
(`scripts/build-from-archive.sh <sha> --features attempt-resolution`). Chosen
2026-09-03 by the beadrs-90f9a509 candidate audit; recorded by beadrs-12dd0849:

1. **It is the restored-lineage twin of `e115609`** — the commit this record
   originally named. Twin status is two-way verified:
   `bead-attempt-resolution-e115609.metadata.json` declares
   `restored_lineage_twin_sha = 861cdcbfebeb70a9ebc6a2e33ee98cef97274fec`,
   and the twin's commit message matches the metadata's `git_commit_message`.
2. **The original candidate `e115609` is rejected as a build target**: the
   object no longer exists in any clone (`git cat-file -t
   e1156098b01264bb998797047115521261443c13` fails — see "SHA lineage and
   provenance" above). A SHA that cannot resolve cannot be rebuilt from.
3. **It resolves everywhere**: it is one of the five SHAs the verification
   loop below requires to be OK from any fresh clone of origin, and
   `git branch -r --contains 861cdcb` lists `origin/main`.
4. **It is the earliest resolvable commit carrying the complete
   attempt-resolution feature**: `src/model/attempt.rs`,
   `src/service/attempt.rs`, and `src/service/capabilities.rs` are all in its
   tree. (The feature flag itself lands earlier, at `c8836e0`; the pre-flag
   baseline is `bf10936`.)
5. **Its compiled source is still current**: `git diff --stat
   861cdcb..HEAD -- src/` is empty — every later commit touches only docs,
   tests, build tooling, and pin bookkeeping. A feature-enabled build from
   this SHA compiles the same code as one from HEAD, while naming the feature
   commit rather than a later bookkeeping tip.

Commits landing after this declaration (including the commit that records
this paragraph) do not supersede it: the SHA above remains the declared build
target, and by item 5 it produces the same attempt-resolution code as any
later tip of `main`.

## Current HEAD Binary (Pinned)

**Commit SHA (rebuild target)**: `b0d7840f6c96cd45e16ea05b7babdb42ef0d2654`  
**Short SHA**: `b0d7840`  
**Message**: `docs(attempts-binary): add comprehensive build process and verification documentation`  
**Date**: 2026-09-02  
**Purpose**: HEAD binary built WITH attempt-resolution feature, pinned byte-exact

The compiled tracked source at this commit is identical to `861cdcb` (the commits in between touch only docs), but the pinned bytes are unique because build.rs re-embeds the build timestamp. This is the binary of record for the current HEAD state — see its metadata file for the `-dirty` version-marker explanation and the rebuild-non-reproducibility caveat.

**Note on this pin's SHA (informational):** the pinning commit 63c2ee8's message names `b0d7840`, which at pin time had been force-pushed out of `main`, so the pin was recorded against the builder's own (now lost) lineage commit instead — the pin is `bead-attempt-resolution-f25ab5c`: its filename shaslice, its metadata `git_commit_sha: f25ab5c91c09a3408f23b9cdf2f3e95e81abc060`, and the binary's embedded version string (`bead 0.2.6 (f25ab5c-dirty …)`) all agree on `f25ab5c`. Merge b057d2768a859270b2d9e8855f1467bfb3521a84 restored the pushed-away lineage, making `b0d7840` reachable again as the content-identical twin of that build commit, so this file lists `b0d7840` only as the rebuild target; the pin's provenance of record remains `f25ab5c`, and the pinning message's claim stays informational history, never authority.

## Binary Metadata

Pins follow the `<name>-<shaslice>` naming scheme (`<shaslice>` = first 7 hex characters of the **original build commit**, which for the four 2026-09-01/02 pins is a lost-lineage object — see "SHA lineage and provenance"); the two baseline pins predate this convention — see `pinned-binaries/README.md`, "Pin inventory".

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

**Last Updated**: 2026-09-03
**Document Version**: 1.6
**Verification Status**: ✅ Complete - All acceptance criteria met

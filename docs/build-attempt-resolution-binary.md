# Building bead-rs with the attempt-resolution feature

This document records how the feature-enabled attempt-resolution binary is
built, which commit to build it from, and how it differs from the pre-feature
baselines. The pin records themselves live in [`pinned-binaries/`](../pinned-binaries/);
the step-by-step metadata-capture walk-through is
[docs/attempts-binary-build.md](attempts-binary-build.md) and the full
distinctness report is
[`pinned-binaries/BINARY_VERIFICATION.md`](../pinned-binaries/BINARY_VERIFICATION.md).

## Pinned binaries of record

The feature-enabled binaries live in `pinned-binaries/` — the pin location of
record (`pinned-binaries/COMMITS.md`):

| Pin | sha256 | Built from (provenance) | Rebuild target (resolvable twin) |
|---|---|---|---|
| `bead-attempt-resolution-f25ab5c` | `9a8455f25bacf5bc961bd740442fdc1b30a67fb6e38d304c23c97a57cf57b04e` | `f25ab5c91c09a3408f23b9cdf2f3e95e81abc060` (lost lineage) | `b0d7840f6c96cd45e16ea05b7babdb42ef0d2654` |
| `bead-attempt-resolution-e115609` | `68fe8d534721be4ba4147312364d8f0b216b62f3093e85e7c91f0a0db695a645` | `e1156098b01264bb998797047115521261443c13` (lost lineage) | `861cdcbfebeb70a9ebc6a2e33ee98cef97274fec` |

The pins were built from commits on a lineage that a 2026-09-02 force-push
removed; a later merge restored that work as **content twins** (same subject,
author date, and tree — different SHAs). The built-from SHAs survive only in
each pin's `*.metadata.json` and no longer resolve; the rebuild targets do.
See `pinned-binaries/COMMITS.md`, "SHA lineage and provenance", before
reasoning about any of these SHAs.

## The commit to build from

**`861cdcbfebeb70a9ebc6a2e33ee98cef97274fec`** is the declared canonical
feature-enabled build SHA (`pinned-binaries/COMMITS.md`, "Integration Test
Binary — Declared Feature-Enabled Build SHA"). It is the earliest resolvable
commit carrying the complete attempt-resolution feature
(`src/model/attempt.rs`, `src/service/attempt.rs`,
`src/service/capabilities.rs` are all in its tree), and the commits after it
touch only docs, tests, build tooling, and pin bookkeeping —
`git diff --stat 861cdcb..HEAD -- src/` is empty — so a feature-enabled build
from it compiles the same code as one from any later tip of `main`.

## Build procedure

`scripts/build-from-archive.sh` is the only sanctioned build path: it
extracts the commit with `git archive` into a scratch directory and builds
there, so the shared checkout's HEAD, index, stash, and working tree are
never touched (`BUILD_PROCEDURE.md`, "Build Rule" — checking out a pinned
commit in the shared checkout is how another worker's uncommitted hour was
erased on 2026-09-01/02). Never `git checkout <sha>` to build a pin.

```bash
cd /home/coding/bead-rs

# The documented rebuild of the declared feature-enabled build SHA:
scripts/build-from-archive.sh 861cdcbfebeb70a9ebc6a2e33ee98cef97274fec \
  --features attempt-resolution

# A throwaway build (verification, not a pin) — keep it out of pinned-binaries/:
scripts/build-from-archive.sh 861cdcbfebeb70a9ebc6a2e33ee98cef97274fec \
  --features attempt-resolution \
  --name bead-rebuild-check --out /var/tmp
```

Under the hood the script runs `cargo build --release --locked` with
`CARGO_TARGET_DIR` pointed at the scratch tree, copies the binary and a
metadata file to `--out` (default `pinned-binaries/`), and refuses to
overwrite an existing pin. It requires ~5 GB free under `~/scratch` and
leaves the scratch directory in place only on failure.

## Environment

- **Toolchain:** rustc/cargo 1.97.1 built the current pins; the crate's MSRV
  is 1.85 (`Cargo.toml` `rust-version`, edition 2024). A recorded pin's hash
  is specific to the toolchain that produced it.
- **Feature flags:** `--features attempt-resolution` (the feature is an empty
  marker — see "Distinctness" below; `--no-default-features` builds the same
  code, which is exactly what the `bead-pre-attempt-resolution` baseline pin
  proves).
- **`--locked`:** the script always passes it; the pinned `Cargo.lock` is part
  of the recipe.
- **Environment variables:** `SOURCE_DATE_EPOCH` (pins the embedded build
  timestamp) and `BEAD_COMMIT_SHA` (names the commit in trees with no `.git`,
  such as this script's archive extractions) together make two builds of the
  same tree byte-identical — `tests/reproducible_build.rs` asserts that. The
  script's own metadata output documents them.

### What "reproduce" means here

A rebuild of the same source is **not** byte-identical by default:
`build.rs` embeds a wall-clock `BEAD_BUILD_TIMESTAMP`, so every unpinned
build hashes differently. Consequences:

- **Verify a pinned binary by hash comparison, never by rebuilding** —
  compare its sha256 against the `binary_sha256` in its `*.metadata.json`.
- A fresh build of `861cdcb` reproduces the *recipe* (same code, same
  capabilities), not any recorded pin's hash. A rebuild hash differing from
  the pin's is the expected outcome, not a failed verification.
- With `SOURCE_DATE_EPOCH` and `BEAD_COMMIT_SHA` set, two builds of the same
  tree are byte-identical — that reproduces a build recipe, still never a
  recorded pin, since every pin was built without them.

## Distinctness

Re-verified 2026-09-03 (beadrs-0737200a) by `sha256sum` over the pins and
comparison against each `*.metadata.json` — all four match their recorded
hashes and no two are equal:

| Binary | sha256 | `--version` | `resolve` in `--help` | `attempt_outcome` in `capabilities` |
|---|---|---|---|---|
| `bead-pre-feature` (0.2.4 baseline) | `7e0e73defebb75fc987ddf8b6fb959f47c73ccbbcd7e066e2af302a6a43db6b5` | `bead 0.2.4 (af023ad 2026-09-01T19:14:12Z)` | absent | absent |
| `bead-pre-attempt-resolution` | `d0da42bbf59b721bc64bc3d55610844efe3f1f06e37c2d9494c0b3dda6e29ac6` | `bead 0.2.6 (946a727 2026-09-02T01:35:01Z)` | present | present |
| `bead-attempt-resolution-e115609` | `68fe8d534721be4ba4147312364d8f0b216b62f3093e85e7c91f0a0db695a645` | `bead 0.2.6 (e115609-dirty 2026-09-02T07:23:55Z)` | present | present |
| `bead-attempt-resolution-f25ab5c` | `9a8455f25bacf5bc961bd740442fdc1b30a67fb6e38d304c23c97a57cf57b04e` | `bead 0.2.6 (f25ab5c-dirty 2026-09-02T10:52:25Z)` | present | present |

Every hash is pairwise distinct, so the feature-enabled binaries are
demonstrably different binaries from the pre-feature ones. What the
difference **does not** prove: `attempt-resolution` is an empty cargo
feature that gates no code, and the flag-less `bead-pre-attempt-resolution`
already carries the full resolve functionality. The gap between
`bead-pre-feature` and the pins reflects all development from 0.2.4 to the
pin commits (including the arrival of `resolve` itself), not the flag.
Distinctness is a hash-level fact; attribution is recorded above.

## Verification steps

```bash
cd /home/coding/bead-rs

# 1. Hashes: every pin must match its metadata, and no two may be equal
for b in pinned-binaries/bead-*; do
  [ -x "$b" ] || continue
  sha256sum "$b"
done

# 2. Distinctness at the function level
./pinned-binaries/bead-pre-feature --help | grep -c resolve          # 0
./pinned-binaries/bead-attempt-resolution-f25ab5c --help | grep -c resolve  # 1
./pinned-binaries/bead-pre-feature capabilities | jq '.attempt_outcome // "absent"'
./pinned-binaries/bead-attempt-resolution-f25ab5c capabilities \
  | jq '.attempt_outcome.supported'                                  # true

# 3. The documented rebuild executes end to end (throwaway location)
scripts/build-from-archive.sh 861cdcbfebeb70a9ebc6a2e33ee98cef97274fec \
  --features attempt-resolution --name bead-rebuild-check --out /var/tmp
```

## References

- [`pinned-binaries/COMMITS.md`](../pinned-binaries/COMMITS.md) — commit
  lineage, the declared feature-enabled build SHA, and the resolvability
  verification loop
- [`pinned-binaries/BINARY_VERIFICATION.md`](../pinned-binaries/BINARY_VERIFICATION.md)
  — the complete distinctness and capability verification report
- [`docs/attempts-binary-build.md`](attempts-binary-build.md) — build +
  metadata-capture walk-through per pin
- [`docs/pinned-binaries.md`](pinned-binaries.md) — per-binary pin records
  with runnable rebuild invocations
- [`BUILD_PROCEDURE.md`](../BUILD_PROCEDURE.md) — the Build Rule and why
  archive extraction is the only sanctioned path
- [ADR-011](../adr/011-atomic-idempotent-attempt-resolution.md),
  [ADR-012](../adr/012-capability-gated-attempt-contract-rollout.md) — the
  attempt-resolution contract this binary exists to exercise

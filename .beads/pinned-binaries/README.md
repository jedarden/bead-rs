# Pinned Binaries for Capability Testing

This directory contains pinned bead-rs binaries for testing capability differences between pre-feature and feature-enabled versions.

## Binaries

### Feature-Enabled Binary (attempt-resolution-complete)
- **Binary:** `bead-feature-enabled`
- **Built from:** `5bb28bf7b853be7ba244adf3ce4c76b8d1bd01e5` — lost-lineage provenance since the 2026-09-02 twin-lineage force-push (unreachable in every clone); recorded by the binary's embedded version string `bead 0.2.6 (5bb28bf-dirty 2026-09-01T20:16:37Z)`
- **Rebuild target (restored-lineage twin):** `e38430e42d2d915e8035dbaf7bcd59b43e12acbb` — same subject and author date, reachable from `origin/main` (reconciled by beadrs-da594f4b, 2026-09-03)
- **Committed:** 266a4e5 (`feat(pluck): set up pinned binaries and compatibility framework`), an `origin/main` ancestor
- **Date:** 2026-09-01 15:45:58 -0400
- **Message:** `docs(boundaries): document attempt-resolution feature boundary commits`
- **Hash:** `e6a8ffb8b9d6b6cbba2d98f0458e62c3e211c1590d7abacd178419299a41a318` (verified live 2026-09-03; compare bytes, never rebuild — the hash embeds a wall-clock build timestamp)
- **Capabilities:** Full attempt-resolution support with atomic idempotent outcome recording (verified live 2026-09-03: `capabilities` reports `attempt_outcome.supported=true` with `resolve` present; `resolve --help` exits 0)

### Pre-Feature Binary
**Status:** Built and pinned — the pin of record lives in the canonical registry
- **Binary:** `pinned-binaries/bead-pre-feature` (tracked; sha256 `7e0e73defebb75fc987ddf8b6fb959f47c73ccbbcd7e066e2af302a6a43db6b5`)
- **Built from:** `af023ad` (release 0.2.4); embedded version string `bead 0.2.4 (af023ad 2026-09-01T19:14:12Z)`
- **Registry entries:** `pinned-binaries/commits.json` (`pre_feature`) and `pinned-binaries/COMMITS.md`
- **History:** this manifest originally targeted the `attempt-resolution-pre` boundary commit `53dade07ff2b9afda87e67459a825ec7e138dafa`, which was lost with the 2026-09-02 twin-lineage force-push (content twin: `785e4bb`). No build is pending; the reconciled 0.2.4 baseline is the pre-feature pin of record (reconciled by beadrs-455a56ac, 2026-09-03).

## Verifying the Pre-Feature Binary

Do not rebuild to verify — `build.rs` embeds a wall-clock timestamp, so a fresh
build never reproduces the recorded sha256. Compare bytes against the recorded
hash, and never check out old commits inside this shared workspace (the only
sanctioned rebuild path is `scripts/build-from-archive.sh <sha>`):

```bash
sha256sum pinned-binaries/bead-pre-feature         # must match 7e0e73de…db6b5
./pinned-binaries/bead-pre-feature --version       # bead 0.2.4 (af023ad 2026-09-01T19:14:12Z)
./pinned-binaries/bead-pre-feature capabilities    # output carries no attempt_* keys
./pinned-binaries/bead-pre-feature resolve --help  # fails: unrecognized subcommand 'resolve'
```

## Capability Testing Framework

The test framework validates:
1. **Capability Detection:** `bead capabilities` shows attempt-outcome support
2. **Resolve Command:** `bead resolve` is available/absent
3. **Why Command:** Attempt information in `bead why` output
4. **Checkpoint Persistence:** Attempt outcomes survive checkpoint round-trips
5. **NEEDLE Fallback:** Worker starvation detection and recommendation behavior

## Usage

```bash
# Test feature-enabled binary (capabilities output is always JSON; --format is accepted by no pin)
./bead-feature-enabled capabilities | jq '.attempt_outcome.supported'
./bead-feature-enabled why bead-123abc

# Test pre-feature binary
./pinned-binaries/bead-pre-feature capabilities   # no attempt_* capability keys
./pinned-binaries/bead-pre-feature resolve --help # fails: unrecognized subcommand 'resolve'
```

## Verification

See `tests/pinned_binary_capability.rs` for automated capability testing.

---

**Created:** 2026-09-01  
**Updated:** 2026-09-03 (beadrs-da594f4b — recorded the feature-enabled pin's restored-lineage twin `e38430e` as its rebuild target; earlier the same day beadrs-78ced0f1 dropped the `capabilities --format json` examples, which no pin accepts)  
**Bead:** `beadrs-da594f4b`

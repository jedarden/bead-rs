# Attempt-resolution conformance evidence — commit 98f088a

Status: **PASS — all quality gates green, capability matrix validated live, NEEDLE
fallback/atomic paths covered by 96 green tests plus fresh live probes,
checkpoint recovery matrix covered by 98 green tests across 15 suites.**

- Evidence bead: `beadrs-9bbe4057` (this report), executed 2026-09-13
- Gating run: `beadrs-68e30e41` (closed), evidence dir
  `/home/coding/.needle/state/gates/beadrs-68e30e41/`
- Commit under test: `98f088a2e7d57628c768d7e64740fc8bc3364438`
  (`test(checkpoint): cover fixture outcomes and actions`) — `main` HEAD at
  gate time, in sync with origin
- Downstream consumer: `beadrs-15fcfce1` (attempt-resolution compatibility,
  concurrency, and recovery conformance)

Every result below was produced against **one exact commit** via a
`git archive <sha>` extraction (the shared checkout's unrelated in-flight
files were excluded from the gated tree by construction), a private
`CARGO_TARGET_DIR`, and a clean `TMPDIR`.

## 1. Quality gates — exact commands and results

All six gates exit 0 (source:
`gates/beadrs-68e30e41/gates-summary.json`, per-gate logs under `logs/`):

| Gate | Exact command | Exit | Result | Duration |
|---|---|---|---|---|
| fmt | `cargo fmt --check` | 0 | clean | — |
| clippy | `cargo clippy --all-targets -- -D warnings` | 0 | 0 warnings | 33 s |
| test | `cargo test` | 0 | **1290 passed; 0 failed** (86 suites) | 353 s |
| package | `cargo package --locked` | 0 | `bead-rs-0.2.6.crate` verified | 23 s |
| install | `scripts/build-from-archive.sh 98f088a2e7d57628c768d7e64740fc8bc3364438 --name gate-98f088a --out <evidence>/pin` (release, `--locked`) | 0 | sha256 `49c045ab…07c` (below) | 128 s |
| smoke | installed binary: `--version`, `--help`, help-names-`resolve`, `init`, `list --ready`, `show`, `doctor`, `close`, `list closed` | 0 | every step exit 0; `doctor` all scopes OK | — |

Test total reconciles exactly: 273 (`src/lib.rs` unit) + 268 (`src/main.rs`
unit) + 747 (80 integration suites) + 2 (doc) = **1290**.

## 2. Binaries of record — git commits and hashes

Every binary listed here was re-verified by `sha256sum` against its recorded
`binary_sha256` on 2026-09-13 — **all match**.

| Binary | Role | Built from (provenance) | Rebuild target (`build-from-archive.sh`) | sha256 |
|---|---|---|---|---|
| `pinned-binaries/bead-pre-feature` | 0.2.4 pre-attempt baseline | `af023ad47740cf5458f52398e70937b2cc1c18df` (lost lineage) | `ea4e317e697306275aa1a781497a133f472c0df5` | `7e0e73defebb75fc987ddf8b6fb959f47c73ccbbcd7e066e2af302a6a43db6b5` |
| `pinned-binaries/bead-pre-attempt-resolution` | 0.2.6 pre-flag baseline | `946a7271796e15452c4a8a1f1ff9efc05d3e7307` (lost lineage) | `bf1093601dbb6367378a09c813700b4664115a51` | `d0da42bbf59b721bc64bc3d55610844efe3f1f06e37c2d9494c0b3dda6e29ac6` |
| `pinned-binaries/bead-attempt-resolution-e115609` | feature-enabled pin | `e1156098b01264bb998797047115521261443c13` (lost lineage) | `861cdcbfebeb70a9ebc6a2e33ee98cef97274fec` | `68fe8d534721be4ba4147312364d8f0b216b62f3093e85e7c91f0a0db695a645` |
| `pinned-binaries/bead-attempt-resolution-f25ab5c` | feature-enabled pin, binary of record for the pre-fix tree | `f25ab5c91c09a3408f23b9cdf2f3e95e81abc060` (lost lineage) | `b0d7840f6c96cd45e16ea05b7babdb42ef0d2654` | `9a8455f25bacf5bc961bd740442fdc1b30a67fb6e38d304c23c97a57cf57b04e` |
| `pinned-binaries/bead-8e5839b` | first post-resolve-fix pin | = rebuild target `8e5839bc3028cf50e2162ee28ac60836c5e73aa8` (`fix(beadrs-b29a9005): pass mutable resolve transaction`) | same | `2d42c2878e7197958be7abb320b536c2c212edd89dd9cbd4ec21b0eb92490965` |
| `gates/beadrs-68e30e41/pin/gate-98f088a` | commit-under-test release build | = rebuild target `98f088a2e7d57628c768d7e64740fc8bc3364438` | same | `49c045ab39d672f101ff001f289f5f8b3a7bc721ef35cf01b51cc6f42eb8307c` |

Provenance caveats (details in `pinned-binaries/COMMITS.md`, "SHA lineage and
provenance"): the four 2026-09-01/02 pins were built from commits of the
force-pushed-away lineage; the "built from" SHAs are expected to fail
`git cat-file -e` and the "rebuild target" SHAs are their restored-lineage
content twins. Pin filenames keep the original build commit's 7-hex slice.
Every recorded pin hash is **hash-only** (build.rs embeds a wall-clock
timestamp): verify by byte comparison against `*.metadata.json`, never by
rebuilding. `bead-8e5839b` and `gate-98f088a` were built by
`scripts/build-from-archive.sh` from resolvable commits and carry
authoritative `git_commit_sha` in their metadata.

Embedded version strings observed live 2026-09-13: `bead 0.2.4 (af023ad
2026-09-01T19:14:12Z)`, `bead 0.2.6 (946a727 2026-09-02T01:35:01Z)`,
`bead 0.2.6 (e115609-dirty 2026-09-02T07:23:55Z)`, `bead 0.2.6 (f25ab5c-dirty
2026-09-02T10:52:25Z)`, `bead 0.2.6 (unknown 2026-09-04T00:01:36Z)`,
`bead 0.2.6 (unknown 2026-09-13T05:32:45Z)`. The `unknown` commits are the
honest value for git-archive extractions, which carry no `.git`.

## 3. Capability absence/presence matrix — live-validated 2026-09-13

Probed with `<binary> capabilities` (exit 0, JSON) in six isolated
fresh-init workspaces. Key fields:

| Capability | pre-feature 0.2.4 | pre-attempt-resolution 0.2.6 | e115609 | f25ab5c | 8e5839b | gate 98f088a |
|---|---|---|---|---|---|---|
| `contract` | native-v1 | native-v1 | native-v1 | native-v1 | native-v1 | native-v1 |
| `store_layout` | 1 | 1 | 1 | 1 | 1 | 1 |
| `atomic_claim` | true | true | true | true | true | true |
| `logical_revision` | true | true | true | true | true | true |
| `auto_flush` | true | true | true | true | true | true |
| `attempt_outcome` block | **absent** | present (`supported: true`) | present | present | present | present |
| command count | 25 | 30 | 30 | 30 | 30 | 31 |
| `resolve` / `watchdog` / `resource` / `analyze-exclusion` | **absent** (all four) | present | present | present | present | present |
| `redact` | absent | absent | absent | absent | absent | **present** |

Verdict, matching `needle-variant-dispatch-validation.md` §3 and extended to
the two newer binaries: the capability delta is **exactly** the
attempt-resolution surface (one clean step 0.2.4 → 0.2.6) plus the
historical-redaction surface (`redact`, present only on the commit under
test). No capability advertised by an older binary is missing from a newer
one; nothing regressed. Committed coverage:
`capability_detection` 18, `capability_variant_matrix` 11,
`pinned_binary_capability` 22, `cli_capabilities` 11,
`capability_framework` 5, `binary_variant_integration` 5 (+6 `#[ignore]`d —
each builds multiple binary variants from source, documented as too slow for
the default run; runnable with `--ignored`),
`secret_rejection` 9 — **81 tests, all green** in the gate run.

## 4. NEEDLE fallback / atomic path coverage

Committed, gate-green — 96 tests across 11 suites:

| Suite | Tests | Covers |
|---|---|---|
| `needle_variant_dispatch_paths` | 11 | both pins: 8-way claim exclusivity, empty-frontier null, revision fencing exit 4, assigned-open never handed out, frozen resolve defect |
| `needle_variant_paths` | 10 | same contract on `needle_variant_paths` side incl. loud/non-corrupting failure arms |
| `fallback_starvation_recovery` | 3 | starvation fallback activates and recovers |
| `needle_v1_compatibility` | 11 | the needle-v1 command surface NEEDLE's loop drives |
| `concurrency_recovery_variants` | 11 | claim/lease/recovery races |
| `cli_claim` | 11 | claim semantics incl. assignment-held refusal |
| `r002_leased_claims` | 12 | leased claims, renewal, expiry |
| `r003_revision_guards` / `r003_revision_guard_atomicity` | 12 / 6 | `--if-revision` fencing, atomicity |
| `r034_stale_in_progress` / `r035_assignment_held_diagnosis` | 4 / 5 | stale-claim handling |

Live probes, 2026-09-13 (fresh workspaces under `/var/tmp`, exact argv):

1. **Fallback path degrades cleanly on the 0.2.4 pin** —
   `bead-pre-feature resolve probe-1 --attempt-id a1 --outcome
   verified_success` → clap rejects before store contact:
   `error: unrecognized subcommand 'resolve'` (+ similar-subcommand tip),
   **exit 2**, no workspace touched.
2. **Frozen defect stays frozen in the old feature pins** —
   `bead-attempt-resolution-f25ab5c resolve probe-def --attempt-id a1
   --outcome verified_success` → `Integrity error … no such column:
   updated_at_revision`, **exit 5**, loud, atomic (no mutation) — exactly as
   recorded 2026-09-03; a pin is a frozen artifact.
3. **The resolve fix is live from the first fix pin onward** —
   `bead-8e5839b resolve bead-a8cc1dc4 --attempt-id a-fix2 --outcome
   verified_success --action close --reason "probe close"` → receipt
   `ao-f77c7044…`, `Resulting State: closed`, issue `Closed`, revision
   1 → 2, **exit 0**. Same probe on the gate binary (`bead-4684aead`,
   receipt `ao-dd2c8234…`) → identical shape, **exit 0**.
4. **Replay detection** — re-resolving the same `--attempt-id` with a
   different payload → `Conflict: Attempt a-fix2 already resolved …`,
   **exit 4** (identical-payload idempotency is asserted by
   `resolve_attempt_e2e`, 10 tests, green).
5. **Atomic claim exclusivity on the commit under test** — 8 concurrent
   `gate-98f088a claim --assignee racer-N` processes on a one-bead queue:
   all 8 exit 0, exactly 1 winner (store state: `in_progress`,
   `assignee: racer-2`, `claim_epoch: 1`, revision 2), 7 clean
   `"bead_id": null`. No duplicate assignment.
6. **Revision fencing on the commit under test** —
   `update <id> --notes "stale write" --if-revision 99` →
   `Conflict: Revision mismatch: expected 99, found 1`, **exit 4**,
   revision unchanged; the same update at `--if-revision 1` succeeds,
   **exit 0**, revision → 2.

## 5. Checkpoint recovery test matrix

Gate-green — 98 tests across 15 suites. Scenario → suite → result:

| Recovery scenario | Suite(s) | Tests | Result |
|---|---|---|---|
| Old (pre-feature) and new checkpoint fixtures import; outcome/action vocabulary coverage; sharded + monolithic formats | `checkpoint_fixture_conformance` | 14 | green |
| Export → import round trip; replay idempotency after restore | `checkpoint_round_trip_conformance` | 3 | green |
| `sync import` flag surface incl. `--restore-into-empty` | `cli_sync_import` | 18 | green |
| Verified restore of one named generation; refuses unverified sources | `r036_verified_restore` | 9 | green |
| Recovery rehearsal (`doctor --rehearse`) against a disposable copy | `r015_recovery_rehearsal` | 5 | green |
| Fresh-clone rebuild: `init` + import from durable checkpoint | `clone_recovery` | 5 | green |
| Divergent checkpoint histories rejected | `checkpoint_mutation_guard` | 4 | green |
| Pointer-declared tombstones applied after pointer commit | `checkpoint_tombstones` | 3 | green |
| Publication lock (stale lock does not block) | `checkpoint_publication_lock` | 3 | green |
| Monolithic vs sharded mode selection | `checkpoint_mode_selection` | 9 | green |
| Forensic-view salvage / archaeology | `r029_checkpoint_archaeology` | 5 | green |
| Attempt receipts survive checkpoints; integrity across restore | `attempt_receipt_checkpoint_integrity` | 3 | green |
| Automatic post-mutation publication | `post_commit_publication` | 13 | green |
| Pending-migration reporting on init / on open | `schema_upgrade_on_init`, `test_migration_on_open` | 2 / 2 | green |

## 6. Resolve/attempt surface (the feature under conformance)

Gate-green: `resolve_attempt_e2e` 10 (receipt emission, close + revision
bump, replay idempotency, tier progression, `--if-revision` guard,
unknown-issue exit 3), `attempt_outcome_round_trip` 4,
`attempt_receipt_diagnostics` 11, `attempts_mod` 19, `watchdog_cli` 2 —
**46 tests**. Live cross-check in §4 probes 2–4.

## 7. Out-of-evidence scope

The shared checkout carries uncommitted in-flight files that are **not** part
of the gated tree and therefore not covered by any result above:
`tests/checkpoint_resilience.rs`, `tests/concurrent_replay_fencing.rs`,
`tests/dependency_blocker_status.rs`, `tests/reproducible_build.rs`,
`src/scan/*`, and modifications to `build.rs`, `src/main.rs`,
`scripts/build-from-archive.sh`, `BUILD_PROCEDURE.md`,
`research/specs/needle-cli-contract-v1.md`,
`docs/verification/needle-variant-dispatch-validation.md`,
`tests/build_from_archive_checkout_untouched.rs`,
`tests/needle_variant_dispatch_paths.rs`. They belong to still-open beads
(`beadrs-3b98c8a7`, `beadrs-9b13797a`, `beadrs-e2dbc36f`, `beadrs-faef002d`,
…) and enter the evidence base only when their own dispatch commits and a
later gate re-runs against that commit.

## 8. Verdict and re-verification recipe

The commit-under-test passes every quality gate; the capability matrix is
clean and monotonic across all six binaries; NEEDLE's fallback and atomic
paths are proven both by 96 committed green tests and by fresh live probes;
checkpoint recovery is covered end-to-end by 98 green tests. No defect is
reachable without explicit capability negotiation, and the one historical
defect (resolve's missing column) is confirmed fixed from pin `8e5839b`
onward while staying faithfully recorded in the frozen pre-fix pins.

Re-verify locally:

```console
$ cargo test --test needle_variant_dispatch_paths --test needle_variant_paths -- --test-threads=1
$ cargo test --test checkpoint_fixture_conformance --test cli_sync_import --test r036_verified_restore
$ cargo test --test capability_variant_matrix --test pinned_binary_capability
$ sha256sum pinned-binaries/*          # compare against *.metadata.json
$ <binary> capabilities                # §3 matrix, any binary, fresh workspace
```

(On this box `cargo test` with a dirty tree runs locally under cgroup
limits; a clean tree submits to iad-ci. Suites marked `*_variant_*` drive the
pinned binaries and need nothing but the pins.)

## 9. Re-verification — 2026-09-13, second dispatch

The first dispatch on the evidence bead committed this report (`10db987`)
and then hit its hard timeout before verifying and closing. The second
dispatch re-verified every mechanical claim above live; all green:

- **Binaries re-hashed by content.** All five `pinned-binaries/` pins
  (`bead-pre-feature`, `bead-pre-attempt-resolution`,
  `bead-attempt-resolution-e115609`, `bead-attempt-resolution-f25ab5c`,
  `bead-8e5839b`) and `gates/beadrs-68e30e41/pin/gate-98f088a` — every
  sha256 matches its recorded `binary_sha256`.
- **Gate evidence re-read.**
  `gates/beadrs-68e30e41/gates-summary.json`: all six gates
  `exit_code: 0`, overall `ALL GATES PASSED`; `logs/test.log` re-totals
  **1290 passed / 0 failed**.
- **No source drift since the gate commit.**
  `git diff --stat 98f088a2..HEAD -- src tests Cargo.toml Cargo.lock
  build.rs scripts/build-from-archive.sh` is empty — the five commits since
  `98f088a2` are `chore(beads)` checkpoint syncs and documentation — so the
  gate results remain the results of the current source tree.
- **Report is at origin.** `10db987` is an ancestor of `origin/main`, and
  the working-tree copy of this file is clean at `HEAD` at re-verification
  time.

Acceptance trace for the evidence bead: exact commands + arguments → §1
(and per-gate logs in the evidence dir); commit hashes for all binaries →
§2; test results formatted → §1/§4/§5/§6; capability absence/presence
matrix → §3; NEEDLE fallback/atomic path coverage → §4; checkpoint
recovery matrix → §5; persistent location → this file, committed and
pushed.

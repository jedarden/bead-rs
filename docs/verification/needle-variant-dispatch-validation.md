# NEEDLE dispatch-path validation — both pinned binary variants

Status: **VERIFIED — both variants deploy-safe in a NEEDLE context, with one
recorded defect fenced behind the capability gate** (detail in §6).

Validation bead: `beadrs-146944d1`, executed 2026-09-03. The paths validated
here are the ones NEEDLE's dispatch loop actually drives — atomic claim,
revision fencing, the claim→close lifecycle, starvation fallback — driven
against the pinned executables themselves, never a local build. The suite is
`tests/needle_variant_dispatch_paths.rs`; the complementary consumer-contract
suite is `tests/needle_variant_paths.rs` (report:
[needle-variant-paths-validation-2026-09-03.md](needle-variant-paths-validation-2026-09-03.md)).

## 1. Pins under test, provenance-verified

Resolved from `pinned-binaries/commits.json` and byte-checked against each
pin's recorded `binary_sha256` before any test drove it (the suite asserts
this; re-verified live):

| Role | Binary | Version | sha256 match |
|---|---|---|---|
| `pre_feature` | `bead-pre-feature` | `bead 0.2.4 (af023ad 2026-09-01T19:14:12Z)` | yes |
| `attempt_resolution_f25ab5c` | `bead-attempt-resolution-f25ab5c` | `bead 0.2.6 (f25ab5c-dirty 2026-09-02T10:52:25Z)` | yes |
| `attempt_resolution_e115609` | `bead-attempt-resolution-e115609` | `bead 0.2.6 (e115609-dirty 2026-09-02T07:23:55Z)` | yes |

All four registry pins verify; the two feature-enabled pins were both probed
live and behave identically for everything below.

## 2. Execution record

Two full green runs on 2026-09-03, serial (`--test-threads=1`), local
cgroup-limited build of the dirty shared checkout (the suites themselves only
execute the pins):

```console
$ cargo test --test needle_variant_dispatch_paths --test needle_variant_paths -- --test-threads=1
running 11 tests ... test result: ok. 11 passed; 0 failed   (run 1: 15.36s, run 2: 8.97s)
running 10 tests ... test result: ok. 10 passed; 0 failed   (run 1: 10.71s, run 2: 6.22s)
```

16 of the 21 tests are variant validation; 5 are `capability_framework`
harness unit tests. The concurrency tests (8 parallel claimant processes per
variant) passed on both runs, so the exclusivity result below is not a
single-sample fluke.

## 3. Contract identity — no variant-specific dispatch code needed

`capabilities` is identical across the pins for every field the dispatch loop
reads: `contract`, `implementation`, `store_layout`, `atomic_claim: true`,
`logical_revision: true`, `auto_flush`. All twelve core commands
(`capabilities, init, create, list, claim, update, close, reopen, release,
sync, why, doctor`) are advertised by both pins.

The capability delta is exactly the attempt-resolution surface: the
pre-feature pin advertises no `attempt_outcome` block and none of `resolve`,
`watchdog`, `resource`, `analyze-exclusion`; the 0.2.6 pins advertise all four
plus `attempt_outcome.supported: true`.

## 4. Atomic paths hold on both variants

- **Claim exclusivity, exercised for real**: 8 concurrent claimant processes
  on a one-bead queue → exactly 1 winner, 7 clean `"bead_id": null` at exit 0,
  bead left `in_progress` with the winner as assignee. No duplicate
  assignment on either pin, both runs.
- **Empty frontier** is `"bead_id": null` at exit 0 on both pins — NEEDLE
  reads it as "no work", never as a failure.
- **Revision fencing**: a stale `--if-revision` rejects with exit 4 and
  `Conflict: Revision mismatch ... expected 1`, no panic, and no state change;
  the same update at the current revision succeeds. Identical on both pins.
- **Assigned-open beads are never handed out**: after
  `update --assignee`, `claim` returns null on both pins (the 0.2.6 pin
  additionally hides the bead from `list --ready` — §7).

## 5. Fallback path — the pre-feature pin degrades cleanly

Every capability-gated invocation on the 0.2.4 pin is rejected by clap before
any store contact, with a helpful tip and exit 2:

```console
$ bead-pre-feature resolve probe-… --attempt-id a1 --outcome verified_success
error: unrecognized subcommand 'resolve'

  tip: some similar subcommands exist: 'release', 'reopen', 'resource', 'restore'
EXIT=2
```

`watchdog` degrades the same way. The core loop is unimpaired on the same
binary: the full claim → release → assign → clear-assignee → close → reopen →
close round trip passes on both pins, and process-boundary error codes are
stable (unknown subcommand → exit 2 via clap; commands run outside any
workspace → exit 3; no panics anywhere).

## 6. RECORDED DEFECT — resolve fails on every invocation, loudly and atomically

Both feature-enabled pins advertise `attempt_outcome.supported: true`, yet
`resolve` fails on every invocation, against a workspace the same binary
initialized:

```console
$ bead-attempt-resolution-f25ab5c resolve probe-c0b36023 \
    --attempt-id att-live-probe --outcome verified_success --action close
Error: Integrity error: Failed to read issue: SQLite error: no such column:
  updated_at_revision in SELECT updated_at_revision, base_status,
  attempt_tier, consecutive_failures FROM issues WHERE id = ?1 at offset 7
EXIT=5

$ bead-attempt-resolution-f25ab5c show probe-c0b36023 | grep -E '^(Status|Revision):'
Status: Open
Revision: 1
```

Root cause confirmed in source at HEAD `ab1e0b2`:
`src/service/attempt.rs:248` (`get_issue_state`) selects `updated_at_revision`,
and no migration in `src/store/migrations.rs` creates that column — the
issues-table migrations add `revision` (line 376) and the attempt columns
(lines 565–567), never `updated_at_revision`. The working checkout carries an
uncommitted candidate fix (select `revision` instead) belonging to another
in-flight change; it is **not** part of any pin and was not evaluated here.

Deploy-safety analysis, which the suites pin as executable assertions:

- The failure is **loud** (exit 5, structured `Integrity error`, no panic) —
  a consumer cannot mistake it for a resolution.
- The failure is **atomic / non-corrupting** — issue stays `open`, revision
  unchanged, no receipt, no partial lifecycle leak.
- The advertised-but-broken surface is reachable **only through the ADR-012
  negotiation gate**: a consumer that never negotiates `attempt_outcome` never
  invokes `resolve`. NEEDLE's legacy reconciliation path (§5) is fully intact
  on both pins.

Verdict: the feature-enabled pin stays deployable behind the gate; the
advertised exactly-once resolution contract is simply unavailable until the
schema defect is fixed. `resolve_execution_fails_with_recorded_integrity_defect_on_feature_enabled_pin`
and `resolve_failure_is_loud_and_non_corrupting_on_feature_enabled_pin` pin
the failure arm today and start enforcing the receipt + replay-idempotency
contract automatically on whichever side of the fix they run.

## 7. DOCUMENTED DIFFERENCE — starvation visibility

With an assigned-open bead in the workspace:

| | pre-feature 0.2.4 | feature-enabled 0.2.6 |
|---|---|---|
| `list --ready` shows the bead | yes (overstates the frontier) | no (excluded) |
| starvation diagnostic written | no | `.beads/diagnostics/pluck-starvation-diagnostic.log`, naming the bead and the `--clear-assignee` remedy |
| `claim` hands it out | no | no |

This is a visibility difference, not a double-assignment hazard: claim refuses
an assigned-open bead on both pins. A NEEDLE consumer on the old pin sees a
ready-frontier count it cannot actually claim; on 0.2.6 the frontier is
accurate and the remedy is written to disk.

## 8. Interoperability — mixed fleets and in-place upgrades

NEEDLE may drive one workspace with either binary across an upgrade, and both
directions pass: the 0.2.6 binary claims and closes work in a 0.2.4-init
workspace (which then still reads correctly under 0.2.4), and the 0.2.4
binary claims, fence-updates, and closes work in a 0.2.6-init workspace.
Neither variant strand-breaks the other's store.

## 9. Verdict

Both variants are safe to deploy in a NEEDLE context:

1. The fallback path NEEDLE reconciles through (needle-v1 command surface,
   atomic claim, revision fencing, lifecycle mutations) is behaviorally
   identical and fully working on both pins.
2. Every degradation is a clean process-boundary error (2/3/4/5), never a
   panic and never a silent success.
3. The one real defect — resolve's missing column — fails loudly and without
   state damage, and is unreachable without explicit capability negotiation
   (ADR-012). It should be fixed before any consumer relies on
   `attempt_outcome.supported: true`, but its presence does not block
   deploying either pin for the dispatch loop.

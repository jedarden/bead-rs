# NEEDLE binary-variant behavioral differences and deploy safety

Status: **both pinned variants are deploy-safe in a NEEDLE context; the one
real defect is loud, non-corrupting, and unreachable without explicit
capability negotiation** (§5). Synthesis document for split child 5/5 of
`beadrs-146944d1` (`beadrs-955e0ad0`), 2026-09-03.

Every behavioral claim in §3–§4 was re-verified live against the pins by the
executing dispatch on 2026-09-03, on top of the committed suites — the pin
sha256s, the capability documents, the exit-code matrix, the fencing probe,
and the starvation-visibility difference were all reproduced first-hand for
this document, not transcribed.

## 1. What this consolidates, and where the evidence actually lives

`beadrs-146944d1` was auto-split into a 5-child chain: child 2 (fallback,
`beadrs-bf16ae1c`), child 3 (atomic paths, `beadrs-8a4a5a85`), child 4 (error
handling, `beadrs-0a324a37`), child 5 (this synthesis).

**Provenance note, stated plainly:** children 2–4 are closed with **empty
notes** — the Phase 19.4 gate closed them ("verification is the gate's job",
per the checkpoint close events) rather than a worker recording evidence on
the beads. The evidence those children were to carry exists, but it lives in
the artifacts, not on the beads:

- `tests/needle_variant_paths.rs` (consumer/contract suite) and
  `tests/needle_variant_dispatch_paths.rs` (dispatch-loop suite), with their
  green-run execution records, landed in commit `598f585`;
- the two verification reports:
  [needle-variant-paths-validation-2026-09-03.md](../verification/needle-variant-paths-validation-2026-09-03.md)
  and
  [needle-variant-dispatch-validation.md](../verification/needle-variant-dispatch-validation.md);
- `beadrs-e4609b53`'s own close notes, which do carry the capability-matrix
  evidence.

This document is the cross-variant summary the umbrella acceptance asked for,
with the per-variant deploy-safety verdict made explicit (§5).

## 2. Pins under test

Resolved from the authoritative registry `pinned-binaries/commits.json`; all
four sha256-verified against their `*.metadata.json` live on 2026-09-03:

| Role | Binary | Version | sha256 match |
|---|---|---|---|
| `pre_feature` | `bead-pre-feature` | `bead 0.2.4 (af023ad 2026-09-01T19:14:12Z)` | yes |
| `pre_attempt_resolution` | `bead-pre-attempt-resolution` | `bead 0.2.6 (946a727 2026-09-02T01:35:01Z)` | yes |
| `attempt_resolution_e115609` | `bead-attempt-resolution-e115609` | `bead 0.2.6 (e115609-dirty 2026-09-02T07:23:55Z)` | yes |
| `attempt_resolution_f25ab5c` | `bead-attempt-resolution-f25ab5c` | `bead 0.2.6 (f25ab5c-dirty 2026-09-02T10:52:25Z)` | yes |

**Registry hygiene:** the repo-level `pinned-binaries/` registry is
authoritative and fully verified. The workspace-level
`.beads/pinned-binaries/MANIFEST.json` is **stale** and divergent: it still
records `pre_feature` as `requires_manual_build` with `sha256: null` (the
binary exists, at the repo level), and its usage examples invoke
`capabilities --format json` — a flag no pin accepts (capabilities output is
natively JSON). Split child 1 (`beadrs-455a56ac`) remains open against that
stale entry; it needs reconciliation, not a build.

## 3. Observed behavioral differences

### 3.1 Capability document

| | pre-feature 0.2.4 | 0.2.6 family (all three pins) |
|---|---|---|
| `contract` | `native-v1` | `native-v1` (identical) |
| `atomic_claim` / `logical_revision` | `true` / `true` | `true` / `true` (identical) |
| `attempt_outcome` block | absent | present, `supported: true`, 5 outcomes |
| advertised commands | 25 | 30 — adds `resolve`, `watchdog`, `resource`, `analyze-exclusion` |

The delta is exactly the attempt-resolution surface. Every field the NEEDLE
dispatch loop reads is identical across variants.

### 3.2 Fallback path — how the 0.2.4 pin degrades

Every capability-gated invocation on the 0.2.4 pin is rejected by clap before
any store contact — the only degradation route that variant has:

```console
$ bead-pre-feature resolve x --attempt-id a --outcome verified_success
error: unrecognized subcommand 'resolve'
EXIT=2
$ bead-pre-feature watchdog
error: unrecognized subcommand 'watchdog'
EXIT=2
```

No partial work, no store mutation, no panic. The needle-cli-contract-v1
required surface (12 core commands: `capabilities, init, create, list, claim,
update, close, reopen, release, sync, why, doctor`) is fully present and
passes identically on both variants — so ADR-012 fallback to the legacy
reconciliation sequence works everywhere.

### 3.3 Atomic paths — identical on both variants

- **Claim exclusivity**: 8 concurrent claimants over a one-bead queue →
  exactly 1 winner, 7 clean `bead_id: null` at exit 0 (suite-pinned, two green
  runs per the dispatch-validation report).
- **Empty frontier** → exit 0 with no `bead_id` on both variants.
- **Revision fencing** (live-probed on the 0.2.4 pin): stale `--if-revision`
  rejects with exit 4 leaving state untouched; the same update at the current
  revision succeeds at exit 0.
- **Assigned-open beads are never handed out** by `claim` on either variant.

### 3.4 Process-boundary error codes

| Exit | Meaning | pre-feature 0.2.4 | 0.2.6 family |
|---|---|---|---|
| 2 | unrecognized subcommand (clap) | yes — the capability-gate degradation route | yes |
| 3 | command run outside any workspace | yes (live-probed) | yes (live-probed) |
| 4 | stale `--if-revision` conflict | yes (live-probed) | yes (suite-pinned) |
| 5 | resolve integrity failure | n/a — no `resolve` | **yes — every resolve-capable pin, see §4** |

No panics were observed on any pin under any probe.

### 3.5 Record schema delta — additive only

`show --json` on the 0.2.6 family adds `effective_status` and
`manual_blocked`; no field is removed or re-typed. The
needle-cli-contract-v1 unknown-field rule makes this contract-legal with zero
consumer changes.

### 3.6 Starvation visibility (live-probed, assigned-open bead present)

| | pre-feature 0.2.4 | 0.2.6 family |
|---|---|---|
| bead visible in `list --ready` | **yes** (overstates the frontier) | no (excluded) |
| starvation diagnostic written | no | `.beads/diagnostics/pluck-starvation-diagnostic.log` naming the bead and the `--clear-assignee` remedy |
| `claim` hands the bead out | no | no |

A visibility difference, not a safety one: `claim` refuses an assigned-open
bead on both variants. A 0.2.4-based consumer may simply see a ready count it
cannot actually claim.

### 3.7 Mixed fleets / in-place upgrades

Both directions pass (dispatch-validation §8): a 0.2.6 binary can claim and
close work in a 0.2.4-init workspace (which still reads correctly under
0.2.4), and vice versa. Neither variant strand-breaks the other's store.

## 4. Recorded defect — `resolve` fails on every resolve-capable pin

All three 0.2.6-family pins advertise or implement `resolve`, and on **all
three** every invocation fails:

```console
$ bead-attempt-resolution-f25ab5c resolve <id> --attempt-id att-probe \
    --outcome verified_success --action close
Error: Integrity error: Failed to read issue: SQLite error: no such column:
  updated_at_revision in SELECT updated_at_revision, base_status,
  attempt_tier, consecutive_failures FROM issues WHERE id = ?1 at offset 7
EXIT=5
$ bead-pre-attempt-resolution resolve …
EXIT=5   # same failure — the blast radius covers this pin too
```

(The earlier reports named only the two `attempt_resolution_*` pins; the
`pre-attempt-resolution` pin was probed for this document and fails
identically.)

- **Loud**: exit 5 with a structured `Integrity error` — a consumer cannot
  mistake it for a resolution.
- **Non-corrupting**: the target issue stays `open` at an unchanged revision;
  no receipt, no partial lifecycle mutation.
- **Root cause** (analysis at HEAD `ab1e0b2`): `get_issue_state` in
  `src/service/attempt.rs` selects `updated_at_revision`; no migration in
  `src/store/migrations.rs` creates that column.
- **Tracked**: `beadrs-6b891bb7` (in_progress). The uncommitted candidate fix
  in the shared working checkout belongs to that effort and is part of **no**
  pin.

## 5. Deploy-safety verdict

**pre-feature pin (`bead-pre-feature`, 0.2.4) — SAFE for NEEDLE dispatch
use.** The needle-v1 fallback surface a NEEDLE consumer reconciles through is
complete and behaviorally identical to the 0.2.6 family; all degradation is a
clean process-boundary error (exit 2/3), atomic claim and revision fencing
hold, and mixed-fleet interop passes both directions. Only caveat: `list
--ready` can overstate the frontier when assigned-open beads exist (§3.6) —
treat the frontier as advisory and trust `claim`'s answer.
Evidence: §3.2–§3.4, §3.7;
[needle-variant-dispatch-validation.md](../verification/needle-variant-dispatch-validation.md)
§4–§5; suites in commit `598f585`.

**feature-enabled pins (0.2.6 family: `bead-pre-attempt-resolution`,
`bead-attempt-resolution-e115609`, `bead-attempt-resolution-f25ab5c`) — SAFE
for NEEDLE dispatch use behind the ADR-012 capability-negotiation gate; NOT
safe to rely on for attempt resolution.** The dispatch loop (atomic claim,
fencing, lifecycle, fallback) is identical to the 0.2.4 behavior and fully
working. But do **not** consume `attempt_outcome.supported: true` — the
advertised resolution contract fails on every invocation (§4). The failure is
loud and non-corrupting, so an accidental reliance is detectable at the
process boundary, never a silent success or store damage; still, no consumer
should negotiate `attempt_outcome` until `beadrs-6b891bb7` lands.
Evidence: §4; [needle-variant-dispatch-validation.md](../verification/needle-variant-dispatch-validation.md)
§6; `resolve_failure_is_loud_and_non_corrupting_on_feature_enabled_pin`.

**Follow-ups recorded by this document:** fix and land `beadrs-6b891bb7`
before any `attempt_outcome` consumer ships; reconcile or retire the stale
`.beads/pinned-binaries/MANIFEST.json` (open child `beadrs-455a56ac`), whose
`requires_manual_build` status no longer describes reality.

## 6. Evidence index

- Suites: `tests/needle_variant_paths.rs`, `tests/needle_variant_dispatch_paths.rs`
  (commit `598f585`; execution records in the two verification reports).
- Reports: `docs/verification/needle-variant-paths-validation-2026-09-03.md`,
  `docs/verification/needle-variant-dispatch-validation.md`.
- Pin registry: `pinned-binaries/commits.json` + `*.metadata.json`
  (sha256-verified live 2026-09-03), `pinned-binaries/BINARY_VERIFICATION.md`.
- Beads: umbrella `beadrs-146944d1`; children `beadrs-bf16ae1c`,
  `beadrs-8a4a5a85`, `beadrs-0a324a37` (closed by the gate, empty notes — see
  §1); capability framework `beadrs-e4609b53` (evidenced close); resolve
  defect `beadrs-6b891bb7`; stale-manifest child `beadrs-455a56ac`.
- This document's own live probes (2026-09-03): pin sha256 verification,
  capability-document diff, exit codes 2/3/4/5, resolve failure on
  `f25ab5c` and `pre-attempt-resolution`, starvation-visibility difference.

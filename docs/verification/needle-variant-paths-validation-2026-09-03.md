# NEEDLE consumer-path validation — both pinned binary variants (2026-09-03)

Status: **VERIFIED — the needle-v1 fallback surface and the atomic paths are
behaviorally identical on both pins; the only output delta is additive and
contract-legal; one advertised capability is defective and fenced (§4)**.

Validation bead: `beadrs-146944d1`, executed 2026-09-03. Companion to
[needle-variant-dispatch-validation.md](needle-variant-dispatch-validation.md),
which covers the same pins from the dispatch-loop side. Suite:
`tests/needle_variant_paths.rs`, driving the pinned executables via
`capability_framework::capability_variant_pair` (provenance- and byte-checked
before use).

## 1. Pins and execution record

- `pre_feature` → `bead-pre-feature`, `bead 0.2.4 (af023ad 2026-09-01T19:14:12Z)`
- `capability_present` → `bead-attempt-resolution-f25ab5c`,
  `bead 0.2.6 (f25ab5c-dirty 2026-09-02T10:52:25Z)`
- Second feature-enabled pin `bead-attempt-resolution-e115609` (0.2.6)
  probed live and behaviorally identical.

Two green runs on 2026-09-03 (`--test-threads=1`): `11 passed; 0 failed` +
`10 passed; 0 failed`, both runs — 16 validation tests plus 5 harness unit
tests. All four registry pins matched their recorded `binary_sha256`.

## 2. Fallback path — needle-cli-contract-v1 required surface

`needle_v1_required_surface_works_on_both_variants` runs one full pass of the
required-command surface in a fresh workspace per pin: `--version`, `create`
(ID-only stdout), `list --json` surfacing the new record, `show --json`
carrying all nine contract fields (`id, title, description, priority, status,
assignee, dependencies, created_at, updated_at`), server-selected `claim`,
label, dependency, `close` → `reopen`, `doctor`, and `sync flush-only`
publishing a `checkpoint/current.json` + `forensic.jsonl` pair.

This is the whole integration story on the pre-feature pin, and it passes
there — and equally on the feature-enabled pin, which matters because ADR-012
lets NEEDLE fall back to the legacy reconciliation sequence at any time. It
does: the fallback path is intact on both variants.

## 3. Atomic paths — identical on both variants

- **Concurrent distinct claims**: 8 threads race `claim --assignee ... --json`
  over a 10-bead queue on each pin; every claimant wins exactly one bead and
  no bead is handed out twice (`concurrent_claims_receive_distinct_beads_on_both_variants`).
- **Empty queue** is exit 0 with no `bead_id`, on both pins.
- **Revision guard**: `update --if-revision <current>` succeeds; the same
  fencing revision a second time exits 4 and leaves the committed notes
  untouched (`stale_revision_guard_rejects_identically_on_both_variants`).
- **Stable process-boundary errors** (`error_exit_codes_are_stable_across_variants`):
  unknown subcommand → clap exit 2 with "unrecognized subcommand", no panic;
  any command outside a workspace → exit 3. Identical on both pins, exercised
  in a bare directory with no `.beads` ancestor.

## 4. Advertised-but-defective capability: attempt resolution

The feature-enabled pin advertises `attempt_outcome.supported: true` while
`resolve` fails on every invocation with:

```console
Error: Integrity error: Failed to read issue: SQLite error: no such column:
  updated_at_revision in SELECT updated_at_revision, base_status,
  attempt_tier, consecutive_failures FROM issues WHERE id = ?1 at offset 7
EXIT=5
```

Root cause at HEAD `ab1e0b2`: `get_issue_state` in `src/service/attempt.rs`
selects `updated_at_revision`; no migration in `src/store/migrations.rs`
creates it. `resolve_failure_is_loud_and_non_corrupting_on_feature_enabled_pin`
pins the safety invariants a negotiating consumer actually needs — the failure
is loud (nonempty structured stderr, exit 5, no panic) and non-corrupting (the
target issue remains `open`, revision unchanged) — and, on whichever side of
the future fix it runs, exercises the atomic receipt + replay-idempotency
contract. Full analysis and the deploy-safety verdict:
[needle-variant-dispatch-validation.md §6](needle-variant-dispatch-validation.md).

## 5. Behavioral differences between the variants

Measured live by diffing `show --json` records of identical creates:

- **Additive record fields**: the 0.2.6 pin adds `effective_status`
  (`"open"` on a fresh issue) and `manual_blocked`; the 0.2.4 pin lacks both.
  No field was removed or re-typed. The needle-cli-contract-v1
  additional-fields rule permits this — consumers must ignore unknown fields,
  so the delta requires no consumer code.
- **Capability document**: `attempt_outcome` block and the
  `resolve` / `watchdog` / `resource` / `analyze-exclusion` commands appear
  only on 0.2.6; the core NEEDLE command set and all dispatch-relevant
  capability fields (`atomic_claim`, `logical_revision`, `store_layout`,
  `contract`, `implementation`, `auto_flush`) are identical.
- **Starvation visibility**: with an assigned-open bead present, 0.2.4 still
  lists it in `list --ready` and writes no diagnostic; 0.2.6 excludes it from
  the frontier and writes
  `.beads/diagnostics/pluck-starvation-diagnostic.log` naming the bead and
  the `--clear-assignee` remedy. `claim` refuses the bead on both — a
  visibility difference, not a safety one.
- **Everything else** — exit codes, error text shape, claim semantics,
  fencing, lifecycle transitions, checkpoint publication — is behaviorally
  identical across the pins.

## 6. Verdict

Both variants are safe for NEEDLE to deploy and interoperate with: the
fallback surface a consumer reconciles through works unchanged on both, the
atomic guarantee carriers (server-selected claim, `--if-revision`) hold on
both, error handling is clean and stable on both, and the single defect is
gated behind explicit capability negotiation, where it fails loudly and
without corrupting state.

# NEEDLE dispatch-path validation across the pinned binary variants

Validation of the paths NEEDLE's dispatch loop drives — atomic claim, revision
fencing, the claim→close lifecycle, starvation fallback, and the
attempt-resolution surface — against both pinned binary variants, with the
behavioral differences and a per-variant deploy-safety verdict.

- **Task:** beadrs-146944d1
- **Defect found and filed:** beadrs-6b891bb7 (`resolve` selects a column no
  migration creates; every `resolve` exits 5)
- **Automated suite:** `tests/needle_variant_dispatch_paths.rs` (11 tests, all
  passing; each drives the pinned executables directly, byte-verified against
  their pin metadata)
- **Date:** 2026-09-03

## Variants under test

| Role | Binary | Embedded version | sha256 (recorded, byte-verified) |
|---|---|---|---|
| `pre_feature` | `pinned-binaries/bead-pre-feature` | `bead 0.2.4 (af023ad 2026-09-01T19:14:12Z)` | `7e0e73defebb75fc987ddf8b6fb959f47c73ccbbcd7e066e2af302a6a43db6b5` |
| `attempt_resolution_f25ab5c` | `pinned-binaries/bead-attempt-resolution-f25ab5c` | `bead 0.2.6 (f25ab5c-dirty 2026-09-02T10:52:25Z)` | recorded in `bead-attempt-resolution-f25ab5c.metadata.json`, re-verified from bytes |

Both pins were byte-checked (`sha256sum -c` against their `.metadata.json`)
before any behavioral probe ran, so every result below is attributable to the
recorded pins and not to whatever binary happens to sit at that path.

Method: direct CLI invocations in disposable `/var/tmp` workspaces (no foreign
`.beads` ancestor), plus source inspection of committed HEAD for root causes.
Exit codes are the process exit codes.

## Shared surface — identical on both variants

`bead capabilities` reports the same values on both pins for every field the
dispatch loop depends on:

| Field | Both pins |
|---|---|
| `contract` / `implementation` | `native-v1` / `bead-rs` |
| `store_layout` | 1 |
| `atomic_claim` | `true` |
| `logical_revision` | `true` (`--if-revision` fencing) |
| `auto_flush` | `true` (checkpoint published after each mutation) |
| `checkpoint_modes` / `checkpoint_formats` | identical sets |
| `priorities`, `statuses` | identical |

Commands `capabilities init create list claim update close reopen release
sync why doctor` are present on both pins — the full NEEDLE core loop.

### Atomic claim (validated live on both pins)

- 8 concurrent `bead claim --assignee worker-N --json` processes on a one-bead
  queue → **exactly 1 winner**, 7 losers each returning
  `{"bead_id":null,...}` with exit 0, no duplicate assignment, bead left
  `in_progress` with the winner's assignee.
- Empty frontier → `{"bead_id":null,...}`, exit 0 (not an error) on both.
- An assigned-but-open bead is **never** handed out by `claim`, on either pin
  — not to other workers, not to its own assignee.

### Revision fencing (validated live on both pins)

- `update <id> --status deferred --if-revision 1` against revision 2 →
  **exit 4**, stderr `Conflict: Revision mismatch: expected 1, found 2. The
  issue has been modified since you retrieved it.`, **no state change**.
- The same update at the current revision → exit 0.
- Double close with a different reason → exit 4
  (`Conflict: Issue already closed with different reason`).
- Unknown flag / malformed invocation → exit 2 (clap usage error).
- No error path observed a panic.

### Lifecycle (validated live on both pins)

create → claim (`in_progress` + assignee) → release (`open`, assignee cleared)
→ assign while open (stays `open`) → `update --clear-assignee` (recoverable)
→ close (`closed`) → reopen (`open`, assignee cleared) → close. All steps
exit 0 on both pins; `reopen` clears the assignee on both, so a reopened bead
returns to the claimable frontier.

### Cross-variant interoperability (validated live, both directions)

- Workspace initialized by `bead-pre-feature`, then claimed and closed by the
  feature-enabled pin: all exit 0.
- Workspace initialized by the feature-enabled pin, then claimed,
  fence-updated (`--if-revision 2`), closed, and `sync flush-only`'d by
  `bead-pre-feature`: all exit 0.

A NEEDLE host may therefore drive one workspace with either binary across an
upgrade window.

## Behavioral differences

1. **Attempt-resolution advertisement and commands.** The pre-feature pin
   omits `attempt_outcome` from the contract and carries no attempt-outcome /
   resolve schemas; the feature-enabled pin advertises
   `attempt_outcome.supported: true` with all four conformance knobs and the
   `resolve` command. `watchdog`, `resource`, and `analyze-exclusion` exist
   only on the feature-enabled pin (`watchdog` on the pre-feature pin: exit 2,
   `error: unrecognized subcommand 'watchdog'`).
2. **Degradation of capability-gated commands.** On the pre-feature pin,
   `resolve` and `watchdog` are rejected by clap with exit 2 and
   `unrecognized subcommand '…'` — a clean, non-panic fallback signal. On the
   feature-enabled pin, `resolve --help` succeeds and documents
   `--attempt-id/--outcome/--action`; an invocation is recognized and fails on
   domain grounds (see the defect below), never as an unknown subcommand.
3. **Starvation visibility (NEEDLE fallback).** With an assigned-open bead in
   the workspace:
   - pre-feature pin: `bead list --ready` **still lists the bead** (it
     overstates what `claim` will deliver) and **no diagnostic is written**;
   - feature-enabled pin: `bead list --ready` **excludes it** and writes
     `.beads/diagnostics/pluck-starvation-diagnostic.log`, naming the bead and
     the remedy (`bead update <id> --clear-assignee`).
   `claim` refuses the bead on both pins, so this is a *visibility*
   difference, not a double-assignment hazard. On the pre-feature pin NEEDLE
   can observe the classic starvation shape — non-empty frontier, claims
   returning null — with nothing in the diagnostics directory to explain it.
4. **Resolve execution (defect, feature-enabled pin only).** See next section.

## Recorded defect: `resolve` fails on every workspace (beadrs-6b891bb7)

- **Symptom.** On the feature-enabled pin, in a workspace its own `init`
  created: `bead resolve <id> --attempt-id att-001 --outcome verified_success
  --action close` → **exit 5**, stderr:
  `Error: Integrity error: Failed to read issue: SQLite error: no such column:
  updated_at_revision in SELECT updated_at_revision, base_status,
  attempt_tier, consecutive_failures FROM issues WHERE id = ?1`.
- **Root cause (source-confirmed).** `get_issue_state` in
  `src/service/attempt.rs` selects `updated_at_revision`, but no store
  migration creates that column — `src/store/migrations.rs` adds `revision`
  (line 376) and `attempt_tier` / `consecutive_failures` (lines 563–567), and
  `updated_at_revision` appears nowhere else in `src/`. The `IssueState`
  field being populated is `revision`, so the SELECT almost certainly meant
  the migration-created `revision` column. Committed HEAD carries the same
  code, so this is not pin-specific decay.
- **Why existing tests miss it.** `tests/attempt_outcome_round_trip.rs`
  inserts `attempt_outcomes` rows directly into SQLite and never drives
  `get_issue_state`; the variant suites assert only that `resolve` is
  *recognized* (not "unrecognized subcommand"), which an exit-5 integrity
  error satisfies.
- **Containment.** The failed resolve is atomic: the bead stays `Open` at its
  prior revision, and the core dispatch loop is unaffected. A consumer that
  falls back to `close` / `release` on resolve failure loses nothing but the
  receipt.
- **Blast radius.** The entire resolve execution path — replay detection,
  revision guard, fencing token, receipts — is unreachable at runtime while
  `capabilities` advertises `attempt_outcome.supported: true`. Those knobs
  could not be validated end-to-end on any binary; they are advertisement-only
  today.
- **Regression pin.**
  `tests/needle_variant_dispatch_paths.rs::resolve_execution_fails_with_recorded_integrity_defect_on_feature_enabled_pin`
  asserts the current failure mode (exit 5, named column, no state change)
  and must be flipped to assert a successful resolve + receipt when the
  column bug is fixed.

## Deploy-safety verdict

- **`bead-pre-feature` (0.2.4): safe for the NEEDLE dispatch loop, with
  mandatory feature detection.** The shared contract is intact, atomic claim
  and fencing behave identically to the new pin, and the lifecycle is
  unimpaired. A consumer MUST consult `capabilities` and treat the absence of
  `attempt_outcome` / `resolve` / `watchdog` as "fall back to
  update/close/release" — the clap exit-2 rejection is only the backstop. Its
  `list --ready` overstatement (difference 3) means frontier *display* cannot
  be trusted as a claimability predicate on this binary; only `claim`'s
  verdict counts.
- **`bead-attempt-resolution-f25ab5c` (0.2.6): safe for the NEEDLE dispatch
  loop; the resolution path is NOT yet usable end-to-end.** Atomic claim,
  fencing, lifecycle, watchdog, and starvation diagnostics are all correct,
  and it can finish work started by the older binary. But
  `attempt_outcome.supported: true` must not be read as "resolve works":
  until beadrs-6b891bb7 is fixed, every resolve exits 5, so NEEDLE should
  treat resolve as optional-with-fallback (on resolve failure, fall back to
  `close`/`release`, which behave correctly) rather than as a hard
  dependency.

Neither variant requires a NEEDLE-side code fork: the dispatch loop, the
fencing flags, and the lifecycle verbs are byte-for-byte the same contract on
both. The only consumer-side logic the differences demand is (a) feature
detection before calling capability-gated commands, and (b) not trusting
`list --ready` as claimability truth on the pre-feature pin.

## Reproducing

```bash
cd /home/coding/bead-rs
cargo test --test needle_variant_dispatch_paths    # 11 tests, ~9s

# byte-verify the pins first
cd pinned-binaries
sha256sum -c <(python3 -c "
import json
for n in ('bead-pre-feature','bead-attempt-resolution-f25ab5c'):
    m=json.load(open(n+'.metadata.json')); print(m['binary_sha256']+'  '+n)")
```

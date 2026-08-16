# ADR-005: Diagnose issues held off the ready frontier by assignment; never release them automatically

**Status**: Accepted

**Date**: 2026-08-16

**Decision-makers**: bead-rs maintainers

## Context

An issue that is `open` **and** carries an assignee is not an active claim — a
claim sets `in_progress` — but it is excluded from the ready frontier, so no
worker will ever select it.

Nothing surfaced that shape. `show` renders it as an ordinary healthy open
issue. `doctor`'s dependency check reports blocked counts derived from the
dependency graph and says nothing about assignment. A workspace can therefore
accumulate unclaimable work indefinitely while every diagnostic reports clean.

Field evidence, 2026-08-16: a sweep of 66 workspaces found **583** issues in
this state across 47 of them. Ten workspaces had an entirely empty ready
frontier while workers ran against them and starved. Median age 26 hours,
oldest 37 hours. `doctor` reported all ten as healthy. In one workspace, 39 of
40 such issues had zero active blockers — assignment alone was holding them.

Reaching the state is legitimate and intentional:

- `reopen` preserves assignment by design (`reopen_issue_impl` retains the
  assignee; `reopen --help` documents "Preserves existing assignee";
  `test_reopen_retains_assignee` locks it).
- `release` deliberately refuses an assigned open issue and names the remedy:
  "Cannot release assigned open issue - use 'update --clear-assignee' instead".

So the store is behaving as specified. The defect is purely one of
**observability**: a legitimate state with a severe operational consequence had
no diagnostic.

R034 already adopts stale `in_progress` detection. It does not cover this case,
because these issues are not `in_progress`.

## Decision

Add a `ready_frontier` check to `doctor`, reported under the `dependencies`
scope, that warns when open issues carry an assignee, names a bounded sample of
the offending ids, and states the exact remedy
(`bead update <id> --clear-assignee`).

The check is **advisory only**. `doctor` never clears an assignee, not even
under `--repair`.

## Rationale

Diagnose-don't-mutate is already this project's settled position for anything
whose correct resolution depends on operator intent. Section 7 states that
version 0.1 "never ... alters lifecycle/dependency data automatically" and that
such cases are diagnosed with a recommended manual remedy. R034 adopts the same
stance for stale `in_progress` ("Advisory only: doctor never releases work
itself").

Assignment is exactly that kind of state. Only the operator knows whether a
given assignment still means something — a deliberately parked issue reserved
for a named person is indistinguishable, at the schema level, from residue left
by a worker that moved on. Auto-clearing would silently destroy intent in the
first case to fix the second.

The `dependencies` scope is the right home: it already answers "what can
actually be claimed right now" via the blocked-issue count. Assignment
exclusion is the same question reached by a different route.

## Consequences

### Benefits

- The starvation mode that produced 583 stuck issues becomes visible in the
  tool operators already run when a workspace feels wrong.
- The warning is directly actionable: it names ids and the exact command.
- No behavior change to `reopen`, `release`, `update`, or claim selection.

### Drawbacks

- A workspace that legitimately parks work under an assignee will warn every
  run. There is currently no way to mark an assignment as intentionally
  held; such an operator would learn to ignore the line, which is the classic
  path to a warning losing its meaning.
- The check reads assignment state directly rather than through a shared
  eligibility explanation, so its notion of "excluded from the frontier" is a
  second implementation of a rule R001 also owns.

### Alternatives Considered

- **Auto-clear under `--repair`**: rejected. Indistinguishable from destroying
  a deliberate reservation, and contrary to section 7 and R034.
- **Clear the assignee in `reopen`**: rejected. See the field evidence — only 1
  of the 583 observed issues originated from `reopen`, so it would have fixed
  ~0.2% of the harm while breaking a documented, tested contract.
- **Report as an error rather than a warning**: rejected. The state is
  legitimate; a nonzero exit would break automation for workspaces that park
  work intentionally.
- **Extend R034 to cover it**: rejected as a starting point. R034 is scoped to
  non-leased `in_progress` beads aged past an interval; folding a different
  lifecycle state into it would blur a specification already written. They
  should converge on shared reason codes instead (see Implementation).

## Implementation

Landed in `18fa7ee`: `check_ready_frontier` in `src/service/doctor.rs`, wired
into the dependencies scope as a warning, with
`test_doctor_reports_open_issue_held_by_assignee` covering healthy → warning →
cleared.

Outstanding follow-up, deliberately not in the initial commit:

1. Emit R001 semantic reason codes rather than prose. R034 requires that stale
   detection "share reason codes with R001/R019 rather than inventing parallel
   semantics"; this check must meet the same bar, and today it does not.
2. Decide whether an intentionally-held assignment can be declared, so the
   warning stays meaningful in workspaces that park work on purpose.
3. Machine-readable output: the ids are currently prose inside the message.

Tracked as post-0.1 roadmap item **R035**.

## Related

- Section 7, Diagnostics and recovery — the diagnose-don't-mutate rule
- **R034** — stale `in_progress` detection; the sibling case, same stance
- **R001** — explain claim and readiness decisions; owns the reason codes this
  check should be emitting
- **R019** — starvation diagnostics

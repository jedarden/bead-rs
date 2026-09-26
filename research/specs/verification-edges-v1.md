# Verification edges v1 specification

Status: normative.

Implements plan section 12 R025, authorized by
`docs/adr/001-declared-verification-edges-over-title-heuristics.md`. This
specification defines the third dependency kind `verifies` and the advisory
inverted-verification-gate diagnosis derived from it. It constrains only the
dependency graph and the diagnostics that read it; it adds no command, no
stored status, and no readiness rule beyond the ones already normative.

## The declared kind

`bead dep add BLOCKED BLOCKER --kind verifies` records that BLOCKER checks
the work BLOCKED performs. The edge is stored in the common orientation —
BLOCKED is the blocked issue, BLOCKER the verifier — exactly as the other
kinds store their rows, and `bead dep remove BLOCKED BLOCKER --kind verifies`
removes it.

Native mutation accepts exactly the kinds `blocks`, `relates_to`, and
`verifies`, and fails closed on any other kind string. Checkpoint interchange
is the one channel that may carry an unknown kind: it preserves such edges
verbatim without interpreting them, per the R018 unknown-schema rule. A
`verifies` edge is auditable like any other: each add or remove appends its
`dependency_added` or `dependency_removed` event with the kind recorded in
the event detail, on the same transaction as the edge change.

`capabilities` advertises the accepted set as `dependency_kinds`, and `bead
schema explain` documents that only `blocks` gates readiness.

## Insertion and cycles

Insertion is structural and never consults issue titles. No command infers a
check relationship, or any dependency, from title text, prefixes, or any
other issue field; the declared edge is the only signal.

An edge insert accepts every coexistence of kinds over one (BLOCKED, BLOCKER)
pair. In particular a `verifies` edge may be added to a pair that already
carries `blocks`, and a `blocks` edge may be added to a pair that already
carries `verifies`, in either order, and both rows persist. A self-edge is
rejected for every kind, including `verifies`.

Cycle detection traverses `blocks` edges only. A `blocks` insert that would
close a directed cycle is still rejected; `verifies` edges never contribute
to that traversal, so cycles formed entirely of `verifies` edges are
accepted, in the same class as `relates_to`.

## Readiness

Only `blocks` gates readiness. A `verifies` edge never removes an issue from
the ready frontier and never adds one; the eligibility query is unchanged by
the kind's presence. An issue whose only incoming edges are `verifies` edges
is ready when every other readiness condition holds, even if its verifier is
open.

## Inverted verification gates

An inverted verification gate is a pair (BLOCKED, BLOCKER) such that a
`blocks` edge from BLOCKED to BLOCKER exists and a `verifies` edge over the
same pair also exists. The pair states that the check must close before the
work it checks may start, an ordering no execution can satisfy. The graph is
acyclic and internally consistent in this shape; only the declared
`verifies` edge makes the fault decidable, which is why the diagnosis is
structural rather than heuristic.

`bead doctor --scope dependencies`, and every default run that includes the
dependencies scope, reports the check `inverted_verification_gates`:

- With no inverted pairs the check reports status `ok`.
- With one or more inverted pairs the check reports status `warning`. The
  human-readable finding begins `WARN ` and names a bounded sample of the
  pairs plus a remainder count when the list is truncated.
- The JSON details carry `count`, the complete `gates` list — one object per
  pair with `blocked`, `blocker`, and the stable reason code
  `inverted_verification_gate` — a `remedy` naming
  `bead dep remove BLOCKED BLOCKER --kind blocks` or `--kind verifies`, and
  an `explanation`. An `ok` check carries `count` `0` and an empty `gates`
  list.
- Pairs are reported in one canonical order so two runs over the same graph
  produce identical output.

The diagnosis is advisory in both directions. Insertion never rejects an
inverted gate, because a deliberate "prove the baseline green before touching
the work" ordering is structurally identical to the authoring slip and only
the author can distinguish them. The doctor check never mutates: it does not
remove or re-orient edges, and `doctor --repair` does not act on the finding.
A warning does not fail the run; `doctor` still exits 0.

## Why reporting

When an issue is held by an unfinished blocker that also carries a `verifies`
edge to it, `bead why` reports the stable reason code `blocked_by_verifier`
in addition to the ordinary unfinished-blocker code, and the blocker detail
marks the verifier with `verifies_blocked_issue`. The code answers "blocked
by the bead that verifies it", which the ordinary code cannot name. An issue
whose verifier does not gate it reports neither.

## Conformance

The behavioral suites are `tests/verifies_edges.rs` (kind surface, diagnostic,
why, capabilities, schema explain), `tests/verifies_concurrency.rs`
(concurrent edge insertion), and `tests/verifies_fixtures.rs` over the
edge-shape fixtures in `tests/fixtures/verifies/`, which pin the
inverted-gate, deliberate-gate, no-gate, title-inference-guard,
`verifies`-cycle, and kind-coexistence (`relates_to` + `verifies` over one
pair) shapes as data.

# ADR-001: Diagnose Inverted Verification Gates From a Declared Edge Kind, Not From Issue Titles

**Status**: Proposed

**Date**: 2026-08-12

**Decision-makers**: bead-rs release owner

## Context

A dependency graph can be acyclic, satisfy every constraint `bead-rs` enforces,
and still encode work in an order that cannot happen. The recurring instance is
an **inverted verification gate**: an edge recorded so that the bead which
*checks* some work blocks the bead which *performs* it. "Add tilde expansion
helper function" waits on "Run clippy and fix warnings"; the helper can never
be written, because the lint that would examine it must close first.

`bead-rs` is structurally immune to three neighbouring defect classes, by
design rather than by accident:

- **Cycles** — §3.4 rejects a `blocks` edge that would close a directed cycle,
  with detection and insertion in the same transaction.
- **Stale blocked status** — there is no stored `blocked` status to go stale;
  `base_status` is `open | in_progress | deferred | closed` and readiness is
  computed from authoritative rows.
- **Readiness disagreeing with the graph** — the eligibility query embeds the
  unfinished-blocker test, so a claim cannot contradict the edge set.

Inversion is not covered by any of these. An inverted gate is normally
**acyclic** — one edge, no return path — so cycle rejection accepts it, and
because readiness is computed correctly the bead is correctly reported as
blocked. The graph is internally consistent and semantically wrong.

Measured evidence that this is a real and recurring authoring error rather than
a theoretical one, gathered 2026-08-12 from two live workspaces built on a
different bead implementation:

- 21 inverted edges in one workspace, arising at a steady 4–6% of newly created
  beads across three months — a persistent authoring slip, not a one-time
  migration artifact.
- 24 inverted edges plus 7 cycles in a second, found on the first run of a
  detector. Several were two-node rings that were simultaneously a cycle and an
  inversion; cycle rejection alone would have caught only those.

A title-shape heuristic (blocker title starts with "Verify"/"Validate"/"Run
clippy", blocked title starts with "Implement"/"Add"/"Fix") does find these. It
was implemented in the other tool and works. It is nonetheless the wrong
mechanism for `bead-rs`: the store already commits to "cross-tool recognition
without title heuristics", titles are free text with no stable cross-tool
meaning, and the rule misclassifies ordinary titles that merely contain a
verification noun ("Add logging verification and run test suite").

## Decision

Introduce a third dependency kind, `verifies`, that lets an author *declare*
the check relationship, and diagnose inversion structurally from that
declaration. Report the diagnosis; never reject the edge at insert time. Do not
infer the relationship from issue titles under any circumstance.

Specified as post-0.1 item **R025**; §3.4's two-kind set is unchanged for 0.1.

## Rationale

Making the relationship explicit converts a guess into a fact. With a
`verifies` edge from V to I present, "a `blocks` edge whose blocker `verifies`
its blocked bead" is decidable, exact, and carries no false positives — where
the heuristic's accuracy depends on an authoring convention no schema enforces.

The cost is that the diagnosis only sees pairs someone bothered to declare, so
coverage is opt-in and starts at zero. That is the correct trade for this
project: a diagnostic that is silent until told something is safer than one
that is confidently wrong about free text, and the declaration is independently
useful to R023's `why` explanation.

Detection must stay advisory. A deliberate "prove the baseline is green before
anyone touches this" gate is a legitimate ordering, and is structurally
identical to the error — only the author knows which one they meant. This
matches R021's existing posture: policy lint "is advisory and cannot make a
bead eligible or ineligible."

## Consequences

### Benefits

- Exact rather than probabilistic; no false positives from title wording.
- Reuses the existing dependencies doctor scope that already runs cycle
  detection, rather than adding a parallel diagnostic surface.
- The `verifies` edge is independently valuable: R023 can answer "why is this
  blocked?" with "by the bead that verifies it", which is the answer that
  actually tells the reader something is wrong.
- Keeps the "no title heuristics" property intact.

### Drawbacks

- Opt-in coverage. Undeclared pairs are invisible; the diagnostic finds nothing
  in a workspace that never adds `verifies` edges.
- A third kind touches the checkpoint format, the interchange profiles, and
  every consumer that enumerates kinds — a wider blast radius than a lint.
- Requires authoring discipline the tool cannot enforce, which is precisely the
  weakness the heuristic approach avoids.

### Alternatives Considered

- **Port the title heuristic.** Rejected: contradicts the store's stated
  cross-tool recognition principle, and misclassifies titles containing a
  verification noun. Cheap and immediately effective, but wrong for this
  codebase; it remains appropriate in a tool that has already accepted
  title-shaped signals.
- **Reject inverted edges at insert time, as cycles are rejected.** Rejected: a
  verification gate ahead of implementation is sometimes exactly what the
  author means, and no structural test distinguishes intent. Rejection would
  make a legal ordering unexpressible.
- **Infer verification from issue type rather than a new edge kind.** Rejected:
  type describes the bead, not the relationship. A `test` bead may block an
  implementation bead for reasons unrelated to verifying it, and the same test
  bead may verify one bead while merely relating to another.
- **Do nothing; rely on cycle rejection.** Rejected: an inverted gate is
  normally acyclic, so cycle rejection sees nothing. It caught only the
  two-node subset in the measured workspaces.

## Implementation

Post-0.1, gated on this ADR being accepted. R025 carries the normative text.
Sequenced after R017, which already extends edge semantics and must treat
`verifies` consistently in its conditional-edge cycle analysis.

## Related

- `docs/plan/plan.md` §3.4 — dependency canonicalization, kinds, and cycle rejection
- `docs/plan/plan.md` R017 — conditional dependencies (edge semantics)
- `docs/plan/plan.md` R021 — workspace policy lint (advisory-diagnostic posture)
- `docs/plan/plan.md` R023 — unified `why` explanation facade
- `docs/plan/plan.md` R025 — the item this decision authorizes

## Supersedes

None.

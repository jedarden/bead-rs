# `verifies` Edge-Shape Fixtures (R025, ADR-001)

Data-driven fixtures for the declared `verifies` dependency kind and the
structural inverted-verification-gate diagnosis. `tests/verifies_fixtures.rs`
drives the real CLI over every case in `cases.json` and asserts the observable
behavior recorded there; the `verifies_edges.rs` and `verifies_concurrency.rs`
suites pin the same contract procedurally. This directory exists so the two
gate shapes are reviewable as data, independent of any test's authoring code.

## Provenance

Every case is invented for `bead-rs` under
`research/specs/clean-room-protocol.md`. No record from another bead
implementation and no real workspace was consulted. Governing requirement:
`research/specs/verification-edges-v1.md` (plan R025), authorized by
`docs/adr/001-declared-verification-edges-over-title-heuristics.md`.

Titles in the fixtures are deliberately shaped (implementation-flavored,
verification-flavored) so the suite can prove they are never consulted: the
tool under test must diagnose from declared edges only, never from title
heuristics.

## Cases

| Case | Edge set | Expected |
|------|----------|----------|
| `inverted-gate` | `blocks` then `verifies` on the same (work, checker) pair | One advisory gate finding; the gate still gates |
| `deliberate-baseline-gate` | `verifies` then `blocks` on the same (work, baseline) pair | Observably identical to `inverted-gate` — the deliberate reading must stay legal |
| `declared-check-no-gate` | `verifies` alone | No finding; both beads stay ready |
| `plain-gate-no-declaration` | `blocks` alone, checker-flavored titles | No finding — a title heuristic would flag this shape; the tool must not |
| `verifies-cycle` | A two-bead `verifies` ring | No finding; the cycle check stays `ok` — only `blocks` edges form cycles |
| `relates-to-plus-verifies` | `relates_to` then `verifies` on one pair | Both rows persist (every kind coexistence is accepted); no finding, both beads stay ready |

## Observable behavior under test

For every case the suite asserts:

- **Insertion never rejects** — every edge in the case commits, in the
  fixture's `order`, regardless of kind or orientation (ADR-001: report, never
  reject; a deliberate gate must stay expressible).
- **Doctor posture is advisory** — `bead doctor --scope dependencies` exits 0
  with `has_errors: false`; the `inverted_verification_gates` check reports
  `ok` or `warning` exactly as the case expects, and reported gates match the
  case's expected `(blocked, blocker)` key pairs.
- **Readiness keys on `blocks` alone** — the ready frontier membership matches
  the case's `ready_beads`: a `verifies` edge never removes a bead, a `blocks`
  edge always does.
- **`why` keys on the declared edge** — the distinct `blocked_by_verifier`
  reason code appears on the case's `why_subject` exactly when a declared
  `verifies` edge pairs with a `blocks` gate from the same blocker.

## Adding a case

Append an object to `cases` in `cases.json` following the existing schema
(`beads` keyed by short name, `edges` with an insertion `order`, and an
`expected` block for each observable above). The suite interprets the file
generically; no test code changes are needed for a new shape.

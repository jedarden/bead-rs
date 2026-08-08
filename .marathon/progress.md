# Marathon Coding progress

This is an append-only handoff log for autonomous iterations. Record verified
facts, failed approaches, limitations, and the next recommended action. Do not
rewrite or delete earlier entries.

## 2026-08-07 — Harness initialized

- Repository has an independent Git root and Apache-2.0 licensing.
- Sanitized clean-room, interchange, NEEDLE, and conformance specifications
  exist under `research/specs/`.
- The current binary is only a transparent specification scaffold.
- No release feature in `.marathon/feature_list.json` has been implemented.
- Start with F001 and maintain Rust 1.75 compatibility.

## 2026-08-07 — Implementation plan prepared

- Added `docs/plan/plan.md` as the implementation blueprint for the independent
  SQLite model, lifecycle, dependencies, claiming, profiles, checkpoint safety,
  diagnostics, testing, and release gates.
- Added a sanitized behavior report that excludes upstream schema, SQL, tests,
  source, and internal design.
- Wired the plan into the mandatory start-of-iteration reading order.
- No feature pass state changed; implementation and verification remain for
  later Marathon iterations.

## 2026-08-07 — Post-0.1 feature ideation recorded

- Ran the workspace `plan-idea-gen` funnel against `docs/plan/plan.md`: 100
  generated ideas, 26 triage survivors, 15 pairwise advancers, and 10 final
  candidates after adversarial and completeness passes.
- Added every idea and rejection reason to `docs/notes/ideas-ledger.md`, with
  implementation dossiers for the finalists.
- The candidates were not added to the 0.1 roadmap or feature ledger; adoption
  awaits an explicit product decision.
- No feature pass state changed.

## 2026-08-07 — Feature candidates dispositioned

- Promoted decision explanations, fenced leases, logical revisions, safe
  queries/views, and public schema identification to the post-0.1 roadmap.
- Specified `schema_ref` so each bead identifies its governing immutable public
  schema independently from its profile and private SQLite layout.
- Deferred resource locks, bulk manifests, idempotency keys, and worker
  capabilities to the ideas ledger.
- Rejected native SQLite backup/restore; JSONL is the portable recovery backup,
  while SQLite supplies ACID live operation.
- No F001-F014 pass state changed.

## 2026-08-07 — Second feature ideation run recorded

- Re-ran the workspace `plan-idea-gen` funnel after the first product
  disposition, excluding all previously considered mechanisms.
- Generated 100 new ideas, retained 25 through triage, advanced 15 through
  pairwise comparison, and selected 10 after adversarial and completeness
  passes.
- Appended every candidate, verdict, and finalist dossier to the ideas ledger.
- No second-run finalist was promoted into the plan; selection remains a
  separate product decision, and no F001-F014 pass state changed.

## 2026-08-07 — Second-run candidates adopted

- Promoted all ten second-run finalists into the post-0.1 roadmap.
- Defined comments as complete durable backup content while making comment
  bodies optional in normal retrieval through explicit projection flags.
- Added scoped doctor diagnostics, bounded declarative conditional
  dependencies, and namespaced schema-bound structured bead data.
- Added `research/specs/extended-bead-payload-v1.md` for the portable payload
  and diagnostic contracts.
- No F001-F014 pass state changed.

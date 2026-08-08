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

## 2026-08-08 — Intelligent claim contract specified

- Expanded plan section 3.5 from simple FIFO ordering into a full versioned
  scheduling contract while retaining `fifo-v1` for release 0.1.
- Specified completion-unlock impact, ready-age promotion,
  least-recently-served rotation, failure classification, unproven-work
  preference, retry cadence, quarantine, and attempt-epoch reset.
- Added bounded initial-context and lazy retrieval behavior for NEEDLE-style
  claim-then-prompt dispatch.
- Defined atomicity, explanation, derived-cache correctness, schema additions,
  policies, and conformance scenarios under roadmap feature R019.
- Defined native P0-P5 priority ordering from urgent through aspirational,
  including aspirational-worker opt-in and explicit lossy profile mapping.
- No F001-F014 pass state changed.

## 2026-08-08 — Priority range corrected to P0-P4

- Supersedes the P0-P5 detail in the immediately preceding planning entry.
- Removed P5 from the active plan and interchange specification.
- P4 is now the aspirational/backlog tier and retains optional automatic-worker
  opt-in behavior.
- The P0-P4 range matches observed bead tooling and avoids an unnecessary lossy
  compatibility mapping.
- No F001-F014 pass state changed.

## 2026-08-08 — File-intent gating deferred

- Explicitly excluded predeclared file manifests, planning gates, intent
  fencing, file-derived dependency enforcement, and post-diff path checks from
  the adopted roadmap.
- Preserved the explored collision-reduction concepts in the ideas ledger for
  reconsideration under a future file-writing mechanism.
- Removed the roadmap's stray implication that resource conflicts are already
  part of claim/readiness explanations.
- No F001-F014 pass state changed.

## 2026-08-08 — Third feature ideation run recorded

- Applied the workspace `plan-idea-gen` funnel at quick scale: 40 base ideas,
  three crossover/completeness entrants, clustering, triage, pairwise ranking,
  adversarial kill pass, and ten finalists.
- Kept the run constrained to optional, local mechanisms and excluded the
  recently deferred file-intent/write-set gating design.
- Appended every candidate, verdict, and finalist dossier to the ideas ledger;
  none is adopted yet.
- No F001-F014 pass state changed.

## 2026-08-08 — Third-run candidates dispositioned

- Promoted cross-profile comparison, policy lint, general mutation dry-run,
  unified `why`, and explicit recurrence materialization to R020-R024.
- Clarified that general mutation dry-run extends the existing migration/import
  dry-run contract to update, lifecycle, and dependency operations.
- Rejected dependency rationale and graph slice export; deferred secret lint and
  portable execution outcomes to notes.
- Left verifiable acceptance evidence pending further product consideration.
- No F001-F014 pass state changed.

## 2026-08-08 — Claim performance and capacity plan specified

- Made ranking hybrid and incremental: writes maintain or invalidate only
  affected inputs, while claims shortlist, finalize request/time-dependent
  ranking, and revalidate authoritative eligibility atomically.
- Added a deterministic rapid-fire lifecycle harness covering claim/close,
  claim/release, mixed lifecycle, and dependency-churn workloads.
- Defined worker saturation sweeps at 100, 1k, 10k, 100k, and 1m beads,
  schema-stable benchmark reports, a machine-relative capacity profile, fast CI
  smoke coverage, and explicit resource-limited outcomes.
- Added F015 so implementation and verification of the harness is required
  before packaging and release completion.
- Clarified that ranking operates only on the ready frontier and that benchmark
  results distinguish total graph size from frontier width and graph shape.
- Expanded every scale to an approximately logarithmic 1-to-200-agent sweep and
  required complete degradation curves rather than stopping at first failure.
- Added measurable SQLite-efficiency constraints for bounded indexed queries,
  short writer-lock holds, query-plan checks, contention, WAL behavior, and
  write amplification.
- No feature pass state changed.

## 2026-08-08 — CLI help and man-page contract specified

- Required short and long help for every public command path, argument, option,
  value domain, default, conflict, and requirement, usable without a workspace.
- Defined reproducible section-1 `bead(1)` and per-command man pages generated
  from the authoritative `clap` command tree and structured supplements.
- Added recursive coverage, examples, snapshot/drift, cross-link, package
  content, and explicit non-system installation requirements.
- Required root help and `bead(1)` to teach the intended workflow, ready
  frontier, lifecycle, dependency semantics, atomic claims, and backup boundary.
- Added F016 and made packaging depend on complete CLI documentation.
- No feature pass state changed.

## 2026-08-08 — Git-trackable forensic checkpoints specified

- Required flushed portable artifacts to retain the full bead corpus across all
  lifecycle states plus continuous durable audit-event history for later
  forensic investigation.
- Defined automatic monolith-to-sharded transition using a canonical manifest,
  content-addressed objects, incrementally split hash-prefix issue partitions,
  and immutable sequence-ranged event shards.
- Required atomic manifest-last publication, complete hash/partition/event
  validation, monolithic/sharded restore equivalence, and changed-path reports.
- Clarified that checkpoint artifacts are committed by the surrounding Git
  workflow and mirrored to GitHub; bead-rs itself never commits or pushes.
- Added F017 and made packaging depend on its verified implementation.
- No feature pass state changed.

## 2026-08-08 — Gap-review round 1

- Clarified that the native core is implementation-ready while the 0.1 release
  remains blocked on independently approved F012 external-profile fixtures.
- Separated minimal 0.1 FIFO claiming from post-0.1 intelligent scheduling and
  lease/fencing fields, and marked incorporated roadmap subsets explicitly.
- Defined native `release`, optional create descriptions, core read-only comment
  projections, import dry-run, and a complete capabilities command inventory.
- Added an authoritative checkpoint generation/mode pointer with atomic mode
  transitions, tombstones, and complete Git changed-path semantics.
- Split forensic import into exact empty-store restore and provenance-preserving
  merge with UUID, event identity, continuity, replay, and divergence rules.
- No feature pass state changed.

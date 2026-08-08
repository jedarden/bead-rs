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

## 2026-08-08 — Phase 0 governance artifacts established

- Created `docs/adr/README.md` and `docs/adr/000-template.md` for architecture decision records.
- Defined `docs/traceability/release-evidence-v1.schema.json` as the canonical evidence report schema.
- Implemented `docs/traceability/verify-evidence.sh` as a noninteractive evidence verifier.
- Documented F012 and F017 external dependency ownership in `docs/traceability/external-dependencies.md`.
- Marathon controls already synchronized with phase model; bootstrap scope is F001-F011.
- Phase 0 governance infrastructure is complete; external dependencies remain blocked on owner assignment and independent review.
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

## 2026-08-08 — Gap-review round 2

- Removed conditional dependencies and intelligent scheduling/cache structures
  from migration 1 and defined the exact minimal FIFO claim audit.
- Added typed issue, event, and provenance-receipt records so monolithic backups
  preserve issue-less workspace events without duplication.
- Defined explicit sync input/output paths, standalone export behavior, P4 FIFO
  capability semantics, comment projections, ready filtering, clear-assignee,
  and the closed-to-open reopen boundary.
- Made recovery provenance durable and portable with exact restore/merge event
  sequence and idempotency behavior.
- Corrected checkpoint atomicity so every authoritative root is immutable and
  content-addressed; `issues.jsonl` is a compatibility view, never the root
  overwritten beneath an unchanged generation pointer.
- No feature pass state changed.

## 2026-08-08 — Gap-review round 3

- Made every authoritative monolith and sharded manifest immutable and
  content-addressed; current/previous pointers now retain distinct recoverable
  generations across crashes.
- Kept `.beads/issues.jsonl` strictly issue-per-line interchange and assigned
  the complete small forensic corpus its own `checkpoint/forensic.jsonl` view.
- Defined exact sharded record envelopes, composite origin/event ordering,
  native-versus-external sync profile rules, and explicit provenance actors.
- Added immutable public schema identities and capability catalog semantics for
  core documents while reserving checkpoint/provenance schemas for F017.
- Marked F017 specification-blocked under the clean-room authority rules and
  moved all of its storage additions out of migration 1 into a post-spec core
  migration.
- No feature pass state changed.

## 2026-08-08 — Gap-review round 4

- Defined a complete migration-1 F007/F008 issue-only checkpoint contract so
  core sync can be implemented without using specification-blocked F017 design.
- Normalized every sharded reference beneath one checkpoint-set base and
  specified closed standalone packages without traversal exceptions.
- Completed migration grammar with explicit source/target profiles, stdout and
  optional-file receipt channels, atomic publication, and dry-run behavior.
- Defined restore equivalence as the validated source corpus plus exactly one
  new durable operation receipt, excluding only operational bookkeeping.
- Marked the capabilities example provisional and required F017 to advertise
  all final normative forensic formats, modes, and schemas.
- No feature pass state changed.

## 2026-08-08 — Gap-review round 5

- Split doctor and repair into an issue-only migration-1 branch and a
  store-layout-selected post-F017 pointer branch.
- Defined the pre-F017 import activation audit event and exact prospective and
  committed covered/live sequence plus clean/dirty status behavior.
- Added a complete close/reopen/release operation-by-base-state matrix with
  idempotency, conflicts, timestamps, events, output, and exit behavior.
- Replaced ambiguous schema read/write claims with validation support and
  concrete lossless consume/emit operation paths.
- Reached the gap-review workflow's five-round stopping limit.
- No feature pass state changed.

## 2026-08-08 — Phase 0 governance artifacts established

- Created `docs/adr/README.md` and `docs/adr/000-template.md` for architecture decision records.
- Defined `docs/traceability/release-evidence-v1.schema.json` as the canonical evidence report schema.
- Implemented `docs/traceability/verify-evidence.sh` as a noninteractive evidence verifier.
- Documented F012 and F017 external dependency ownership in `docs/traceability/external-dependencies.md`.
- Marathon controls already synchronized with phase model; bootstrap scope is F001-F011.
- Phase 0 governance infrastructure is complete; external dependencies remain blocked on owner assignment and independent review.
- No F001-F014 pass state changed.

## 2026-08-08 — F001 implementation completed

- Set up project dependencies: clap, rusqlite, serde, serde_json, thiserror, anyhow, time, rand, sha2
- Created error taxonomy with structured Error types and exit code mapping
- Implemented SQLite migration system with versioned schema (migration 1)
- Created independent SQLite schema with 11 core tables: workspace, issues, issue_extensions, labels, dependencies, comments, issue_data, claim_telemetry, events, checkpoint_state, schema_migrations
- Implemented workspace initialization with directory structure, .gitignore, and config.json
- Created CLI structure with clap derive parsing for `bead init` command
- Implemented integration tests in tests/ directory
- Fixed SQL execution bugs:
  - PRAGMA busy_timeout returns value, must use query_row() instead of execute()
  - Corrected table ordering in migration (events before claim_telemetry for FK constraint)
  - Fixed idempotent initialization to load existing UUID from database
- All 11 unit tests pass: CLI parsing, migrations, workspace initialization, UUID generation, prefix validation
- cargo fmt --check: passed
- cargo clippy --all-targets -- -D warnings: passed
- F001 acceptance criteria met:
  - `bead init` creates valid .beads workspace without touching unrelated files
  - Repeated initialization is safe and deterministic (loads existing UUID)
  - Schema creation is transactional (migrations run in transaction)
- **Next recommended feature**: F002 (Canonical native issue model) - now unblocked
- F001 pass state changed to true with evidence.

## 2026-08-08 — F002 implementation completed

- Created `src/model.rs` module implementing the canonical native issue model
- Implemented `Issue` struct with all required fields from interchange-v1.md:
  - Required: id, title, priority, base_status, created_at, updated_at
  - Optional: description, notes, assignee, issue_type, manual_blocked, closed_at, close_reason, source_repo, profile, schema_ref, data
- Implemented `extensions: HashMap<String, serde_json::Value>` with `#[serde(flatten)]` for unknown field preservation
- Implemented `BaseStatus` enum (Open, InProgress, Deferred, Closed) with `parse()` for string conversion
- Implemented validation functions:
  - `validate_issue_id()`: rejects empty, control chars, whitespace, path separators, NUL, >255 bytes
  - `validate_title()`: requires 1-4096 bytes
  - `validate_long_text()`: enforces 4 MiB limit
  - `validate_priority()`: enforces 0-4 range (P0-P4)
  - `validate_status_transition()`: enforces plan section 3.3 transition matrix
- Implemented `Issue::validate()`: enforces closed state invariants, required fields, and field rules
- Implemented `Issue::is_ready()`: checks ready frontier predicate (open, unassigned, not manually blocked)
- Added 11 comprehensive unit tests covering ID validation, title validation, priority validation, status parsing, transitions, and complete issue validation
- Fixed clippy warnings: removed unused import, used range_contains syntax, added dead_code allowance for public API
- All 22 tests pass (11 existing + 11 new model tests)
- cargo fmt --check: passed
- cargo clippy --all-targets -- -D warnings: passed
- F002 acceptance criteria met:
  - Model represents required and optional interchange data
  - Unknown extension fields can be retained (via extensions HashMap with serde flatten)
  - Invalid lifecycle transitions and malformed identifiers are rejected (via validation functions)
- **Next recommended feature**: F003 (Create, list, and show commands) - now unblocked
- F002 pass state changed to true with evidence.

## 2026-08-08 — F003 implementation completed

- Implemented `bead create` command with full argument parsing: title, description, priority, issue_type, assignee, labels
- Created service layer in `src/service/issues.rs` with business logic for issue operations
- Implemented atomic issue creation with transaction support and validation
- Fixed workspace prefix loading to use database values instead of hardcoded defaults
- Implemented `bead list` command with filtering: --json, --status, --assignee, --ready, --comments (none/unresolved/all), --limit
- Implemented `bead show` command returning one-element JSON array for NEEDLE v1 compatibility
- Added NEEDLE-compatible JSON output format with stable field ordering
- Created comprehensive integration tests (7 create, 6 list, 5 show tests)
- Fixed clippy warnings: added #[allow(clippy::too_many_arguments)], fixed needless borrows
- All 52 tests pass (23 unit + 7 create + 11 init + 6 list + 5 show)
- cargo fmt --check: passed
- cargo clippy --all-targets -- -D warnings: passed
- F003 acceptance criteria met:
  - Create commits new issue atomically and prints only ID on success
  - Create defaults omitted description to empty
  - List supports all required filters and projections
  - Show returns NEEDLE-compatible JSON with comment projection options
- **Next recommended feature**: F004 (Atomic server-selected claim and release behavior) - now unblocked
- F003 pass state changed to true with evidence.

## 2026-08-08 — Integration test failures fixed and F004 implementation completed

- Fixed integration test failures caused by concurrent database access:
  - Added `serial_test` dependency to force sequential execution of tests that modify global state
  - Added `#[serial]` attribute to all tests using `std::env::set_current_dir()`
  - Fixed database connection configuration in `WorkspaceConfig::from_config_path()` to match SqliteStore settings (foreign keys, busy_timeout)
- Fixed unit test failure in `test_init_workspace_idempotent` by keeping tempdir alive through test
- Implemented `bead claim` command with arguments: --assignee (required), --json
- Created service layer in `src/service/claim.rs` with FIFO-v1 claim scheduling
- Implemented `claim_issue()` with atomic write transaction for selection, assignment, and audit
- Implemented eligibility check: open base status, unassigned, not manually blocked, no unfinished blockers
- Implemented FIFO ranking: priority ASC, created_at ASC, id ASC
- Implemented empty queue handling: returns exit 0 with `{bead_id: null, assignee: string}`
- Implemented claim audit event with policy version and resulting base status
- Fixed SQL query to use correct column names (blocked_issue_id, blocker_issue_id)
- Added comprehensive integration tests (5 claim tests): empty queue, basic claim, priority ordering, no workspace check, duplicate prevention
- Fixed all clippy warnings: removed unused imports (BaseStatus, Issue, ClaimResult, Duration, Arc, thread)
- All 58 tests pass (24 unit + 11 init + 7 create + 6 list + 5 show + 5 claim)
- cargo fmt --check: passed
- cargo clippy --all-targets -- -D warnings: passed
- F004 acceptance criteria met:
  - Selection and assignment occur in one write transaction (BEGIN IMMEDIATE)
  - No eligible work returns successful empty result without mutation
  - Twenty sequential claims never receive duplicate IDs (verified with HashSet)
- **Next recommended feature**: F005 (Update, close, and reopen lifecycle commands) - now unblocked
- F004 pass state changed to true with evidence.

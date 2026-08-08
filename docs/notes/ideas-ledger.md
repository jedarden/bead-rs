# bead-rs ideas ledger

This append-only ledger records feature ideation runs, including rejected
ideas, so future runs can avoid repetition. A finalist is not part of the
release roadmap until explicitly adopted in `docs/plan/plan.md` and the
Marathon feature ledger.

## 2026-08-07 — plan-idea-gen: post-0.1 expansion

Target: `docs/plan/plan.md`. Pool: 100. After deduplication: 94 distinct
mechanisms. Triage survivors: 26. Pairwise advancers: 15. Kill-pass survivors:
9. Completeness entrants: 3, of which 1 survived. Finalists: 10.

Hard constraints: clean-room inputs; private independent SQLite schema; JSONL
interchange; Rust 1.75; no unsafe; no upstream database mutation; explicit
profiles; minimal complexity; do not duplicate F001-F014.

Verdicts: **FINALIST** means eligible for adoption, not yet adopted; KILL means
the idea lost this run for the stated reason.

| # | Idea | Cluster | Verdict and reason |
| ---: | --- | --- | --- |
| 1 | Decline instead of claim — temporarily exclude unsuitable work for one worker | Claim | KILL: per-worker exclusion state costs more than capability matching |
| 2 | Explain claim/readiness — emit semantic reasons candidates were skipped | Explainability | **FINALIST:** debugs empty queues and selection without mutation |
| 3 | Negative dependencies — prohibit concurrent issues | Claim | KILL: resource locks express the mechanism more directly |
| 4 | Expiring claim leases — return abandoned work safely | Claim | MERGED into finalist 4+68 with fencing |
| 5 | Failure-first issues — surface rollback instructions before execution | Planning | KILL: useful content convention, not a core mechanism |
| 6 | Evidence-gated closure — require configured completion evidence | Policy | KILL: narrower than reliability and coordination finalists |
| 7 | Anti-priority aging — report chronically bypassed work | Observability | KILL: metrics/decision trace can expose this later |
| 8 | Dependency challenge — attach rationale/verification to edges | Planning | KILL: metadata burden exceeds readiness value |
| 9 | Undo receipt — conditional inverse tokens for mutations | Mutation | KILL: broad inverse semantics are complex beside revision guards |
| 10 | Hard read-only mode — prohibit opening write transactions | Policy | KILL: valuable invariant but too small for a roadmap feature |
| 11 | Refuse ambiguous nested workspaces | Safety | KILL: should be folded into F001 discovery validation |
| 12 | Work quarantine — isolate policy-invalid imported records | Recovery | KILL: conflicts with all-or-nothing active import; reporting is safer |
| 13 | Completion veto edges — blockers that specifically prevent close | Planning | KILL: existing blocking semantics plus policy can cover it |
| 14 | Optimistic revision tokens — reject stale mutations | Mutation | MERGED into finalist 14+71 |
| 15 | Named saved views — persist validated filters and sort order | Query | MERGED into finalist 15+26+53 |
| 16 | Recurring issue templates — create next task on close | Planning | KILL: event-triggered generation expands automation surface |
| 17 | Work-in-progress limits — enforce local capacity atomically | Claim | KILL: resource/capability routing is more fundamental |
| 18 | Resource locks — atomically exclude conflicting claimed work | Claim | **FINALIST:** prevents local external-resource collisions |
| 19 | Semantic checkpoint diff — compare live/checkpoint/migration views | Interchange | KILL: useful later, lower leverage than bulk preview and schemas |
| 20 | Profile lockfile — pin exact adapter semantics per workspace | Interchange | KILL: explicit immutable profiles already supply most value |
| 21 | Claim explain plan — preview ordering/filter decisions | Explainability | MERGED into finalist 2+21 |
| 22 | Dead-letter review state — quarantine repeatedly failed work | Claim | KILL: failure counters and policy are underspecified |
| 23 | Correlation IDs — propagate operation IDs through audit output | Observability | KILL: cheap future addition, not top-ten roadmap weight |
| 24 | Declarative admission policy — require fields/labels/transitions | Policy | KILL: policy language scope exceeds immediate core value |
| 25 | Verifiable support bundle — package nonsecret diagnostics and hashes | Recovery | KILL: backup/restore provides more direct recovery value |
| 26 | Query projections — safe selected-field expression grammar | Query | MERGED into finalist 15+26+53 |
| 27 | Optional local daemon — amortize CLI startup via Unix socket | Platform | KILL: violates v0.1 shape and adds lifecycle/security burden |
| 28 | Remote store protocol — multi-host authoritative queue | Platform | KILL: distributed service is explicitly out of scope |
| 29 | Git checkpoint publisher — automatic commit/push | Platform | KILL: Git automation is explicitly out of scope |
| 30 | Dynamic issue-type plugins — load custom validation/lifecycle code | Platform | KILL: unsafe stability and provenance surface |
| 31 | Web dashboard — serve live graphs and worker occupancy | Platform | KILL: server/UI burden dwarfs local CLI value |
| 32 | Cross-workspace federation — claim through a store catalog | Platform | KILL: multi-store atomicity is not defined |
| 33 | Database replication — synchronize native SQLite operations | Platform | KILL: conflicts with private local authoritative-store model |
| 34 | Embedded lifecycle scripting | Platform | KILL: security and reproducibility cost is prohibitive |
| 35 | External identity provider and roles | Policy | KILL: requires service/auth boundary absent from the product |
| 36 | Binary interchange format | Interchange | KILL: JSONL simplicity outweighs speculative size savings |
| 37 | Object-storage checkpoints | Platform | KILL: network/cloud integration belongs outside the core |
| 38 | Live upstream database adapter | Interchange | KILL: violates the clean-room interoperability boundary |
| 39 | LLM-generated decomposition | Platform | KILL: provider/network coupling and nondeterminism are out of scope |
| 40 | `bead next` — preview next ready issue | CLI | KILL: small view derivable from list/query |
| 41 | Grouped count command | Query | KILL: safe query aggregation can subsume it |
| 42 | Generated shell completions | CLI | KILL: good packaging task, not differentiated feature |
| 43 | Generated manpages | CLI | KILL: good packaging task, not roadmap-level behavior |
| 44 | Atomic JSONL batch create | Bulk | KILL: bulk transaction manifest generalizes it |
| 45 | Environment defaults with explicit precedence | CLI | KILL: convenience introduces hidden context |
| 46 | Unambiguous short-ID resolution | CLI | KILL: opaque full IDs are safer for automation |
| 47 | Structured JSON errors | Ecosystem | KILL: stable exit taxonomy suffices initially; additive later |
| 48 | Machine-readable exit-code reference | Ecosystem | KILL: schema output can describe errors more generally |
| 49 | Query checkpoints without import | Interchange | KILL: valuable but secondary to native recovery and profiles |
| 50 | Atomic multi-update | Bulk | MERGED into finalist 50+55 |
| 51 | Local issue templates | Planning | KILL: content convenience without core coordination leverage |
| 52 | Effective config inspection | CLI | KILL: should accompany configuration work, not stand alone |
| 53 | Composable query language | Query | MERGED into finalist 15+26+53 |
| 54 | Dependency graph traversal and critical paths | Query | KILL: useful but selection explanation covers urgent need first |
| 55 | Bulk transaction files — declarative all-or-none mutations | Bulk | MERGED into finalist 50+55 |
| 56 | Issue cloning with provenance | Planning | KILL: templates/bulk manifests can reproduce the workflow |
| 57 | Parent-child task hierarchy | Planning | KILL: major model expansion before demonstrated need |
| 58 | Milestones and derived progress | Planning | KILL: reporting layer depends on hierarchy decisions |
| 59 | Markdown checklist import | Planning | KILL: parsing ambiguity conflicts with deterministic automation |
| 60 | Audit timeline query | Observability | KILL: good follow-on, but top ten favors correctness mechanisms |
| 61 | Claim filters by labels/type/priority/capability | Claim | KILL: capability sets retained; general filters risk starvation |
| 62 | Claim reservation sets | Claim | KILL: increases hoarding and lease complexity |
| 63 | Declarative dependency subgraph rewiring | Bulk | KILL: bulk transaction manifest can later include edges |
| 64 | Workspace merge preview | Recovery | KILL: conflict policy requires more profile evidence first |
| 65 | SQLite online backup | Recovery | MERGED into finalist 65+66 |
| 66 | Empty-target validated restore | Recovery | MERGED into finalist 65+66 |
| 67 | Mutation idempotency keys — safely deduplicate retries | Mutation | **FINALIST:** resolves ambiguous-timeout retries |
| 68 | Lease fencing tokens — reject stale post-reassignment workers | Claim | MERGED into finalist 4+68 |
| 69 | Crashpoint fault-injection harness | Verification | KILL: release engineering gate, not product feature |
| 70 | Query and transaction resource budgets | Safety | KILL: should be baseline hardening inside relevant features |
| 71 | Logical revisions resilient to clock anomalies | Mutation | MERGED into finalist 14+71 |
| 72 | Optional checkpoint signatures | Interchange | KILL: key management exceeds local provenance benefit |
| 73 | Complete import rejection report | Recovery | KILL: strong runner-up, but existing line errors satisfy v0.1 |
| 74 | Audit event hash chain | Observability | KILL: detects tampering but supplies no authentication |
| 75 | Lock-contention telemetry | Observability | KILL: useful after real workload evidence identifies need |
| 76 | Downgrade export from future schema | Recovery | KILL: unsafe to promise without future-schema knowledge |
| 77 | Interactive quickstart | Novice UX | KILL: interactive branch expands tests without coordination value |
| 78 | Terminal board view | Novice UX | KILL: display surface duplicates external tooling |
| 79 | Natural-language status aliases | CLI | KILL: canonical vocabulary is clearer for automation |
| 80 | Corrective-command error suggestions | Novice UX | KILL: risks unsafe advice; improve errors incrementally |
| 81 | Workflow-oriented help pages | Novice UX | KILL: documentation task rather than new capability |
| 82 | Dependency sentence rendering | Novice UX | KILL: useful output polish, not top-ten feature |
| 83 | Interactive dependency preview/confirmation | Novice UX | KILL: noninteractive determinism matters more |
| 84 | Workspace status summary | Novice UX | KILL: composes doctor, counts, and readiness |
| 85 | Example workflow checkpoints | Novice UX | KILL: fixtures/docs concern rather than runtime feature |
| 86 | Typo-aware command hints | CLI | KILL: clap may supply adequate hints at lower maintenance cost |
| 87 | Human-duration filters | Query | KILL: add only after the query grammar proves necessary |
| 88 | Explain effective blocked status | Explainability | MERGED into finalist 2+21 |
| 89 | Metrics snapshot | Observability | KILL: final comparison favored worker routing as more central |
| 90 | Worker heartbeat/presence | Observability | KILL: presence invites daemon/distributed assumptions |
| 91 | Pluggable priority scoring policies | Claim | KILL: threatens deterministic selection and fairness simplicity |
| 92 | Full-text issue search | Query | KILL: safe general querying precedes FTS indexes/ranking |
| 93 | Transactional notification outbox | Integration | KILL: no dispatcher contract or demonstrated consumer yet |
| 94 | Signed HTTP webhooks | Integration | KILL: network retry/security service is out of scope |
| 95 | Actor role policy | Policy | KILL: identity/authentication boundary is absent |
| 96 | SLA/due-date fields | Planning | KILL: broadens model before NEEDLE/core adoption evidence |
| 97 | Epics, estimates, progress rollups | Planning | KILL: high model/UI complexity and overlaps hierarchy |
| 98 | Public profile adapter SDK | Ecosystem | KILL: premature API stabilization before two native adapters ship |
| 99 | Machine-readable schemas by profile/version | Ecosystem | **FINALIST:** strengthens consumer contracts and binding generation |
| 100 | Versioned benchmark budgets | Verification | KILL: adopt as a release gate, not an end-user feature |

### Completeness-gap entrants

| Idea | Cluster | Verdict and reason |
| --- | --- | --- |
| Worker capability declarations — exact required/offered string sets participate in atomic selection | Claim | **FINALIST:** routes heterogeneous workers without client races |
| Worker class aliases — map names to capability sets in config | Claim | KILL: configuration sugar before capability semantics prove stable |
| Capability mismatch explanation | Explainability | MERGED: included in explain claim/readiness finalist |

### Finalist dossiers

#### 1. Explain claim and readiness decisions (2+21+88)

Emit a nonmutating, machine-readable decision trace using versioned semantic
reason codes: lifecycle, assignment, blocker IDs, manual blocking, resource
conflict, or missing capabilities. It won the explainability cluster because
it makes the core selector operable without exposing SQL or query structure.

- Complexity: **M**
- First step: specify reason-code stability, redaction, ordering, and a
  `bead explain-ready [ID] --json` envelope.
- Strongest objection: traces may couple consumers to selector internals;
  expose domain reasons only, not query plans.

#### 2. Fenced claim leases (4+68)

Make leases opt-in per claim, issue a monotonically increasing fencing token,
and require that token for renewal or completion so abandoned work returns
without allowing a stale worker to mutate reassigned work. It beat WIP limits
and batch reservations because ownership recovery is the sharper agent-fleet
failure mode.

- Complexity: **L**
- First step: specify lease states, clock rules, logical fencing token, expiry
  transition, renewal, and stale-token errors.
- Strongest objection: clocks and renewals add substantial state complexity;
  fencing and opt-in defaults must preserve current simple claims.

#### 3. Logical revision guards (14+71)

Expose an issue revision and accept `--if-revision` on every mutation, using a
logical counter rather than timestamps to reject lost updates and survive wall
clock anomalies. It outranked undo tokens because prevention is smaller and
more deterministic than universal inverse operations.

- Complexity: **M**
- First step: add revision semantics to the canonical model and define conflict
  output without changing interchange `updated_at` behavior.
- Strongest objection: revision plumbing touches every mutation and adapter.

#### 4. Safe query language and saved views (15+26+53)

Provide a deliberately small typed grammar for boolean issue filters,
deterministic sorting, projections, and named local views—never raw SQL. It won
the query cluster because it subsumes counts, next-item previews, and most
shell-side JSON filtering.

- Complexity: **L**
- First step: specify a v1 grammar limited to equality, set membership,
  conjunction, lifecycle/dependency predicates, sort, and field projection.
- Strongest objection: query languages easily become unbounded products; freeze
  a minimal v1 and reject unknown syntax.

#### 5. Atomic resource locks (18)

Let issues declare normalized local resource keys; a claim atomically acquires
them and excludes ready issues needing an already held key. It beat generic
negative dependencies because the intent and lifetime are explicit.

- Complexity: **M**
- First step: define resource key validation, acquisition/release lifecycle,
  and readiness reason codes in a single workspace.
- Strongest objection: users may mistake this for distributed locking; scope
  guarantees strictly to one native store.

#### 6. Atomic bulk transaction manifests (50+55)

Validate a versioned JSON manifest of creates, updates, labels, dependencies,
and closes, show a dry-run diff, then commit all operations or none. It won the
bulk/planning cluster by enabling safe automation without scripts or a broad
plugin system.

- Complexity: **L**
- First step: define manifest v1 with existing command primitives, local
  references for newly created IDs, validation order, and result mapping.
- Strongest objection: a transaction DSL can duplicate the CLI and complicate
  errors; v1 must remain a thin composition format.

#### 7. Native backup and empty-target restore (65+66)

Use SQLite's online backup API to produce a consistent native backup plus a
hashed manifest, and restore only into a new empty workspace after full
validation. It beat support bundles because it directly addresses recoverable
data loss without lossy interchange conversion.

- Complexity: **M**
- First step: specify backup format/version, integrity manifest, path safety,
  and the prohibition on in-place restore.
- Strongest objection: users may expect long-term portability; clearly separate
  version-bound recovery backups from JSONL interchange.

#### 8. Mutation idempotency keys (67)

Accept a caller key and request hash for creates, updates, claims, and closes;
an identical retry returns the committed result while a reused key with
different input conflicts. It beat richer JSON errors because it resolves the
underlying ambiguous-timeout failure.

- Complexity: **M**
- First step: specify key scope, canonical request hashing, retention bounds,
  transactional result capture, and mismatch behavior.
- Strongest objection: dedupe records can grow forever; define bounded
  retention without allowing unsafe early reuse.

#### 9. Machine-readable public schemas (99)

Emit versioned JSON Schema documents for issue records, capabilities,
migration receipts, bulk manifests, decision traces, and structured errors.
It won the ecosystem cluster because it helps every consumer without
stabilizing Rust internals as a public SDK.

- Complexity: **S**
- First step: hand-author and test schemas for existing `native-v1` and
  `needle-v1` public documents, then expose `bead schema`.
- Strongest objection: a schema can promise more stability than intended;
  publish only explicitly versioned contracts.

#### 10. Worker capability declarations (gap entrant)

Issues declare exact required capability strings and claimers declare offered
capabilities; matching occurs inside the atomic selection transaction and
mismatches appear in decision traces. It filled the final-set gap for
heterogeneous agent fleets and displaced metrics as more central to correct
coordination.

- Complexity: **M**
- First step: specify normalized string sets, subset matching, JSON fields,
  claim flags, indexes, and behavior when requirements are absent.
- Strongest objection: unconstrained capability taxonomies become metadata
  sprawl; use opaque exact-match strings and no inheritance in v1.

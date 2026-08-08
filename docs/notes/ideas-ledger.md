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

### Product decision — 2026-08-07

- **Adopted:** explain claim/readiness decisions, fenced claim leases, logical
  revision guards, safe query language/saved views, and machine-readable public
  schemas.
- **Schema refinement:** each native bead carries an immutable public
  `schema_ref`; schema discovery, capabilities, and migration receipts expose
  the identifiers for interoperability.
- **Deferred to this ledger:** atomic resource locks, atomic bulk transaction
  manifests, mutation idempotency keys, and worker capability declarations.
- **Rejected:** native SQLite backup and restore. JSONL is the portable backup
  and recovery contract; SQLite primarily supplies ACID live operation.

## 2026-08-07 — plan-idea-gen run 2: backup and schema depth

Target: `docs/plan/plan.md`. Generated: 100 new ideas. Deduplicated mechanisms:
90. Triage survivors: 25. Pairwise advancers: 15. Kill-pass survivors: 9.
Completeness entrants: 3, with 1 survivor. Finalists: 10. All candidates were
checked against the prior 100 ideas and adopted/deferred decisions.

| # | Idea and mechanism | Cluster | Verdict |
| ---: | --- | --- | --- |
| 1 | Backup completeness proof — rebuild and semantically compare every durable fact | Backup | **FINALIST**, merged into semantic recovery proof |
| 2 | Freshness budget — bound tolerated unflushed sequence/age | Backup policy | **FINALIST**, merged into backup freshness contract |
| 3 | Closure cooling period — reversible pre-close interval | Lifecycle | KILL: adds state for a policy-specific concern |
| 4 | Claim abstention evidence — record voluntary worker skips | Claim | KILL: capability/decision features cover the useful semantics |
| 5 | Import without trust — validation-only mode incapable of activation | Import | KILL: substantially overlaps existing dry-run |
| 6 | No-title identity — display an external reference as primary identity | Identity | KILL: weakens the required human summary invariant |
| 7 | Dependency expiry — stop blocking after an instant | Dependency | KILL: time-based correctness is hazardous and surprising |
| 8 | Assignee allowlist — restrict assignment locally | Policy | KILL: identity semantics are not established |
| 9 | Immutable issue mode — force supersession rather than reopen | History | KILL: irreversible policy without demonstrated need |
| 10 | Backup-first mutation — require verified backup before risky writes | Backup policy | MERGED into freshness contract as explicit enforcement mode |
| 11 | Reopen reason — retain rationale for undoing closure | Lifecycle | KILL: useful field polish, not top-ten mechanism |
| 12 | Negative labels — schema-enforced forbidden labels | Typed data | KILL: schema constraints can express it without a feature |
| 13 | Delete nothing — archive through supersession records | Archive | KILL: deletion is not yet a planned operation |
| 14 | Schema negotiation — select exact mutually supported representation | Schema | **FINALIST**, merged into negotiation catalog |
| 15 | Schema transformation graph — explicit one-way conversion paths | Schema | KILL: fixture and trust multiplication is premature |
| 16 | Content-addressed backup manifest — hash JSONL/config/schema/sequence | Backup | **FINALIST**, merged into atomic generations |
| 17 | Normalized external refs — namespace/key rows with uniqueness | Integration | **FINALIST**, merged into external references |
| 18 | Threaded comments — portable reply-to discussion graph | Collaboration | **FINALIST**, merged into comments feature |
| 19 | Ledger reconciliation — reconcile event totals to exported state | Backup | MERGED into semantic recovery proof |
| 20 | Compiler-style import diagnostics — sorted line/pointer/schema/semantic errors | Import | **FINALIST:** actionable complete repair report |
| 21 | Output content negotiation — ordered acceptable schema list | Schema | MERGED into schema negotiation catalog |
| 22 | Backup attestation — tool/schema/hash verification statement | Evidence | KILL: unsigned statement adds little beyond manifest |
| 23 | Savepoint validation — report independent batch errors then roll back | Automation | KILL: no adopted bulk operation yet |
| 24 | Disposable recovery drill — reconstruct, doctor, compare, destroy | Recovery | **FINALIST** through completeness round |
| 25 | Event-sourcing snapshots — compact events with sequence continuity | Events | KILL: unnecessary architecture expansion |
| 26 | Friendly schema aliases — resolve names to immutable URNs | Schema | KILL: exact URNs are clearer for interoperability |
| 27 | Encrypted JSONL backups | Platform | KILL: key management belongs outside core |
| 28 | Compressed JSONL backups | Platform | KILL: external compression composes adequately |
| 29 | Remote schema registry | Platform | KILL: network and trust boundary out of scope |
| 30 | Cross-workspace reference registry | Platform | KILL: federation semantics out of scope |
| 31 | OS keychain integration | Platform | KILL: no adopted secret-bearing feature |
| 32 | Filesystem-watcher auto-import | Platform | KILL: silent mutation and daemon behavior |
| 33 | NFS multiwriter locking | Platform | KILL: unsupported distributed SQLite usage |
| 34 | WASM backup reader | Platform | KILL: packaging target before demand evidence |
| 35 | Hosted schema catalog | Platform | KILL: website/service is outside the CLI |
| 36 | Binary attachments | Platform | KILL: breaks compact portable JSONL boundary |
| 37 | Schema-defined custom lifecycles | Typed data | KILL: data-driven code semantics become a plugin system |
| 38 | Executable subcommand extensions | Platform | KILL: provenance and security expansion |
| 39 | Automatic tracker bridge | Platform | KILL: network sync and conflict handling out of scope |
| 40 | `sync --status` — backup/live sequence, age, hash, verification | Backup policy | MERGED into backup freshness contract |
| 41 | Backup manifest sidecar | Backup | MERGED into atomic generations |
| 42 | Standalone schema validation | Schema | KILL: thin mode already implied by import/schema support |
| 43 | Schema identification command | Schema | KILL: already substantially planned through `schema_ref` |
| 44 | Comment add/list commands | Collaboration | MERGED into portable threaded comments |
| 45 | External-reference commands | Integration | MERGED into namespaced external references |
| 46 | Archive filter | Archive | KILL: archive state not justified yet |
| 47 | Duplicate detector | Integration | MERGED exact-ref collision detection; title heuristics killed |
| 48 | JSON Pointer extension update | Typed data | KILL: mutation surface is too low-level |
| 49 | Profile field-presence report | Schema | KILL: import diagnostics can report it |
| 50 | Bundled schema examples | Schema | KILL: fixture/documentation requirement, not feature |
| 51 | Comment-only export | Collaboration | KILL: full backup should remain complete and canonical |
| 52 | Backup dry-run sizing | Backup policy | KILL: low value beside actual atomic flush |
| 53 | Mutation receipt — revision/sequence/backup staleness | Events | MERGED partly into cursor change feed; standalone receipt killed |
| 54 | Schema-bound issue types | Typed data | **FINALIST**, merged with annotations |
| 55 | Typed annotations — namespaced JSON validated by schema URNs | Typed data | **FINALIST**, merged with issue-type binding |
| 56 | Supersession links | History | KILL: introduces lineage model before demand |
| 57 | Atomic issue split | History | KILL: depends on rejected supersession/bulk semantics |
| 58 | Atomic issue merge | History | KILL: same lineage and conflict burden |
| 59 | Comment thread resolution | Collaboration | MERGED into portable threaded comments |
| 60 | Actor/session history query | Events | KILL: first-run audit timeline objection still stands |
| 61 | Dependency-closed subset export | Interchange | KILL: partial backups invite hidden omissions |
| 62 | Deterministic ID namespace remap | Import | KILL: rewriting stable IDs is high risk |
| 63 | Cross-schema semantic diff | Schema | KILL: transformation graph premature |
| 64 | Profile round-trip certificate | Evidence | KILL: conformance evidence belongs in tests/receipts |
| 65 | Backup rotation command | Backup policy | KILL: filesystem retention composes externally |
| 66 | Per-field import provenance | History | KILL: large storage/write amplification |
| 67 | Atomic JSONL/manifest pair replacement | Backup | MERGED into atomic generations |
| 68 | Interrupted-backup scavenger | Recovery | KILL: doctor baseline can absorb it |
| 69 | Previous verified generation pointer | Backup | MERGED into atomic generations |
| 70 | Missing/corrupt SQLite recovery preflight | Recovery | MERGED into disposable recovery rehearsal |
| 71 | Restore provenance marker | Recovery | MERGED into disposable recovery rehearsal |
| 72 | Restore equivalence gate | Backup | MERGED into semantic recovery proof |
| 73 | Unknown-extension canonicalization test | Backup | KILL: required conformance case, not feature |
| 74 | Schema-bundle self-check | Schema | KILL: release gate, not user-facing feature |
| 75 | SQLite/JSONL divergence alarm | Backup policy | MERGED into freshness contract |
| 76 | Backup corruption localization | Recovery | MERGED into complete import diagnostics |
| 77 | Disk-full fault lane | Verification | KILL: test requirement, not product feature |
| 78 | Unicode comparison policy | Identity | KILL: specification decision, not feature |
| 79 | Recovery guide command | Recovery | KILL: documentation output behind actual rehearsal |
| 80 | Plain-language backup confidence | Backup policy | MERGED into freshness contract |
| 81 | Plain-language schema explanation | Schema | KILL: documentation can ship with schemas |
| 82 | Issue provenance view | History | KILL: source fields already expose core fact |
| 83 | Comment handoff view | Collaboration | KILL: query/view layer can compose comments later |
| 84 | Corrective schema guidance | Schema | KILL: compiler diagnostics carry stable evidence first |
| 85 | Interactive restore wizard | Recovery | KILL: noninteractive rehearsal is safer and testable |
| 86 | Archive explanation | Archive | KILL: depends on rejected archive state |
| 87 | Human external-reference display | Integration | KILL: output polish within external refs, not separate feature |
| 88 | Schema support matrix | Schema | MERGED into schema negotiation catalog |
| 89 | JSONL history directory | Backup policy | KILL: atomic two-generation recovery is enough initially |
| 90 | External encryption command envelope | Platform | KILL: shell composition already supports it |
| 91 | Machine-readable schema compatibility catalog | Schema | MERGED into schema negotiation catalog |
| 92 | Custom-field indexes | Typed data | KILL: premature optimization and schema coupling |
| 93 | Comment mentions | Collaboration | KILL: notifications/identity semantics absent |
| 94 | Cursor change feed with gap detection | Events | **FINALIST:** deterministic incremental local-consumer protocol |
| 95 | Snapshot pagination token | Query | KILL: safe query scope should stabilize first |
| 96 | Approved import source registry | Import | KILL: workspace policy before source demand evidence |
| 97 | Telemetry retention policy | Archive | KILL: premature before storage growth evidence |
| 98 | Schema deprecation lifecycle | Schema | KILL: strongest runner-up; only one schema currently exists |
| 99 | Conformance badge document | Evidence | KILL: unsigned self-attestation has weak assurance |
| 100 | Recovery benchmark | Verification | KILL: performance gate rather than product feature |

### Completeness entrants

| Idea | Verdict |
| --- | --- |
| Disposable recovery rehearsal with semantic re-export comparison | **FINALIST:** exercises the actual JSONL disaster-recovery path safely |
| Recovery runbook generator | KILL: documentation rather than a mechanism |
| Scheduled recovery reminder | KILL: requires an external scheduler/notification system |

### Finalist dossiers

#### 1. Semantic backup completeness proof

Reconstruct a disposable store from one JSONL generation, re-export it, and
compare every durable user-visible fact—including unknown extensions,
dependencies, comments, schema references, and revisions—against the captured
source snapshot. It won backup integrity because calling JSONL a backup is only
defensible if recovery is proven lossless.

- Complexity: **L**
- First step: define the complete recoverable-state inventory and semantic
  equality rules independently of SQLite row layout.
- Strongest objection: full reconstruction is expensive; keep it explicit and
  stream/hash where semantic equivalence permits.

#### 2. Atomic versioned backup generations

Write JSONL and a content-addressed manifest into a new generation, verify both,
then atomically switch a tiny current-generation pointer while retaining the
previous verified generation. It beat a plain sidecar because two independently
replaced files can be silently mismatched after a crash.

- Complexity: **M**
- First step: specify generation directory naming, manifest fields, pointer
  replacement, cleanup, and recovery after every interrupted boundary.
- Strongest objection: it changes the simple `.beads/issues.jsonl` layout;
  preserve that path as the current-generation compatibility view.

#### 3. Backup freshness contract

Expose live event sequence, backed-up sequence, age, hash, and verification
state, with an optional workspace freshness budget and explicit precondition for
high-risk mutations. It won backup operations because users must know exactly
what data a last-flush backup does not contain.

- Complexity: **M**
- First step: define freshness states and `sync --status --json`, keeping policy
  informational unless explicitly enabled.
- Strongest objection: enforced freshness can obstruct normal work; default to
  visibility and make enforcement scoped.

#### 4. Schema negotiation catalog

Let producers and consumers exchange exact readable/writable schema URN sets and
select their intersection, including explicit read-only or lossy status. It won
schema interoperability because `schema_ref` identifies a format but does not
by itself establish mutual support.

- Complexity: **M**
- First step: define deterministic negotiation input/output and add the catalog
  to capabilities without network discovery.
- Strongest objection: negotiation can become protocol bureaucracy; use exact
  identifiers and no compatibility inference.

#### 5. Portable threaded comments

Complete the existing comment model with add/list/reply/resolve commands,
stable IDs, authorship, and deterministic JSONL round trips. It won
collaboration because comments preserve agent handoffs and review context that
otherwise disappears from a task's formal fields.

- Complexity: **M**
- First step: specify comment schema, immutable bodies, reply-to constraints,
  resolution state, ordering, and backup behavior.
- Strongest objection: comment history increases backup size; pagination and
  complete export semantics must remain deterministic.

#### 6. Namespaced external references

Attach normalized `(namespace, key, value)` references such as tracker IDs or
commit identifiers, optionally unique within a namespace, without replacing
native bead IDs. It won integration identity by enabling deduplication and
cross-tool recognition without unstable title matching.

- Complexity: **S**
- First step: define namespace/key validation, uniqueness modes, CLI operations,
  and profile mapping.
- Strongest objection: arbitrary integrations create metadata sprawl; keep the
  model generic and prohibit network resolution.

#### 7. Schema-bound typed annotations

Allow an issue type to permit namespaced JSON annotations whose values identify
and validate against immutable public schema URNs. It won typed extensibility
because tools can exchange structured domain data without plugins, raw SQL, or
custom executable lifecycle logic.

- Complexity: **L**
- First step: specify annotation envelope, schema lookup, validation timing,
  unknown-schema preservation, and profile loss reporting.
- Strongest objection: this can become a plugin system in disguise; schemas may
  validate data only and cannot execute code or define transitions.

#### 8. Cursor-based local change feed

Emit deterministic public mutation records after an event cursor, with snapshot
identity and explicit gap detection. Unlike the previously rejected audit
timeline, this is a versioned consumer protocol for incremental local indexing
and adapters rather than merely a human history view.

- Complexity: **L**
- First step: define event schema, cursor lifetime, compaction/gap behavior, and
  how a consumer resynchronizes from JSONL.
- Strongest objection: it creates another compatibility surface; limit it to
  committed local events and require full-backup resync after gaps.

#### 9. Complete import diagnostic report

Validation collects a bounded, deterministically sorted set of errors carrying
line, JSON Pointer, schema keyword, and semantic reason, while guaranteeing no
activation. It won import usability because one-error-per-run repair loops are
painful for large interoperable backups.

- Complexity: **M**
- First step: define diagnostic schema, stable codes, ordering, maximum count,
  and truncation marker.
- Strongest objection: aggregating errors can consume memory or cascade; cap
  results and distinguish root errors from suppressed dependents.

#### 10. Disposable recovery rehearsal

Build a temporary workspace from the current JSONL backup, run integrity and
schema checks, re-export for semantic comparison, record a nonsecret report,
and delete only the temporary workspace. It filled the operational gap because
a backup is trustworthy only when its real recovery path is exercised.

- Complexity: **M**
- First step: specify temporary-path safety, recovery phases, cleanup guarantees,
  and report fields.
- Strongest objection: it overlaps semantic backup proof; the proof is the
  primitive, while rehearsal orchestrates the end-to-end operator workflow.

### Product decision — 2026-08-07, run 2

- **Adopted:** all ten run-2 finalists: semantic backup proof, atomic backup
  generations, freshness contract, schema negotiation, portable threaded
  comments, external references, schema-bound typed annotations, local change
  feed, complete import diagnostics, and disposable recovery rehearsal.
- **Comment projection:** comments are durable bead content and always present
  in the JSONL recovery backup. Ordinary retrieval omits comment bodies by
  default and can request unresolved or complete conversation context.
- **Additional adopted requirements:** scoped doctor/diagnostic modes,
  declarative conditional dependencies, and general namespaced structured JSON
  data governed by public schema references.

## Deferred file-writing coordination — 2026-08-08

Agents commonly work on the same branch, so coordinating beads that may touch
the same files could reduce edit collisions. The explored design would have:

- recorded exact or pattern-based read/write file intents on a bead;
- used a read-only discovery phase before accepting the intended write set;
- derived ordinary blocking dependencies between beads with overlapping write
  intents, preferably as a deterministic chain rather than an all-pairs graph;
- versioned the accepted intent with a base revision, intent hash, and fencing
  token;
- required intent expansion before writing an undeclared path; and
- compared the resulting diff with the accepted intent before completion.

This design is **deferred, not adopted**. Requiring workers to predict files and
pass a planning or manifest gate is too restrictive for the current product.
Natural-language work also cannot reliably determine every eventual write in
advance. No claim, update, or close operation should enforce these mechanisms.
A different file-writing coordination model will be designed later.

Potentially reusable ideas remain notes only: normalized repository-relative
paths, read-versus-write modes, compact dependency chains for known overlaps,
derived-edge provenance, and diagnostics that explain suspected collisions.
They require a new product decision and specification before implementation.

## 2026-08-08 — plan-idea-gen run 3: evidence without gates

Target: `docs/plan/plan.md`. Generated: 40 new base ideas plus 3 crossover or
completeness entrants. Deduplicated mechanisms: 43. Triage survivors: 25 plus
2 crossovers. Pairwise advancers: 14. Kill-pass survivors: 12. Finalists: 10.
This quick-scale run checked candidates against both prior 100-idea runs,
R001-R019, and the decision to defer file-writing gates.

| # | Idea and mechanism | Cluster | Verdict |
| ---: | --- | --- | --- |
| 1 | Claim rehearsal — simulate the next N selections without assignment | Claim insight | KILL: future queue simulation misleads under concurrency and failures |
| 2 | Negative readiness query — report minimal changes that would make one bead ready | Claim insight | MERGED into finalist 31 |
| 3 | Closure proof bundle — machine-readable evidence separate from prose | Evidence | MERGED into finalist 41 |
| 4 | Dependency rationale — optional reason and provenance on every edge | Graph | **FINALIST:** makes graph structure explain itself |
| 5 | Failure salvage brief — bounded handoff view after a failed attempt | Outcomes | MERGED into finalist 42 |
| 6 | Acceptance checklist — ordered, individually checkable completion items | Evidence | MERGED into finalist 41 |
| 7 | Review lifecycle overlay — optional verification without another terminal status | Review | KILL: should wait for acceptance evidence semantics |
| 8 | Label vocabulary schema — allowed labels, aliases, descriptions, deprecations | Policy | KILL: schema-bound data can carry it until demand proves native semantics |
| 9 | Actor provenance envelope — bounded tool/harness/session mutation metadata | Outcomes | MERGED into finalist 42 |
| 10 | Graph slice export — deterministic dependency neighborhood around one bead | Graph | **FINALIST:** portable bounded agent/visualization context |
| 11 | Workspace bundle migration — merge several workspaces with namespace receipts | Migration | KILL: cross-workspace identity and dependency collision policy is premature |
| 12 | Portable read-only query snapshot with schemas and hashes | Interchange | KILL: partial export is easily confused with recovery backup |
| 13 | Cross-profile comparison — semantic losses for two renderings side by side | Migration | **FINALIST:** makes interoperability loss concrete before export |
| 14 | Detached planning annotations — disposable nonauthoritative planning guesses | Metadata | KILL: metadata without durable semantics invites drift |
| 15 | Alternate display aliases for immutable IDs | Identity | KILL: creates ambiguous human identity |
| 16 | Workspace policy lint — diagnose contradictory or ineffective configuration | Policy | **FINALIST:** prevents opaque scheduling mistakes without enforcement |
| 17 | Command examples as versioned capability data | CLI | KILL: documentation surface, not a top product mechanism |
| 18 | Deterministic shell completion generated from command metadata | CLI | KILL: tooling polish before the command surface stabilizes |
| 19 | Compact graph summary — counts, depth, and next actionable relationship | Claim insight | KILL: graph slice and why facade subsume it |
| 20 | Mutation preview — validate and show semantic delta without commit | Mutation | **FINALIST:** safe inspection of consequential operations |
| 21 | Atomic compare-and-swap batch of revision-guarded mutations | Mutation | KILL: duplicates deferred bulk manifests with greater conflict complexity |
| 22 | Saved claim presets — named policy parameters and projections | Policy | KILL: ordinary versioned workspace configuration suffices |
| 23 | Dependency reason search | Graph | KILL: safe queries can compose over edge rationale |
| 24 | Deterministic clone from selected fields | CLI | KILL: copying is modest convenience and can duplicate stale assumptions |
| 25 | Manual queue pinning inside a priority band | Policy | KILL: another time-sensitive scheduling override |
| 26 | Sensitive-content lint — warn on likely credentials before JSONL export | Reliability | **FINALIST:** reduces accidental secret persistence without blocking work |
| 27 | Public semantic-state fingerprint | Reliability | KILL: backup manifests and semantic proof already hash stronger boundaries |
| 28 | Partial-read backup damage map without activation | Recovery | KILL: could be mistaken for safe partial recovery |
| 29 | Clock anomaly diagnostics | Reliability | KILL: scoped doctor can absorb it as a conformance case |
| 30 | Resource budget report — size, largest beads, growth, projection pressure | Reliability | KILL: useful diagnostic, but less central than the final ten |
| 31 | `why` command — status, readiness, rank, and legal next operations | Claim insight | **FINALIST:** one stable human and machine explanation facade |
| 32 | Machine-readable legal-next-action error hints | CLI | KILL: error-schema polish after the lifecycle API stabilizes |
| 33 | Workspace tour — health, freshness, ready work, and active profile | Claim insight | KILL: composes existing diagnostic and capability commands |
| 34 | Priority explanation in capabilities | CLI | KILL: should be part of the existing capability contract |
| 35 | Plain-sentence dependency direction | CLI | KILL: presentation feature only |
| 36 | Explicit recurring-bead materialization — no scheduler or daemon | Workflow | **FINALIST:** repeatable work without hidden automation |
| 37 | Outcome artifact catalog — typed references to produced commits/reports/files | Outcomes | MERGED into finalist 42 |
| 38 | Local metrics snapshot for throughput, age, failures, and queue health | Reliability | KILL: premature analytics surface before meaningful history exists |
| 39 | Deterministic sample of completed work for review | Review | KILL: creates a second scheduler before review semantics exist |
| 40 | Explicit revision-guarded undo recipe | Mutation | KILL: reliable semantic inverses are narrower than mutation preview |
| 41 | Verifiable acceptance evidence — checklist items linked to evidence | Evidence | **FINALIST:** combines task definition and optional completion proof |
| 42 | Portable execution outcome — bounded attempt, actor, handoff, and artifact envelope | Outcomes | **FINALIST:** interoperable result context without full conversations |
| 43 | Revisit conditions for deferred beads | Policy | KILL: dependencies and saved queries can express the useful portion |

### Finalist dossiers

#### 1. Dependency rationale

Attach an optional concise reason and provenance envelope to each dependency
edge. It won the graph-semantics cluster because knowing *why* A blocks B is
more durable and actionable than merely drawing the edge.

- Complexity: **S**
- First step: specify rationale size, provenance fields, mutation semantics,
  deterministic backup ordering, and loss behavior in external profiles.
- Strongest objection: metadata increases graph and backup weight; keep it
  optional, bounded, and excluded from readiness evaluation.

#### 2. Graph slice export

Export a deterministic, bounded dependency neighborhood around a selected bead,
with explicit depth, direction, and field projection. It beat compact summaries
because agents and visualization tools can consume the same portable structure.

- Complexity: **M**
- First step: define root, inbound/outbound traversal, cycle-safe ordering,
  truncation markers, schema, and maximum node/edge budgets.
- Strongest objection: a partial graph can look like a backup; label the format
  non-recoverable and never accept it through backup import.

#### 3. Cross-profile comparison

Render one bead or fixture through two explicit profiles and report field-level
semantic preservation, transformation, and loss before migration. It won the
interchange cluster because receipts explain what happened after conversion,
while this lets an operator decide beforehand.

- Complexity: **M**
- First step: define a semantic comparison result keyed by canonical field path
  and reuse profile adapters without comparing incidental JSON formatting.
- Strongest objection: adapter combinations multiply quickly; support only
  explicitly named installed profiles and bound record counts.

#### 4. Workspace policy lint

Statically diagnose contradictory, unreachable, redundant, or ineffective
claim and retention configuration without changing it. It won policy safety
because complex versioned scheduling becomes operable only when mistakes are
explainable before workers encounter an empty queue.

- Complexity: **M**
- First step: enumerate policy invariants and define stable, versioned warning
  codes plus a read-only `policy check --format json` result.
- Strongest objection: lint rules can lag policy versions; bind every rule to
  exact policy/config schema versions and fail closed on unknown versions.

#### 5. Mutation preview

Validate a proposed update, close, reopen, or dependency change and emit its
exact semantic delta without committing. It won mutation safety because it
helps humans and automation inspect consequences without imposing a new gate.

- Complexity: **M**
- First step: specify canonical before/after field deltas, derived readiness
  effects, exit behavior, and the captured revision/sequence.
- Strongest objection: state may change after preview; mark it advisory and let
  callers pair execution with R003 revision guards.

#### 6. Sensitive-content lint

Scan content headed for the portable JSONL backup for high-confidence credential
shapes and report field locations without retaining or printing matched values.
It won reliability because accidental secret persistence is costly and backups
are explicitly durable and Git-friendly.

- Complexity: **M**
- First step: define a small built-in versioned rule set, redacted diagnostic
  schema, size bounds, and explicit warning-only invocation.
- Strongest objection: detection has false positives and false negatives; never
  claim completeness, never block flush by default, and do not load executable
  or remote rule packs.

#### 7. `why` command

Provide one read-only entry point that explains effective status, readiness,
claim ranking factors, active blockers, and currently legal next operations.
It won novice usability because users should not have to assemble several
diagnostic commands to understand one bead.

- Complexity: **M**
- First step: define a stable facade schema over lifecycle rules and R001/R019
  reason codes, ensuring it calls the same domain evaluators.
- Strongest objection: it overlaps claim explanations; it must remain a facade,
  never a parallel policy engine.

#### 8. Explicit recurring-bead materialization

Store a nonexecuting recurrence template and create its next bead only through
an explicit command, carrying a deterministic series reference. It won workflow
automation because repeated maintenance is common while daemon scheduling and
hidden mutation remain out of scope.

- Complexity: **L**
- First step: specify immutable template versions, occurrence identity,
  selected copied fields, idempotency, and explicit materialization receipts.
- Strongest objection: recurrence can become a scheduler by stealth; core must
  never wake, poll, or create occurrences without a direct invocation.

#### 9. Verifiable acceptance evidence

Add ordered acceptance items whose status may remain unchecked, satisfied,
waived, or linked to typed evidence, while closure policy stays configurable.
It won evidence/review because it connects the requested outcome to the proof
without forcing every project into a heavyweight workflow.

- Complexity: **L**
- First step: define item IDs, ordering, immutable statement/revision rules,
  evidence references, waiver rationale, backup mapping, and optional policy.
- Strongest objection: checklists create bureaucracy; default to absent and do
  not require satisfaction for close unless a workspace explicitly opts in.

#### 10. Portable execution outcome

Record a bounded, schema-versioned result envelope for an attempt: outcome
class, actor/tool provenance, concise handoff, and typed artifact references.
It won the outcome cluster because both successful and failed work need portable
context, but copying prompts or entire conversations would explode bead size.

- Complexity: **L**
- First step: define allowlisted fields, byte limits, privacy rules, attempt
  linkage, projection controls, and profile loss reporting.
- Strongest objection: provenance can expose identities or prompt material;
  prohibit arbitrary environment capture and keep verbose context in optional
  comments or external references.

### Run-3 disposition

No finalist is adopted automatically. Selection remains a product decision;
until then all ten are candidates in this ledger and no F001-F014 or R001-R019
state changes.

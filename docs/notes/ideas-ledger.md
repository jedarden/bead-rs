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

Product decision on 2026-08-08:

- **Rejected:** dependency rationale and graph slice export.
- **Adopted:** cross-profile semantic comparison, workspace policy lint,
  general mutation dry-run, the unified `why` facade, and explicit recurring
  bead materialization. These are R020-R024 in the plan.
- **Deferred to notes:** sensitive-content lint and portable execution outcomes.
- **Pending:** verifiable acceptance evidence, awaiting a product decision after
  further explanation. It remains neither adopted nor rejected.
- General mutation dry-run is an extension rather than a duplicate: the plan
  already required dry-run for migration/import analysis, but not for ordinary
  update, lifecycle, or dependency mutations.
- No F001-F014 pass state changed.

## 2026-08-12 — field observation: inverted verification gates

Not an ideation run. This entry records the provenance of R025, which came from
measured defects in two live workspaces built on a different bead
implementation rather than from a generated idea pool.

Observation: a dependency graph can be acyclic, internally consistent, and
still order work so it cannot execute — a bead that *verifies* some work
recorded as the blocker of the bead that *performs* it. "Add tilde expansion
helper function" blocked by "Run clippy and fix warnings".

Measured 2026-08-12:

- Workspace A: 21 such edges, appearing at a steady 4-6% of newly created beads
  across three months. A persistent authoring slip, not a migration artifact.
- Workspace B: 24 such edges plus 7 cycles, found on a detector's first run.
  Several were two-node rings that were both a cycle and an inversion.

Why this is not already covered here: `bead-rs` rejects cycles at insertion
(§3.4), derives readiness from authoritative rows rather than a stored status,
and embeds the blocker test in the eligibility query — so it is immune to the
three neighbouring defect classes. Inversion is not one of them. An inverted
gate is normally acyclic, so insertion-time cycle rejection accepts it, and
readiness then correctly reports a bead that can never become ready.

- **Adopted:** R025, declared `verifies` edge kind plus advisory inverted-gate
  diagnosis in the dependencies doctor scope. Authorized by ADR-001.
- **Rejected:** the title-prefix heuristic that detected these in the
  originating workspaces. It works, and it is the wrong mechanism here — the
  store commits to cross-tool recognition without title heuristics, and prefix
  matching misclassifies titles that merely contain a verification noun ("Add
  logging verification and run test suite"). Cost of the structural
  alternative, recorded honestly: coverage is opt-in and starts at zero.
- **Rejected:** rejecting inverted edges at insertion, as cycles are rejected.
  A deliberate "prove the baseline green before touching it" gate is legitimate
  and structurally identical to the error.
- No F001-F017 pass state changed. R025 is post-0.1 and sequenced after R017.

## 2026-08-15 — plan-idea-gen run 4: post-R026 operations

Target: `docs/plan/plan.md` (revision 5). Pool: 60 — deliberately scaled below
the 100 default because runs 1-3 already banked ~243 mechanisms; every
candidate here was generated against that corpus and duplicates were dropped
at generation time rather than manufactured for the count. Clusters: 9.
Triage survivors: 25. Crossover merges: 1 (checkpoint archaeology = query +
diff + bisect over committed generations). Pairwise advancers: 15. Kill-pass
survivors: 12. Completeness entrants: 1, which survived and displaced the
weakest advancer. Finalists: 10.

Context that shaped this run: bead-rs is now the canonical bead CLI across the
surrounding environment (two machines, ~19 workspaces, thousands of beads);
R026 automatic flush was adopted the same day with its prerequisite beads
filed; the migration period produced a real wrong-CLI-against-wrong-store data
-loss incident (SEAM, 2026-08-14); and the multi-clone flush-commit-push /
pull workflow is now the de facto replication path between machines. Several
run-1/run-2 kills and deferrals were legitimately resurrected because those
facts invalidate their original objections; each resurrection states why.

Hard constraints (kill criteria): clean-room (never parse another bead
implementation's formats); no daemon/sockets/network sync; bead-rs never runs
Git; JSONL is the sole backup boundary; deterministic and noninteractive with
no hidden automation; no title/content heuristics; Rust 1.75, no unsafe; new
formats require normative specs and independent review; additive to NEEDLE v1;
no duplication of F001-F017 or R001-R026.

| # | Idea and mechanism | Cluster | Verdict |
| ---: | --- | --- | --- |
| 1 | Remote-advanced reconcile — checkpoint-ahead-of-db after `git pull` becomes a recognized state with `sync reconcile` merging the verified pointer-selected checkpoint into the live store | Git transport | **FINALIST** |
| 2 | Checkpoint query without import — read-only list/query against a named checkpoint artifact | Git transport | MERGED into finalist archaeology (resurrects run-1 #49: R026 makes per-commit generations ubiquitous) |
| 3 | `init --from-checkpoint` — one-command clone recovery | Git transport | KILL: wrapper over two existing commands |
| 4 | Unblock plan — readiness-ordered open transitive blockers for a target bead | UX | KILL: composes from `why` (R023) and dependency queries (R004) |
| 5 | Self-defending workspace discovery — stop at the first `.beads`, fail closed without a bead-rs fingerprint instead of walking past a foreign store to a parent workspace | Safety | **FINALIST** |
| 6 | Create-time context-fit lint — warn when a description exceeds the configured legacy-worker ceiling | UX | KILL: R019's context-fit work redefines the ceiling; a pre-R019 lint hardcodes what R019 replaces |
| 7 | Chain-affinity claim — reserve a bead's sole dependent for the same worker | Claim | KILL: reservation/hoarding objection from run 1 stands |
| 8 | Idempotent create by unique ref — `create --unique-ref ns:key` returns the existing bound bead instead of duplicating | Creation integrity | **FINALIST** |
| 9 | As-of reconstruction — `show --as-of SEQ` via replay | History | KILL: archaeology over committed generations answers the same forensic questions without a new replay surface |
| 10 | Stuck-work report — open beads that cannot become ready, with reasons | Operator | KILL: expressible via query over deferred/blocked chains; R025 already covers the inverted-gate class |
| 11 | Pretty review export — annotated human checkpoint rendering | History | KILL: presentation only |
| 12 | Long-poll claim — `claim --wait` bounded internal retry with jitter | Claim | KILL: convenience beside locks; callers script this today without correctness loss |
| 13 | Generation bisect — predicate-driven search over historical checkpoints | Git transport | MERGED into finalist archaeology |
| 14 | Three-way checkpoint merge driver for git-conflicted checkpoint files | Git transport | KILL: with reconcile, a conflicted pull resolves by taking either checkpoint side then reconciling and reflushing; three-way file merge becomes unnecessary |
| 15 | Retention/compaction with receipts — prune ancient closed history into sealed archive generations | Retention | KILL (strongest runner-up): plan-contemplated and inevitable, but L-complexity, spec-gated, and R026 P1/P2 must land first |
| 16 | Preemption advisory flag on lower-priority in_progress beads | Claim | KILL: advisory with no consumer; drifts toward wall-clock coordination |
| 17 | Built-in transient retry — global bounded backoff on exit 6 | Process | KILL: callers already own retry policy; embedding one contests it |
| 18 | Generation semantic diff — `sync diff A B` issue/event delta between checkpoints | Git transport | MERGED into finalist archaeology (resurrects run-1 #19 under the R026 argument) |
| 19 | Declared `duplicates` edge kind with close-one-keep-one workflow | Creation integrity | KILL: spec-heavy cleanup; finalist 8 prevents the duplicate at the source |
| 20 | Reason-code explain — `bead explain CODE` | CLI | KILL: documentation surface; schema explain and man pages carry it |
| 21 | Prometheus textfile metrics rendering | Observability | KILL: metrics-snapshot objection stands; an exporter composes from existing `--json` |
| 22 | Soft-ordering `after` edge kind | Claim | KILL: policy complexity for unproven need |
| 23 | Shipped git-hook templates | Packaging | KILL: packaging/docs, not mechanism |
| 24 | Caller-owned stdio session — one JSON request/response per line, no socket, parent-owned lifecycle | Process | **FINALIST** (resurrects run-1 daemon kill: the socket-lifecycle and security objections do not apply to stdio) |
| 25 | Stable embedded library API as a semver crate | Process | KILL: API stabilization cost exceeds subprocess pain; stdio session captures most of the win |
| 26 | Atomic bulk transaction manifests — validate, dry-run diff, commit all-or-none | Bulk | **FINALIST** (deferred in run 1; resurrected: R026 turns N-command materialization into N published generations, and interrupted materialization still discards whole workspaces) |
| 27 | Read-only fleet aggregation across an explicit workspace list | Operator | KILL: composes from per-workspace status JSON in a shell loop |
| 28 | Worker capability declarations matched in the claim transaction | Claim | KILL: deferral stands; heterogeneity remains telemetry-grade evidence, locks won the cluster |
| 29 | Atomic resource locks — declared local resource keys acquired atomically at claim | Claim | **FINALIST** (deferred in run 1; resurrected: NEEDLE ADR-015 adopts bead-level serialization for shared checkouts and the duplicate/conflicting-work incident class recurred — locks mechanize the accepted model) |
| 30 | Structured stderr diagnostics (`--log-format json`) | Process | KILL: additive-later objection from run 1 stands |
| 31 | Per-invocation claim policy override | Claim | KILL: reproducibility risk for a niche need |
| 32 | Doctor recovery guidance — exact next commands per failure state | Operator | KILL: mechanical gate beats prose; superseded by 33 in pairwise, which itself fell to the gap entrant |
| 33 | Freshness gate exit mode — `sync --status --check` nonzero when dirty | Operator | KILL: flag-sized and composable; displaced by the completeness entrant |
| 34 | `create --stdin-json` — one issue document as input | Creation integrity | KILL: subsumed by bulk manifests |
| 35 | `init --demo` seeded lifecycle workspace | UX | KILL: fixtures/docs concern (run-2 objection stands) |
| 36 | `--id-only` output mode | CLI | KILL: flag-sized convenience |
| 37 | Global `--workspace PATH` | CLI | KILL: flag-sized convenience |
| 38 | Graph rendering — `query --format dot\|mermaid` | Power user | KILL: composes from `--json` plus external tooling |
| 39 | `dep chain A B C` — serialization chain in one call | Power user | KILL: sugar; locks address the underlying serialization need at the correct layer |
| 40 | Query-scoped label mutation | Bulk | KILL: subsumed by bulk manifests |
| 41 | Claims-paused maintenance mode | Claim | KILL: rare event at configuration-flag scale |
| 42 | Workspace metadata (name/purpose) in status | Operator | KILL: config nicety |
| 43 | Contention counters in `sync --status` | Reliability | KILL: run-1 objection stands until R026 concurrency evidence exists |
| 44 | Flush disk-space preflight | Reliability | KILL: atomic-rename semantics already bound the damage; rare failure |
| 45 | Binary version-skew warning | Reliability | KILL: narrow window; migration gate covers the dangerous direction |
| 46 | Global `--read-only` flag | Reliability | KILL: run-1 "too small" objection stands |
| 47 | Stale in-progress detection — doctor scope for non-leased in_progress beads with no recent events | Operator | **FINALIST** |
| 48 | Gitignore shadow warning | Safety | KILL: reimplementing gitignore semantics without git is a correctness minefield; `git check-ignore` composes |
| 49 | Network-filesystem warning | Safety | KILL: detection reliability unknown; docs cover it |
| 50 | Human `bead status` summary | Novice UX | KILL: workspace-tour objection from run 3 stands |
| 51 | Color output with NO_COLOR | Novice UX | KILL: polish |
| 52 | Close feedback listing newly ready beads | Novice UX | KILL: collides with the exact-stdout success contract |
| 53 | ID suggestions on not-found | Novice UX | KILL: polish; auto-resolution remains rejected |
| 54 | MCP server over stdio | Process | MERGED into finalist 24 as a consumer of the session protocol |
| 55 | Static HTML report | Competitor | KILL: dashboard-adjacent; composes externally |
| 56 | CSV projection | Competitor | KILL: trivial projection over existing query output |
| 57 | Changelog generator from closed beads | Competitor | KILL: archaeology shares the machinery and answers strictly more; repos can template from its JSON |
| 58 | Fork identity for clones — `sync fork` re-origins a cloned workspace with a provenance-chained new store UUID | Git transport | **FINALIST** |
| 59 | Prebuilt-binary distribution polish | Packaging | KILL: packaging, not mechanism |
| 60 | Named recovery points — tags on generations | Git transport | KILL: wrapper-scale; rides on pointer machinery whenever transport work lands |

### Completeness entrant

| Idea | Verdict |
| --- | --- |
| Sensitive-content flush lint — versioned built-in credential-shape rules scan checkpoint-bound content, warn-only, never printing matched values | **FINALIST** (deferred in run 3; resurrected: the environment's credential-write guard covers only one agent harness, other fleet adapters bypass it, and under R026 a leaked secret becomes an immortal Git object at mutation speed rather than flush speed) |

### Finalist dossiers

#### 1. Remote-advanced reconcile

`bead sync reconcile` recognizes the state where the committed checkpoint is
ahead of the live database — the normal result of `git pull` in the
multi-machine flush-commit-push workflow — verifies the pointer and event
continuity, and merges the checkpoint into the live store through the existing
`--merge` machinery. Today doctor classifies covered-ahead-of-live as an
integrity failure, which misdiagnoses the fleet's daily replication path.

- Complexity: **M**
- First step: specify the state taxonomy — verified-pointer event-stream
  superset of live versus every other covered>live case — so genuine
  corruption is never masked, then wire the guided merge.
- Strongest objection: it legitimizes a state currently treated as integrity
  failure; the specification must keep the corruption cases fail-closed.

#### 2. Fork identity for clones

`bead sync fork` re-origins a cloned workspace under a new store UUID with a
provenance-chained receipt. Two clones of one repository currently share a
store UUID, so divergent event streams at the same origin sequence are
rejected as divergence with no reconciliation path. Forking makes clones
distinct origins whose histories merge composably, the way git remotes do.

- Complexity: **M**
- First step: specify fork receipt fields, UUID derivation provenance, and
  doctor guidance that detects same-UUID divergence and names the fix.
- Strongest objection: a mistaken fork makes future same-store merges look
  foreign; the receipt chain and an explicit operator step are mandatory.

#### 3. Self-defending workspace discovery

Workspace discovery stops at the first `.beads` directory on the walk and
fails closed if it lacks the bead-rs fingerprint, instead of walking past a
foreign store and silently operating on a parent workspace. Motivated by the
2026-08-14 SEAM incident, where wrong-CLI-against-wrong-store produced schema
errors and a destructive misrepair. The guard claims only "not a bead-rs
workspace" — it never identifies the foreign format.

- Complexity: **S**
- First step: change the discovery walk to terminate at any `.beads` and
  validate the fingerprint before selection, with a precise diagnostic.
- Strongest objection: a repo could legitimately nest a foreign `.beads`
  above a bead-rs workspace; an explicit override flag must exist.

#### 4. Atomic resource locks

Issues declare normalized local resource keys; a claim atomically acquires
them and excludes ready issues needing a held key. Deferred in run 1;
resurrected because NEEDLE ADR-015 explicitly adopts bead-level serialization
as the answer to shared-checkout collisions and the duplicate/conflicting-work
incident class has recurred — locks mechanize the discipline the fleet
currently maintains by hand.

- Complexity: **M**
- First step: define resource-key validation, acquisition/release lifecycle
  bound to claim/release/close, and readiness reason codes, scoped strictly to
  one native store.
- Strongest objection: users may mistake it for distributed locking; naming
  and documentation must anchor it to a single workspace.

#### 5. Idempotent create by unique ref

`create --unique-ref ns:key` binds an R011 external reference at creation
inside the insert transaction; if the reference is already bound, the command
returns the existing bead's ID instead of creating a duplicate. This kills
the duplicate-bead class at its source — dispatchers that materialize beads
from the same defect/source identifier race today and produce twin beads.

- Complexity: **S**
- First step: define the ref-hit contract, including the closed-bead case
  (return the closed ID with distinct output, or fail with exit 4 under a
  flag) so automation cannot silently loop on finished work.
- Strongest objection: the closed-bead semantics must be explicit or retries
  oscillate between "already exists" and "work is done".

#### 6. Atomic bulk transaction manifests

Validate a versioned JSON manifest of creates, updates, labels, dependencies,
and closes with local references for new IDs, show a dry-run diff, then commit
all operations in one transaction. Deferred in run 1; resurrected because
R026 turns an N-command materialization into N published checkpoint
generations, and an interrupted materialization still discards the whole
disposable workspace — one manifest is one transaction and one generation.

- Complexity: **L**
- First step: define manifest v1 strictly as a thin composition of existing
  command primitives with validation order and a result map.
- Strongest objection: transaction DSLs creep toward duplicating the CLI;
  v1 must refuse any semantics a single existing command does not already have.

#### 7. Caller-owned stdio session

`bead session --stdio` reads one JSON request per line and writes one response
per line, with the parent process owning the lifecycle. No socket, no daemon,
no shared state beyond the store itself. Amortizes per-invocation process and
connection startup for fleets that issue thousands of calls, and gives MCP and
editor integrations a native host to sit on. Resurrects the run-1 daemon kill:
those objections were socket lifecycle and security surface, which stdio does
not have.

- Complexity: **L**
- First step: specify the session protocol as a normative versioned contract
  (request envelope, error mapping to the exit taxonomy, capability
  negotiation) before implementation, per the format-governance rule.
- Strongest objection: it is a second public surface that must be specified,
  conformance-tested, and kept in lockstep with the CLI forever.

#### 8. Checkpoint archaeology

One read-a-checkpoint substrate serving three read-only operations over
committed generations: query a named checkpoint artifact without import,
semantically diff two generations at issue/event granularity, and
bisect-style predicate search across a series. Under R026 every commit
carries a generation, so committed history becomes a queryable timeline of
workspace state — the git-history forensics story. Resurrects run-1 #49 and
#19, whose "secondary" objections predate per-commit generations.

- Complexity: **M**
- First step: implement verified read-only loading of a pointer/manifest/
  monolith into an ephemeral in-memory view, then expose `query --checkpoint`
  and `sync diff` over it; never accept these views as import input.
- Strongest objection: a queryable partial view can be mistaken for a
  recovery source; outputs must be explicitly non-importable.

#### 9. Stale in-progress detection

A doctor scope reporting non-leased `in_progress` beads whose last event is
older than a configured interval, with the exact `release` suggestion. Workers
die without releasing; leases (R002) solve this only for opted-in claims, and
the fleet's dominant path is non-leased. Advisory only — doctor never releases
work itself.

- Complexity: **S**
- First step: add the check to the existing doctor scope framework with a
  configured threshold and stable diagnostic code.
- Strongest objection: overlaps R019's planned starvation diagnostics and
  R002 leases; justified as the narrow, immediately executable subset for the
  non-leased majority.

#### 10. Sensitive-content flush lint

A versioned, built-in set of high-confidence credential-shape rules scans
checkpoint-bound fields at flush (and on demand), reporting field locations
without retaining or printing matched values. Warn-only by default; no remote
or executable rule packs. Deferred in run 3; resurrected because the
environment's credential-write guard protects only one agent harness while
the fleet runs several, and under R026 a leaked secret becomes an immortal
Git-tracked object at mutation speed.

- Complexity: **M**
- First step: define the rule-set version, redacted diagnostic schema, and
  size bounds; wire as a doctor scope plus an optional flush-time warning.
- Strongest objection: false positives and negatives are inherent; it must
  never claim completeness and never block flush by default.

### Run-4 disposition

Product decision on 2026-08-15:

- **Adopted:** remote-advanced reconcile, fork identity for clones,
  checkpoint archaeology, self-defending workspace discovery, atomic resource
  locks, idempotent create by unique reference, atomic bulk transaction
  manifests, and stale in-progress detection. These are R027-R034 in the plan
  (revision 6). Resource locks and bulk manifests thereby leave the section 14
  deferred list they had occupied since run 1.
- **Deferred to this ledger:** caller-owned stdio session (with MCP hosting as
  a consumer of it) and sensitive-content flush lint.
- **Beads:** `beadrs-de075bba` (R027), `beadrs-eeabe47a` (R028),
  `beadrs-2ba14020` (R029), `beadrs-17aa4ef9` (R030), `beadrs-1ce8d4a6`
  (R031), `beadrs-57f9ef2f` (R032), `beadrs-e854d52a` (R033),
  `beadrs-90c9afc9` (R034), under genesis `beadrs-d6f98dab`.
- Separately requested, outside this run's pool: an assessment of upgrading
  bead-rs from the Rust 1.75 MSRV to current stable. Assessed and adopted the
  same day as ADR-004 (MSRV 1.85 + edition 2024 + CI-enforced floor + scoped
  dependency refresh), plan revision 7, beads `beadrs-e3ff78f3`,
  `beadrs-dd914e5b`, `beadrs-c0e4fb66` under genesis `beadrs-c69cd4c2`.
- No F001-F017 pass state changed.

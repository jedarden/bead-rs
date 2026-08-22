# Remote-advanced reconcile v1 specification

Status: normative.

Implements plan section 12 R027. This specification is the state taxonomy
and command contract for recognizing and reconciling a durable checkpoint
that is ahead of the live database. It must exist and be reviewable before
implementation evidence is accepted; plan prose alone is not implementation
authority.

## Scope and safety rule

The Git-transported workflow commits `.beads/checkpoint/` and not
`.beads/beads.db`, so after pulling another machine's flush the durable
checkpoint contains work the live database does not. R027 recognizes that
state — **remote-advanced** — and makes one command reconcile it:
`bead sync reconcile`.

Two boundaries are absolute:

- `bead-rs` never runs Git, never inspects repository or remote state, and
  never conditions behavior on the transport that delivered the checkpoint
  (ADR-009). Remote-advanced is a *store relationship*, observable from the
  workspace artifacts alone.
- Recognizing remote-advanced legitimizes exactly one previously-failing
  shape. Every other checkpoint-ahead-of-live configuration remains a
  fail-closed integrity failure with the same refusal it has today. This
  specification narrows nothing else.

## Definitions

- The **live store** is the SQLite database. The **live sequence** `L` is
  `MAX(sequence)` over all events, `0` when the table is empty.
- The **durable checkpoint** is the pointer `.beads/checkpoint/current.json`
  together with the immutable generation it selects.
- The **covered sequence** `C` is the pointer's `snapshot_sequence`.
- The **workspace UUID** is `workspace.uuid` in the live store; the
  **pointer UUID** is the pointer's `store_uuid`.
- An event's **wire identity** is the pair
  `(origin_store_uuid, origin_event_sequence)`. Rows created by native
  mutations carry NULL origin columns; publication derives their identity as
  this store's UUID plus a sequence numbered after the highest explicit
  local-UUID identity, in local ingestion order. All identity comparisons in
  this specification use the derived identity, which is exactly the identity
  every published checkpoint already claims for those rows.
- An event's **public content** is `(issue_id, kind, actor, time, detail)`,
  with `actor` compared after applying the export default (a NULL actor is
  equivalent to `"system"`).
- The **staged stream** is the checkpoint generation selected by the pointer,
  loaded through the forensic staging machinery and passing its
  source-intrinsic validation: schema declarations, canonical ordering,
  dependency existence and acyclicity, and per-origin event continuity. A
  native origin starts at sequence 1. An R028 fork origin instead starts at
  the fork point plus 1 only when a staged `fork` receipt names that origin
  and the fork identity encodes the same point; later events remain contiguous.
  This is the compatibility-preserving sequence design fixed by
  `fork-identity-v1.md`, not a general relaxation for truncated streams.
- The pointer is **verified** when it parses, declares a supported mode, a
  nonempty store UUID, a nonnegative snapshot sequence, and an active root
  whose bytes hash to the pointer's declared SHA-256; when no
  pointer-declared tombstone remains unresolved on disk; and, in monolithic
  mode, when the `forensic.jsonl` compatibility view is byte-identical to
  the selected root.

## State taxonomy

The **sync relationship** between the live store and the durable checkpoint
is exactly one of:

| Relationship | Definition |
| --- | --- |
| `absent` | No pointer exists. |
| `behind` | `C < L`. The live store has unflushed work. |
| `aligned` | `C == L`. (Pointer and recorded-state health are reported separately; `aligned` claims nothing about them.) |
| `remote-advanced` | `C > L` **and** every qualifier in the next section holds. |
| `covered-ahead-integrity-failure` | `C > L` and at least one qualifier fails. |

The relationship is total over these five values and is computed from the
artifacts alone. `covered-ahead-integrity-failure` is not a remedy state:
no command in this specification merges, publishes over, or repairs it.

### Remote-advanced qualifiers

All of the following MUST hold for `remote-advanced`; each failure keeps the
covered-ahead state fail-closed:

1. **Verified pointer.** The pointer is verified as defined above.
2. **Valid staged stream.** The pointer-selected generation stages and passes
   source-intrinsic forensic validation, including the receipt-anchored R028
   fork-origin start rule above.
3. **Same origin.** The pointer UUID equals the workspace UUID. A checkpoint
   from a different store is a foreign merge input (`sync import-only
   --merge`), never a remote-advanced reconciliation.
4. **Event-stream superset.** Every live event — enumerated with its derived
   wire identity — appears in the staged stream with identical public
   content. The staged stream may contain additional events (that is what
   "ahead" means); the live stream may not contain any event the checkpoint
   lacks, nor a different content for a shared identity. An empty live
   stream trivially satisfies this qualifier.
5. **Honest recorded state.** The live store's recorded `checkpoint_state`,
   when present, has `covered_event_sequence <= L`. A database claiming to
   have published more history than it contains is internally inconsistent
   and fails closed.

Qualifier 4 is the entire corruption boundary. A live event missing from the
checkpoint, or sharing an identity with different content, is same-store
divergence: two histories for one store UUID with no common extension. It
MUST NOT be merged, published over, or diagnosed as benign. (Re-origining a
diverged clone is R028 `sync fork`, out of scope here; until it exists the
only remedies are manual forensics and verified restore from a retained
generation.)

The recorded generation ID differing from the pointer's generation ID is
**expected** in the remote-advanced state and is not a fault: the database
records the last local publication, the pointer records the pulled one.
Commands that report readiness MUST NOT present that disagreement alone as
an integrity failure once the remote-advanced qualifiers hold.

## The command

```text
bead sync reconcile --actor ACTOR [--dry-run]
```

`--actor` is required and validated exactly as `sync import-only` validates
it (nonempty, at most 255 bytes, no control characters). There is no
`--input`: reconcile acts on the workspace's own durable checkpoint against
the workspace's own live store, which is the whole of the state it named.

Behavior:

1. Compute the sync relationship. Anything other than `remote-advanced` is a
   usage refusal (exit 2) naming the actual relationship and its remedy:
   `behind` names `bead sync flush-only`; `absent` and `aligned` state there
   is nothing to reconcile.
2. A `covered-ahead-integrity-failure` relationship is an integrity refusal
   (exit 5) that names the first failed qualifier. The refusal MUST NOT
   mutate anything, including under `--dry-run`.
3. With `--dry-run`, report prospective merge counts and the receipt preview
   without mutating the live store, writing the checkpoint, or creating a
   receipt.
4. Otherwise merge the staged stream into the live store through the
   existing merge machinery: one transaction, conflict detection, issue
   reconciliation by timestamp, an actor-attributed `merge` provenance
   receipt, and a merge summary event. Reconcile MUST NOT publish a
   checkpoint generation by its own action; under the automatic publication
   default the post-commit chokepoint publishes the generation covering the
   merge, and with publication suppressed the workspace is left dirty for
   `sync flush-only`, exactly like any other committed mutation.
5. Reconcile is idempotent at the state level: once reconciled (and
   published), the relationship is no longer `remote-advanced`, and a second
   invocation refuses with nothing to reconcile. A reconcile that committed
   but whose publication was suppressed leaves the relationship `behind`,
   which also refuses.

## Local-identity canonicalization

The merge machinery's idempotence check matches imported events by explicit
wire identity, but native mutations write NULL origin columns, so a naive
same-UUID merge into a live store re-inserts every pre-existing event as a
duplicate row and every later export then carries each event twice under two
identities. That is silent audit corruption, and `reconcile` is built on the
merge path, so the machinery itself MUST be identity-complete:

- **Validation.** Same-UUID merge conflict detection MUST compare against
  live events enumerated with their derived wire identities, not only rows
  with explicit origin columns. A staged event whose identity collides with
  a derived local identity and whose public content differs is a conflict,
  refused before mutation.
- **Execution.** A same-UUID merge MUST, inside its transaction, write the
  derived wire identities into the origin columns of the local NULL-origin
  events it verified against (a canonicalization that changes no public
  content, no primary key, and no ordering, and is idempotent because the
  derivation is deterministic). After canonicalization the existing
  identity-based dedup sees the matching rows, imports only the checkpoint's
  new suffix, and leaves one row per wire identity.

This corrects `sync import-only --merge` for same-UUID inputs as well; the
duplication it could produce was never specified behavior.

## Flush-only and mutation discipline

`bead sync flush-only` MUST refuse to publish when the relationship is
`remote-advanced` (exit 4, a reconciliation conflict, naming
`bead sync reconcile --actor <you>`): publishing the live store over the
pulled pointer discards the pulled generation's advancement and can
tombstone its objects. It MUST likewise refuse when the relationship is
`covered-ahead-integrity-failure` (exit 5, naming the failed qualifier):
publishing over a state this specification defines as unrecoverable-by-
automation destroys the evidence an operator or verified restore needs.
Exporting an issue-only copy with `--output` is unaffected; it writes
outside the checkpoint set and publishes nothing.

Mutating before reconciling is operator error and stays undetected by
design: a local mutation committed while remote-advanced advances the live
sequence under a pointer that already claims that sequence, and the next
classification then reports `covered-ahead-integrity-failure` (or `aligned`
with a disagreeing pointer) rather than silently succeeding. The workflow is
pull, reconcile, then work.

## Reporting

- `bead sync status` reports the relationship in text and as a JSON field
  `relationship` with the five values above. In the `remote-advanced`
  relationship the report names the reconcile remedy and
  `ready_to_commit` remains false. In `covered-ahead-integrity-failure` the
  report names the first failed qualifier.
- `bead doctor` reports `remote-advanced` as a distinct actionable
  diagnostic — not an integrity failure and not silent health — carrying a
  stable machine-readable state marker and the reconcile remedy. Doctor
  never reconciles, including under `--repair`.
  `covered-ahead-integrity-failure` remains an integrity failure exactly as
  before, with the failed qualifier named.

## Conformance scenarios

1. **Pull produces remote-advanced.** Two clones share one store UUID; the
   remote advances and publishes; copying the checkpoint into the lagging
   clone yields relationship `remote-advanced` from `sync status` (text and
   JSON).
2. **Reconcile merges.** After reconcile, the live store contains the
   checkpoint's issues and events, a merge receipt attributed to the actor,
   and — under the automatic default — a freshly published generation whose
   covered sequence equals the live sequence; `sync status` then reports
   `aligned` and ready to commit.
3. **No duplicated audit trail.** After reconcile (and after the equivalent
   `sync import-only --merge`), every wire identity appears exactly once in
   the live store and in the next published generation.
4. **Dry-run is inert.** `--dry-run` leaves the live sequence, checkpoint
   pointer, and receipt table unchanged.
5. **Refusals.** Reconcile refuses with exit 2 for `behind` (naming
   `flush-only`), `aligned`, and `absent`; with exit 5 for a tampered root
   (hash mismatch), a foreign pointer UUID, and a live event absent from or
   conflicting with the staged stream.
6. **Flush-only refuses covered-ahead.** In `remote-advanced` flush-only
   exits 4 naming reconcile; in `covered-ahead-integrity-failure` it exits 5
   naming the qualifier. The pointer and its objects are left untouched.
7. **Doctor distinguishes.** Doctor reports `remote-advanced` as the
   distinct actionable diagnostic with the reconcile remedy, and
   `covered-ahead-integrity-failure` as an integrity failure naming the
   qualifier; the two outputs differ.
8. **Divergence stays fail-closed.** A local mutation committed while
   remote-advanced yields a live event the pulled checkpoint lacks (or a
   content conflict at a shared identity); reconcile and flush-only both
   refuse, and doctor reports the integrity failure.

## Relationship to existing commands

`sync import-only --merge` remains the lower-level interchange primitive and
keeps accepting arbitrary checkpoint inputs, including foreign-UUID ones.
`bead restore` remains the named-generation disaster-recovery path for an
unusable or missing live store and is unaffected. Reconcile is neither: it
is the daily replication step for a healthy live store sitting behind its
own workspace's pulled checkpoint, and it refuses every input that is not
exactly that.

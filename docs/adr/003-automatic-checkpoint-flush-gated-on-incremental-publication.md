# ADR-003: Make Checkpoint Flush Automatic on Mutation, Gated on Incremental Publication

**Status**: Proposed

**Date**: 2026-08-15

**Decision-makers**: bead-rs release owner

## Context

Since the first plan revision `bead-rs` has held one rule about durability:
**nothing flushes implicitly**. A mutation commits to SQLite; the checkpoint
under `.beads/checkpoint/` advances only when someone runs
`bead sync flush-only`. Section 6.2 states it, root help states it, the README
states it, and `sync --status` exists to report the resulting gap.

The rule buys a real property — a flush is a deliberate, reviewable
publication step, and the artifact a repository commits is one an operator
chose to commit. It also has a real cost, and the cost falls on exactly the
population `bead-rs` is built for. A fleet worker that mutates and exits
without flushing leaves the durable checkpoint behind the database. Because
`.beads/beads.db` is not committed, a clone reproduces the last *flushed*
state, so unflushed work is not merely stale, it is invisible to every other
worker and to recovery. The failure is silent: each command succeeded, and
nothing in its output says the durable copy moved.

That is not a hypothetical. The surrounding environment already encodes
"flush before you commit" as an operating rule for agents precisely because
the tool does not do it, and a checkpoint restored after a missed flush loses
whatever the last session did.

The obvious remedy is the one a neighbouring tool already ships: flush
automatically after every successful mutation, and leave `sync flush-only` as
an idempotent final check. Adopting it here is a change to checkpoint
semantics and to the published command contract, so it requires this record.

### What measurement shows

Flush cost was measured against the shipped `bead` 0.1.3 binary in a scratch
workspace on the development host:

| Issues | `sync flush-only` wall time | Checkpoint bytes |
| --- | --- | --- |
| 100 | 0.01 s | 78 KB |
| 400 | 0.03 s | 357 KB |
| 1600 | 0.07 s | 1.47 MB |

Flush is linear in total workspace size, because
`publish_forensic_checkpoint` re-reads every issue, event, and receipt and
re-serializes a complete monolith on each call. At 1610 issues a `bead create`
costs about 9 ms and a flush about 52 ms, so flushing on every mutation would
make each mutation roughly **6.8x** more expensive, with the multiplier
growing linearly as the workspace grows.

Wall time is the smaller problem. Three defects in the current publication
path turn per-mutation flushing into unbounded growth:

1. **The monolithic root is not content-addressed.** Section 6.1.1 requires
   object filenames to contain their content SHA-256 so identical content is
   reused. The monolithic writer instead names the root from the generation
   ID — `src/service/checkpoint.rs:3567` writes
   `objects/{generation_id}.jsonl`. The sharded writer does use content
   hashes, but sharded mode is unreachable: `cmd_sync_flush_only` hardcodes
   `CheckpointMode::Monolithic`. Two consecutive flushes with byte-identical
   content were observed to produce two distinct objects (18 → 20). Dedup can
   never hit.
2. **Tombstones are declared but never applied.** Section 6.2 step 6 requires
   applying the pointer-declared tombstones after the pointer commits.
   `current.json` was observed declaring 18 `deleted_paths`, with all 18 still
   present on disk — and with `current.json` itself wrongly listed among them.
   Ten mutate-and-flush cycles at ~1600 issues grew the checkpoint from 3
   objects / 1.47 MB to 13 objects / 7.83 MB.
3. **Most mutations do not advance the event sequence.** The dirtiness
   contract in section 6.2 is "every committed semantic mutation advances the
   live event sequence." It does not hold. The public `create_issue`
   (`src/service/issues.rs:14`) writes no audit event — only the unused
   `create_issue_internal` does — and `dependencies.rs`, `external_refs.rs`,
   and `data.rs` contain no event inserts at all. After ~1620 `bead create`
   calls the `events` table held 0 rows and `bead changes --latest` reported
   `max_sequence: 0`.

Composed, these mean per-mutation flushing would append a full-workspace-sized
immutable object per mutation, never collect the predecessors, and be unable
to tell from the event sequence whether a flush was needed at all. For a
Git-tracked directory that is a repository-size incident, not a slow command —
this project's own environment has already absorbed one 817 MB history from
committing large generated artifacts.

## Decision

Adopt **automatic flush on successful mutation as the default behavior**, with
`--no-auto-flush` and a workspace configuration key as escape hatches, and
`bead sync flush-only` retained as an explicit idempotent operation.

Gate activation on the checkpoint publication path becoming **incremental**.
Automatic flush ships only when a flush writes work proportional to the change
rather than to the workspace. Until every prerequisite in section 6.2.1 of the
plan passes, the default remains explicit flush, and the automatic path is
neither enabled nor advertised in the capability document.

`bead-rs` still never invokes Git. Automatic flush publishes into the working
tree; committing remains entirely the caller's business. The plan's standing
rejection of automatic Git publication is unaffected.

## Rationale

The property that motivates automatic flush is *the durable checkpoint is
never silently behind the database*. Coalescing schemes — flush every N
mutations, flush after T seconds, flush at process exit — reduce cost by
reintroducing exactly the window the change exists to close, and they make the
gap nondeterministic instead of merely present. If the guarantee is worth
having it must be per-mutation.

Per-mutation flushing is affordable only if a flush is proportional to the
delta. Section 6.1.1 already designs for this: content-addressed objects
reused byte-for-byte, issue shards split by bead-ID prefix, and audit events
sealed into append-only tail objects specifically so "a frequently updated
bead [does not] rewrite its entire history-bearing issue shard." Under that
design a single mutation rewrites one issue shard, appends one event tail
object, and publishes a new manifest and pointer. The architecture that makes
automatic flush viable is therefore already specified — it is simply not the
one the code takes today.

Sequencing the decision this way keeps two things from being confused. The
prerequisites are not new scope invented to justify delay: items 1 and 2 above
are the implementation diverging from section 6.1.1 and 6.2 as already
written, and item 3 is a stated invariant that is not upheld. They are
defects, and each is independently worth fixing whether or not the flush
default ever changes. What this ADR adds is that automatic flush *multiplies*
all three by the mutation rate, which promotes them from latent to blocking.

Explicit flush was never valuable in itself. It was the conservative default
for a publication step whose cost and correctness were not yet bounded. Once
they are, the deliberate-publication argument no longer outweighs a fleet
losing work to a missed command.

## Consequences

### Benefits

- A clone, a restore, and a `doctor` run reflect the last mutation rather than
  the last remembered flush. The most common way to lose fleet work disappears.
- Agent instructions lose a manual step that has to be repeated in every
  workspace's `AGENTS.md`, and whose omission fails silently.
- `sync --status` becomes an assertion that should always hold, so a dirty
  checkpoint changes from routine to a real signal worth failing a gate on.
- Fixing the prerequisites bounds checkpoint growth for explicit flushing too;
  the object leak exists today at whatever rate operators flush.

### Drawbacks

- Every mutating command acquires a publication step, so a mutation can now
  fail after its transaction commits. The command contract must define that
  split outcome rather than leave it to each call site.
- Concurrent workers newly contend on checkpoint publication, not just on
  SQLite. Pointer replacement needs its own serialization, and a lost race
  must degrade to "someone else published a newer generation," never to a
  torn pointer.
- Mutation latency rises by the incremental publication cost even in the best
  case, which is a real tax on the rapid-fire lifecycle benchmarks in
  section 3.5.10.
- The working tree changes on every mutation. Operators who ran read-modify
  loops expecting a quiet tree will see churn, and Git status becomes noisy
  between commits.
- It reverses a rule stated in the README, root help, man pages, and the
  surrounding environment's agent instructions. All of them become wrong on
  the day the default flips and must change in the same release.

### Alternatives Considered

- **Flush per mutation on the current monolithic path**: rejected. It is the
  straightforward reading of the request and it is the trap — quadratic bytes
  written, one leaked full-size object per mutation, and linear-in-workspace
  latency on every command.
- **Coalesced flush (every N mutations or T seconds)**: rejected as the
  default. It preserves a silent staleness window, which is the defect being
  fixed, and replaces a deterministic gap with a timing-dependent one. It
  remains a legitimate opt-in for very large workspaces once the automatic
  path exists.
- **Flush at process exit**: rejected. A crashed or `SIGKILL`ed worker is
  precisely the case that loses work, and `bead-rs` has no supervising
  process to fall back on.
- **A background flushing daemon**: rejected. Section 6.2 and the plan's
  standing rejections keep `bead-rs` a synchronous CLI with no daemon or
  network authority; a daemon would also reintroduce the staleness window.
- **Keep explicit flush and document harder**: rejected. It is the status
  quo, it has already failed in practice, and documentation cannot make a
  missed command observable to the worker that missed it.
- **Automatic Git commit after flush**: remains rejected, unchanged. Flushing
  and publishing history are different decisions with different blast radii.

## Implementation

Tracked as **R026** in section 12 of the plan, with the normative contract in
section 6.2.1. Four prerequisites (P1-P4) must pass before the default flips:

- **P1** — content-address the monolithic root and reuse identical objects.
- **P2** — apply pointer-declared tombstones; stop listing `current.json` as
  deleted; bound the retained object set to the generations the pointer and
  `previous.json` reference.
- **P3** — emit an audit event for every committed semantic mutation, so the
  event sequence is a sound dirtiness signal.
- **P4** — make sharded mode reachable and selected by the recorded thresholds
  so publication cost tracks the delta.

Then the automatic path itself: a single post-commit publication chokepoint,
defined split-failure semantics and exit code, publication locking, the
`--no-auto-flush` flag and `checkpoint.auto_flush` configuration key, an
`auto_flush` field in the capability document, and the documentation reversal
across README, root help, man pages, and `AGENTS.md`.

This is a one-time contract change followed by ongoing maintenance of the
incremental publication path.

## Related

- Plan section 6.1.1 — adaptive sharded checkpoint set (the incremental design
  this ADR depends on)
- Plan section 6.2 and new section 6.2.1 — flush algorithm and the automatic
  flush contract
- Plan section 12, R026 — automatic checkpoint flush on mutation
- Plan section 13 — release gate for R026
- Plan section 3.5.10 — rapid-fire lifecycle capacity benchmarks, which bound
  the acceptable per-mutation publication cost
- `research/specs/checkpoint-set-v1.md` — specification-blocked; P1, P2, and
  P4 are constrained by it

## Supersedes

None. This ADR revises the flush default recorded in plan section 6.2; it does
not supersede an earlier ADR. The rejection of automatic Git publication
stands.

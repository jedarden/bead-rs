# Checkpoint Set v2 Specification (DRAFT — FOR INDEPENDENT REVIEW)

**Status**: DRAFT — ready for independent review; normative for the adaptive
set layout upon acceptance
**Created**: 2026-10-23
**Author**: beadrs-a4fd9f40 specification dispatch (specification role,
clean-room)
**Predecessor**: `research/specs/checkpoint-set-v1.md` (record schemas,
monolithic and sharded-v1 modes, restore-equivalence semantics)
**Source**: field measurements of 2026-10-23 (section 2) and the settled
layout decisions they motivate

## Abstract

Checkpoint Set v2 defines an incremental durable-checkpoint layout for
bead-rs in which the bytes a publication adds to repository history scale
with what changed, not with how large the workspace is. Issue-shaped records
live in many small content-addressed shards partitioned by issue-ID prefix;
events live in sealed, immutable, append-only segments plus one open tail;
pointer and manifest files grow with the number of shards and segments only,
never with record content. `forensic.jsonl` is not rewritten on each
publication. Every workspace in adaptive mode uses this layout; monolithic
output remains available only as an explicit forced mode under its existing
safety limits. The first publication under this layout is a full rewrite and
is documented as such; restore, reconcile, fork, and verified-restore read
both this layout and the older ones.

## 1. Motivation: publication bytes scale with store size today

A field sweep on 2026-10-23 covered all 92 bead-rs workspaces under
`/home/coding`, git object accounting in the largest repositories, and every
bead invocation in agent transcripts (`~/.claude/projects`, 32,149 calls;
`~/.codex/sessions`, 80,311 calls; regex-based, approximate). Uncompressed
blob bytes added to git history under `.beads/` over the last 30 days:

| Workspace | Mode | Beads | Checkpoint commits | Bytes added | Bytes/commit |
|---|---|---|---|---|---|
| tradegraph-platform | sharded | 1,206 | 2,400 | 13.8 GB | 5.7 MB |
| bead-rs | monolithic | 891 | 1,391 | 4.05 GB | 2.9 MB |
| ARMOR | monolithic | 2,981 | 105 | 2.5 GB | 24 MB |
| NEEDLE | monolithic | 3,298 | 64 | 2.4 GB | 37 MB |
| commitgraph | sharded | 3,080 | 4 | 110 MB | 27.6 MB |

Bytes scale with store size, not with what changed, in both modes. The
mechanism is shard granularity. In monolithic mode the active root is one
object holding the entire store, so every publication re-objects the whole
corpus. In sharded-v1 mode the split thresholds (50,000 issues or 50 MiB per
issue shard; 100,000 events or 64 MiB per event shard) leave a 3,000-issue
workspace with essentially one issue shard and a handful of event shards, so
a one-issue mutation rewrites a shard the size of the store. Delta
compression hides part of this inside packfiles, but every clone, fetch, and
push still pays the uncompressed object set, and a forge has refused
oversized pushes in this environment before. Automatic flush (ADR-003,
activated 2026-08-21) publishes after every successful mutation, which turns
the per-publication cost into the dominant git-history cost of the fleet.

## 2. Relation to checkpoint-set-v1

`checkpoint-set-v1.md` remains normative for what it defines: the issue,
event, and provenance-receipt record schemas; monolithic mode; sharded-v1
mode; the authoritative-pointer discipline; and the restore-equivalence
definition. This specification does not amend those.

Checkpoint Set v2 is normative, upon acceptance, for the adaptive set layout
defined here: shard partitioning and sizing, event segment sealing, pointer
and manifest bounds, the migration path, and the publication byte budget.
Where v1 and v2 both speak about the set layout, v2 governs. The two
documents are intended to be read together; v2 reuses v1's record schemas
byte-for-byte and changes only where records live and how publications
write them.

## 3. Format identity and discovery

- Format string: `checkpoint-set-v2`.
- Pointer mode string: `set-v2` (values already defined elsewhere:
  `monolithic`, `sharded`).
- Discovery is unchanged: `.beads/checkpoint/current.json` is the sole
  authoritative pointer. Readers — restore, reconcile, fork,
  verified-restore, doctor — dispatch on the pointer's mode/format field and
  accept `monolithic`, `sharded`, and `set-v2`. Import never chooses between
  roots or views by existence or modification time.
- Every relative reference in a pointer or manifest is a slash-separated
  path relative to the normalized checkpoint-set base (the directory
  containing `current.json`), normalized and validated exactly as v1
  requires: no absolute paths, no `.`/`..` components, no backslashes, no
  symlinked components, no resolved path outside the base.

Directory layout, all under the checkpoint-set base:

```
current.json                  pointer (mode set-v2)
previous.json                 retained prior pointer
set/manifest-<gen>.json       generation manifest
set/shard/<sha256>.jsonl      keyed-record shards (issues, receipts, outcomes, redactions)
set/seg/<sha256>.jsonl        sealed event segments
set/rec/<sha256>.jsonl        oversize records, referenced by shards and segments
set/tail.jsonl                the one open event segment
```

The `set/` tree is physically disjoint from the `objects/` and `manifests/`
trees of the older layouts, so a reader that has dispatched on the mode never
collides with the other formats' files.

## 4. Record corpus

Record types and their schemas are v1's, unchanged: `issue`, `event`,
`provenance_receipt`, plus the later keyed record classes the pointer already
counts (attempt outcomes, redaction records). Two placements exist:

- **Keyed records** (every record class with a stable identifier: issues,
  provenance receipts, attempt outcomes, redaction records) live in
  **shards** (section 5). A record appears in exactly one shard at any
  generation.
- **Events** form an append-only **log** partitioned into segments
  (section 6). An event appears in exactly one segment at any generation.

The pointer's counts (issue, event, receipt, attempt-outcome,
redaction-record, total) are carried unchanged from v1 and must match the
manifest.

## 5. Keyed-record shards

### 5.1 Partitioning

Shards partition keyed records by **prefix of the canonical record ID**.
Each shard owns the class of IDs that begin with its prefix string; prefix
classes are disjoint by construction and cover the whole ID space (a prefix
with no shard owns only IDs that do not exist). Each record class is
partitioned independently with its own prefix space; issue shards are by far
the most populous and drive the sizing rules below, which apply to every
class with its own counts.

Inside a shard, records are v1 JSONL records (`record_type` plus payload),
one per line, ordered by record ID ascending, LF-terminated, valid UTF-8.
A shard file is content-addressed: its name is the SHA-256 of its exact
bytes, and the name and the manifest-recorded digest must agree.

### 5.2 Sizing bounds

| Bound | Value | Meaning |
|---|---|---|
| `SHARD_TARGET` | 32 KiB | typical shard size; the operating point |
| `SHARD_SPLIT` | 96 KiB | a shard above this after a publication must split |
| `SHARD_MERGE` | 24 KiB | a full-sibling group at or below this coalesces |
| `RECORD_INLINE` | 64 KiB | records above this move to `set/rec/` (section 6.3) |

A typical shard stays under 64 KiB. A one-record mutation rewrites only the
shard holding that record.

### 5.3 Split rule

After a publication, any shard whose byte length exceeds `SHARD_SPLIT`
splits. The new depth is one plus the length of the longest common prefix of
the member record IDs (never shorter than the current prefix plus one
character); the shard is replaced by one child per distinct next character.
Because record IDs are unique, any shard holding two or more distinct IDs
splits into two or more non-empty children; a single-record shard cannot
split and is permitted to exceed `SHARD_SPLIT` only in that case, which the
`RECORD_INLINE` bound caps at roughly the bound plus record framing. Split
decisions derive only from the shard's own contents, so they are
deterministic and reproducible from any equivalent state.

### 5.4 Merge rule

After a publication, for each shard at depth d, if the total byte length of
the full-sibling group sharing the depth d−1 prefix is at or below
`SHARD_MERGE`, the group coalesces into the parent prefix. The rule applies
bottom-up, once, per publication. This keeps shard count proportional to
live data after deletions instead of drifting upward forever.

## 6. Event log: sealed segments and the open tail

### 6.1 Segments

The event stream is the v1 canonical order — ascending
`(origin_store_uuid, origin_event_sequence)` — concatenated over, in order:
all sealed segments in manifest order, then the open tail. Per-origin event
sequences are contiguous across the whole concatenation.

| Bound | Value | Meaning |
|---|---|---|
| `SEG_SEAL` | 64 KiB | the tail above this after a publication seals |

### 6.2 Seal rule

A publication appends its new events to the tail. If the tail's post-append
byte length exceeds `SEG_SEAL`, the tail **seals**: it is renamed to its
content-addressed name `set/seg/<sha256>.jsonl`, recorded in the manifest as
a sealed segment with its sequence ranges, and a new empty tail opens in the
same publication. A sealed segment is immutable for the life of the
workspace: it is never appended to, rewritten, repacked, or re-split. The
manifest never records two sealed segments with the same digest. A sealed
segment is therefore at most `SEG_SEAL` plus one publication's appended
bytes.

### 6.3 Oversize records

Any single record (keyed record or event) whose JSONL line would exceed
`RECORD_INLINE` is stored as its own content-addressed object
`set/rec/<sha256>.jsonl` holding exactly that line; the shard or segment
stores a reference object (`record_type`, `ref: true`, `path`, `sha256`,
`byte_length`) in its place. Content addressing makes the cost of a large
record a one-time cost: rewriting the shard that references it, or sealing
the segment that references it, adds no bytes for the referenced record.
Readers resolve references recursively (depth limit: 1; a reference inside
`set/rec/` is invalid). Hashes are verified before use.

## 7. Pointer and manifest

### 7.1 Pointer

`current.json` for mode `set-v2` records, canonically: `schema_version`,
`format` (`checkpoint-set-v2`), `mode` (`set-v2`), `generation_id`
(monotonically increasing per workspace), `store_uuid`, `snapshot_sequence`,
`active_root` (`path` `set/manifest-<generation_id>.json` plus the SHA-256
of the complete manifest bytes — filename and recorded digest must agree),
the complete record counts, and `created_at`. It deliberately does **not**
carry `added_paths`/`replaced_paths`/`deleted_paths`: under this layout the
retained-file set is defined structurally (section 7.3), the transition is
derivable from `previous.json` and the two manifests, and dropping the
arrays keeps the pointer a fixed-shape small file. The pointer must stay
under 4 KiB for the life of any workspace; growth with shard count is
forbidden in the pointer by construction. `previous.json` is the prior
pointer, retained unchanged.

### 7.2 Manifest

`set/manifest-<generation_id>.json` records: format and schema version,
store UUID, snapshot sequence and maximum local ingestion sequence, creation
time, profile, the complete record counts, the partition bounds of sections
5.2 and 6.1, and one table row per artifact:

- issue shard rows: prefix, path, SHA-256, byte length, issue count;
- rows for each other keyed-record class's shards, with their own counts;
- sealed segment rows: path, SHA-256, byte length, event count, inclusive
  sequence ranges;
- one tail row: path, SHA-256, byte length, event count, sequence ranges;
- oversize-record rows are not tabulated; shards and segments reference
  `set/rec/` paths directly and readers verify them.

The manifest's size grows with the number of shards and segments only — at
roughly 128 bytes per row — never with issue or event content. It contains
no record payloads. No authoritative manifest is ever overwritten or reused
for different bytes (v1 rule). For a store with fewer than 1,000 shard and
segment entries the manifest stays under 128 KiB; workspaces that large are
outside the budget scenario's operating point but remain bounded by the same
structural rule.

### 7.3 Retention and tombstones

After a successful publication, every file under the checkpoint-set base not
referenced by the current pointer, the previous pointer, or their manifests
is deleted. The retained object set is therefore bounded by the two
referenced generations at all times, and the deletion set is structural, not
enumerated in the pointer. A crash between pointer swap and deletion leaves
orphan files that the next publication reaps.

### 7.4 Publication procedure and concurrency

1. Write every new or rewritten artifact (shards, sealed segments,
   `set/rec/` objects, new tail) to `.tmp` siblings, fsync each file.
2. Write the new manifest to a `.tmp` sibling, fsync, and verify all hashes
   and counts it declares.
3. Write the new `current.json` to a `.tmp` sibling with the old pointer
   promoted to `previous.json`, fsync, then atomically rename it over
   `current.json`.
4. Apply the structural deletions of section 7.3; remove the sealed tail's
   old file.

A crash before step 3 leaves the old generation authoritative; a crash after
step 3 leaves the new one authoritative. Publication state is derived from
the live store and the pointer only — the format stays Git-unaware (ADR-009
stands: no Git inspection is required or permitted to order publications).
Two concurrent publishers serialize on the monotonic `generation_id`: a
publisher that observes the pointer's generation advanced past the one it
based its work on discards its staging, re-derives from the live store, and
redoes the publication; the loser exits 0 having produced no torn pointer,
no partially applied deletion set, and no lost mutation.

## 8. `forensic.jsonl` in this layout

`checkpoint/forensic.jsonl` is not rewritten on each publication in this
layout — it is not written at all. A workspace migrated from monolithic mode
retains its existing `forensic.jsonl` as a frozen historical view; it is
never appended or rewritten afterward, and its function (a nonauthoritative
whole-store view) is superseded by the manifest-addressed set. Tools that
read `forensic.jsonl` must instead read the pointer-selected segments; no
reader may require the file to exist in a `set-v2` workspace.

## 9. Modes and configuration

- **Adaptive (default)** selects the `set-v2` layout for every workspace.
  Every new workspace, and every existing workspace at its first publication
  after upgrade, publishes in `set-v2`.
- **Forced monolithic** remains available as an explicit operator choice
  (`checkpoint.layout = "monolith"` or the existing forcing flag) with its
  existing safety limits: 50,000 issue records, 64 MiB total, 8 MiB for any
  individual record line. Forcing a monolith never bypasses record/byte
  safety limits; adaptive refuses a forced-monolith publication that would
  exceed them (exit 1, with guidance), and the workspace stays on its
  current layout.
- Forced sharded-v1 output is not offered; `sharded` remains a reader-only
  legacy mode for existing generations.

## 10. Migration

The first publication of an existing workspace under adaptive mode is a
**full rewrite**: every live record becomes a shard row, every event
becomes segment or tail content, one manifest and one pointer are written,
and the old generation remains available as `previous.json` until the next
publication reaps it. The rewrite is documented as such:

- `sync --status` reports the pending upgrade (current mode, target mode,
  and that the next publication rewrites the full store) before it happens;
- root help, README, and the generated man page for `sync` state that the
  first `set-v2` publication of an existing workspace is a full rewrite and
  carries a one-time cost proportional to store size.

Migration is crash-safe by the section 7.4 ordering: the `set/` tree is
fully written and verified before the pointer swap, so a crash before the
swap leaves the old layout authoritative and orphans to be reaped. Workspaces
on sharded-v1 migrate by the same path.

**Dual-layout readers.** Restore, reconcile, fork, and verified-restore read
all three layouts, dispatching on the pointer. A checkpoint in any layout
restored into an empty store yields a store semantically equivalent to the
source under v1's restore-equivalence definition; a `set-v2` checkpoint and
a monolithic checkpoint of the same store are interchangeable as restore
sources. New publications after a restore follow the writer's mode rule
(section 9), so a restored adaptive workspace migrates to `set-v2` at its
next publication.

## 11. Publication byte budget

**Definitions.** The *new blob bytes* of a publication are the sum of
uncompressed sizes of git blobs newly added under `.beads/` by the commit
that records it. Canonical measurement, given the parent commit `P` and the
publication commit `C`:

```
git rev-list --objects P..C -- .beads/ | awk '{print $1}' > new-objects
git rev-list --objects P   -- .beads/ | awk '{print $1}' > old-objects
comm -23 <(sort new-objects) <(sort old-objects) > added-objects
git cat-file --batch-check='%(objectname) %(objecttype) %(objectsize)' \
  < added-objects | awk '$2=="blob"{s+=$3} END{print s}'
```

**Budget.** A single-issue update (one record mutation producing a
bounded event append) in a 3,000-issue workspace adds **at most 256 KiB** of
new blob bytes per publication, pointer files included. The structural
bounds deliver this: one rewritten shard at or under `SHARD_SPLIT` (96 KiB;
a tail-sealing publication adds a sealed segment at or under
`SEG_SEAL` + append and opens an empty tail, together under 80 KiB), a
manifest of roughly 128 bytes per row for a few hundred entries (tens of
KiB), and a fixed-shape pointer under 4 KiB. One exception is disclosed
rather than hidden: a changed record larger than `RECORD_INLINE` (64 KiB)
adds its own byte length to `set/rec/` exactly once and is deduplicated by
content addressing in every later publication; a budget publication whose
changed record is at or below `RECORD_INLINE` never triggers this. Files
unchanged by a publication add no bytes: unchanged shards and sealed
segments are already in history under their content addresses, and the
conformance budget scenario asserts the bound across consecutive
publications touching different issues.

## 12. Conformance scenarios

Each scenario is named; implementations claiming `checkpoint-set-v2` pass
all of them. Fixtures are built with independently invented records per the
clean-room protocol.

1. **`set-v2-migration-full-rewrite`** — a monolithic workspace at the
   safety limits' operating point and a sharded-v1 workspace each publish
   once under adaptive mode; the publication writes the complete `set/`
   tree, the old generation remains restorable via `previous.json`, and
   `sync --status` reported the pending full rewrite beforehand.
2. **`set-v2-dual-layout-reader-equivalence`** — restore into an empty
   store, fork import, reconcile, and verified restore each succeed from a
   monolithic source and from a `set-v2` source of the same store, and the
   restored stores pass v1 restore-equivalence against the source.
3. **`set-v2-budget-single-issue-update`** — a migrated 3,000-issue,
   9,000-event fixture publishes twenty successive single-issue updates;
   measuring each by the section 11 procedure, every publication is at or
   under 256 KiB of new blob bytes and the median is at or under 64 KiB.
4. **`set-v2-shard-split-on-update-storm`** — updates concentrate on one
   prefix class until it exceeds `SHARD_SPLIT`; the shard splits by the
   section 5.3 rule, children are at or under `SHARD_SPLIT`, coverage of
   all IDs is unchanged, and a re-publication from an equivalent state
   produces byte-identical children.
5. **`set-v2-shard-merge-on-deletion`** — deleting records shrinks a full-
   sibling group to at or below `SHARD_MERGE`; the group coalesces to the
   parent prefix, and the retained set stays bounded by the two referenced
   generations.
6. **`set-v2-tail-seal-and-immutability`** — appends drive the tail past
   `SEG_SEAL`; it seals to its content address, a fresh tail opens, and the
   sealed segment's name, digest, and bytes are unchanged across at least
   five further publications.
7. **`set-v2-oversize-record-once-cost`** — an update writes a 200 KiB
   record; it lands in `set/rec/` once, the referencing shard stays small,
   and subsequent unrelated single-issue publications add no bytes for it.
8. **`set-v2-pointer-boundedness`** — across at least fifty publications,
   `current.json` and `previous.json` remain under 4 KiB and the manifest
   contains no record payload, only table rows.
9. **`set-v2-forensic-frozen`** — across at least ten publications of a
   migrated workspace, `forensic.jsonl`'s bytes and mtime are unchanged,
   and restore succeeds with the file absent.
10. **`set-v2-crash-ordering`** — with publication interrupted before the
    pointer swap the old generation stays authoritative and the next
    publication reaps the orphans; interrupted after the swap, the new
    generation is authoritative and the sealed tail's removal is idempotent.
11. **`set-v2-concurrent-publication`** — two publishers race on one
    workspace; the loser exits 0 after re-deriving, and at quiesce the
    pointer names one complete generation covering every committed mutation.
12. **`set-v2-forced-monolith-limits`** — forcing monolithic output on a
    workspace beyond its safety limits is refused with exit 1 and no
    pointer change; forcing within the limits succeeds and never bypasses
    the limits.

## 13. Security considerations

All content addressing uses SHA-256, verified before use, including `set/rec/`
and `set/seg/` objects and the tail against its manifest row. Path
normalization, symlink rejection, and base containment follow v1. The
pointer swap is an atomic rename, so no reader observes a partial
generation; the old generation remains fully readable until the new one is
durable. The format performs no Git operations and derives no ordering from
Git state. Oversize-record references are depth-limited so a crafted
checkpoint cannot recurse.

## 14. Relation to pending sharded-reachability proposals

Ten pending, unapproved proposals ask to make sharded mode reachable —
beadrs-292dfbbe, beadrs-36340b5e, beadrs-37da66c7, beadrs-62acb271,
beadrs-84f9a125, beadrs-9269c2fd, beadrs-c2b0b216, beadrs-d23fdce6,
beadrs-e5c770dc, beadrs-f4b70090. They rest on a stale premise: sharded
mode is already live in three workspaces, and the section 1 measurements
show it does not bound publication bytes. This specification neither adopts
nor rejects them; their disposition is left to the human reviewer.

## 15. Terminating definitions

- **Adaptive**: the default mode that selects `set-v2` for every workspace.
- **Keyed record**: a record class whose members have stable unique IDs;
  sharded by ID prefix.
- **Sealed segment**: an immutable, content-addressed, never-rewritten event
  file.
- **Tail**: the single open event segment; the only mutable checkpoint file.
- **Split/merge**: the deterministic pair of rules keeping shards inside the
  sizing bounds as data grows and shrinks.
- **New blob bytes**: the section 11 measurement of a publication's git cost.
- **Full rewrite**: a migration publication that re-objects the entire
  store; performed once per workspace at layout adoption.

---

**INDEPENDENT REVIEW REQUIRED**: as with v1, this draft is normative for the
adaptive set layout upon independent review. The reviewer confirms: the
layout, partitioning, sealing, migration, and byte budget are complete and
unambiguous; the conformance scenarios are sufficient to bind an
implementation; the relationship to `checkpoint-set-v1.md` is coherent; and
clean-room principles have been maintained.

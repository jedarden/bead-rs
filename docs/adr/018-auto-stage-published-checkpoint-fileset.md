# ADR-018: Auto-Stage the Published Checkpoint Fileset

**Status**: Accepted

**Date**: 2026-09-13

**Decision-makers**: bead-rs maintainers

**Narrows**: ADR-003 (staging is not publication; the rejection of automatic
Git publication stands) and ADR-013 (the Git boundary gains a write side, the
way ADR-013 gave ADR-009's ordering rejection a read-side exception)

## Context

R026 made publication automatic: every successful mutation publishes a
checkpoint generation, and `sync status` (ADR-013, ADR-017) reports whether
Git can reach every published file. Assembling the Git commit that carries
the published set stayed manual, and two ways that goes wrong are structural
rather than carelessness. Both were hit on 2026-09-02:

1. `git commit <pathspec>` stages tracked modifications but **not freshly
   written untracked objects**. Committing `.beads/checkpoint` by pathspec
   silently omitted a freshly written object, and NEEDLE's
   checkpoint-verification hook rejected the commit:
   `checkpoint-publish: error: staged current.json root is missing from the
   index: objects/0a82a10e….jsonl`. Without that hook the commit would have
   recorded a checkpoint referencing an object the tree never carried.

2. **Compaction renames objects** (R099), so a hand-written pathspec is
   stale the moment it is typed. One operation surfaced as two R099
   renames plus an untracked directory.

Both leave a committed pointer selecting files the commit does not carry —
and both are invisible until a consumer of the clone tries to replay.
Reporting the gap (`sync status` names every unreachable path) does not
assemble the commit; the worker still has to reconstruct the exact set by
hand under time pressure.

## Decision

After a publication commits (the chokepoint inside
`publish_forensic_checkpoint_inner`, after the pointer transaction), bead-rs
stages into the Git index **exactly the fileset that publication made
authoritative**:

- both pointers: `current.json` always, `previous.json` when one exists;
- every object the new generation references
  (`referenced_paths`);
- every retained object the outgoing pointer still references (the retained
  previous set; empty under redaction, which deliberately does not retain
  the pre-redaction generation);
- the monolithic compatibility view when this generation wrote one;
- and the removal of every tombstoned object, staged as a deletion **when
  and only when Git tracks the path** — a deletion pathspec for a
  never-tracked path aborts the whole `git add`.

Runtime files (`RUNTIME_CHECKPOINT_FILES`, today `publish.lock`) are never
staged: synchronization metadata is not published state, and a pulled
checkpoint carrying someone else's publication lock would be damage, not
state (ADR-017's carve-out, applied to the write side).

Three boundaries shape the feature:

- **Staging, never committing.** A commit publishes history; staging only
  marks shared checkpoint state as ready for the commit that was always
  going to carry it. Branch policy, history shape, and commit messages
  remain entirely the caller's business, and ADR-003's rejection of
  automatic Git publication stands.
- **Best-effort, never gating.** A staging failure cannot fail the mutation
  or the publication: the checkpoint is already durable, and publication
  decisions key on checkpoint internals alone (ADR-017). A failure prints a
  one-line warning with the remedy on stderr, and the ordinary `sync
  status` reachability buckets surface whatever did not stage.
- **On by default, with the standard escape hatches.** The compiled default
  is `true` (`AUTO_STAGE_COMPILED_DEFAULT`); `checkpoint.auto_stage: false`
  in `.beads/config.json` is the durable opt-out, in the exact shape of
  `checkpoint.auto_flush`. Workspaces outside any Git repository never
  spawn a subprocess (ADR-013's discovery rule, reused for the write side).
  The capability document advertises the compiled default as the additive
  `auto_stage` field, the handshake R026 established with `auto_flush`:
  workspace state changes behavior, never the advertisement.

The mechanism is shelling out to `git`, per ADR-013's settled choice — the
write side does not inherit a library dependency adopted for the read side.
fsmonitor and the untracked cache stay disabled for the invocation;
`--no-optional-locks` is deliberately *not* passed, because updating the
index is the operation, not an opportunistic side effect.

## Rationale

Staging is idempotent, mutates no history, and needs no branch policy — it
removes the entire error class at the source rather than detecting it after
the fact. `sync status` already computes the authoritative set; handing the
same set to `git add` costs one subprocess per publication.

The obvious objection is polluting a concurrent worker's index on a shared
checkout. It inverts here: checkpoint files are shared state that is
supposed to be committed, so being swept into another worker's commit is
the correct outcome for them — unlike a source file being swept, which is
the real hazard, and a separate concern this ADR deliberately does not
touch.

## Consequences

- Whoever commits next picks up a complete, consistent checkpoint fileset
  no matter how they invoke Git — `git commit .beads/checkpoint`,
  `git commit -a`, or a GUI. The omitted-object class is closed at the
  source, and NEEDLE's checkpoint-verification hook becomes a backstop
  instead of the only line of defense.
- A concurrent Git operation holding `index.lock` makes staging fail; the
  warning fires and the next publication retries. Publication is
  serialized by the publication lock, so the staged set trails the winning
  generation only until its own staging runs.
- A candidate the publication recorded but that is already gone from the
  worktree (a concurrent publisher's superseding generation) is skipped
  rather than staged; the next publication stages the set that replaced
  it.
- A workspace that deliberately gitignores the checkpoint makes
  `git add` of the named paths fail every publication; such a workspace
  should set `checkpoint.auto_stage: false` — the warning names the
  remedy.

## Implementation

- `service::git_stage::stage_published_checkpoint` — the staging module
  (additions resolved against disk, deletions filtered through
  `git ls-files` to tracked paths).
- `publish_forensic_checkpoint_inner` — assembles the candidate set
  post-commit and calls the module behind
  `CheckpointConfig::auto_stage_enabled`.
- `CheckpointConfig.auto_stage` / `AUTO_STAGE_COMPILED_DEFAULT` — the
  workspace key and compiled default.
- `Capabilities.auto_stage` plus the `schema.rs` field lists — the additive
  capability handshake.

## Related

- ADR-003 (automatic flush, no automatic Git publication), ADR-009 (no
  Git-based ordering), ADR-013 (read-only reachability probe), ADR-017
  (readiness gate on reachability; publication keys on internals)
- Plan sections 6.2 and 6.2.1
- Parent requirement: bead `beadrs-0a813970`

# ADR-021: Preserve legacy event identities during workspace UUID repair

## Status

Accepted — 2026-10-11

## Context

Some migration-era stores contain an empty `workspace.uuid` in SQLite while
their tracked `.beads/config.json` already has the workspace UUID. A checkpoint
published from that state can therefore contain event identities whose
`origin_store_uuid` is the empty string. Treating those rows as unset after the
database adopts the committed UUID changes selectors for existing events.

## Decision

`bead doctor` fails when the database UUID is empty or differs from
`config.json`. `bead doctor --repair` adopts the config UUID only for the empty
database-UUID case. Before the update it canonicalizes NULL-origin rows while
the empty UUID is still the local identity basis. A stored empty UUID paired
with a positive event sequence is then treated as an explicit legacy identity,
so subsequent canonicalization, redaction, restore, and export preserve it.

Non-empty database/config divergence is not guessed at: it may be a copied or
forked history and requires explicit reconciliation. `bead init` uses the same
repair for an empty existing store and refuses to silently accept either bad
state through the lower-level initialization path.

Recovery rehearsal uses the checkpoint pointer's UUID for its temporary config,
and accepts legacy empty-origin event records when the checkpoint itself has a
non-empty store UUID. Thus `bead doctor --rehearse` verifies the repaired
workspace without rewriting already-published event identities.

## Consequences

- Existing checkpoints remain addressable by their original event identity,
  including the legacy `":<sequence>"` form.
- New events use the committed config UUID after repair.
- The empty-UUID condition is visible and actionable instead of passing doctor.
- A non-empty divergence remains an integrity failure until an operator makes
  the history decision explicitly.

## Origin of the defect

The affected path was the existing-workspace branch of `bead init` and its
schema migration-era callers: once a database already had a `workspace` row,
initialization returned its UUID without comparing it with the tracked config.
That allowed a row written with `uuid = ''` during the bf-to-bead-rs migration
period to persist indefinitely. Initialization now repairs the empty case or
fails closed on divergence.

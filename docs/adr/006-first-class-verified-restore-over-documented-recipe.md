# ADR-006: Make explicit restore a first-class verified command, not a documented multi-step recipe

**Status**: Proposed

**Date**: 2026-08-16

**Decision-makers**: bead-rs maintainers

## Context

Section 7 is deliberate that recovery is operator-driven: version 0.1 "never
reconstructs issue rows, drops unknown tables, deletes the database, rewrites a
corrupt checkpoint, or alters lifecycle/dependency data automatically. Diagnose
those cases and recommend explicit manual recovery." Post-F017 repair likewise
"fails closed and recommends explicit restore from a named, verified immutable
generation."

That stance is right. The gap is that the *recommended* explicit restore has no
command. Operators reconstruct it from prose, and it currently circulates in
downstream environment documentation as a two-step recipe along the lines of:
initialize a fresh workspace, then run an import-only sync from the forensic
checkpoint with a restore-into-empty flag and an actor.

Every property that makes this the correct recovery path — fresh schema, the
right input artifact, empty-target guard, attributed actor — is carried by
operator memory of flag spellings. It is executed exactly once per incident, by
someone who has just discovered their store is broken, under time pressure, on
the belief that a wrong move loses data.

That combination has already produced one real loss. On 2026-08-14 a workspace
was recovered only because its checkpoint happened to be current; the incident
was triggered by applying a *different* tool's recovery recipe to a bead-rs
store, which silently reinitialized it with the wrong schema. The failure was
not that the correct recipe is hard — it is that "recovery" is a body of lore
rather than a verb the tool offers, so an operator under pressure reaches for
whichever recipe they can remember.

## Decision

Add an explicit, operator-invoked `bead restore` that performs the
already-recommended restore from a named, verified generation as one verified
operation: select and verify the source artifact, refuse a non-empty target
unless explicitly forced, attribute the actor, and report exactly what was
restored.

Restore stays **explicit and never automatic**. `doctor` continues to
recommend it and never performs it. This ADR changes only whether the
recommended path is a command or a recipe.

## Rationale

The safety properties belong in code, not in a runbook. An empty-target guard
the tool enforces cannot be forgotten; an empty-target flag the operator must
remember to type will eventually be typed wrong or omitted, and the moment it
is, is precisely the moment the store is already damaged.

A named command is also the only thing that can be *pointed at*. Foreign
recovery recipes are dangerous mainly because bead-rs offers no obvious
alternative to reach for; `bead restore --help` is a far better answer to "how
do I recover this" than a paragraph in someone's environment doc.

This composes with, and does not duplicate, the adjacent adopted items. R029
materializes historical generations into explicitly **non-importable** views,
so archaeology can never be mistaken for a recovery source — restore is the
sanctioned importable path R029 deliberately refuses to be. R027 reconciles a
remote-advanced checkpoint into a *live, healthy* store; restore addresses a
store that is missing, empty, or structurally unusable.

## Consequences

### Benefits

- The recovery path the plan already recommends becomes executable and
  testable, rather than reconstructed from prose per incident.
- Guards (verified source, empty-target refusal, actor attribution) are
  enforced instead of remembered.
- Gives operators a first-class answer to "how do I recover", reducing the pull
  toward foreign recipes that corrupt the store.

### Drawbacks

- A command named `restore` invites the assumption that recovery is routine and
  safe. Naming, help text, and output must keep the operation feeling as
  consequential as it is.
- Adds a second sanctioned import path alongside `sync import-only`; the
  specification must say plainly which is authoritative and why, or the lore
  problem simply moves.

### Alternatives Considered

- **Document the recipe more prominently**: rejected. It is already documented;
  documentation is what failed. The 2026-08-14 loss happened with the correct
  recipe available.
- **Automatic recovery when `doctor` detects an unusable store**: rejected.
  Directly contrary to section 7's fail-closed rule, and it would act on a
  store whose intended contents only the operator knows.
- **Extend `doctor --repair`**: rejected. Section 7 scopes `--repair` to
  non-destructive housekeeping over a *verified live database*. Restore is the
  case where that premise has already failed.

## Implementation

Post-0.1. Requires its own normative specification and conformance scenarios
per section 12, covering at minimum:

- source selection and verification, and refusal of any unverified or
  R029-view artifact;
- the empty-target guard and what, if anything, may override it;
- actor attribution and the receipt/provenance record written;
- the exact relationship to `sync import-only`, including whether that path
  becomes internal.

Tracked as post-0.1 roadmap item **R036**.

## Related

- Section 7, Diagnostics and recovery — the fail-closed rule this preserves
- **R029** — checkpoint archaeology; deliberately non-importable, complementary
- **R027** — remote-advanced reconcile; healthy-store case, distinct from this
- **ADR-002** — agent-guided rehydration over cross-tool migration

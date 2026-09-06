# ADR-016: Keep workspace probes observational

**Status**: Accepted

**Date**: 2026-09-04

**Decision-makers**: bead-rs maintainers

## Context

`WorkspaceConfig::probe` classifies the first workspace boundary as Ready,
Uninitialized, NotFound, or NotBeadRs. That classification is used by normal
commands, `init`, `restore`, capabilities, and `doctor`.

The probe previously opened `beads.db` through the normal configured
connection. That path creates a missing SQLite file and applies pending
migrations. Merely asking whether a workspace was initialized could therefore
create its schema or upgrade it. In particular, `bead doctor` mutated the
uninitialized clone it was supposed to diagnose before returning its explicit
restore recommendation.

This made observation and repair indistinguishable. It also made migration
evidence unreliable: a command could not report the schema transition it owned
because discovery might already have performed it.

## Decision

Workspace probing is observational. It never creates a database, changes
pragmas, applies a migration, or installs connection-local mutation hooks.

If `beads.db` is absent, probing returns Uninitialized without opening it. If it
exists, probing opens it read-only and reads only the workspace identity needed
for classification. A command that proceeds from Ready opens its own normal
configured connection; that command-owned connection may then apply pending
migrations according to the migration contract.

`init` remains the explicit command for initializing or deliberately upgrading
a workspace, and must obtain an accurate before/after migration result rather
than relying on side effects of discovery. `doctor` remains read-only by
default and never initializes, restores, or upgrades a store.

## Rationale

Classification is on the read side of the authority boundary. Making it
mutating means every consumer of discovery silently gains repair authority,
including diagnostics and capability inspection. Keeping it observational
restores a simple invariant: no mutation occurs until the selected command owns
and reports it.

Read-only probing still permits older initialized stores to be classified as
Ready because the workspace identity table is part of the original schema.
The next normal connection performs any pending additive migration before the
command uses newer tables.

## Consequences

### Benefits

- `doctor` can prove that it did not initialize or restore a fresh clone.
- Capability inspection and discovery no longer have hidden write effects.
- Migration reporting can attribute a before/after transition to the command
  that requested it.
- Recovery automation has a stable separation between diagnosis and repair.

### Drawbacks

- Probe-only callers no longer receive migration as an accidental side effect.
- Corrupt or unreadable existing databases still surface an error during
  classification; observational does not mean error-suppressing.
- `init` needs a deliberate migration API that does not auto-migrate before it
  captures the prior version.

### Alternatives Considered

- **Keep using the configured connection in probe**: rejected because opening
  it can create and migrate state during read-only commands.
- **Special-case only doctor**: rejected because every probe caller should have
  the same non-mutating contract.
- **Treat every existing database as Ready without reading it**: rejected
  because an empty or wrong database would be misclassified and fail later with
  a less actionable error.

## Implementation

The observational probe landed in commit `36432b2`. Store discovery tests
assert that probing a tracked identity with no database leaves the database
absent, and the R036 verified-restore suite asserts that doctor leaves an
uninitialized store without schema.

Accurate `init` migration reporting is tracked separately by
`beadrs-5c27b273`; it must preserve automatic migration for normal configured
connections while measuring the explicit init transition before it occurs.

## Related

- [ADR-006: First-class verified restore](006-first-class-verified-restore-over-documented-recipe.md)
- `tests/r036_verified_restore.rs`
- `tests/schema_upgrade_on_init.rs`
- beads `beadrs-e498fb31` and `beadrs-5c27b273`

## Supersedes

None.

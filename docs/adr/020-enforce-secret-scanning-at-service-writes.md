# ADR-020: Enforce secret scanning at service writes

**Status**: Proposed

**Date**: 2026-10-02

**Decision-makers**: bead-rs maintainers, pending independent review

**Narrows**: ADR-014's requirement to scan every semantic mutation before commit.

## Context

The installed CLI calls `cli_secret_scan::prepare` before dispatch. The public
library also exports mutating services, including issue creation, comments,
structured data, manifests, and recurrence materialization. Those functions
can write through a `Connection` or `SqliteStore` without entering the CLI
preflight. A recurrence can generate issue text from a recovered template after
the original template scan. Thus the current command inventory proves CLI
coverage, not coverage of every public bead-rs write.

## Decision

Every public semantic write service must pass the same built-in, versioned
scanner before its first write transaction commits. The service boundary
constructs the complete canonical text of the records it will persist,
including expanded or derived text. The CLI uses that boundary and retains
ADR-014's redacted exit-2 diagnostic and same-transaction acknowledgment audit.

The public API must not expose an unguarded write entry point. A write context
bound to the workspace and effective scanner policy may be passed explicitly;
raw connection helpers used inside an already guarded transaction become
private. Callers that cannot construct a valid context fail closed. Arbitrary
SQL executed by a caller holding its own SQLite connection remains outside
the bead-rs API guarantee.

## Rationale

The service is the last common boundary for CLI and library callers. Scanning
only at checkpoint publication would leave sensitive bytes in SQLite and would
violate the store/checkpoint equivalence contract. Running an external Git
scanner for each write would add an optional executable to this boundary.

## Consequences

- A newly added public service mutation requires a canonical text inventory
  and tests that call the service directly.
- Bulk operations reject atomically before any partial commit. Internal helper
  calls inside one guarded transaction do not scan or audit twice.
- Existing library callers may need to adopt a write context. Any API break is
  explicit in the next release and capability documentation.
- High-confidence rule gaps and fingerprint acknowledgments retain the limits
  documented in ADR-014; this decision does not claim perfect detection.

## Implementation

Specify the context, complete write inventory, error and audit behavior, and
concurrent transaction semantics in `secret-write-boundary-v1` before code.
Test public service calls, CLI calls, manifests, recurrence expansion,
idempotent no-ops, rollback, and checkpoint nonpublication with synthetic
candidates assembled at test time.

## Related

- ADR-014; `research/specs/secret-rejection-v1.md`
- R039 and BR-T29 through BR-T31 in `docs/plan/plan.md`

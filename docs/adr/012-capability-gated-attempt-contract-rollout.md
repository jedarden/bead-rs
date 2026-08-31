# ADR-012: Roll Out Attempt Resolution Through Versioned Capabilities

**Status**: Accepted

**Date**: 2026-08-31

**Decision-makers**: bead-rs maintainers and software-factory operators

## Context

NEEDLE fleets may encounter different bead-rs releases and explicitly bound
legacy backends. Selecting behavior from a binary version string is brittle;
silently trying a new command is unsafe for a lifecycle mutation. bead-rs
already exposes a machine-readable capabilities document and a versioned
`needle-v1` consumer contract.

The attempt-resolution feature must be deployable without requiring an
all-at-once fleet upgrade or causing old consumers to misinterpret new
checkpoint records.

## Decision

The portable attempt contract will be an additive, explicitly versioned
capability. Consumers must negotiate it before supplying an attempt ID at
claim or invoking atomic resolution.

The capabilities document will advertise at least:

- the supported attempt-outcome schema version;
- atomic resolve support and allowed outcome/action values;
- idempotent replay and conflict behavior;
- claim-attempt correlation support;
- revision and fencing support for resolution;
- checkpoint representation and schema references.

Capability absence means unsupported, not false or best-effort support. NEEDLE
may use its tested legacy reconciliation sequence, but it must mark that
attempt as non-atomic and may not claim exactly-once resolution.

The existing `native-v1` and `needle-v1` documents gain additive fields under
their compatibility rules. If the change cannot be made additively, publish a
new contract version instead of silently changing the old one.

## Rationale

Capability negotiation lets the factory canary the new primitive and retain a
safe fallback while preserving reproducible behavior. It also makes the
backend contract, not operational folklore, the authority for feature use.

## Consequences

### Benefits

- Mixed-version fleets fail closed rather than guessing from versions.
- NEEDLE telemetry can record whether a resolution was atomic or reconciled.
- The transition can be canaried and rolled back independently.
- Other consumers can adopt the primitive from the public schema.

### Drawbacks

- Capability and schema fixtures become release-gate artifacts.
- Two NEEDLE paths exist during the migration and both require tests.
- Additive checkpoint compatibility must be proven for old and new readers.

### Alternatives Considered

- **Require bead-rs 0.x.y by version string**: rejected because distribution
  builds and backports make version inference unreliable.
- **Probe by invoking `resolve --help`**: rejected because it does not prove
  schema or semantic support.
- **Flag-day fleet upgrade**: rejected because it enlarges rollback and outage
  risk without improving the contract.

## Implementation

Add capability fixtures and a pinned old/new consumer matrix. Release evidence
must show native and NEEDLE capability output, schema validation, old-client
checkpoint tolerance, new-client fallback, atomic crash/replay behavior, and
successful automatic checkpoint publication.

## Related

- [Current product plan](../plan/plan.md)
- [ADR-010: Store attempt facts, not learning policy](010-store-attempt-facts-not-learning-policy.md)
- [ADR-011: Atomic idempotent attempt resolution](011-atomic-idempotent-attempt-resolution.md)
- `research/specs/needle-cli-contract-v1.md`

## Supersedes

None.


# ADR-011: Resolve an Attempt and Its Lifecycle Transition Atomically

**Status**: Accepted

**Date**: 2026-08-31

**Decision-makers**: bead-rs maintainers and software-factory operators

## Context

bead-rs already provides atomic claims, revisions, leases and fencing, audited
lifecycle mutations, failure-aware scheduling state, and automatic checkpoint
publication. A caller resolving an execution attempt must currently combine
separate operations to record the outcome, change failure state, close or
release the issue, and prove the resulting state. A crash or retry between
those operations can double-count failure or leave outcome history and
lifecycle state inconsistent.

The scheduling service has an internal bead-failure operation, but there is no
public portable attempt receipt that atomically owns the whole transition.

## Decision

Add a versioned service and CLI operation that commits one attempt outcome and
its requested issue transition in one SQLite transaction.

Conceptually:

```text
bead resolve ISSUE_ID \
  --attempt-id ATTEMPT_ID \
  --outcome verified-success|work-failure|infrastructure-failure|cancelled|indeterminate \
  --action close|release|block|quarantine|none \
  --reason REASON \
  [--if-revision REVISION] \
  [--fencing-token TOKEN] \
  [--evidence-ref NAMESPACE:VALUE]...
```

The final command shape is owned by the normative specification; the example
does not bypass that review.

Within one transaction the service must:

1. validate the attempt-outcome schema and bounded metadata;
2. verify issue ownership, expected revision, and fencing token when supplied;
3. reject a previously resolved attempt with a different semantic payload;
4. return the original result without mutation for an identical replay;
5. append an immutable attempt outcome and ordinary audit event;
6. update attempt-tier/retry state only for classifications whose normative
   semantics require it;
7. apply the requested legal lifecycle transition;
8. return the resulting issue state, revision, attempt tier, and receipt ID.

Automatic checkpoint publication occurs only after the transaction commits,
using the existing publication contract. A publication failure is reported as
checkpoint state and must not roll back or repeat the committed semantic
mutation.

An attempt ID is unique within a workspace. Its first successful resolution
binds the complete canonical request hash. Reuse with the same hash is
idempotent; reuse with different content is a conflict.

## Rationale

This is the smallest substrate change that lets NEEDLE obtain exactly-once
durable resolution semantics across crashes and retries. It reuses bead-rs's
existing transactional, revision, fencing, event, scheduling, and checkpoint
primitives rather than creating a second lifecycle system.

## Consequences

### Benefits

- Outcome facts, failure tiers, lifecycle, audit, and checkpoint cannot drift
  through partial application.
- Replay after an unknown client result is safe.
- Infrastructure failure can be recorded without penalizing the issue.
- Consumers receive the authoritative resulting state directly.

### Drawbacks

- Requires a migration, new checkpoint record or compatible event payload,
  service API, CLI surface, schemas, help, and conformance fixtures.
- Outcome vocabulary becomes a public versioned compatibility contract.
- Old clients continue using a multi-operation compatibility path.

### Alternatives Considered

- **Only expose `record-failure`**: rejected because lifecycle and outcome
  receipt could still diverge.
- **Use an atomic bulk manifest assembled by NEEDLE**: rejected as it lacks a
  first-class idempotent attempt identity and portable outcome semantics.
- **Make close itself accept arbitrary attempt metadata**: rejected because
  failure, release, cancellation, and no-op outcomes are equally important.

## Implementation

Implementation artifacts are enumerated in the current plan: normative spec,
model/schema, migration, service transaction, CLI, capabilities, checkpoint
round trip, tests, documentation, and NEEDLE consumer conformance. Every
mutating path must be atomic, auditable, concurrency-tested, and clean-room
traceable.

## Related

- [Current product plan](../plan/plan.md)
- [ADR-010: Store attempt facts, not learning policy](010-store-attempt-facts-not-learning-policy.md)
- [ADR-012: Capability-gated rollout](012-capability-gated-attempt-contract-rollout.md)
- `research/specs/needle-cli-contract-v1.md`
- NEEDLE ADR-024: Attempt, evidence, and resolution are the unit of factory work

## Supersedes

The portable execution-outcome envelope item in the former deferred-feature
list. General mutation idempotency remains a separate possible feature.


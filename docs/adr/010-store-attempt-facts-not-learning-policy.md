# ADR-010: Store Attempt Facts, Not Learning or Orchestration Policy

**Status**: Accepted

**Date**: 2026-08-31

**Decision-makers**: bead-rs maintainers and software-factory operators

## Context

NEEDLE is evolving from bead-oriented process accounting to identified
execution attempts, evidence, and semantic resolutions. Durable attempt facts
must survive orchestrator crashes and be safe under concurrent replay. It
would be tempting to extend bead-rs further into prompt construction,
verification semantics, retrospectives, memory retrieval, lesson confidence,
or policy experiments.

Those concerns change rapidly, depend on the orchestrator and workspace, and
would compromise bead-rs's small deterministic task-state boundary.

## Decision

bead-rs will persist only portable, auditable attempt facts needed to make a
task transition atomic and idempotent. It will not interpret prompts,
evidence quality, agent reasoning, verification policy, memory, lessons,
experiments, or policy authority.

The portable fact boundary may include:

- caller-provided attempt ID and actor identity;
- issue ID, starting/expected revision, lease and fencing information;
- bounded harness/tool identity fields;
- a versioned outcome classification selected by the caller;
- requested lifecycle action and reason;
- opaque, bounded evidence references or receipt identifiers;
- the committed resulting revision, lifecycle state, attempt tier, event, and
  checkpoint generation.

bead-rs validates schema, ownership, revision/fencing, idempotency, and legal
lifecycle transitions. The caller remains responsible for deciding whether
evidence proves success and whether a policy change improved the factory.

## Rationale

This boundary gives orchestrators one durable concurrency-safe fact without
coupling bead-rs to a model provider or learning system. It also preserves the
clean-room and transport-independent architecture: the contract is specified
in bead-rs's own normative schema and exercised through public process
behavior.

## Consequences

### Benefits

- Attempt resolution can be atomic with task state and audit history.
- Different orchestrators can share the primitive without sharing policy.
- Sensitive prompt and memory content stays outside the work store.
- bead-rs remains deterministic and locally testable.

### Drawbacks

- NEEDLE must maintain its own rich traces, evidence bundles, memory catalog,
  and learning evaluation.
- Opaque evidence references can become dangling pointers; bead-rs guarantees
  only their stored identity, not external availability.
- The versioned outcome vocabulary requires compatibility discipline.

### Alternatives Considered

- **Store complete traces and reflections in bead-rs**: rejected because it
  expands the data and privacy boundary and makes task state policy-aware.
- **Keep all attempt facts in NEEDLE files**: rejected as the final task
  transition would retain a crash and replay window outside the authoritative
  store.
- **Encode attempt state in labels or notes**: rejected because it is not a
  typed, atomic, or safely deduplicated contract.

## Implementation

Define an independently reviewed normative `attempt-outcome-v1` specification
and fixtures before implementing the store or CLI changes. Preserve unknown
extension fields in checkpoint round trips only when the specification assigns
that compatibility behavior; do not infer it from another implementation.

## Related

- [Current product plan](../plan/plan.md)
- [ADR-011: Atomic idempotent attempt resolution](011-atomic-idempotent-attempt-resolution.md)
- [ADR-012: Capability-gated rollout](012-capability-gated-attempt-contract-rollout.md)
- NEEDLE ADR-024: Attempt, evidence, and resolution are the unit of factory work

## Supersedes

None.


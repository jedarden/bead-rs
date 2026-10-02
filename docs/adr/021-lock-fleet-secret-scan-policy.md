# ADR-021: Lock secret scanning policy for managed fleets

**Status**: Proposed

**Date**: 2026-10-02

**Decision-makers**: bead-rs maintainers, pending independent review

**Narrows**: ADR-014's workspace modes and fingerprint acknowledgments for
operator-managed deployments.

## Context

ADR-014 defaults to `enforce`, but `.beads/config.json` can select `advisory`
or `off`, and an invocation can acknowledge a blocking fingerprint. Workers
that can edit a workspace can therefore weaken a gate intended to catch their
own accidental disclosures. The existing capability advertises the effective
mode, but observing a downgrade after a write is too late to prevent it.

## Decision

Managed fleet installations use an artifact built with a compiled
`managed-secret-policy` feature. It requires `enforce` mode and refuses
worker-supplied or workspace acknowledgments. Conflicting or malformed
workspace policy fails closed without echoing input. No workspace file or
environment variable can disable the compiled minimum. Capabilities and
doctor report the effective mode and the compiled policy identity.

The general-purpose artifact retains ADR-014's versioned modes and exact
acknowledgments. False-positive resolution for the managed artifact requires
independent specification review; the initial safe remedy is exact redaction.

## Rationale

An environment variable or a policy file inside the agent-writable checkout
is not an authority boundary. The pinned fleet binary can make accidental
downgrades visible and fail closed. A worker with arbitrary
filesystem access or a different binary can still bypass a local scanner;
the Forgejo push gate remains the independent publication boundary.

## Consequences

- Managed deployments must build, pin, and distribute the strict artifact.
  The feature and policy handshake require independent review in
  `secret-write-boundary-v1`.
- Fleet rollout must inventory every workspace and verify capabilities rather
  than infer protection from a default config value.
- Locked policy may reject documentation-shaped false positives until the
  affected range is redacted. A future administrator resolution needs a
  separate reviewed contract.

## Implementation

Define the trust and failure model, build feature, capability schema,
false-positive path, and synthetic tamper cases in the normative contract.
Implement only after exact-hash independent acceptance. Coordinate the Git
transport check with the existing fast-secret-scanner rollout beads; bead-rs
does not shell out to it on the mutation path.

## Related

- ADR-014; ADR-020; R039 and BR-T29, BR-T30, BR-T32
- `secret-scanner` fleet rollout: `fss-3aa3e0b6`, `fss-5f007601`

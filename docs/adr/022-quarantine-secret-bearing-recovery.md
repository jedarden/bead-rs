# ADR-022: Quarantine secret-bearing recovery before publication

**Status**: Proposed

**Date**: 2026-10-02

**Decision-makers**: bead-rs maintainers, pending independent review

**Narrows**: ADR-014's report-only recovery exception and ADR-015's sanitized
redaction publication.

## Context

Restore and import read data that may predate the secret gate. ADR-014 allows
those operations to complete while reporting findings. A caller can also
provide a local artifact that has never been published. Automatic checkpoint
flush can then copy a newly detected blocking value into Git-trackable files.
Refusing all recovery would make historical workspaces unrecoverable; allowing
automatic publication defeats the containment goal.

## Decision

Verified recovery may enter the local store, but newly detected unacknowledged
blocking findings put the workspace into a durable quarantine before the
recovery transaction commits. Quarantine suppresses automatic checkpoint
publication and staging. `sync flush-only` and `sync commit` refuse to publish
the affected semantic state. `sync status` and doctor expose a redacted
reason, count, and remediation path. A fingerprint-selected redaction clears
quarantine only after revalidation of live state and retained generations. The sanitized
publication path remains available to `bead redact`. A separate
administrator-controlled false-positive resolution may be added only after
its own exact contract is reviewed; the initial path uses redaction.

The checkpoint still represents committed state exactly whenever it is
published. Quarantine is an explicit dirty-state interval, not a filtered
checkpoint. Existing source artifacts and Git history cannot be made secret
free by this mechanism; they require containment and rotation as appropriate.

## Rationale

This preserves recovery access while preventing bead-rs from creating a new
Git-trackable copy of a finding. The gate belongs before publication, and its
decision must be durable so a later process cannot flush around it.

## Consequences

- Recovery can succeed locally while publication is held. CLI output must
  distinguish that outcome from an ordinary post-commit publication failure.
- Restore, import-only, reconcile, explicit flush, auto-flush, auto-stage, and
  `sync commit` need one consistent quarantine verdict under concurrency.
- A false positive can require redacting a legitimate text range. An
  administrator resolution would need a separate trusted, audited contract.

## Implementation

The normative contract must define the exact transaction and checkpoint state
machine, restart behavior, interaction with redaction tombstones, and fail
closed handling of scanner errors. Test a new local artifact, legacy
checkpoint findings, interrupted publication, revalidation, and a clean
recovery. Use only synthetic candidates assembled at runtime.

## Related

- ADR-003, ADR-014, ADR-015, ADR-018, ADR-019, ADR-020, ADR-021
- R039 and BR-T29, BR-T30, BR-T33 in `docs/plan/plan.md`

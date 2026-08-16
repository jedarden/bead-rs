# ADR-009: Do not make bead-rs Git-aware to enforce checkpoint/pull ordering

**Status**: Rejected

**Date**: 2026-08-16

**Decision-makers**: bead-rs maintainers

## Context

In Git-transported deployments the checkpoint is committed and shared between
machines, which creates an ordering hazard: flushing a local store over a
checkpoint that has newer content pulled from elsewhere can discard the remote
work. Downstream environment documentation carries this as a standing rule —
never flush a checkpoint before pulling — and it exists purely as operator
discipline, enforced by nothing.

The tempting fix is for `bead-rs` to notice. It knows its workspace root; it
could detect a Git repository, check whether `.beads/` has unpulled upstream
changes, and warn or refuse before flushing.

This ADR records why that was considered and rejected.

## Decision

Rejected. `bead-rs` will not inspect Git state, invoke Git, or condition any
behavior on a repository's remote-tracking status.

The underlying hazard is already owned by **R027**, which detects the resulting
*store state* — a committed, pointer-verified checkpoint ahead of the live
database — without any knowledge of the transport that produced it.

## Rationale

**It contradicts a stated boundary.** R027 states the position explicitly:
"`bead-rs` still never runs Git." That is not incidental. The store is a local
SQLite database plus checkpoint artifacts; Git is one transport among possible
others (a shared filesystem, object storage, rsync, a backup tool). Coupling
correctness to one transport makes every other deployment a second-class case
in which the safety check silently does nothing.

**Detecting the state is strictly better than detecting the transport.** The
condition that actually matters is "the checkpoint contains work the live
database does not." That is observable from the artifacts alone, is true
regardless of how the checkpoint arrived, and is exactly what R027 specifies —
including the taxonomy separating a genuinely remote-advanced pointer from a
covered-ahead-of-live integrity failure. A Git check would be a proxy for this:
strictly less accurate, and wrong in both directions. It would fire on
unrelated `.beads/` churn, and stay silent when a checkpoint arrived by any
non-Git route.

**It would be an unbounded surface.** "Is there unpulled upstream work" is not
one question. It requires handling detached heads, absent or multiple remotes,
shallow clones, submodules, worktrees, unconfigured upstreams, and
authentication failures on any network probe — each a way for a *safety* check
to produce a false verdict, on the path of a routine operation, in a tool whose
value proposition is that mutations are atomic and auditable.

## Consequences

### Benefits

- Keeps the store transport-agnostic; no deployment is second-class.
- Avoids importing Git's failure modes into the flush path.
- Concentrates effort on R027, which fixes the hazard for every transport.

### Drawbacks

- Until R027 ships, the ordering rule remains unenforced operator discipline,
  and the documented "pull before flush" guidance stays load-bearing.
- Operators may reasonably expect a Git-hosted tool to understand Git, and will
  have to be told, once, why it deliberately does not.

### Alternatives Considered

- **Warn but never block on detected unpulled changes**: rejected. A warning
  still requires the full detection surface and all its false verdicts, while
  providing a weaker guarantee than R027's state check.
- **Opt-in Git awareness behind configuration**: rejected. It concentrates the
  risk on exactly the users who enable it believing they are safer, and leaves
  two divergent safety models to specify and test.
- **Document the ordering rule in `bead-rs` itself**: partially accepted — not
  as behavior, but the R027 specification should state the Git-transported
  workflow it reconciles, so the hazard is described somewhere in-project
  rather than only downstream.

## Implementation

None. No code change. Effort belongs on R027.

## Related

- **R027** — remote-advanced checkpoint reconcile; owns this hazard, and states
  the never-runs-Git boundary
- **R028** — fork identity for cloned workspaces; adjacent multi-machine case
- Section 6, JSONL backup and compatibility profiles

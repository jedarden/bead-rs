# ADR-013: Read-Only Git Reachability Reporting Through the Git Binary

**Status**: Accepted (amended by ADR-017)

**Date**: 2026-09-02

**Decision-makers**: bead-rs maintainers

> **Amended 2026-09-13 by [ADR-017](017-gate-ready-to-commit-on-git-reachability.md):**
> the `ready_to_commit` status line now derives from this probe — "ready" and
> *not already committed* — by the parent contract's explicit requirement.
> The clause below still holds where it was scoped: the probe module itself
> stays strictly read-only and conditions nothing; `sync flush-only` keys on
> the internals verdict alone (`checkpoint_consistent`), so publication never
> waits on the transport.

## Context

`bead sync status` answers "ready to commit" from checkpoint internals alone:
generation alignment, root verification, compatibility-view agreement,
tombstones, recorded-state agreement. It says nothing about whether Git can
reach any of it. Observed live on 2026-09-02 in the commitgraph workspace: a
freshly flushed checkpoint reported `Dirty: no / Root: objects/2ab05eb7…
(verified) / Ready to commit: yes` while `git status` showed
`current.json`, `forensic.jsonl`, and `previous.json` modified and the entire
`objects/` directory untracked. The checkpoint was internally consistent and
entirely absent from the only channel that carries it between workers that
share no filesystem.

Plan section 6.2 already assumes this question is answered: it requires
"machine-readable freshness and changed-path information so an external Git
workflow can verify that every checkpoint mutation is included in its commit",
and makes "include every reported path in the same Git commit" a standing rule
of repository automation. The report, however, cannot say whether that
inclusion ever happened — `ready_to_commit` is structurally unable to consider
it, because `bead-rs` shells out to Git nowhere in `src/`.

This collides with a documented boundary. ADR-009 rejected Git awareness for
checkpoint ordering, quoting R027's "`bead-rs` still never runs Git", and the
same wording appears as a user-facing claim in `sync flush-only` and
`sync reconcile` help text. That rejection was about *conditioning behavior*
on repository and remote state — enforcing pull-before-flush ordering, a
safety verdict derived from the transport. The need here is different in kind:
a reporting surface that tells the operator what the transport can see.

The mechanism is a genuine choice, and the planned follow-up work
(auto-staging checkpoint paths) inherits whatever is decided here:

- **Shell out to the `git` binary**: exact Git semantics by construction;
  depends on `git` being on PATH.
- **Rust library** (`gix`, pure Rust; `git2`, libgit2 bindings): no PATH
  dependency; re-derives Git's semantics and adds substantial dependency
  weight to a CLI whose only heavy build today is bundled SQLite.

## Decision

`bead sync status` gains a read-only Git reachability report, first-class in
both text and JSON like the R027 relationship field: whether the checkpoint's
files are committed, staged, unstaged, or untracked, which paths fall in each
bucket, and an explicit unavailable answer — git missing, or no repository
above the workspace — rather than a silent pass. The report never gates a
decision: no command refuses, retries, or mutates because of it.

The absolute boundary is restated, narrowed, not reversed: **bead-rs never
mutates Git state from a read path, never reads remote-tracking or network
state, and never conditions any behavior on the transport that delivered a
checkpoint.** What is newly permitted is a *reporting* read of the local
worktree–index–HEAD relationship. ADR-009's rejection of Git-based ordering
enforcement stands unamended; a reachability line in status output enforces
nothing.

The mechanism is **shelling out to `git`**, never a Git library:

- `git -C <root> --no-optional-locks -c core.fsmonitor=false
  -c core.untrackedCache=false status --porcelain=v1 -z --untracked-files=all
  -- .beads/checkpoint` classifies every pending path.
- `git --no-optional-locks ls-files -- .beads/checkpoint` distinguishes
  tracked from never-tracked.
- `git check-ignore --` distinguishes *unreachable because excluded* (a
  `.gitignore` covering `.beads/` silently defeats the handoff forever) from
  merely not-yet-committed.

Repository discovery is a plain walk up from the workspace root for a `.git`
directory or file, so workspaces outside any repository — the common case in
tests and throwaway directories — never spawn a subprocess at all and get the
unavailable answer for free.

## Rationale

**Git answers the question authoritatively; a library re-derives it.** "Can
Git reach this file" is defined by Git's own exclusion rules, index state,
worktree configuration, worktrees and submodules, and sparse checkouts. A
library implementation re-implements that surface and drifts from it; shelling
out makes Git's verdict the answer by construction. Porcelain v1 is a stable,
documented contract, and `-z` output needs no path quoting.

**The read-only guarantee is stronger with the binary.** `--no-optional-locks`
is Git's own documented switch for read-only inspection: it suppresses the
opportunistic index refresh and lock-taking that a plain `git status` performs.
Disabling `core.fsmonitor` and `core.untrackedCache` for the invocation adds
that no daemon is spawned and no index extension is written. With `git2` or
`gix`, equivalent guarantees would have to be reconstructed and argued from
library internals.

**Dependency weight is real for this project.** `gix` pulls a large pure-Rust
tree; `git2` builds vendored libgit2. Both tax every build — including the
pinned-binary and reproducible-build procedures — to avoid a dependency on a
binary that is present everywhere checkpoints are actually transported by Git.
Where it is absent, the probe degrades to an explicit "unavailable" answer,
which is the honest output, not a failure.

**The follow-up work wants the same family.** Auto-staging checkpoint paths is
a `git add` — a subprocess either way. Settling on the binary now means the
write-side feature does not inherit a library dependency adopted for the
read-side.

**Graceful degradation is required regardless of mechanism.** Workspaces
legitimately exist outside Git, so the probe must be best-effort in any
implementation; the unavailable answer is part of the contract, not an error
path.

## Consequences

### Benefits

- The durability illusion becomes visible at the exact place operators already
  look: "Ready to commit: yes" is joined by whether the checkpoint has actually
  been committed, staged, or is still untracked.
- The ignored-checkpoint case — the one configuration where the handoff can
  never succeed and nothing else reports it — is named explicitly.
- First git dependency lands behind a documented, narrow boundary instead of
  by accident.

### Drawbacks

- The absolute "never runs Git" wording in ADR-003, ADR-009, the R027
  specification, the plan, and two help texts is no longer literally true and
  is narrowed by this ADR in the same change.
- `sync status` now depends on a `git` binary for one line of output; on
  systems without it the line reports unavailable instead of a verdict.
- Porcelain parsing is a stable but external contract; a Git release that
  changed it would degrade the line (to unavailable or a parse-safe answer),
  not the checkpoint semantics around it.

### Alternatives Considered

- **`gix`**: no PATH dependency, but a large dependency tree and re-derived
  status semantics; rejected per the rationale above.
- **`git2`/libgit2**: same re-derivation concern plus a native build; rejected.
- **Report nothing; document that operators must run `git status` themselves**:
  rejected — this is today's behavior and the observed failure.
- **Fold reachability into `ready_to_commit`**: rejected. That field gates
  `flush-only`'s idempotent short-circuit; making it false for an
  uncommitted-but-consistent checkpoint would re-publish on every flush in an
  uncommitted workspace and entangle checkpoint integrity with transport
  state — the coupling ADR-009 rejected. (Revisited and adopted by ADR-017,
  which removes the stated drawback by splitting the internals verdict into
  `checkpoint_consistent` for the short-circuit to key on.)
- **Auto-stage the checkpoint as part of this change**: rejected — a mutation
  of Git state conditioned on a report, exactly what the narrowed boundary
  still forbids without its own decision.

## Implementation

- `src/service/git.rs`: the probe (discovery, three invocations, porcelain
  classification) with unit tests over porcelain parsing.
- `CheckpointStatusReport` carries the result as an optional field so JSON
  consumers see it first-class; `sync status` text renders the reachability
  line and the pending-path buckets.
- Boundary wordings updated in the R027 specification, plan sections that
  state the boundary, and the `sync flush-only` / `sync reconcile` help text,
  each pointing here.
- Integration tests cover: no repository, untracked checkpoint, staged,
  committed, unstaged modification after a later flush, and the
  checkpoint-ignored case.

## Related

- **ADR-009** — rejected Git awareness for *ordering enforcement*; its safety
  decision stands, its absolute wording is narrowed here.
- **ADR-003** — automatic flush; its "never invokes Git" line narrowed here.
- **R027 remote-advanced reconcile** (`research/specs/remote-advanced-reconcile-v1.md`)
  — transport-agnostic reconcile; unchanged.
- Plan section 6.2 — the external Git workflow this report serves.

## Supersedes

The literal "bead-rs never runs Git / never invokes Git / never inspects
repository state" wording of ADR-003, ADR-009, the R027 specification, plan
sections 6.2.1, R026, and R027, and the `sync flush-only` / `sync reconcile`
help text. ADR-009's rejection itself is not superseded.

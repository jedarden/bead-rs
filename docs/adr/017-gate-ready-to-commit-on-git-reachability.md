# ADR-017: Gate `ready_to_commit` on Git Reachability

**Status**: Accepted

**Date**: 2026-09-13

**Decision-makers**: bead-rs maintainers

**Amends**: ADR-013 (the probe's reporting-only boundary; see Reconciliation)

## Context

ADR-013 landed the read-only Git reachability probe and made the durability
illusion visible next to the readiness line — but deliberately left
`ready_to_commit` keyed on checkpoint internals alone, explicitly rejecting
the fold. The rejection had one stated reason: `ready_to_commit` gates
`sync flush-only`'s idempotent short-circuit, so an uncommitted-but-consistent
checkpoint reading `false` there would re-publish on every flush and entangle
checkpoint integrity with transport state — the coupling ADR-009 rejected.

The parent requirement (chain `beadrs-71121ad2`) restores the fold by demanding
the semantics operators already assume: **"Ready to commit" must mean ready
AND not already committed.** The reproduction ADR-013 documented still reads
`yes` after the probe: a consistent, verified checkpoint with `current.json`
and `forensic.jsonl` modified and the whole `objects/` directory untracked
reports `Ready to commit: yes` while Git cannot reach a byte of it. The probe
made that state visible on an adjacent line and left the green light on.

## Decision

**Split the verdict, then gate.** `CheckpointStatusReport` gains
`checkpoint_consistent`: the internals verdict alone (pointer verifies, event
coverage aligned, tombstones resolved, view agrees, recorded state agrees).
`ready_to_commit` becomes the compound: internals **and** Git reachability.

The gate is a pure function, `git::commit_readiness`, so the whole contract is
pinned by unit tests:

- **(a) Consistent and fully committed** — the internals verdict passes
  through unchanged: `ready_to_commit` is yes with no added reason.
- **(b) Consistent with anything uncommitted** — never a bare yes. Any
  staged, unstaged, untracked, or ignored checkpoint path flips the verdict
  to no, and the reason names every pending bucket with its paths, so the
  standing plan-6.2 rule ("include every reported changed path in the same
  Git commit") is executable from the output alone. The `ignored` bucket
  blocks too: it is the one shape the Git handoff can never repair.
- **(c) Probe unavailable** — explicit, never a silent yes. No repository
  above the workspace, or no `git` binary, becomes a not-ready reason
  carrying the probe's own explanation.

Two boundaries are preserved exactly:

- **No published checkpoint does not gate.** `git_reachability` is `None`
  there, and the internals verdict already names the gap ("no checkpoint
  published"). The gate passes that verdict through untouched.
- **`sync flush-only` keys on `checkpoint_consistent`, not on
  `ready_to_commit`.** Publication never waits on the transport: an
  uncommitted-but-consistent checkpoint still short-circuits as "already
  current". This is what dissolves ADR-013's objection — the field whose
  transport-sensitivity was rejected no longer feeds the short-circuit, so
  the fold no longer re-publishes on every flush in an uncommitted workspace.

**Runtime checkpoint metadata never reaches a bucket.** `publish.lock`
exists only while a publication is in flight, is excluded by `bead init`'s
own `*.lock` ignore rule, and would be damage rather than state if it ever
traveled in a pulled checkpoint. The probe carves it out of every input, so
a transient or stale lock can neither read as an unhealable `ignored` gap
(the bucket the gate treats as blocking) nor as untracked work. Published
checkpoint files keep every bucket.

### Reconciliation with ADR-013

ADR-013's clause — "the report never gates a decision: no command refuses,
retries, or mutates because of it" — scoped **the probe module**, and the
module keeps every byte of it: the probe is strictly read-only, shells out
only with `--no-optional-locks` and fsmonitor/untracked-cache disabled,
reads no remote or network state, and no command's control flow inside the
probe changes on its verdict. What changes is downstream: the `sync status`
readiness line now *derives from* the probe by the parent's explicit
requirement. ADR-013's rejected alternative ("fold reachability into
`ready_to_commit`") is adopted here in the only form that survives its own
rationale — with the internals verdict split out so the short-circuit keeps
keying on internals. ADR-009's rejection of Git-based ordering enforcement
stands unamended: the gate reports a handoff gap; it never orders a flush,
a commit, or a pull.

## Rationale

**The compound reading is what the field always promised.** Plan 6.2 makes
`ready_to_commit` the pre-commit gate for repository automation; an
automation that trusts a `yes` which excludes the commit-handoff state is
automation that ships an unreachable checkpoint. The reproduction above is
not hypothetical: it is the steady state of every active workspace between
a flush and its next commit — precisely when automation looks.

**The split beats a separate `committed` line** (the other shape
considered): consumers already read `ready_to_commit` and
`not_ready_reasons`, and the parent requires the *verdict*, not another
informative field — ADR-013 already shipped the informative line. The split
additionally gives `flush-only` an exact key (`checkpoint_consistent`)
instead of recomputing internals from reasons.

**Unavailable must read no.** A silent yes in a repo-less workspace is the
same durability illusion with the probe switched off; the honest answer is
"cannot answer", and a gate that cannot answer does not pass.

**The reason names paths because the remedy needs them.** The documented
remedy for a not-ready checkpoint is to include the reported paths in the
next Git commit; a bare "not committed" reason would leave the consumer to
re-derive what the probe already classified.

## Consequences

- `ready_to_commit` reads **no with a named reason** in every workspace
  whose checkpoint has not reached its Git commit — including workspaces
  outside any repository, where the reason is the probe's unavailability.
  Fixtures that asserted readiness in repo-less tempdirs now establish the
  Git handoff first (init + commit of `.beads/checkpoint`); the assertions
  themselves are unchanged, because under the final contract "ready to
  commit" includes being reachable.
- JSON consumers gain `checkpoint_consistent`. Consumers that want the old
  internals-only verdict — suppression and recovery checks, mainly — read
  it instead of `ready_to_commit`.
- The flush-only idempotence contract is unchanged in behavior and now
  documented against the field it actually keys on.
- Existing workspaces with a pre-`*.lock` `.beads/.gitignore` no longer
  surface a phantom `publish.lock` entry; the carve-out is in the probe, so
  it holds regardless of ignore-rule vintage.

## Implementation

- `src/service/git.rs`: `commit_readiness` (the pure gate) with unit tests
  pinning (a), (b) — including staged-only, ignored, and the parent's
  reproduction shape — and (c); `RUNTIME_CHECKPOINT_FILES` carve-out with
  its two tests.
- `src/service/checkpoint.rs`: `checkpoint_consistent` field;
  `gate_readiness` applies the gate on every `forensic_checkpoint_status`
  return path.
- `src/main.rs`: `cmd_sync_flush_only` short-circuits on
  `checkpoint_consistent`; status text documents the folded verdict.
- `src/cli.rs`: `sync status` help states the five-part internals contract
  plus the reachability clause; `sync flush-only` help names the read-only
  probe; JSON field list gains `checkpoint_consistent`.
- Integration fixtures updated to establish the Git handoff before
  asserting readiness: `checkpoint_tombstones`, `checkpoint_mutation_guard`,
  `checkpoint_publication_lock`, `atomic_dependent_creation`,
  `r027_remote_advanced_reconcile`, `concurrency_recovery_variants`,
  `post_commit_publication`, `r033_bulk_manifests`.
- `research/specs/needle-cli-contract-v1.md`: the "Sync status Git
  reachability" section states the final contract.

## Related

- **ADR-013** — the probe; its reporting-only clause is scoped to the
  module and restated above; its rejected alternative is adopted with the
  split that removes its drawback.
- **ADR-009** — no Git awareness for checkpoint ordering; stands unamended.
- **ADR-003** — automatic flush; unaffected: publication decisions key on
  checkpoint internals only.
- Plan section 6.2 — the external Git workflow the gate serves.

# ADR-019: The Explicit `sync commit` Command

**Status**: Accepted

**Date**: 2026-09-14

**Decision-makers**: bead-rs maintainers

**Narrows**: ADR-018 (auto-staging stops at the index; this adds the
commit half that staging deliberately stopped short of) and ADR-003 (the
rejection of automatic Git publication now has a sanctioned *explicit*
counterpart, which is not the same as an automatic one)

## Context

Publication is automatic (R026) and staging the verified fileset is
automatic (ADR-018), but the commit that carries the checkpoint to other
machines stayed manual — and committing a checkpoint by hand fails in
exactly the ways ADR-018 catalogued: a bare pathspec omits freshly written
untracked objects, compaction stales a hand-typed pathspec, and on a
shared checkout the wrong invocation sweeps unrelated staged work into
history (commit 780243a in commitgraph).

The obvious remedy — commit automatically after every publication — was
considered and rejected (bead `beadrs-b63a06e5`), each reason observed on
2026-09-02 rather than hypothesised:

1. **Pre-commit gates.** Checkpoint commits were blocked for hours by a
   tree-wide Definition of Done gate failing on an unrelated Go
   regression. An automatic committer must either be hostage to unrelated
   code health or bypass the gate; bypassing is precisely what lets
   broken code land.
2. **Shared checkouts.** Several workers mutate `.beads` concurrently on
   one working tree. Committing on every mutation contends for the index
   lock with agents' own staging — the race that swept an unrelated file
   into commit 780243a.
3. **Committing is policy, not storage.** Branch, detached HEAD,
   mid-rebase or mid-merge state, authorship and message are decisions the
   tool would have to guess, and guessing wrong rewrites shared history.
4. **A commit alone does not propagate.** Without a push the checkpoint
   still never leaves the machine, and automatic pushing is a
   substantially larger hammer.

Meanwhile `sync status` (ADR-013, ADR-017) already answers "ready to
commit" precisely, and reporting the gap does not assemble the commit: the
worker still has to reconstruct the exact set by hand under time pressure.

## Decision

`bead sync commit` — one explicit subcommand that keeps the decision with
whoever owns the branch and removes the *mechanical* ways to get the
commit wrong:

- **Refuse before touching anything.** No published checkpoint; a dirty
  checkpoint (the live store has unflushed work); a remote-advanced
  checkpoint (a pull delivered work the store has not reconciled); a
  covered-ahead integrity failure; an internally inconsistent checkpoint;
  a detached HEAD (the commit would be reachable from no branch — Git
  itself allows the shape, so this guard is the only backstop); Git
  unavailability; and checkpoint files an ignore rule excludes. Every
  refusal names its remedy and keys on the same
  `CheckpointStatusReport` `sync status` prints, so the command and the
  report cannot disagree. A referenced file missing from disk is
  checkpoint damage and refuses the commit rather than enshrine the
  damage in history — status verifies only the root object, so this gate
  is the commit command's own.
- **Stage the verified set.** The fileset the current generation makes
  authoritative — both pointers, every object either pointer still
  references, the compatibility view when one exists, and the removal of
  every tombstoned object Git tracks — staged through ADR-018's staging
  module. Unlike publication's best-effort staging, a staging failure
  here is fatal: committing without the verified set staged is exactly
  the damage this command exists to prevent.
- **Commit with a bead-only pathspec.** `git commit -- <verified set>`
  records exactly those paths. A pathspec commit ignores every other
  staged path — Git itself leaves it staged — so another worker's
  in-flight index on a shared checkout survives untouched.
- **Never bypass the gates.** No `--no-verify`: a pre-commit hook that
  rejects the commit rejects it here too, and its output surfaces
  verbatim. The command never creates branches and never pushes; the only
  policy it supplies is the message, which `--message` overrides (the
  default names the published generation in the `chore(beads): …
  [bead-rs]` convention this repository's history already uses).
- **Idempotent.** Against a consistent checkpoint whose verified set Git
  already reaches, it commits nothing and exits 0 — the same short-circuit
  `sync flush-only` offers on the publication side.

The mechanism is shelling out to `git` per ADR-013's settled choice, with
fsmonitor and the untracked cache disabled for every invocation.

The workspace operation lock deliberately does **not** cover the command:
it mutates Git, never beads state, and holding the lock across a
potentially slow pre-commit gate would queue every other worker's
mutations behind it — the hostage shape reason 1 rejects. The publication
it might race is benign: the verified set is re-read from the artifacts,
and a superseding generation is consistent by construction.

## Rationale

Staging removed the error class *at the source*, but only for whoever
commits next, and only when they invoke Git over the index. The residual
gap is the commit itself, and no automatic mechanism can cross it without
reopening all four objections above. An explicit command crosses it with
the operator present: the gates fire before anything is touched, the
pathspec is computed from the pointers rather than typed, and the
unrelated-staging sweep is structurally impossible rather than avoided by
care.

## Consequences

- The Git-transported workflow becomes: mutate (auto-flush), `bead sync
  commit`, push. Every step is explicit except publication, whose
  automaticity ADR-003 settled.
- A rejecting pre-commit gate fails the command; the operator resolves the
  gate or commits by hand. This is the intended outcome, not a defect: the
  gate holds exactly as much authority over checkpoint commits as over
  any other commit.
- The command does not push. A committed-but-unpushed checkpoint still
  does not propagate; `sync status` on the far side keeps reporting
  whatever it can see, and pushing remains the operator's act.
- Workspaces that gitignore the checkpoint are refused with the remedy
  (amend the rule or commit by hand); the command never stages through an
  ignore rule.
- `--dry-run` reports the prospective staging set and commit decision
  without touching the index or history, for exactly the "what would this
  do" moments a shared checkout generates.

## Implementation

- `service::git_commit::commit_verified_checkpoint` — the gates, the
  verified-set assembly (`read_pointer_referenced_files` over both
  pointers, reused from publication), staging through
  `git_stage::stage_published_checkpoint`, the pathspec commit, and the
  idempotent short-circuit.
- `SyncCommand::Commit` / `SyncCommitOptions` — the CLI surface,
  `--message` and `--dry-run`.
- `cli_checkpoint_guard::policy` — `sync commit` classifies read-only
  against the workspace's beads state.
- `tests/sync_commit.rs` — the CLI-boundary contract: exact-fileset
  commits, the shared-index survival property, each refusal, idempotence,
  dry-run, the pre-commit-hook case, and the message rules.

## Related

- ADR-003 (automatic flush, no automatic Git publication), ADR-013
  (read-only reachability probe), ADR-017 (readiness gate), ADR-018
  (auto-stage the published fileset)
- Parent requirement: bead `beadrs-b63a06e5`

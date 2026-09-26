# NEEDLE CLI contract v1

Status: draft normative consumer contract.

This contract describes the bead-store behavior required by NEEDLE. Command
spellings are public process-boundary facts. It does not prescribe internal
implementation.

## Process rules

- Commands execute relative to a workspace containing `.beads/`.
- Successful machine-readable commands write valid UTF-8 to stdout and exit 0.
- Diagnostics go to stderr and never corrupt JSON stdout.
- Failures exit nonzero; an empty queue is a successful domain result.
- Mutations are durable before exit 0.

## Required commands

| Operation | Invocation shape | Required result |
| --- | --- | --- |
| Version | `--version` | Nonempty name and semantic version |
| List all | `list --json --limit 999999` | JSON array or one JSON object per line |
| List open | `list --json --status open --limit 999999 [--assignee VALUE]` | Same record stream |
| Show | `show ID --json` | Nonempty JSON array or record stream whose first record is the issue |
| Claim | `claim [--model M] [--harness H] [--harness-version V] --assignee A --json` | JSON object containing `bead_id`; selection and assignment are one atomic transaction |
| Update | `update ID [--status S] [--assignee A] [--notes N]` | Mutation committed before success |
| Reopen | `reopen ID` | Issue becomes open according to lifecycle rules |
| Close | `close ID --reason TEXT` | Issue becomes finished and retains reason |
| Create | `create --title T --description D [--label L]...` | stdout contains the new ID only |
| Add label | `label add ID --label L` | Idempotent label presence |
| Remove label | `label remove ID --label L` | Idempotent label absence |
| Add dependency | `dep add BLOCKED BLOCKER --kind blocks` | BLOCKER prevents BLOCKED readiness |
| Remove dependency | `dep remove BLOCKED BLOCKER` | Matching edge removed |
| Flush | `sync --flush-only` | Committed state checkpointed to `.beads/issues.jsonl` |
| Import | `sync --import-only` | Valid checkpoint reconciled into native state |
| Check | `doctor` | Human-readable lines; warnings begin `WARN ` |
| Repair | `doctor --repair` | Repairs only diagnosed conditions; repaired lines begin `FIXED ` |

## Dependency kinds

`--kind` on `dep add` accepts `blocks`, `relates_to`, or `verifies`, and
`capabilities` advertises the same set as `dependency_kinds`. Only `blocks`
affects eligibility, so the required result in the table above is unchanged.
`relates_to` and `verifies` never change readiness, and cycles among
non-`blocks` edges are accepted; a `blocks` edge that would close a directed
cycle is still rejected, as is any self-edge.

`verifies` (ADR-001) declares that BLOCKER checks the work BLOCKED performs. A
`blocks` edge whose blocker also carries a `verifies` edge to the same blocked
bead is legal at insert and is never rejected; `doctor` reports each such pair
as an advisory warning that names the remedy (`bead dep remove BLOCKED BLOCKER
--kind blocks` or `--kind verifies`). The relationship is taken only from the
declared edge — titles are never inspected. Declared kinds survive the
checkpoint round trip verbatim; unknown kinds stay preservable in the
checkpoint but fail closed for native mutation.

## Issue JSON minimum

NEEDLE requires `id`, `title`, `description`, `priority`, `status`, `assignee`,
`dependencies`, `created_at`, and `updated_at`. Labels must be an array when
present. Status output must use one of `open`, `in_progress`, `done`, `closed`,
`completed`, `blocked`, or `deferred` for this contract version.

Additional fields are allowed. A single record must not be duplicated in one
response.

## Claim semantics

- Only ready, open work is eligible.
- Concurrent successful claim calls receive distinct issue identifiers.
- Assignment to the requested actor is committed with selection.
- With no eligible work, return exit 0 and a JSON object without a nonempty
  `bead_id`.
- Model and harness values are telemetry hints and do not change correctness.

## Store layout

The workspace contains `.beads/`. `issues.jsonl` is the portable checkpoint.
The native database filename may be `beads.db` for initial NEEDLE health-check
compatibility, but consumers must not use its schema as an API.

## Sync status Git reachability

`bead sync status` reports whether Git can reach each published file under
`.beads/checkpoint`. The text form starts with `Git: STATUS`, then prints the
`committed`, `staged`, `unstaged`, `untracked`, and `ignored` groups in that
order. Each group carries its file count followed by every workspace-relative
path in the group, including when a path has both a staged and an unstaged
disposition. Empty groups still print their zero count.

When Git cannot answer, the text form prints the single line
`Git: unavailable: WHY` instead of the groups. A missing `git` binary and a
workspace outside a Git repository are availability results, not command
failures: `sync status` still exits 0. The JSON form mirrors this contract in
`git_reachability`, with `status`, optional `unavailable_reason`, and arrays
named for the five groups. `git_reachability` is `null` when no checkpoint has
been published.

Readiness is gated on reachability (ADR-017): `ready_to_commit` is true only
when the checkpoint is internally consistent AND Git can reach every
published file. Anything staged, unstaged, untracked, or ignored under
`.beads/checkpoint` holds the verdict at false, with each pending bucket and
its paths named in `not_ready_reasons`; an unavailable probe is likewise
explicitly not ready, carrying the probe's own explanation — never a silent
yes. No published checkpoint does not gate: the internals verdict already
names that gap. `checkpoint_consistent` in the JSON carries the
internals-only verdict for consumers that must not depend on the transport —
`sync flush-only`'s idempotent short-circuit keys on it, so an
uncommitted-but-consistent checkpoint publishes nothing on a re-flush.

## Capability handshake extension

The native integration adds:

```text
bead capabilities --format json --profile needle-v1
```

It returns a versioned object declaring atomic claim, lifecycle values,
checkpoint modes, supported commands, and store-layout version. This extension
does not replace the required v1 commands until NEEDLE ships a native adapter.


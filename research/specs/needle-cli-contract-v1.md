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
| Add dependency | `dep add BLOCKED BLOCKER --type blocks` | BLOCKER prevents BLOCKED readiness |
| Remove dependency | `dep remove BLOCKED BLOCKER` | Matching edge removed |
| Flush | `sync --flush-only` | Committed state checkpointed to `.beads/issues.jsonl` |
| Import | `sync --import-only` | Valid checkpoint reconciled into native state |
| Check | `doctor` | Human-readable lines; warnings begin `WARN ` |
| Repair | `doctor --repair` | Repairs only diagnosed conditions; repaired lines begin `FIXED ` |

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

## Capability handshake extension

The native integration adds:

```text
bead capabilities --format json --profile needle-v1
```

It returns a versioned object declaring atomic claim, lifecycle values,
checkpoint modes, supported commands, and store-layout version. This extension
does not replace the required v1 commands until NEEDLE ships a native adapter.


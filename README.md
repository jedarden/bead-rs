# bead-rs

`bead-rs` is a clean-room Rust task-coordination system for agent fleets. It
keeps a dependency graph of work items ("beads") in SQLite, hands out exactly
one unblocked bead per request through an atomic claim, and checkpoints the
whole workspace to deterministic JSONL that Git can track.

The installed binary is named `bead`.

![The bead-rs lifecycle: beads, a blocking edge, the ready frontier, an atomic claim, and the checkpoint](docs/img/bead-lifecycle.gif)

Work items form a directed acyclic graph. The **ready frontier** is the set of
beads with nothing left blocking them — open, unassigned, not manually blocked,
and with every `blocks` edge already closed. Agents call `bead claim`, which
selects from that frontier and assigns the bead in a single transaction, so
concurrent claimants never receive the same bead. Closing a bead can expose its
dependents, which advances the frontier.

## Install

```bash
cargo install --path .          # installs the `bead` binary
bead --version
```

Requires Rust 1.75 or newer. SQLite is bundled; there is no system dependency
and no network access at runtime.

## Quick start

```bash
bead init --prefix demo

design=$(bead create --title "Design schema" --priority 1)
store=$(bead create --title "Implement store" --priority 2)
bead create --title "Write docs" --priority 3

# `store` cannot start until `design` is closed.
bead dep add "$store" "$design"

bead list --ready                       # design + docs; store is blocked
bead claim --assignee worker-1          # atomically takes the highest-priority ready bead
bead close "$design" --reason "Schema agreed"
bead list --ready                       # store has now joined the frontier

bead sync flush-only                    # write the checkpoint Git will track
```

![Terminal recording of the quick start: init, three beads, a dependency, the ready frontier, a claim, a close, and a checkpoint flush](docs/img/bead-workflow.gif)

That recording is the sequence above, run against the real binary — note that
`store` is absent from the first `list --ready` because it is blocked, and
present in the second because closing `design` satisfied its only edge.

`bead why --id <ID>` explains any bead's state: whether it is ready, what is
blocking it, how it ranks for claiming, and which operations are currently
legal.

## State model

Two artifacts, with different jobs:

| Path | Role | Committed |
| --- | --- | --- |
| `.beads/beads.db` | SQLite, authoritative live state | No |
| `.beads/checkpoint/` | Deterministic JSONL checkpoint | Yes |

The checkpoint carries issues, the event history, provenance receipts, and the
dependency and label graph. **Nothing flushes implicitly.** A checkpoint is only
as current as the last explicit `bead sync flush-only`, so flush before
committing and periodically during long sessions — otherwise a clone of the
repository reproduces the last flushed state rather than the current one.

Recovering a fresh clone, which arrives with a checkpoint but no database:

```bash
bead init
bead sync import-only --input .beads/checkpoint --restore-into-empty --actor "$USER"
bead doctor
```

`bead doctor` runs read-only integrity checks across store, backup, schema,
dependencies, and comments; `--repair` performs only safe, non-speculative
repairs, and `--rehearse` proves the recovery path by restoring into a throwaway
workspace and comparing the result for semantic equivalence.

## Output contracts

- `--json` on `list`, `changes`, and `query --output-json` emits **NDJSON** —
  one compact object per line, not a JSON array.
- `bead show --json` emits a **one-element array**, which is what NEEDLE's
  subprocess contract expects.
- `bead create` prints only the new bead ID and a newline, so it can be captured
  directly into a shell variable.
- `bead claim` on an empty frontier is success, not failure: exit 0 with `{}`.
- `bead capabilities` emits a machine-readable contract document for feature
  negotiation.

Exit codes are stable across every command:

| Code | Meaning |
| --- | --- |
| 0 | Success |
| 1 | Internal failure |
| 2 | CLI usage or validation error |
| 3 | Workspace, issue, or file not found |
| 4 | Conflict — invalid transition, revision guard, dependency cycle |
| 5 | Malformed input or integrity failure |

## Coordinating a fleet

![Many agents claiming from one ready frontier, backed by SQLite and a checkpoint](docs/img/how-bead-rs-works.png)

- **Atomic claim.** Selection and assignment share one transaction under the
  `fifo-v1` policy (priority ASC, created_at ASC, id ASC). `aging-v1`,
  `impact-v1`, `rotation-v1`, and `balanced-v1` are also available via
  `--policy`.
- **Leases.** `bead claim --lease-ttl SECONDS` issues a lease with a
  monotonically increasing fencing token, so a crashed or partitioned worker
  cannot mutate work that has since been reassigned.
- **Revision guards.** `--if-revision N` on `update`, `release`, `close`, and
  `reopen` gives optimistic concurrency control; a stale revision fails with
  exit 4 rather than silently losing an update.
- **Inspect without reserving.** `bead list --ready` uses the same ordering as
  `claim` but takes nothing off the frontier.
- **Change feed.** `bead changes --since <CURSOR>` lets a consumer catch up
  incrementally instead of rescanning.

## Interoperability

bead-rs keeps a private native schema and interoperates through versioned
interchange profiles rather than by writing to another tool's live database:

- `native-v1` — full fidelity, the default
- `needle-v1` — NEEDLE subprocess compatibility
- `br-v1`, `bf-v1` — external compatibility formats

`bead migrate --from <P> --to <P> --input <F> --output <F>` transforms a
checkpoint between profiles and emits a canonical receipt recording hashes,
counts, and any lossy transformations. `bead compare --id <ID> --source <P>
--target <P>` reports which fields a given profile pair preserves, transforms,
omits, or cannot represent. Unknown fields survive round trips.

See [interoperability notes](docs/notes/interoperability-architecture.md) and
[NEEDLE compatibility](docs/notes/needle-compatibility.md).

## CLI conventions worth knowing

bead-rs is an independent implementation, not a drop-in replacement for any
other task tracker. These conventions catch people out:

| | |
| --- | --- |
| The binary is `bead` | not the crate name `bead-rs` |
| `create` takes a flag | `bead create --title "Title"`, not a positional title |
| Ready work is a filter | `bead list --ready`, not a `ready` command |
| `flush-only` is a subcommand | `bead sync flush-only`, not `bead sync --flush-only` |
| `--json` emits NDJSON | one object per line, not an array (except `show`, which emits a one-element array) |

## Documentation

- `bead --help` for the command inventory; `bead <COMMAND> --help` for the full
  description, examples, and semantics of any command.
- Man pages covering the whole command tree (45 pages, named `bead.1`,
  `bead-create.1`, `bead-sync-flush-only.1`, …) are generated with
  `cargo run --bin generate-man-pages`. See [MAN_PAGES.md](MAN_PAGES.md).
- The [0.1 implementation plan](docs/plan/plan.md) defines the native schema,
  lifecycle, dependency, checkpoint, CLI, and verification design.
- Post-0.1 candidates and rejected alternatives live in the
  [ideas ledger](docs/notes/ideas-ledger.md).

Both animations are generated from committed sources, so they can be refreshed
rather than redrawn:

```bash
# lifecycle animation (needs cairosvg + Pillow)
python3 docs/img/generate-lifecycle-animation.py

# terminal screencast (needs vhs, which needs ttyd + ffmpeg)
cargo install --path . && vhs docs/img/bead-workflow.tape
```

The screencast drives the real binary, so it cannot quietly disagree with the
CLI: if behaviour changes, re-running the tape either shows the change or
visibly fails.

The lifecycle animation is timed against the classical animation principles,
used to carry meaning rather than decoration — the claim fires in 0.2s against
0.4–0.6s elsewhere because atomicity is the point; staging dims whatever the
current beat is not about; and closing `design` sends a ripple along the edge
so that `store` turning ready reads as a consequence rather than a coincidence.
The generator documents the mapping in full.

Every `bead ...` example printed in the help text is parsed by the real CLI in
the test suite, so a documented invocation cannot drift from the interface it
documents.

## Project status

The core CLI is implemented and in use: workspace lifecycle, issue CRUD,
labels, the dependency graph, atomic and leased claims, checkpoint flush and
restore, diagnostics, the query language, the change feed, profiles, and
migration. It has been exercised end to end by building a real multi-bead
project through it.

Treat it as young software rather than settled: it is a 0.1, its history
includes bugs that a green test suite did not catch, and it is worth running
`bead doctor` and a hands-on smoke test of the path you care about before
depending on it for anything critical.

## Independence

`bead-rs` has an independent Git history and is implemented from the
specifications committed to this repository. Clean-room contributors must
follow [AGENTS.md](AGENTS.md) and [PROVENANCE.md](PROVENANCE.md).

`bead-rs` is not affiliated with or endorsed by any other bead implementation.

## License

Apache License 2.0. See [LICENSE](LICENSE) and [NOTICE](NOTICE).

# bead-rs release notes

Newest release first. Each entry names the release's behavior change, the
documentation that moved with it, and anything the release requires of
environments outside this repository.

## R026 activation — automatic checkpoint publication (2026-08-21, plan revision 8)

Shipped in the activation commit `ed4fb3d` (2026-08-21 19:49 -0400), which
records the plan section 13 gate evidence and the ADR-003 status change in
the same tree it flips the compiled default.

**Behavior.** Every successful mutation publishes the checkpoint
automatically after its transaction commits, so the checkpoint is never
silently behind the database and a clone of the repository reproduces the
current state. `bead sync flush-only` remains an explicit idempotent check —
against a current checkpoint it publishes nothing and exits 0. Two hatches
suppress automatic publication and leave the checkpoint to be advanced by
that explicit command: `--no-auto-flush` for one invocation, and
`checkpoint.auto_flush: false` in `.beads/config.json` durably. A stale
checkpoint now means publication was suppressed or failed after a committed
mutation, not that someone forgot a command.

**Documentation reversed with the flip.** README, root and subcommand help in
`src/cli.rs`, plan sections 5.3 and 6.2.1, the generated man pages, AGENTS.md,
and the lifecycle animation (caption and README alt text) all state the
automatic-publication contract. Man pages are regenerated from the help tree;
regeneration was verified byte-reproducible at the closing gate — two
independent `cargo run --bin generate-man-pages` runs from a clean tree
produce byte-identical output across all 50 pages.

**Wording retained by design.** The final gate sweep for the
never-implicit-flush wording families (`flush implicit`, `flushes implicitly`,
`implicit flush`, `implicitly flush`, `never flush`, `nothing flush`) finds
exactly two matches in the repository, both classified by the wording
inventory and retained:

- `docs/adr/009` — "never flush a checkpoint before pulling" is the separate
  pull-before-flush ordering rule for multi-machine workspaces; it is
  orthogonal to implicit versus automatic and survives.
- `docs/adr/003` Context — "nothing flushes implicitly" quotes the pre-flip
  contract this ADR ended; it is decision history, and the ADR's status header
  records the activation.

Plan revision-history text describing the pre-flip explicit-flush default
remains as history and matches none of the wording families verbatim.

**Environments outside this repository.** The surrounding environment's
agent instructions — the home `CLAUDE.md` at `/home/coding/CLAUDE.md` —
carried the never-implicit-flush rule and required a corresponding update
that this repository cannot make for it; this entry records that obligation
explicitly rather than leaving it silently stale. Verified at the closing
gate (2026-08-22): that file now carries the post-flip authority model —
automatic publication after every successful mutation, the two suppression
hatches, and `bead sync flush-only` as the idempotent check — with its one
remaining "nothing flushes implicitly" mention correctly scoped to binaries
built before the activation. Its "never flush a checkpoint before `git
pull`" line is the ADR-009 ordering rule and is expected to survive. The
external update has therefore landed; any workspace still running a
pre-activation binary must keep treating explicit `bead sync flush-only` as
the only publication path.

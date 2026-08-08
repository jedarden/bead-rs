# bead-rs

`bead-rs` is a clean-room Rust task-coordination system for agent fleets. Its
native design targets atomic work claiming, durable SQLite state, Git-friendly
interchange, and compatibility with NEEDLE.

The installed binary is named `bead`.

## Project status

This repository is currently a specification and implementation scaffold. It
is not yet a usable task tracker. Commands will be implemented only after the
corresponding independent specification and conformance fixtures are accepted.

## Compatibility goals

- Native compatibility with NEEDLE's bead-store operations.
- Loss-aware import and export profiles for `beads_rust` and `bead-forge`.
- A private native SQLite schema; interoperability occurs through versioned
  interchange and CLI contracts rather than by mutating another tool's live
  database.
- Preservation of unknown interchange fields during round trips.

See [the research index](research/README.md) and
[interoperability notes](docs/notes/interoperability-architecture.md). The
[0.1 implementation plan](docs/plan/plan.md) defines the independent native
schema, lifecycle, dependency, checkpoint, CLI, and verification design.

## Marathon Coding

The first implementation is designed to run under the independently developed
Marathon Coding harness. The committed mission, feature ledger, progress log,
and launch wrapper live under [`.marathon/`](.marathon/README.md).

The harness must run in an isolated environment that can access this
repository and approved Rust resources, but cannot access source or session
history from other bead implementations.

## Independence

`bead-rs` has an independent Git history and is implemented from the
specifications committed to this repository. Clean-room contributors must
follow [AGENTS.md](AGENTS.md) and [PROVENANCE.md](PROVENANCE.md).

`bead-rs` is not affiliated with or endorsed by `beads_rust` or `bead-forge`.

## License

Apache License 2.0. See [LICENSE](LICENSE) and [NOTICE](NOTICE).

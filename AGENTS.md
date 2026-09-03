# bead-rs clean-room development guide

This repository is an independent implementation. Protect its provenance as
carefully as its correctness.

## Clean-room boundary

- Implement only from files under `research/specs/`, independently authored
  fixtures under `research/fixtures/`, public standards, and dependency API
  documentation.
- Do not inspect, copy, translate, paraphrase, or diff source code from any
  other bead implementation while authoring implementation code.
- Do not copy their tests, fixtures, SQL, comments, help prose, error prose, or
  internal names.
- Observable behavior may be recorded by a separate specification activity.
  Only sanitized behavioral facts belong in `research/specs/`.
- If contaminated material is accidentally viewed, stop work on the affected
  component and record the event in `PROVENANCE.md` before continuing.

## Compatibility

- `.beads/issues.jsonl` is an interchange checkpoint, not the native live
  store.
- Never write another implementation's live SQLite database.
- Unknown JSON fields must survive import/export round trips.
- Compatibility profiles are explicit and versioned; native behavior must not
  silently change to emulate a legacy profile.
- NEEDLE compatibility is defined by
  `research/specs/needle-cli-contract-v1.md`.

## Engineering

- Rust 1.85 is the MSRV (see [ADR-004](docs/adr/004-raise-msrv-to-1.85-and-edition-2024.md)).
- Avoid `unsafe` code.
- Use structured errors and nonzero exit codes for failures.
- Every mutating operation must be atomic, auditable, and concurrency-tested.
- Every successful mutation publishes the Git-tracked checkpoint
  automatically after its transaction commits. `bead sync flush-only` is an
  explicit idempotent check, and `--no-auto-flush` or `checkpoint.auto_flush`
  in `.beads/config.json` suppresses publication for one invocation or
  durably.
- Never claim compatibility without passing the corresponding conformance
  suite.
- Preserve unrelated and untracked work. Never force-push.
- Pinned binaries are built from a git-archive extraction in scratch via
  `scripts/build-from-archive.sh <sha>` — never by stashing, resetting, or
  checking out commits inside this shared checkout; the script is the only
  sanctioned way to build one (see [BUILD_PROCEDURE.md](BUILD_PROCEDURE.md)).

## Verification

Run at minimum:

```text
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

## Marathon Coding

- `.marathon/instruction.md` is the live mission read at every iteration.
- `.marathon/feature_list.json` is the release ledger. Change only a feature's
  `passes` value and its evidence after the stated verification succeeds.
- Append durable handoffs to `.marathon/progress.md`; do not rewrite history.
- Commit one coherent, verified increment per iteration.
- `.marathon/COMPLETE` may be created only after every feature passes and all
  release gates in the mission succeed.
- Publishing to crates.io is intentionally excluded from autonomous authority.

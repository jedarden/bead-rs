# bead-rs clean-room development guide

This repository is an independent implementation. Protect its provenance as
carefully as its correctness.

## Clean-room boundary

- Implement only from files under `research/specs/`, independently authored
  fixtures under `research/fixtures/`, public standards, and dependency API
  documentation.
- Do not inspect, copy, translate, paraphrase, or diff source code from
  `beads_rust`, `bead-forge`, or another bead implementation while authoring
  implementation code.
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

- Rust 1.75 is the initial MSRV.
- Avoid `unsafe` code.
- Use structured errors and nonzero exit codes for failures.
- Every mutating operation must be atomic, auditable, and concurrency-tested.
- Never claim compatibility without passing the corresponding conformance
  suite.
- Preserve unrelated and untracked work. Never force-push.

## Verification

Run at minimum:

```text
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```


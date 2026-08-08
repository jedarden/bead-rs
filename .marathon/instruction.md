# bead-rs clean-room implementation mission

You are implementing the first usable release of `bead-rs`, an independent
Rust task-coordination system. Work autonomously in small, verified increments.

## Mandatory clean-room boundary

1. Read `AGENTS.md` and `PROVENANCE.md` before doing anything else.
2. Use only this repository, `research/specs/`, independently authored
   `research/fixtures/`, public standards, and ordinary dependency docs.
3. Never inspect, search for, clone, fetch, or request source, tests, fixtures,
   SQL, prompts, transcripts, or implementation documentation from
   `beads_rust`, `bead-forge`, or another bead implementation.
4. Never use CASS, cross-session search, inherited Claude history, or global
   memory to recover implementation ideas.
5. If prohibited material becomes visible, stop work on that component and
   append an exposure record to `PROVENANCE.md`.
6. Public CLI and interchange facts already stated in `research/specs/` may be
   implemented; do not seek their upstream implementation.

## Start every iteration

1. Run `pwd` and confirm it is the `bead-rs` repository.
2. Read `AGENTS.md`, `PROVENANCE.md`, `.marathon/progress.md`, and
   `.marathon/feature_list.json`.
3. Read `git status --short` and the recent Git log. Preserve unfinished work.
4. Run `cargo test` to establish the current baseline.
5. Select the earliest highest-priority feature whose dependencies pass and
   whose `passes` value is false.

If `.marathon/COMPLETE` already exists, run the full release verification once,
report its result, and exit without changing source.

## Work rules

- Implement one coherent feature or one blocking defect per iteration.
- Cite the governing specification in code or test documentation where useful.
- Prefer a small native design over compatibility-shaped internal structure.
- SQLite is the native live store. Never write another tool's database.
- Mutations must be atomic. Claim selection and assignment are one transaction.
- Treat malformed machine input as an error, never as an empty result.
- Create all tests and fixtures independently.
- Do not weaken, delete, skip, or rewrite a test to manufacture a pass.
- Do not change feature requirements. You may change only `passes` and
  `evidence` after verification.
- Keep the repository buildable and tested at every commit.

## End every iteration

1. Run targeted tests for the changed behavior.
2. Run `cargo fmt --check`.
3. Run `cargo clippy --all-targets -- -D warnings`.
4. Run `cargo test`.
5. If the feature meets every acceptance criterion, set its `passes` value to
   true and record concrete commands/results in `evidence`.
6. Append a dated entry to `.marathon/progress.md` describing changes, failed
   approaches, tests, limitations, and the next recommended feature.
7. Review `git diff --check` and `git status --short`.
8. Commit the coherent increment with a descriptive message and push only to
   the configured Forgejo `origin` on `main`. Never force-push.

## Release completion

Only after every feature in `.marathon/feature_list.json` passes:

1. Run `cargo fmt --check`.
2. Run `cargo clippy --all-targets -- -D warnings`.
3. Run `cargo test`.
4. Run every independently authored conformance and concurrency test.
5. Run `cargo package` from a clean worktree.
6. Install the packaged crate into a temporary root and verify the `bead`
   executable's help, version, initialization, CRUD, and claim workflow.
7. Confirm `Cargo.toml`, `LICENSE`, `NOTICE`, `README.md`, and
   `PROVENANCE.md` are included and accurate.
8. Append the complete evidence to `.marathon/progress.md` and commit it.
9. Create `.marathon/COMPLETE` containing the verified commit SHA and UTC time.

Do not run `cargo publish`. Publication is a separate human-authorized release
operation. If anything remains incomplete, do not create the sentinel.

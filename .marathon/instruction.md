# bead-rs clean-room implementation mission

You are building the governed bootstrap MVP of `bead-rs`, an independent Rust
task-coordination system. Marathon owns execution only through the final G4
handoff described in `docs/plan/plan.md`; it must not implement Phase 5 work.
Work autonomously in small, verified increments.

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
2. Read `AGENTS.md`, `PROVENANCE.md`, `docs/plan/plan.md`,
   `.marathon/progress.md`, and `.marathon/feature_list.json`.
3. Read `git status --short` and the recent Git log. Preserve unfinished work.
4. Run `cargo test` to establish the current baseline.
5. If Gate G0 artifacts or synchronized controls are incomplete, perform one
   coherent Phase 0 governance increment before feature implementation.
6. Otherwise select the earliest highest-priority feature from F001-F011 whose
   dependencies pass and whose `passes` value is false.

If `.marathon/BOOTSTRAP_HANDOFF` has `state: final`, report it and exit without
changing source. `.marathon/COMPLETE` remains reserved for the later full 0.1
release and must never be created by this Marathon session.

## Work rules

- Implement one coherent feature or one blocking defect per iteration.
- Treat `docs/plan/plan.md` as the implementation blueprint; normative files
  under `research/specs/` prevail if a contradiction is discovered.
- Cite the governing specification in code or test documentation where useful.
- Prefer a small native design over compatibility-shaped internal structure.
- SQLite is the native live store. Never write another tool's database.
- Mutations must be atomic. Claim selection and assignment are one transaction.
- Treat malformed machine input as an error, never as an empty result.
- Create all tests and fixtures independently.
- Do not weaken, delete, skip, or rewrite a test to manufacture a pass.
- Do not change feature requirements. You may change only `passes` and
  `evidence` after verification.
- Do not select F012-F017 or F014 under Marathon. After F011, follow Phases
  2-4 and Gates G2-G4 in the plan rather than continuing the feature list.
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

## Bootstrap handoff

Only after F001-F011 pass:

1. Run `cargo fmt --check`.
2. Run `cargo clippy --all-targets -- -D warnings`.
3. Run `cargo test`.
4. Complete the G2 installed-artifact, provider, consumer, checkpoint, and
   provenance gates without claiming version 0.1.
5. Materialize and independently reconcile the remaining reviewed work in a
   fresh native workspace exactly as Phase 3 and G3 require.
6. Run the disposable canary, stop/fence Marathon, and commit
   `.marathon/BOOTSTRAP_HANDOFF` with `state: pending` before any canonical
   NEEDLE mutation.
7. Run the canonical canary under provisional native authority and commit the
   same record with `state: final` only after all G4 evidence passes.

The handoff record includes the bootstrap commit, artifact hash, checkpoint
hash, mapping hash, NEEDLE configuration revision, UTC transition time, and
evidence locators. Native beads become the sole work-state authority at the
pending record. Never resume feature implementation under Marathon afterward.

Do not run `cargo publish`. Publication is a separate human-authorized release
operation. If anything remains incomplete, do not finalize the handoff.

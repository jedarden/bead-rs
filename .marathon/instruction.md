# bead-rs clean-room implementation mission

You are completing `bead-rs`, an independent Rust task-coordination system.
Marathon owns implementation of the full reviewed project: F001-F017 followed
by every adopted R001-R024 roadmap item. Work autonomously on `main` in small,
verified increments until full-project completion.

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

1. Run `pwd`; if the launcher starts in `.marathon/`, change to its parent,
   then confirm the working directory is the `bead-rs` repository root.
2. Read `AGENTS.md`, `PROVENANCE.md`, `docs/plan/plan.md`,
   `.marathon/progress.md`, and `.marathon/feature_list.json`.
3. Read `git status --short` and the recent Git log. Preserve unfinished work.
4. Run `cargo test` to establish the current baseline.
5. If Gate G0 artifacts or synchronized controls are incomplete, perform one
   coherent Phase 0 governance increment before feature implementation.
6. Otherwise select the earliest highest-priority feature from F001-F017 whose
   dependencies pass and whose `passes` value is false.
7. After F001-F017 pass, materialize R001-R024 into the feature ledger from
   plan section 12, preserving their exact scope and core-incorporated versus
   extension dispositions, then implement the earliest unblocked extension.

The final `BOOTSTRAP_HANDOFF` is historical evidence from the abandoned early
cutover and is not a stop condition. Do not start or delegate implementation to
NEEDLE. `.marathon/COMPLETE` is the only completion sentinel.

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
- Do not change existing feature requirements. You may change only `passes`
  and `evidence` after verification. The one permitted ledger expansion is to
  add R001-R024 verbatim from plan section 12 after F001-F017 pass.
- Implement F012-F017 and F014 under Marathon, then R001-R024. For roadmap
  items marked core-incorporated, record exact evidence from the owning F-item;
  do not duplicate implementation.
- Treat external authorship and independent review as separate iterations and
  record author, reviewer, artifact hash, and review result. A reviewing
  iteration must not modify the artifact it approves. Never self-assert review.
- If one feature is waiting for independent review, work on another unblocked
  feature. Do not weaken a gate merely to keep the loop moving.
- The independent F017 review is complete at
  `docs/reviews/f017-independent-review-2026-08-09.md`. Treat its conformance
  findings as implementation work. Do not create more governance-pause or
  review-preparation documents for F017.
- A blocked iteration must either implement or test an actionable acceptance
  criterion, or exit without a commit. Repeating status prose is not a coherent
  increment.
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

## Full-project completion

Only after F001-F017 and R001-R024 have verified dispositions:

1. Run `cargo fmt --check`.
2. Run `cargo clippy --all-targets -- -D warnings`.
3. Run `cargo test`.
4. Run every final release gate in plan section 13, including installed-package,
   checkpoint restore, profile conformance, stress, help/man-page, provenance,
   and consumer-side NEEDLE compatibility verification. The compatibility
   canary is verification only; it does not transfer execution authority.
5. Generate and verify the final release-evidence report against the exact
   commit and artifact hashes.
6. Confirm the working tree is clean, every coherent increment is committed and
   pushed to Forgejo `origin/main`, and the ledger contains no false feature.
7. Create `.marathon/COMPLETE` containing `state: complete`, the final commit,
   artifact hash, evidence-report hash, verification commands/results, and UTC
   completion time. Do not commit this ignored runtime sentinel.

Do not run `cargo publish`. Publication is a separate human-authorized release
operation. If anything remains incomplete, do not create `.marathon/COMPLETE`.

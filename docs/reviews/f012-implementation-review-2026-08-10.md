# F012 independent implementation review

Date: 2026-08-10 UTC

Reviewer: OpenAI Codex, acting only as implementation/conformance reviewer and
independent of the implementation author and accepted-artifact authors

Reviewed implementation: `67a4bfae41dcd8df2a256a54fe0afd3f2a9c4716`
(`main`, equal to `origin/main` at review start)

Baseline: the independently accepted round-four profile/specification/fixture
hash set in `docs/reviews/f012-independent-review-round4-2026-08-10.md`

Disposition: **rejected**. F012 must remain `passes: false`. The CLI wiring is
operational and several safety properties pass, but the implementation is not
conformant with either accepted external profile and does not implement the
normative loss-report accounting contract.

## Scope and method

I read the repository governance and mission, provenance and plan, the accepted
round-four review and prior review chain, the complete interchange/loss/profile
specifications, both accepted fixture corpora and manifests, and the relevant
profile, checkpoint, CLI, and F012/sync implementation and tests. I did not
inspect prohibited producer material, and I changed no implementation,
specification, or fixture.

Black-box checks used the repository-built `bead` at the reviewed commit in
disposable workspaces. They exercised all 22 br-v1 and 28 bf-v1 invalid cases,
including malformed input, required/type/range failures, valid offset
timestamps, unknown/deferred statuses, dangling references, self edges, and
two-record cycles; both accepted same-profile fixture files; both complete
observed corpora; external-export checkpoint-state immutability; merge-only
external import; and successful dry-run byte immutability.

## Blocking findings

### 1. Accepted bf-v1 producer output cannot be imported

Importing `research/fixtures/bf-v1/observed-valid.jsonl` with explicit
`--profile bf-v1 --merge` fails on its third, valid producer record:

```text
Line 3: issue validation failed: Validation failed: Closed issues must have a close_reason
```

That record contains the observed `"close_reason":""`. The adapter retains it,
but generic native validation at `src/service/checkpoint.rs:929-935` rejects the
valid external representation. The implementation therefore cannot consume the
accepted bf-v1 corpus.

Required correction: stage a profile-valid empty close reason without weakening
native authoring validation, retain it exactly for same-profile export, and add
a CLI test importing the complete accepted observed corpus.

### 2. br-v1 observed-corpus round trip is materially destructive

The complete accepted br-v1 corpus imports, but immediate same-profile export
is not equivalent:

- `status:"closed"` becomes unsupported `status:"finished"` because
  `src/profile/br_v1.rs:33-50` contradicts the accepted reverse table;
- absent `description` becomes explicit `description:""`;
- `owner` is excluded from extensions at `src/profile/br_v1.rs:349-374` but is
  never mapped, so it disappears;
- the valid closed record loses `closed_at` on the operational path;
- dependencies lose `created_at`, `created_by`, `metadata`, and `thread_id` as
  adapters/staging reduce them to `(blocked, blocker, kind)`
  (`src/profile/br_v1.rs:428-435`, `src/service/checkpoint.rs:939-951`).

The bf-v1 edge path has the same metadata truncation
(`src/profile/bf_v1.rs:202-215`). This violates exact same-profile extension,
null/absence, status, and edge-metadata preservation.

Required correction: use `closed` in the br-v1 reverse mapping; preserve every
profile field, optional-field presence state, and complete edge object through
storage; and compare every accepted observed/round-trip case through the CLI.

### 3. Loss reports are not truthful or exactly accounted

Import accounting initializes transformed and omitted counts permanently to
zero (`src/service/checkpoint.rs:885-887`) and classifies every input field as
preserved (`:959-990`) even when the operational path drops or changes it. Thus
the destructive br-v1 round trip above reports no loss.

Export accounting counts generated output fields instead of input occurrences
(`src/service/checkpoint.rs:3360-3389`). A one-issue native bf-v1 export reported
synthesized `design`, `acceptance_criteria`, `notes`, and `events` as preserved,
producing `preserved:13`. Adapter warnings are also collapsed to omission with
`schema_ref_omitted` regardless of category (`:3336-3358`). The accepted exact
loss-report expectations are never asserted by tests.

Required correction: derive accounting from input occurrences and actual
transformations, preserve mandatory reason identity, make counts equal entry
sums, and assert exact report shape/order/counts for every accepted report case
through the CLI.

### 4. Current tests do not establish fixture conformance

`tests/f012_integration.rs` checks only `expected_output` for four invented
round-trip cases and bypasses activation. It does not compare
`expected_report`, drive invalid fixture files, import either complete observed
corpus, or verify edge metadata and operational absence/null/order persistence.
The sync tests use two small happy paths. This allowed all blocking failures
above while all F012 and repository tests passed.

Required correction: add a data-driven CLI conformance harness for every case
in both accepted corpora, including exact reports and failed-import
immutability.

## Checks that passed

- All 50 invalid fixture cases had the expected accept/reject disposition;
  unknown br-v1 and bf-v1 unknown/deferred statuses emitted
  `unknown_status_preserved`.
- Dangling dependencies, self edges, and two-record cycles failed before
  activation.
- External profiles were merge-only; a successful external dry-run left every
  `.beads` file byte-identical and inserted no issue.
- External export left `checkpoint_state` unchanged in exercised cases and used
  temp-file `fsync` plus rename. Exercised issue, dependency, and label ordering
  was correct.
- The four adapter-level round-trip outputs passed exactly, but do not cure the
  operational failures above.
- `cargo fmt --check`: pass.
- `cargo clippy --all-targets -- -D warnings`: pass.
- `cargo test --test f012_integration --test cli_sync --test cli_sync_import`:
  pass (16 + 8 + 17 tests).
- `cargo test`: pass (complete repository suite).

Rust 1.75 was not installed (`rustup toolchain list` showed stable, nightly,
1.87, and 1.95 only), so the MSRV could not be verified. This is an outstanding
release check, not the basis for rejection.

## Final disposition

**Rejected at `67a4bfa`.** Correct findings 1-4 and request a fresh independent
implementation review. Do not mark F012 passing from the green Rust suite; it
does not cover the accepted conformance surface.

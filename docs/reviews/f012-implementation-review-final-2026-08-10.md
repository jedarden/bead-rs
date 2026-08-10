# F012 final independent implementation review

Date: 2026-08-10 UTC

Reviewer: OpenAI Codex, independently reviewing the correction after two prior
implementation-review rejections

Reviewed commit: `3baf98e2efdab447230566af9db61895d9303275`
(`main`, equal to `origin/main` at review start; clean worktree)

Baseline: the complete independently accepted round-four F012 profile,
loss-report, and fixture baseline recorded in
`docs/reviews/f012-independent-review-round4-2026-08-10.md`

Disposition: **rejected with one remaining conformance defect**. Every prior
data-loss, accounting, atomicity, coverage, and MSRV finding is resolved, but
the collision fix reserves a namespace that the accepted profiles define as
ordinary extension space. F012 remains `passes: false`.

## Review method

I reread the accepted profiles, shared loss-report contract, complete fixture
corpora/manifests, prior reviews, correction diff, and current implementation
and tests. I used only permitted repository material and disposable black-box
workspaces. I changed no implementation, specification, fixture, or ledger.

Verification covered both observed corpora; every accepted same-profile and
complete export-report case; all 50 invalid cases; unknown/deferred/null/
absence/order/status and edge metadata; all five private marker families for
both profiles; failure atomicity; export checkpoint-state nonmutation;
merge-only and dry-run safety; report ordering/count sums and distinct status
source accounting; and Rust 1.75 locked check/test.

## Prior findings: resolved

- Both accepted observed corpora import through the CLI and immediately export
  semantically identically by ID.
- bf-v1's empty close reason survives. br-v1 `closed`, optional absence,
  `owner`, `closed_at`, source fields, and full edge metadata survive. bf-v1
  edge metadata and creation order survive.
- The four same-profile fixtures produce exact expected outputs and reports.
  Every complete accepted export-report fixture produces the exact report.
- Operational reports have normative order and balanced entry/count sums.
  `base_status` and `manual_blocked` are distinct `status_mapped` entries.
- All 22 br-v1 and 28 bf-v1 invalid cases had the expected disposition.
- For both profiles, each private family (`__profile_status__`,
  `__profile_dependencies__`, `__profile_null__:`,
  `__profile_empty_array__:`, and `__profile_absent__:`) failed with
  `known_extension_collision`. All ten operational runs left SQLite
  byte-identical.
- External export checkpoint state, merge-only import, dry-run immutability,
  atomic output publication, and expanded coverage passed.

## Remaining blocker: noncolliding unknown fields are rejected

The accepted profiles say every “other field” is an extension whose original
JSON value survives same-profile round trip. They do not reserve the whole
`__profile_` prefix. The loss contract requires failure only when a key
actually collides with a known field after mapping.

Both adapters reject every input key beginning `__profile_`, including keys
unrelated to private markers. An otherwise valid record containing:

```json
"__profile_custom__":{"v":1}
```

failed for both profiles with
`known_extension_collision: __profile_custom__`. This key does not collide
with any status/dependency/null/empty/absent marker. It must be preserved
recursively and reported as `extension_preserved`, not rejected.

Required correction: match only actual private keys/families, or move private
state out of the user extension map. Add adapter and CLI round-trip tests
proving a noncolliding `__profile_custom__` survives while actual marker
families continue to fail atomically.

## Commands and results

- Locked targeted suites: pass (18 F012 + 8 sync + 19 sync-import).
- Invalid matrix: br-v1 22/22, bf-v1 28/28.
- Reserved-family collision/byte-immutability matrix: 10/10.
- Complete observed-corpus operational round trips: pass.
- `cargo fmt --check`: pass.
- `cargo clippy --locked --all-targets -- -D warnings`: pass.
- `cargo +1.75.0 check --locked --all-targets`: pass (four redundant-import
  warnings under that compiler).
- `cargo +1.75.0 test --locked`: pass, complete suite and doc tests.
- Noncolliding `__profile_custom__` round trip: **fail for both profiles**.

## Final disposition

**Rejected at `3baf98e` solely for the overbroad extension-prefix rejection.**
Correct that narrow defect, retain actual collision/atomicity behavior, rerun
the complete accepted and Rust 1.75 suites, and obtain final confirmation. No
other blocking F012 implementation defect was found. F012 remains false.

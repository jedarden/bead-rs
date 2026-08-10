# F012 final independent implementation confirmation

Date: 2026-08-10 UTC

Reviewer: OpenAI Codex, independently confirming the narrow correction after
the complete review at commit `3baf98e`

Reviewed commit: `99be5a7d8bb760ed55efe8612678f7f29515708c`
(`main`, equal to `origin/main` at review start; clean worktree)

Disposition: **accepted**. The sole remaining defect from
`docs/reviews/f012-implementation-review-final-2026-08-10.md` is corrected
without regressing the complete accepted round-four baseline. This review does
not change the ledger; F012 remains false for separate owner activation.

## Confirmation

The guards exactly cover private state consumed by each adapter:

- both profiles: `__profile_status__`, `__profile_dependencies__`,
  `__profile_null__:*`, and `__profile_empty_array__:*`;
- br-v1 only: `__profile_absent__:*`.

Search found no other consumed private key/prefix. Actual collisions still fail
with `known_extension_collision` before activation. bf-v1 correctly leaves the
br-only absence prefix available as ordinary extension space.

Operational import/export preserved recursively exact nested values for br-v1
and bf-v1 `__profile_custom__`, and bf-v1
`__profile_absent__:description`. Adapter reports classify them as
`extension_preserved`. Prior collision atomicity, complete observed corpora,
accepted reports, invalid cases, ordering, metadata, optional presence, empty
close reason, and distinct status-source accounting remain green.

## Verification

- Locked targeted suites: pass (19 F012 + 8 sync + 19 sync-import).
- Operational noncolliding prefixed-extension matrix: 3/3 exact.
- `cargo fmt --check`: pass.
- `cargo clippy --locked --all-targets -- -D warnings`: pass.
- `cargo +1.75.0 check --locked --all-targets`: pass (four redundant-import
  warnings under that compiler).
- Rust 1.75 locked targeted suites: pass (19 + 8 + 19; same warnings).
- The immediately preceding complete Rust 1.75 locked suite and complete
  baseline review are incorporated from the final `3baf98e` review; this
  correction touches only narrowed guards and their regression test.

## Final disposition

**Accepted at `99be5a7`.** No remaining F012 implementation/conformance defect
was found. Ledger activation and broader release gates remain separate owner
actions.

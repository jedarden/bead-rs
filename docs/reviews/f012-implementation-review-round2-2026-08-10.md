# F012 independent implementation review, round two

Date: 2026-08-10 UTC

Reviewer: OpenAI Codex, the prior implementation reviewer, independently
reviewing a correction authored after that rejection

Reviewed implementation: `b89f36360506d2010ae8184e5605ee7c4f7c0823`
(`main`, equal to `origin/main` at review start)

Baseline: the complete independently accepted round-four F012 artifact set
recorded in `docs/reviews/f012-independent-review-round4-2026-08-10.md`

Disposition: **rejected**. The correction resolves every concrete data-loss
reproduction from round one, but full-baseline testing found an unhandled
known/extension collision that silently changes status. Rust 1.75 also fails
with the reviewed dependency lock. F012 must remain `passes: false`.

## Independence and method

I reviewed the exact correction diff from review commit `75e83c6` to
`b89f363`, the accepted profiles and fixtures, loss-report contract, prior
review, relevant profile/checkpoint/CLI code, and expanded tests. I used only
permitted repository material and disposable black-box workspaces. I did not
modify implementation, specifications, fixtures, or the feature ledger.

During verification another worker created uncommitted `Cargo.toml` and
`Cargo.lock` dependency-pin changes after the exact-commit MSRV check. Those
changes are not part of this review, were preserved, and are excluded from the
review commit. Exact-commit targeted tests and black-box runs preceded them;
the later full-suite run used the concurrently edited dependency selection and
is reported with that limitation.

## Round-one findings

### Accepted bf-v1 corpus and empty close reason: resolved

The complete `bf-v1/observed-valid.jsonl` corpus now imports with explicit
merge and immediately exports semantically identically. Its valid closed
record retains `close_reason:""`, `closed_at`, `closed_by_session`, events,
and all other fields.

### br-v1 status, optional presence, fields, and edge metadata: resolved

The complete br-v1 observed corpus also imports and immediately exports
semantically identically by ID. `closed` remains `closed`; absent descriptions
remain absent; `owner`, `closed_at`, `source_repo`, and other optional fields
survive; and dependency `created_at`, `created_by`, `metadata`, `thread_id`,
direction, and order remain exact. bf-v1 dependency metadata and creation order
also survive.

The four accepted invented same-profile cases produce exact expected output
and exact expected reports through the new shared conformance path. Three can
also be activated and exported directly; the fourth intentionally references
blockers outside its one-record corpus and correctly fails operational graph
validation when used alone.

### Accepted loss-report fixtures and operational accounting: substantially resolved

All accepted same-profile expected reports and all complete export
`input_records` expected reports compare exactly in tests. Operational import
and export reports are in normative classification/scope/field/reason order,
and the sum of entry counts equals the sum of `counts` in exercised cases.
Export no longer counts synthesized bf content fields as preserved native
input. Unknown/deferred statuses retain their required reason.

One structural concern remains in operational native export: `base_status` and
`manual_blocked` occurrences are aggregated as `field:"status", count:2`
without a `fields` array. The contract forbids combining distinct fields this
way. This should be corrected, but the silent collision below independently
requires rejection.

### Coverage: resolved for the prior findings

Tests now import/export both complete observed corpora through the real CLI,
compare exact same-profile reports, compare every complete accepted export
report case, and check operational report sum/order invariants. The invalid
fixture corpus remains primarily covered by review-time black-box execution
rather than a committed data-driven test, but all 50 cases retained their
expected accept/reject behavior in this review.

## New blocking finding: private sentinel collides with allowed extensions

Both adapters store preservation state in ordinary extension keys such as
`__profile_status__`. An external unknown field with that exact name is
accepted as an extension, then interpreted as private state during export.
For each profile, this input succeeds:

```json
{"id":"x","title":"X","status":"open","priority":2,"issue_type":"task","created_at":"2030-01-01T00:00:00Z","updated_at":"2030-01-01T00:00:00Z","__profile_status__":"closed"}
```

(The bf-v1 form additionally contains its required content fields and
`events`.) Immediate same-profile export silently emits `status:"closed"` and
omits the unknown `__profile_status__` field. The declared status changed from
open to closed, and the extension disappeared, with no failure and no truthful
loss report.

This violates two normative requirements: unknown JSON must survive exact
same-profile round trip, and an unknown/known-field collision must fail before
output with reason `known_extension_collision`. The same namespace hazard
applies to other private markers including `__profile_dependencies__`,
`__profile_null__:*`, `__profile_empty_array__:*`, and
`__profile_absent__:*`.

Required correction: do not encode adapter control state in the user extension
keyspace. Use typed/non-user storage or reject every collision deterministically
before activation, preserving all noncolliding unknown fields exactly. Add
adapter and operational CLI cases for every private-marker collision and verify
failed import/dry-run workspace immutability and the required machine reason.
Also correct distinct-source-field aggregation in operational export reports.

## Verification

- Complete observed br-v1 and bf-v1 operational round trips: pass.
- Accepted same-profile exact output/report cases: pass.
- Accepted complete export loss-report cases: pass.
- All 22 br-v1 and 28 bf-v1 invalid cases: expected disposition, including
  dangling, self-cycle, two-record cycle, unknown, deferred, offsets, and
  malformed/type/range cases.
- Merge-only restriction, dry-run immutability, export checkpoint-state
  nonmutation, relationship/status/unknown/null/order checks: pass except the
  reserved-name unknown-extension collision above.
- `cargo fmt --check`: pass.
- `cargo clippy --all-targets -- -D warnings`: pass before the concurrent
  dependency edits.
- Exact-commit targeted suites: pass (17 F012, 8 sync, 18 sync-import).
- Full `cargo test`: pass after concurrent uncommitted dependency-pin changes;
  therefore useful but not exact-lock evidence for `b89f363`.
- `cargo +1.75.0 check --locked --all-targets`: **fail** at the reviewed lock;
  `rusqlite 0.32.1` uses experimental `c"..."` literals under Rust 1.75.

## Final disposition

**Rejected at `b89f363`.** The previous four data-loss/report/test findings are
resolved, but the extension collision is a correctness and loss-report blocker,
and the reviewed lock fails the required Rust 1.75 MSRV. Correct both, rerun
the complete accepted corpus and exact-lock suite, and request a fresh
independent implementation review. F012 remains false.

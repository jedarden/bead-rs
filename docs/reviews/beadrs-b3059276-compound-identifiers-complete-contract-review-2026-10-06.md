# Complete ruleset-v4 contract review — first compound correction rejected

Reviewer: `/root/secret_release_review`, distinct from `/root`, author of the
proposed correction. Owning outcome: `beadrs-b3059276` (live quarantine prevents
creation of another owning bead). Review performed on 2026-10-06.

## Exact inputs and review scope

Both SHA-256 hashes matched before and after review:

- `research/specs/secret-ruleset-v4.md`:
  `e3a3b44464d46d3379ee7eb70534684b2e43c8e07bb92f4fe871b909beb018f5`.
- `research/fixtures/secret-ruleset-v4-contract.py`:
  `c0e3146c39e48a84f2c9fa79b00b3e09a57c614b3334dcb2d838c5e5a9ff2c74`.

This review read the complete specification, all sections 1–7 and the appendix,
and the complete independently assembled witness program, not just the delta.
Applicable clean-room guidance, the mission, and the provenance record were
read. No other bead implementation, actual store values, native implementation
code, or native implementation tests were inspected. The reviewer did not
modify the spec, fixture, implementation, plan, or ledger, and performed no
live-store mutation, installation, commit, or push.

## Decision

**Rejected as an implementation baseline.** The proposed non-atom predicate in
section 5.1 can suppress an opaque alphabetic component after a hash or UUID,
contrary to the explicit opaque-sibling preservation requirement.

The predicate rejects a part only when all three of lowercase, uppercase, and
digits coexist, or digit/non-digit transitions exceed two. A part with
arbitrarily alternating lowercase and uppercase letters but no digit passes
both tests. A hash sibling supplies the digits required by `Q`, so the entire
run remains advisory-eligible under the existing contract before the proposed
compound exclusion silently discards it.

Independently assembled witnesses using only runtime fragments reproduced:

| Named witness | `(n,a,d,w,t)` | First failing Q16 step | Compound excluded |
|---|---|---:|---|
| Cache path, 32-byte hex atom, 40-byte alternating-case alphabetic sibling | `(79,77,16,5,74)` | 0 (passes) | true |
| UUID atom, hyphen, same alternating-case alphabetic sibling | `(77,72,16,0,76)` | 0 (passes) | true |

No candidate bytes or locations were printed. The program emitted only case
identifiers, integer metrics, and booleans. Both witnesses are whole-run
controls, not forbidden substring-only checks; the opaque sibling is a
non-atom part without a recognized provider prefix.

Required revision: bound alphabetic case transitions or otherwise explicitly
distinguish bounded mixed-case identifier names from opaque alphabetic parts
in the non-atom predicate. Add independently assembled positive opaque
siblings **without digits**, including hash/path and UUID siblings, requiring
both `Q(run,16)` eligibility and non-exclusion. Keep the predicate deterministic
and byte-defined. Do not weaken `P`, `Q`, blocking/labelled advisory matching,
the 90% advisory-volume gate, fleet dispositions, or hostile-field cost gates.
New exact spec/fixture hashes need a new complete-contract review before native
implementation.

## Complete-contract findings and checks

No additional blocking inconsistency was identified in the remaining reviewed
contract. In particular:

- Scope retains the authoritative mutation, acknowledgment, destruction, and
  managed-boundary contracts; the correction is unlabelled-advisory-only.
- `P` and ordered integer `Q` specify maximal word consumption, strict ratio
  boundaries, the 31/32-byte one-case hex exception, and platform-independent
  byte classification. Existing truth-table representatives and metrics agree.
- Views retain bounded one-level decoding, qualifying invalid-run slot
  accounting, complete raw/normalized/dewrapped scanning, and value-free
  limitation reporting. Derived findings resolve raw ranges and fingerprints.
- Provider, contextual, structural, assignment, JWT/checksum, and local
  assignment-exclusion rules keep their explicit boundaries and dispositions.
- Invocation notice semantics preserve deduplication, tiers, sorted inventory,
  value-free stderr/JSON, semantic no-op, dry-run/rollback, and post-commit
  publication-failure distinctions.
- Live/current/previous diagnostic sources remain independently scanned with
  accurate absent/unreadable status.
- All rules, views, parity, advisory volume, fingerprint dispositions, replay,
  and cost release gates remain present and unchanged.
- The new compound grammar otherwise specifies whole-run atoms, UUID grouping,
  Nix anchoring, terminal punctuation handling, hash-substring negatives,
  mandatory recognized atoms, and preserved `+`/`=` eligibility explicitly.

Commands actually executed:

- `python3 research/fixtures/secret-ruleset-v4-contract.py`: all nine tests
  passed, zero failures; these are specification witnesses, not native
  implementation or fleet conformance.
- `python3 research/fixtures/secret-ruleset-v4-contract.py --table`: succeeded,
  all 14 named metrics/verdict rows agree with the normative truth table.
- An independent `python3 -B` runtime witness process imported only this
  fixture, constructed the two omitted classes, and reproduced the table above.
- `git diff --check -- research/specs/secret-ruleset-v4.md
  research/fixtures/secret-ruleset-v4-contract.py`: passed.
- SHA-256 was recomputed after these checks and remained exactly as listed.

The fixture intentionally does not constitute production conformance for the
full matching/view/notice/coverage contract; all section 7 implementation
checks still apply. No Cargo/native implementation suite was run for this
pre-implementation contract review. This rejection is not approval to scrub
credentials, clear quarantine, freeze ruleset 4, publish, or deploy.

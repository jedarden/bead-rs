# Scoped native detector/view implementation review — accepted

Independent reviewer: `/root/secret_release_review`, distinct from `/root`,
author of the implementation and accepted specification corrections.
Outcome owner: `beadrs-b3059276`. Review date: 2026-10-06.

## Exact scoped inputs

The implementation amendment is accepted at these exact SHA-256 file hashes:

| Path | SHA-256 |
|---|---|
| `src/scan/credential_shape.rs` | `479dc7df58a8a6bf702acd388b005d762d6afcbf3bd834355b4ac1e18924e332` |
| `src/scan/external.rs` | `0a4704c0aa04327bea5d47888e3706d84ddbeb23cfa5626569949d02029610d0` |
| `src/scan/mod.rs` | `65edeca7f2b46ae88b9c54d5f8f1d35b690836934fa55b9af2c647aac18e911b` |
| `src/scan/tests.rs` | `bf7216ffde97f034c3c6c7025ecd608be3c732fcbce27340d3dcb9b34b8b4292` |

All four hashes matched before and after inspection/checks. Governing baselines
were independently accepted before implementation and independently rehashed:

- Complete ruleset-v4 spec `e81a63de397e3612b79b03680ee64115f5c26696895ff2e0ad9be20bf132ea89`
  and fixture `5b98bab331317c32ca4613690716ecbb967dc6b6ebc8c9ad65c922659880af97`.
- Organization quoted-view spec `314ca8dc63e52016973094f787f3c7fd43a071be93872c8558971025e6c94321`
  and fixture `d3c7aabcc900e54d8a67c738a161464b53059cf4f8eb4aa8d7a643108606140d`.

This is acceptance of the **four-file scoped amendment**, not acceptance of
the whole dirty repository, a final source commit, an optimized candidate,
the organization scanner, fleet replay or publication/deployment.

## Assessment

No actionable defect was found against the accepted baselines.

The compound helper performs whole-run terminal-dot handling, original whole
hash recognition, explicit separator and `+`/`=` guards, whole chunk recognition,
UUID group recognition, fixed-width hex atoms, exact anchored Nix hash/name
recognition, and bounded ASCII non-atom qualification. Mixed-case-plus-digit,
digit/non-digit transition, and alphabetic case-change tests match the accepted
grammar. The 512-byte short-circuit cannot suppress an otherwise eligible
advisory because Q independently rejects longer runs.

Only `scan_entropy` switches to this additional exclusion. Blocking and
labelled-assignment fallback code is unchanged. Tests retain eligible opaque
digit-bearing and alphabetic siblings, full original raw ranges, exact
fingerprints, fixed atom boundaries and independent assignment blocking.

The organization second view now produces the exact complete quoted JSON
serialization. Its opening/closing quotes map to empty raw ranges, body bytes
map to complete UTF-8 source characters, and the existing mapped-span filters
discard empty/reversed/out-of-range results. Raw request bytes, framing,
namespaced rule identity, standard raw fingerprints and selection/destruction
paths are unchanged. Quote-containing, control-character, Unicode and empty
serialization witnesses pass.

## Checks actually executed

All Cargo checks ran locally in the shared checkout with output under
`/build/bead-rs`; the host test wrapper reported CPU quota 200% and memory
maximum 6 GiB. Organization integration was explicitly off for hermetic tests.

- `cargo fmt --check`: passed.
- `BEAD_ORG_SECRET_SCANNER=off cargo test --lib scan:: -- --test-threads=1`:
  49 passed, zero failed/ignored, 298 filtered out; targeted library scan tests.
- `BEAD_ORG_SECRET_SCANNER=off cargo test --features managed-secret-policy
  --lib scan:: -- --test-threads=1`: 47 passed, zero failed/ignored, 298 filtered
  out; targeted managed-profile library scan tests.
- `cargo clippy --lib -- -D warnings`: passed; library-only check.
- `cargo clippy --features managed-secret-policy --lib -- -D warnings`:
  passed; managed-profile library-only check.
- `git diff --check -- src/scan/credential_shape.rs src/scan/external.rs
  src/scan/mod.rs src/scan/tests.rs`: passed.
- Final SHA-256 recomputation confirmed all four implementation inputs unchanged.

No unfiltered dirty-checkout integration suite was run, preserving the other
workers' work and avoiding their known nested-build test side effects. Full
source gates, both required GNU architectures/profiles, actual organization
scanner parity, disposable redaction/managed-boundary smoke checks, complete
reachable-fleet dispositions/advisory-volume replay, hostile-field cost,
artifact verification and publication/deployment remain required.

The reviewer changed only this review evidence. No reviewed source/spec/fixture,
plan, ledger, live bead data, credential, binary installation, hook or managed
resource was changed; no commit or push was performed. This acceptance does
not authorize real credential scrubbing, manual quarantine clearing or release
gate waiver.

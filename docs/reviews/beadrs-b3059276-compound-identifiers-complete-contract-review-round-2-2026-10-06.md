# Complete ruleset-v4 exact-input review — accepted

Reviewer: `/root/secret_release_review`, distinct from `/root`, author of the
specification correction and fixtures. Owning outcome: `beadrs-b3059276`;
the live quarantine prevents creation of another owning bead. The reviewer
authored neither the original artifacts nor either correction.

## Exact accepted artifacts

**The complete contract and fixture baseline are accepted, unconditionally,
at these exact SHA-256 hashes:**

- `research/specs/secret-ruleset-v4.md`:
  `e81a63de397e3612b79b03680ee64115f5c26696895ff2e0ad9be20bf132ea89`.
- `research/fixtures/secret-ruleset-v4-contract.py`:
  `5b98bab331317c32ca4613690716ecbb967dc6b6ebc8c9ad65c922659880af97`.

Both hashes were verified before and after review. Scope is all sections 1–7,
the informative appendix, and the entire fixture, **not just the amendment or
prior rejection**. No section is carved out of this specification acceptance.
The artifact header remains proposed; this decision is explicitly hash-bound
and does not silently supersede the earlier accepted baseline.

The reviewer read applicable AGENTS guidance, the live mission and the complete
provenance record. Only permitted repository specifications and independently
assembled witnesses were used. No other bead implementation, actual store
values, native implementation source or implementation tests were inspected.
The reviewer did not modify either reviewed artifact or native implementation,
plan, ledger or live store, and did not commit, push, install or deploy.

## First rejection resolved

The first compound correction could exclude an arbitrarily alternating
alphabetic sibling of a hash/UUID because its name test counted only
digit/non-digit transitions and required digits before rejecting mixed case.
The corrected normative grammar additionally caps adjacent alphabetic
lower/upper-case changes at four in every non-atom part. It counts a pair only
when both bytes are alphabetic; the prior ASCII-alphanumeric requirement keeps
that predicate platform-independent.

The corrected fixture now requires hash/path, UUID, generation and anchored
Nix siblings containing strongly alternating alphabetic material without
digits to pass Q16 and remain non-excluded. Its strict four/five case-change
witnesses confirm the boundary without changing `P` or `Q`. The first
rejection's exact two witnesses were independently rerun and now remain
eligible. Bounded identifier names still exclude; opaque siblings cannot borrow
their atom's digits and then disappear.

## Full-contract assessment

- Scope retains the authoritative mutation/acknowledgment, historical
  destruction and managed write-boundary contracts. The additional exclusion
  is limited to the unlabelled advisory rule and cannot suppress blocking
  findings or the labelled Q-fail advisory fallback.
- `P` and ordered integer `Q` are deterministic, with explicit strict ratios,
  maximal-word consumption, first-failure behavior, and the 31/32-byte one-case
  hex exception. The normative named truth table agrees at all four thresholds.
- All four views remain byte-range-preserving. One-level decoding is bounded
  by qualifying run count/size, consumes invalid qualifying slots, reports
  limitations value-free, and leaves other views scanning the complete field.
  Derived findings retain raw fingerprints and redaction resolvability.
- Provider/context/structural/assignment rules and checksums retain explicit
  captured ranges, alphabet/label/boundary conditions, local exclusions,
  maximal JWT-chain validation, strict complete JSON header rules and
  checksum-failed advisory dispositions.
- Compound exclusions validate the whole run, recognized atoms, and every
  remaining name part. Fixed atom widths, UUID groups, exact generation/bead
  prefix case, anchored Nix store hashes, complete trailing-dot handling,
  hash-substring negatives and `+`/`=` preservation are defined. Bare
  nonstandard-width hexadecimal material retains its prior eligibility.
- Write-time notices retain reported-tier selection, invocation fingerprint
  deduplication, capped findings, sorted ASCII rule IDs, value-free stderr and
  object-only JSON additions. Dry-run/rollback/precommit failures emit neither;
  successful semantic no-ops and postcommit-publication failures are explicit.
- Live/current/previous sources scan independently and distinguish legitimate
  absence from unreadability while retaining findings from successful sources.
- All rule, qualifier, view, fleet Git-layer parity, 90% advisory reduction,
  fingerprint disposition, complete reachable-fleet replay and hostile-field
  cost requirements remain intact. Acceptance does not waive any release gate.

## Independently executed checks

- `python3 research/fixtures/secret-ruleset-v4-contract.py`: nine tests passed,
  zero failures; specification witnesses only, not production conformance.
- `python3 research/fixtures/secret-ruleset-v4-contract.py --table`: succeeded,
  all 14 named integer-metric/verdict rows agree with the complete contract.
- An independent `python3 -B` runtime matrix constructed 24 additional controls:
  five opaque alphabetic atom-sibling classes, all case-change counts one
  through eight, `+`/`=` opaque controls, and all nine compound hexadecimal atom
  widths. All 24 passed the expected exclusion and required Q16 predicates.
  Reports contained only labels, integer metrics or booleans, never values.
- `git diff --check -- research/specs/secret-ruleset-v4.md
  research/fixtures/secret-ruleset-v4-contract.py`: passed.
- SHA-256 recomputation confirmed both frozen inputs were unchanged.

This is **specification/fixture acceptance permitting implementation against
the complete exact baseline**, not implementation, fleet or release approval.
No native Cargo suite was run for this pre-implementation review. Production
tests, distinct implementation verification, actual optimized-artifact replay,
parity, cost, dispositions and publication/deployment gates remain required.
This decision does not authorize real credential rotation/scrubbing or manual
quarantine clearing.

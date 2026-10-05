# BR-T35 write-time stderr compatibility and integrated decision

Date: 2026-10-05.

Reviewer: OpenAI Codex, independent of the specification and ADR authors and
the implementation owners.

**Decision: ACTIONABLE REJECTED.** This is a decision on the complete
`research/specs/secret-ruleset-v4.md` artifact at the exact SHA-256 below.
Standard error is a compatible diagnostic channel under NEEDLE, but section
5.2 does not define a reproducible notice or machine-output contract. The
three latest focused reviews also leave independent ruleset compatibility
blockers. **Preserve the BR-T39 through BR-T44 release gate; do not release
those beads.**

## Scope and method

I reviewed section 5.2 against the NEEDLE process contract, ADR-025, and the
accepted `secret-rejection-v1` contract. I integrated the three closed focused
records for qualifier/exclusion behavior, decoded and structured matching,
and raw-range/redaction behavior, and the earlier focused write-time output
review. This synthesis assesses their recorded results; it does not widen
their scoped acceptances. I used no other bead implementation's source,
tests, fixtures, output, or prose and did not inspect implementation source.
No candidate credential values are recorded here.

## Exact reviewed inputs

Hashes identify complete file bytes, not excerpts.

| Artifact | SHA-256 |
| --- | --- |
| `research/specs/secret-ruleset-v4.md` | `bad7c8102086d8128465e284b839673ceceb84b28e3ea7bf55835dfaee5ad5a9` |
| `research/specs/needle-cli-contract-v1.md` | `64a4057bdec893d81e42a3f23bff86f225a302fc3b5fec18d8ab5cf414ad7233` |
| `docs/adr/025-surface-advisory-findings-and-partial-scan-coverage.md` | `e13bda45279d2b875c9c2dce6d2bdc3e0471e91512cff22ec40487f1a0de4a33` |
| `research/specs/secret-rejection-v1.md` (accepted baseline) | `f6aa7639a8ef1dd509b431853abf64db78d0923ef9e9d59b09e0a4e0e55df231` |
| `docs/reviews/beadrs-c0b467d4-ruleset-v4-randomness-excluded-identifiers-review-2026-10-05.md` | `65c376bf789d996fe2ea77a07eda0269c318662c24ef2e8d907cfc11f25d77d2` |
| `docs/reviews/beadrs-cf08b3e6-ruleset-v4-decoded-bounds-structured-review-2026-10-05.md` | `baec1dce9e17d7a491110b41e99124bd6b475a07c58656e5d7ad38b6854045da` |
| `docs/reviews/beadrs-c335649d-ruleset-v4-raw-range-redact-review-2026-10-05.md` | `afe025dd644e8fdae005fa12488b7e19d15bfb90c5637a59f20cfb5b597ef847` |
| `docs/reviews/beadrs-2f7ad38d-write-time-stderr-compatibility-review-2026-10-04.md` | `0f79ef8d4e7f0d237187840de1fc402b9afd126c0696c129b576621baf84614a` |

## Section 5.2: channel compatibility and notice contract

**Channel routing: compatible. Exact notice and machine-output contract:
actionably rejected.** NEEDLE requires successful machine-readable stdout
to remain valid UTF-8 and requires diagnostics to go to stderr without
corrupting JSON. It does not prohibit a successful command from writing a
redacted diagnostic to stderr. The accepted v1 contract separately requires
matched bytes to stay out of output and requires a complete pre-transaction
scan with atomic rejection for an unacknowledged blocking finding. ADR-025's
intent to surface a successful advisory result without changing exit status
fits those process and mutation boundaries.

These contracts do not make section 5.2's output interoperable. Section 5.2
requires one stderr line naming a count, rule identifiers, and the doctor
command, while omitting matched bytes. It does not define the line grammar,
separators, newline, unique rule ordering, duplicate handling, or how that
line coexists with other success diagnostics. Neither it nor ADR-025 defines
whether `secret_scan.advisory_findings` is a number, array, or object, where
it appears in each machine-readable mutation result, or whether the notice
count is findings, rule/range pairs, or another post-cap/post-deduplication
quantity. The requirement that stdout text remain unchanged is also
ambiguous beside the new machine-output member. These gaps permit two
implementations to report different counts and JSON while satisfying the
current prose.

The earlier focused output review also identified unresolved behavior around
mode-demoted blocking findings, acknowledged findings, semantic no-ops,
rollback, `--no-auto-flush`, and checkpoint-publication failure after a
semantic commit. Those cases need explicit outcomes if the notice is to be a
stable account of a successful write. All notice and JSON forms must remain
value-free and location-free under the accepted v1 redaction boundary.

## Integrated focused-review dispositions

| Focused record | Disposition carried forward |
| --- | --- |
| `beadrs-c0b467d4` — Q qualifier and excluded identifiers | **Actionable rejection.** Step-5 word-run consumption and cursor advancement, required Q truth-table outcomes, and identifier/exclusion boundaries are not deterministic enough to establish blocking versus advisory behavior. |
| `beadrs-cf08b3e6` — decoded bounds and structured blocking | **Actionable rejection.** Decoded-run order, caps, decoding grammar, and over-limit coverage are open; JWT acceptance and table-row delimiter/range behavior can diverge. |
| `beadrs-c335649d` — raw-range/redaction compatibility | **Accepted for section 3.4 only.** Synthetic checks support raw covering ranges and fingerprint revalidation, including deliberate whole-run redaction. This narrow result does not decide which candidate is selected or accept the remaining ruleset. |
| `beadrs-2f7ad38d` — write-time output | **Actionable rejection of section 5.2 serialization and machine output.** It independently finds the same channel-versus-contract split and additional mutation lifecycle ambiguities. |

The section 3.4 acceptance is compatible with retaining the accepted v1 raw
byte coordinates, fingerprints, and redaction revalidation. It cannot repair
unspecified Q outcomes, decoded selection, parser boundaries, or notice
semantics. The two focused rejections affect whether a write is blocked and
which fingerprint/range represents a finding; the section 5.2 rejection
affects how a successful write is reported to a caller.

## Consolidated dispositions

**Compatibility: ACTIONABLE REJECTION for this exact ruleset hash.** The v4
direction can be an additive, versioned extension while retaining the
accepted v1 offline scanner boundary, full pre-write scanning, atomic
blocking rejection, raw-byte finding coordinates, fingerprint-scoped
acknowledgment/redaction, and value-free output. Those preserved invariants
and the narrow section 3.4 acceptance do not supply the missing v4 verdict,
coverage, range-selection, or advisory-output semantics. Compatibility and
conformance are not established for the complete ruleset at this hash.

**Threat model: ACTIONABLE REJECTION.** An implementation can miss or
spuriously block a value at an unresolved Q, identifier, decoded-run,
JWT, or table-row boundary. Different selections can produce different
fingerprints and redaction ranges. An unspecified decoded cap can omit
coverage without an observable partial-scan result. Unspecified advisory
counts or serialization can cause callers to undercount or mis-triage
findings. Redaction revalidation and value-free output remain necessary
controls, but cannot repair missed findings or divergent parser decisions.

## Required corrections and gate

Before a new acceptance review, define and add independent, harmless
conformance cases for the Q cursor and boundary truth table; identifier and
exclusion semantics; decoded alphabet, run ordering, caps, and partial
coverage; JWT parsing; table-row separators and raw spans; and the exact
stderr grammar, newline, count/deduplication basis, rule ordering, JSON type
and location, and mutation lifecycle outcomes. Recompute affected hashes and
review the complete revised ruleset at its new exact SHA-256.

**Preserve the BR-T39 through BR-T44 gate.** All six beads remain blocked
until these findings are resolved, fixtures are recorded, and the revised
exact-hash contract receives independent acceptance. This review makes no
dependency-graph changes and authorizes no implementation or release work.

## Clean-room provenance

This review uses only the named repository specifications, ADR, and prior
independent review artifacts as contract evidence. No other bead
implementation's code or behavioral material was consulted. No provenance
exception or contamination event occurred.

## Final decision

**ACTIONABLE REJECTED** — `research/specs/secret-ruleset-v4.md` at SHA-256
`bad7c8102086d8128465e284b839673ceceb84b28e3ea7bf55835dfaee5ad5a9`.
NEEDLE permits the stderr channel; the exact write-time notice and machine
output are not yet compatible as a reproducible contract. Preserve the
BR-T39 through BR-T44 release gate.

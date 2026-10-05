# Ruleset 4 section 5.2 stderr compatibility and final decision

Date: 2026-10-05.

Reviewer: OpenAI Codex, independent of the specification authors and
implementation owners.

**Decision: REJECTED.** This decision applies to the complete
`research/specs/secret-ruleset-v4.md` at the exact SHA-256 below. The stderr
channel choice is compatible with NEEDLE, but section 5.2 does not define a
reproducible notice or machine-output contract. The focused findings also
leave blocking and advisory behavior insufficiently deterministic for
acceptance.

## Scope and method

I reviewed section 5.2 against `needle-cli-contract-v1`, ADR-025, the accepted
`secret-rejection-v1` contract, and the three committed focused records listed
below. I also checked ADR-023 and ADR-024 for the ruleset and view constraints
that interact with those findings. Hashes identify complete file bytes, not
excerpts. I used no other bead implementation's source, tests, fixtures, output,
or prose, and did not rely on implementation behavior.

## Exact reviewed inputs

| Artifact | SHA-256 |
| --- | --- |
| `research/specs/secret-ruleset-v4.md` | `bad7c8102086d8128465e284b839673ceceb84b28e3ea7bf55835dfaee5ad5a9` |
| `research/specs/needle-cli-contract-v1.md` | `64a4057bdec893d81e42a3f23bff86f225a302fc3b5fec18d8ab5cf414ad7233` |
| `docs/adr/023-widen-blocking-ruleset-to-observed-credential-formats.md` | `39d9174173df3aadc7c268bdd27d139e1091cc12559e20b9d098037683a7359a` |
| `docs/adr/024-match-on-a-normalized-view.md` | `508989f43c984b3a69ef529eaaf0d96ed1caa44873857e91b14aeaaa61de9c86` |
| `docs/adr/025-surface-advisory-findings-and-partial-scan-coverage.md` | `e13bda45279d2b875c9c2dce6d2bdc3e0471e91512cff22ec40487f1a0de4a33` |
| `research/specs/secret-rejection-v1.md` | `f6aa7639a8ef1dd509b431853abf64db78d0923ef9e9d59b09e0a4e0e55df231` |
| `docs/reviews/r038-specification-acceptance-2026-09-03.md` | `a7622b292729a33788ea36340946fd8be5495554510ecc839aebf8763903da8a` |

The focused records reviewed were:

| Focused review | SHA-256 | Finding and disposition carried into this review |
| --- | --- | --- |
| `docs/reviews/beadrs-b60f83c5-ruleset-v4-randomness-excluded-identifiers-review-2026-10-05.md` | `b0a4cb137136e08772a81cf9d32df8481069c9b56f619ee73e45dcb3991c3a48` | Actionable rejection: Q word-run consumption and required truth-table rows are unspecified; identifier lexing, exclusions, precedence, and bead-ID shape are ambiguous. |
| `docs/reviews/beadrs-524683b8-ruleset-v4-decoded-jwt-table-row-review-2026-10-04.md` | `a0b8293c6152647ce9807fb6afcf5f65ad1b8b0320c3ab20511b50cc830ad8be` | Actionable rejection: decoded-run selection and decoding grammar, JWT acceptance details, table-row parsing, and their independent boundary fixtures are insufficiently specified. |
| `docs/reviews/beadrs-ced47d50-ruleset-v4-decoded-bounds-structured-review-2026-10-04.md` | `ecee4cf5aefa411b35be612efaf205c5545c87e03b84cbd2fbac91ad21a65c05` | Actionable rejection: run and byte caps have no deterministic over-limit or coverage disposition; JWT and table-row boundaries and raw spans remain ambiguous; fixtures do not cover these cases. |

The scoped raw-range review remains supporting evidence, not a waiver:
`docs/reviews/beadrs-db0131b7-ruleset-v4-raw-range-redact-review-2026-10-04.md`
has SHA-256 `0746141e99d947464e3233d1da8174e7698c8c8b7d78215da4fd8245d72a17c0`
and accepts section 3.4's raw covering-range construction only. The earlier
stderr review,
`docs/reviews/beadrs-3ecefbce-ruleset-v4-stderr-final-review-2026-10-04.md`,
has SHA-256 `318228a896d30b18c608f9bf1055bb38ec7ca0b06d2eb12eb03d37cd6270757f`;
its channel-versus-serialization distinction is reaffirmed here against the
current focused records.

## Section 5.2 standard-error result

**Channel and routing: PASS. Exact notice contract: REJECTED as underspecified.**

`needle-cli-contract-v1` requires diagnostics to go to standard error and not
corrupt machine-readable standard output. It does not require standard error
to be empty after a successful command. Therefore, one redacted advisory
diagnostic on standard error is compatible with NEEDLE's process-channel
rule. `secret-rejection-v1` also permits diagnostics while requiring that
matched bytes never appear in output, and retains complete pre-transaction
scanning and atomic rejection for unacknowledged blocking findings. ADR-025
supports the intent of surfacing a successful advisory finding without
changing exit status.

Those sources do not define a stable line or complete machine-output schema.
Section 5.2 requires a line naming the count, rule identifiers, and
`bead doctor --scope secrets`, and forbids matched bytes. It does not define
the line's literal UTF-8 grammar, separators, rule-identifier ordering,
newline behavior, or coexistence with other diagnostics. Nor does it define
whether the count is per finding, per distinct rule/range, or per fingerprint
after caps and cross-view deduplication. ADR-025's “additive count” and the
`secret_scan.advisory_findings` member do not establish the member's JSON type
or location. The phrase “standard output text is unchanged” also needs to
distinguish human-readable text from machine-readable JSON that gains a member.

Thus the requested stderr **channel** is compatible, but two conforming
implementations can emit different lines or JSON values. Section 5.2 cannot be
accepted as a reproducible compatibility contract until these details are
normative. Any resolution must keep matched bytes out of diagnostics as the
accepted v1 contract requires.

## Trace of the focused findings

The three current focused records produce independent blockers; none is
resolved by stderr being an allowed channel.

1. **Q and excluded identifiers — `beadrs-b60f83c5`: actionable rejection.**
   Section 2.2 does not state how much a successful word-run match consumes
   or how scanning advances, and section 7 lacks exact rows for its named
   classes and `m=12/16/20` boundaries. Section 4.4 leaves identifier
   components, case handling, keyword/suffix precedence, plural and separator
   behavior, bead-ID shape, and exclusion scope/order open. These choices can
   change blocking versus advisory versus absent findings, their fingerprints,
   and ADR-025 counts.

2. **Decoded views, JWTs, and table rows — `beadrs-524683b8`: actionable
   rejection.** Section 3.2 does not fully define the base64/base64url
   grammar, decoded-run selection at the cap, or treatment of an oversized
   run. Section 4.1 leaves JWT decoding and acceptance boundaries open, and
   section 4.4 leaves table marker, pipe/cell, value, and span behavior
   ambiguous. The independent fixtures do not establish the required positive
   and negative cases. Implementations can therefore disagree on findings
   and redaction ranges.

3. **Decoded bounds and structured blocking — `beadrs-ced47d50`: actionable
   rejection.** The run-count and per-run limits lack deterministic
   over-limit behavior and visible partial-coverage status. JWT header/JSON
   rules and decoded private-key grammar are incomplete. Pipe delimiters,
   marker/separator precedence, punctuation trimming, and exact table-row
   spans are unresolved. The available fixture has no decoded-limit, JWT,
   table-row, or raw-span vectors. These findings reaffirm and extend the
   decoded/JWT/table-row gaps in `beadrs-524683b8`.

The scoped section 3.4 acceptance establishes how a selected derived match can
map to raw coordinates and be revalidated for redaction. It does not decide
which match is selected, resolve these parser ambiguities, or supply the
missing fixtures.

## Consolidated dispositions

**Compatibility: ACTIONABLE REJECTION for this exact ruleset hash.** The v4
direction can be a separately versioned extension that preserves the accepted
v1 closed/offline scanner, full pre-write scan, atomic blocking rejection,
raw-byte finding coordinates, fingerprint-scoped acknowledgment and
redaction, and value-free diagnostics. The reviewed contract does not
deterministically specify Q outcomes, decoded selection and coverage,
structured spans, or advisory notice/count serialization. Compatibility and
conformance are not established at this hash.

**Threat model: ACTIONABLE REJECTION.** An implementation may miss a credential
at an unspecified Q or decoded-run boundary, after an unspecified run cap, or
under ambiguous JWT/table-row parsing. Other parsers may reject ordinary text
or select different raw ranges, changing fingerprints and the bytes selected
for redaction. Unspecified advisory counts and line serialization make
successful writes harder to triage consistently. Value-free diagnostics and
redaction revalidation remain required controls, but cannot repair missed
findings or divergent parsers.

## Required resolution and release gate

Before a new acceptance review, define Q scanning and identifier/exclusion
grammar with exact expected rows; define decoded alphabet, run order, cap and
partial-coverage behavior; close JWT/private-key and table-row parsing with
raw-span vectors; define the exact stderr grammar and JSON type/location,
count/dedup basis, and stable rule order; and add independent, harmless
section-specific conformance fixtures. Recompute the affected hashes and
review the revised complete ruleset at its new exact SHA-256.

**BR-T39 through BR-T44 remain blocked.** Their blocking relationships remain
in place; this rejection does not accept or release any of them.

**Final decision: REJECTED** for
`research/specs/secret-ruleset-v4.md` at SHA-256
`bad7c8102086d8128465e284b839673ceceb84b28e3ea7bf55835dfaee5ad5a`.

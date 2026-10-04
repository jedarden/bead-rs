# BR-T35 aggregate compatibility and threat decision — secret ruleset v4

Date: 2026-10-04.

Reviewer: OpenAI Codex. The reviewer is distinct from the authors of
`secret-ruleset-v4`, ADR-023 through ADR-025, `secret-rejection-v1`, and the
implementation owners.

Decision: **REJECTED** at the exact ruleset-v4 hash recorded below.

## Review boundary and method

This record is the aggregate decision after the focused contract reviews. It
uses only the repository's ADRs, accepted specifications, independently
authored fixture material, and the focused review records listed below. It
does not inspect, copy, translate, paraphrase, or derive behavior from any
other bead implementation, its source, tests, fixtures, output, or prose.

The decision is bound to the complete ruleset-v4 file, not to an extracted
section. Every byte change to the target or its reviewed inputs requires a new
hash and a new review.

## Exact reviewed inputs

The target contract is:

```text
research/specs/secret-ruleset-v4.md
SHA-256: bad7c8102086d8128465e284b839673ceceb84b28e3ea7bf55835dfaee5ad5a9
```

The normative and baseline inputs reviewed for this aggregate are:

| Artifact | SHA-256 |
| --- | --- |
| `research/specs/secret-ruleset-v4.md` | `bad7c8102086d8128465e284b839673ceceb84b28e3ea7bf55835dfaee5ad5a9` |
| `docs/adr/023-widen-blocking-ruleset-to-observed-credential-formats.md` | `39d9174173df3aadc7c268bdd27d139e1091cc12559e20b9d098037683a7359a` |
| `docs/adr/024-match-on-a-normalized-view.md` | `508989f43c984b3a69ef529eaaf0d96ed1caa44873857e91b14aeaaa61de9c86` |
| `docs/adr/025-surface-advisory-findings-and-partial-scan-coverage.md` | `e13bda45279d2b875c9c2dce6d2bdc3e0471e91512cff22ec40487f1a0de4a33` |
| `research/specs/secret-rejection-v1.md` | `f6aa7639a8ef1dd509b431853abf64db78d0923ef9e9d59b09e0a4e0e55df231` |
| `research/specs/needle-cli-contract-v1.md` | `64a4057bdec893d81e42a3f23bff86f225a302fc3b5fec18d8ab5cf414ad7233` |
| `research/specs/historical-redaction-v1.md` | `5658ac80cf9594283ddf65742aff2f4b2020a0a7869a612ee2230ed99033a016` |
| `man/man1/bead-redact.1` | `957b8b72fee4ecbd0a71520b045794b219dbaae5c5d2d0d96eef3dd60f746153` |
| `research/fixtures/README.md` | `dfd99a8d7a5272dc92e5ec5be16a5f50a426e507bcf3ea866e41bbd910284b64` |
| `research/fixtures/secret-write-boundary-v1.json` | `c75f4dbf6242ced3c7e21e501e1de7a0fbef2a5371c601957c809043f0996710` |
| `docs/reviews/r038-specification-acceptance-2026-09-03.md` | `a7622b292729a33788ea36340946fd8be5495554510ecc839aebf8763903da8a` |

The focused review records integrated here are:

| Focused review | SHA-256 | Aggregate disposition |
| --- | --- | --- |
| `docs/reviews/br-t41-randomness-excluded-identifiers-independent-review-2026-10-04.md` | `5d222327393f9155e5ee21fd2474952b5a5a5f7c30efb0ba33c6b068eb7e28e4` | Actionable rejection of qualifier and exclusion semantics |
| `docs/reviews/br-t35-randomness-qualifier-truth-table-independent-review-2026-10-04.md` | `bf77cd450a7ed887dddd4ddb468e2aee66b6162067b193cc3223e816396dda60` | Actionable rejection of the qualifier truth-table slice |
| `docs/reviews/beadrs-524683b8-ruleset-v4-decoded-jwt-table-row-review-2026-10-04.md` | `a0b8293c6152647ce9807fb6afcf5f65ad1b8b0320c3ab20511b50cc830ad8be` | Actionable blockers for decoded views, JWTs, and table rows |
| `docs/reviews/beadrs-db0131b7-ruleset-v4-raw-range-redact-review-2026-10-04.md` | `0746141e99d947464e3233d1da8174e7698c8c8b7d78215da4fd8245d72a17c0` | Narrow acceptance of section 3.4 range construction only |
| `docs/reviews/beadrs-3ecefbce-ruleset-v4-stderr-final-review-2026-10-04.md` | `318228a896d30b18c608f9bf1055bb38ec7ca0b06d2eb12eb03d37cd6270757f` | Actionable rejection of the write-time output contract |

The focused records all identify the same target ruleset hash. The supplied
fixture is a nine-case v1 write-boundary manifest; it is not a v4 detector
truth table and contains no Q, exclusion, decoded-view, JWT, table-row, or
advisory-output cases.

## Traced focused findings

| Finding and review record | Evidence retained | Disposition |
| --- | --- | --- |
| `beadrs-cf4b2115` and its scoped re-review `beadrs-7ee540ac`; `br-t41-randomness-excluded-identifiers-independent-review-2026-10-04.md` | Section 2.2 does not define maximal word-run consumption/advancement or publish exact Q rows. Section 4.4 leaves identifier components, case folding, exclusion boundaries, keyword/suffix precedence, separator equivalence, plural handling, and assignment/value-span precedence unresolved. | **Actionable rejection.** Independent implementations can disagree on whether a labelled value reaches Q, whether it blocks, and which raw bytes are fingerprinted/redacted. |
| `beadrs-3b3bde44`; `br-t35-randomness-qualifier-truth-table-independent-review-2026-10-04.md` | Synthetic values reach the stated inclusive `m` boundaries under one explicit run-consumption reading, but the specification does not select that reading. The required exact rows and independent committed fixture are absent. | **Actionable rejection.** The arithmetic evidence is review evidence, not a normative truth table or conformance oracle. |
| `beadrs-ba83ea53` and its scoped re-review `beadrs-524683b8`; `beadrs-524683b8-ruleset-v4-decoded-jwt-table-row-review-2026-10-04.md` | Section 3.2 does not define qualifying-run selection after the 64-run cap, over-65,536-byte handling, slot accounting, or a complete lenient base64 grammar. JWT padding/duplicate-member rules and table marker, pipe/cell, value-selection, and escaping rules are incomplete. | **Actionable rejection.** Encoded credentials and structured rows can produce divergent or missing findings and raw ranges. |
| `beadrs-e78cea45` and its scoped re-review `beadrs-db0131b7`; `beadrs-db0131b7-ruleset-v4-raw-range-redact-review-2026-10-04.md` | Independent harmless vectors support the section 3.4 raw half-open covering-range construction and v1 fingerprint/redaction revalidation for unchanged content. | **Accepted only for section 3.4.** This narrow result does not establish runtime v4 conformance and does not resolve the URI percent-decoding disagreement between the normalized-view and derived-view rules. |
| `beadrs-aaaebec9` and its final scoped review `beadrs-3ecefbce`; `beadrs-3ecefbce-ruleset-v4-stderr-final-review-2026-10-04.md` | NEEDLE permits diagnostics on stderr and additive JSON, but section 5.2 does not define the advisory JSON type/location, count and deduplication identity, unique rule-ID ordering, or exact one-line UTF-8 stderr grammar. | **Actionable rejection.** Consumers cannot reproducibly parse, count, compare, or triage successful mutation notices without risking the v1 value-free diagnostic boundary. |

These findings preserve the earlier v1 boundary review: complete
pre-transaction scanning, atomic rejection, exact-fingerprint
acknowledgment/redaction, raw-byte coordinates, and diagnostics without
matched values remain required invariants. The section 3.4 acceptance narrows
one earlier range-construction concern; it does not waive any other blocker.

## Compatibility disposition

**ACTIONABLE REJECTION.** The v4 direction is compatible in principle as an
explicitly versioned extension of the accepted v1 contract. It can preserve
the v1 closed/offline scanner, raw-byte coordinates and fingerprints,
fingerprint-scoped acknowledgment and redaction revalidation, complete
pre-transaction scanning, atomic rejection, and additive machine output. The
focused raw-range review supports section 3.4's mapping for that section.

Compatibility is not established for the exact ruleset hash above. Undefined
Q and labelled-assignment parsing, decoded-run selection and base64 grammar,
JWT and table-row boundaries, URI percent-decoding, and advisory JSON/stderr
serialization allow conforming implementations to produce different verdicts,
fingerprints, redaction reaches, counts, or rule lists. No v4 compatibility or
conformance claim may be made for this hash.

## Threat-model disposition

**ACTIONABLE REJECTION.** The unresolved Q and exclusion semantics can miss a
credential in a labelled context or reject ordinary metadata. Unresolved
decoded-run limits and grammar can miss or inconsistently classify encoded
credentials. Ambiguous JWT and table-row parsing changes detection and raw
redaction ranges. URI-view disagreement can change whether a finding exists
and which bytes are replaced. Advisory count, type, location, deduplication,
and ordering ambiguity can cause missed triage or incompatible automation.

Diagnostics must continue to omit matched bytes and locations. Section 3.4's
raw covering ranges provide a defensible boundary for its accepted slice, but
the remaining gaps prevent this exact ruleset from serving as a reproducible
release security boundary. Q is a bounded accidental-disclosure control, not
proof that arbitrary text is secret-free; excluded labels and values that fail
Q remain explicit false-negative boundaries until the contract is revised.

## Required blockers before acceptance

The specification and independent fixtures must be revised to:

1. Define maximal Q word-run matching and advancement, exact truth-table rows
   for every named class and predicate boundary, and every `m` call site.
2. Formalize identifier components, case folding, component boundaries,
   keyword/suffix and exclusion precedence, separator equivalence, plural
   handling, assignment precedence, and exact value/quote/trimming spans.
3. Specify decoded-run alphabet and grammar, run ordering and selection after
   the 64-run cap, over-limit disposition, invalid-byte handling, JWT header
   and JSON-member rules, and table-row marker/pipe/value/escape semantics.
4. Resolve URI percent-decoding relative to the normalized view and retain
   deterministic raw source mapping and redaction-range invariants.
5. Define the advisory JSON type and location, count and deduplication basis,
   stable unique rule-ID ordering, exact stderr serialization/newline behavior,
   coexistence with other diagnostics, and `off`-mode absence.
6. Add independent synthetic positive, negative, boundary, and near-miss
   fixtures for each of those contracts, then review the changed bytes at a
   new exact SHA-256.

## BR-T39 through BR-T44 release gate

Because this aggregate decision is **REJECTED**, BR-T39 through BR-T44 remain
blocked unless and until the reviewed contract is accepted by a new
independent review at a new exact hash. The scoped section 3.4 acceptance does
not release or bypass that gate. No bead in BR-T39 through BR-T44 may be
treated as accepted on the current ruleset hash.

## Final decision

**REJECTED — actionable specification and fixture revision required for
`research/specs/secret-ruleset-v4.md` at SHA-256
`bad7c8102086d8128465e284b839673ceceb84b28e3ea7bf55835dfaee5ad5a9`.**

The compatibility disposition is actionable rejection. The threat-model
disposition is actionable rejection. The section 3.4 raw-range result is
accepted only as a scoped slice. BR-T39 through BR-T44 remain blocked unless
the revised reviewed contract receives independent acceptance.

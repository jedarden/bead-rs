# BR-T35 integrated independent review — secret ruleset v4

Date: 2026-10-04.

Reviewer: OpenAI Codex, independent of the ADR/specification authors and
implementation owners.

## Review boundary and method

This review integrates the four focused BR-T35 findings listed below and their
subsequent scoped re-reviews where available. It checks the exact repository
inputs identified in this record. Evidence comes only from repository ADRs,
accepted specifications, the repository's independent fixture material, the
focused review records, and harmless synthetic range examples in the raw-range
review. No other bead implementation's source, tests, fixtures, output, or
prose was inspected or used.

Hashes below identify the committed inputs reviewed for this decision. The
fixture JSON was parsed and checked for the nine expected write-boundary cases;
it is not a ruleset-v4 detector truth table.

## Reviewed input identities

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

The three committed focused-review artifacts also used to integrate the
evidence are:

| Focused review artifact | SHA-256 |
| --- | --- |
| `docs/reviews/br-t41-randomness-excluded-identifiers-independent-review-2026-10-04.md` | `5d222327393f9155e5ee21fd2474952b5a5a5f7c30efb0ba33c6b068eb7e28e4` |
| `docs/reviews/beadrs-524683b8-ruleset-v4-decoded-jwt-table-row-review-2026-10-04.md` | `a0b8293c6152647ce9807fb6afcf5f65ad1b8b0320c3ab20511b50cc830ad8be` |
| `docs/reviews/beadrs-db0131b7-ruleset-v4-raw-range-redact-review-2026-10-04.md` | `0746141e99d947464e3233d1da8174e7698c8c8b7d78215da4fd8245d72a17c0` |

The exact target of the integrated decision is:

```text
research/specs/secret-ruleset-v4.md
SHA-256: bad7c8102086d8128465e284b839673ceceb84b28e3ea7bf55835dfaee5ad5a9
```

## Integrated predecessor findings

| Focused finding | Integrated evidence and disposition |
| --- | --- |
| `beadrs-cf4b2115`, randomness qualifier and identifier truth table; reaffirmed by `beadrs-7ee540ac` and its review artifact above | **Actionable rejection.** Section 2.2 does not define word-run consumption/advancement or provide the required exact Q boundary rows. Section 4.4 leaves identifier/exclusion tokenization and assignment/value-span precedence underspecified. The write-boundary fixture contains no Q or exclusion cases. |
| `beadrs-ba83ea53`, decoded views, JWTs, and table rows; re-reviewed by `beadrs-524683b8` and its review artifact above | **Actionable rejection.** Section 3.2 does not define qualifying-run selection, over-limit disposition/slot accounting, or a complete lenient base64 grammar. JWT and table-row blocking intent is present, but decoded-view reach and pipe-row parsing are not deterministic. The fixture set has no v4 decoded/JWT/table-row truth cases. |
| `beadrs-e78cea45`, raw ranges and `bead redact`; narrowed by `beadrs-db0131b7` and its review artifact above | **Section 3.4 accepted for range construction only.** The later review's independent vectors support valid raw half-open covering ranges and the v1 fingerprint/redaction revalidation contract. This scoped acceptance does not establish runtime conformance or resolve the earlier URI percent-decoding ambiguity between sections 3.2 and 4.3. The wider decoded-run redaction cost remains an explicit privacy/content-preservation tradeoff. |
| `beadrs-aaaebec9`, write-time advisory output and threat model | **Actionable rejection.** ADR-025 and section 5.2 do not fix the advisory JSON type/location, count and deduplication basis, stable rule order, or exact one-line stderr serialization. NEEDLE permits stderr diagnostics and additive JSON, but does not supply those missing semantics. |

Together, the focused findings leave the full ruleset-v4 contract unable to
produce deterministic blocking/advisory verdicts and machine-readable notices
across independent implementations. The scoped section 3.4 acceptance removes
the earlier raw-range-construction concern for that section; it does not
resolve the independent URI-view ambiguity or the remaining findings above.

## Compatibility disposition

**ACTIONABLE REJECTION.** The v4 direction is compatible in principle as an
explicitly versioned extension that preserves the accepted v1 contract's raw
byte coordinates and fingerprints, exact-fingerprint acknowledgments,
redaction revalidation, complete pre-transaction scan, atomic rejection, and
additive machine output. The focused range review supports the section 3.4
mapping and `bead redact` interface for unchanged content. However, unresolved
qualifier/parser semantics, decoded-run selection and decoding, URI
percent-decoding, and advisory output prevent independent implementations from
reproducing the full v4 contract. No compatibility or conformance claim is
accepted for the target hash.

## Threat-model disposition

**ACTIONABLE REJECTION.** Undefined Q and label parsing can miss a labelled
credential or reject ordinary metadata. Undefined decoded-run selection and
grammar can miss encoded credentials or make JWT results implementation
dependent. URI-view disagreement can change whether a finding exists, while
divergent parsing changes its raw fingerprint and redaction reach. Advisory
count/type/order ambiguity can break secret triage and automation; all emitted
diagnostics must continue to omit matched bytes. Section 3.4's raw covering
ranges provide a defensible redaction boundary for its scoped transforms, but
the remaining gaps prevent this exact ruleset from serving as a reproducible
release security boundary.

## Decision

**REJECTED — actionable specification and fixture revision required.**

BR-T39 through BR-T44 must remain blocked. Before release review, define the Q
truth table and maximal word-run semantics; formalize identifier, exclusion,
assignment, and value-span parsing; specify decoded-run ordering, overflow,
and base64 grammar; resolve URI percent-decoding relative to the normalized
view; define advisory JSON and stderr serialization; and add synthetic v4
fixtures for these boundaries. Then review the changed bytes at a new exact
hash. The scoped section 3.4 acceptance does not waive those requirements.

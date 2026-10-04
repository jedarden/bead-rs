# BR-T35 integrated independent review — secret ruleset v4

Date: 2026-10-03.

Reviewer: OpenAI Codex, independent of the ADR/specification authors and
implementation owners.

## Review boundary and method

This is the integrated BR-T35 decision after the four focused predecessor
reviews. The review used only the repository's ADRs, accepted specifications,
the supplied repository fixture, and the four predecessor review records named
below. No other implementation's source, tests, fixtures, scanner output, or
prose was inspected or used.

The artifact hashes below were recomputed from the committed `HEAD` using
`git show HEAD:<path> | sha256sum`. The section references were checked against
those exact bytes. The supplied JSON fixture was checked as a nine-case
manifest; it is a write-boundary fixture, not a ruleset-v4 truth table.

## Reviewed artifact identities

| Artifact | SHA-256 |
| --- | --- |
| `research/specs/secret-ruleset-v4.md` | `bad7c8102086d8128465e284b839673ceceb84b28e3ea7bf55835dfaee5ad5a9` |
| `docs/adr/023-widen-blocking-ruleset-to-observed-credential-formats.md` | `39d9174173df3aadc7c268bdd27d139e1091cc12559e20b9d098037683a7359a` |
| `docs/adr/024-match-on-a-normalized-view.md` | `508989f43c984b3a69ef529eaaf0d96ed1caa44873857e91b14aeaaa61de9c86` |
| `docs/adr/025-surface-advisory-findings-and-partial-scan-coverage.md` | `e13bda45279d2b875c9c2dce6d2bdc3e0471e91512cff22ec40487f1a0de4a33` |
| `research/specs/secret-rejection-v1.md` | `f6aa7639a8ef1dd509b431853abf64db78d0923ef9e9d59b09e0a4e0e55df231` |
| `research/specs/needle-cli-contract-v1.md` | `64a4057bdec893d81e42a3f23bff86f225a302fc3b5fec18d8ab5cf414ad7233` |
| `research/fixtures/secret-write-boundary-v1.json` | `c75f4dbf6242ced3c7e21e501e1de7a0fbef2a5371c601957c809043f0996710` |

The target artifact for this decision is therefore exactly:

```text
research/specs/secret-ruleset-v4.md
SHA-256: bad7c8102086d8128465e284b839673ceceb84b28e3ea7bf55835dfaee5ad5a9
```

## Integrated predecessor findings

All four focused findings are actionable rejections of the current hash:

1. `beadrs-cf4b2115` reviewed ADR-023 Decision item 5, ruleset-v4 §§2.2
   and 4.4, and the accepted v1 contract. It found no published Q/exclusion
   truth table or v4 fixture rows, and found undefined word-run consumption,
   identifier and bead-ID shape, overlapping assignment operators, whitespace
   precedence, pipe-row delimiter precedence, and exact value/quote/trimming
   spans. The integer predicates are deterministic only after those parsing
   choices are fixed.

2. `beadrs-ba83ea53` reviewed ADR-023 Decision items 2–5, ADR-024 Decision
   items 1–5, ruleset-v4 §§3.1–3.4, 4.1, 4.4, and 4.5, plus the supplied
   fixture. It found that the first-64 decoded-run selection, over-limit
   handling, run-slot accounting, independent-run boundaries, and lenient
   base64 grammar are unspecified. Direct JWT and unambiguous table-row
   intent is present, but decoded-view JWTs and pipe-form rows are not
   deterministic. It also confirms the fixture has no v4 decoded/JWT/table
   truth rows.

3. `beadrs-e78cea45` reviewed ruleset-v4 §3.4 against ADR-024 and accepted
   secret-rejection-v1. It found that a derived-view match lacks a normative
   per-byte source map and half-open raw envelope, including nonempty-range and
   UTF-8-boundary invariants. It also found that the URI requirement to apply
   percent-decoding conflicts with the normalized view's printable-only
   decoding. Until resolved, a fingerprint may not be portable to
   `bead redact` even though redaction revalidation rejects mismatched ranges.

4. `beadrs-aaaebec9` reviewed ADR-025 Decision item 2 and consequence note,
   ruleset-v4 §5.2, NEEDLE CLI contract v1, and secret-rejection-v1. It found
   that `secret_scan.advisory_findings` has no normative JSON type, placement,
   count/deduplication basis, or rule-ID order, and that the exact one-line
   stderr serialization is unspecified. Stderr routing and additive JSON are
   compatible in principle, but the exact contract is not.

These records agree on the release consequence: the current contract cannot
produce deterministic findings, fingerprints, redaction ranges, or advisory
output across independent conforming implementations.

## Compatibility disposition

**ACTIONABLE REJECTION.** The proposed ruleset-v4 direction is compatible in
principle with the accepted v1 contract only if it preserves v1's raw-byte
fingerprints and coordinates, exact-fingerprint acknowledgments, redaction
revalidation, complete pre-transaction blocking scan, atomic rejection, and
additive machine output. The current artifact does not specify enough of the
new qualifier, parser, derived-view mapping, URI decoding, or advisory-output
contract to make that compatibility reproducible. No v4 compatibility or
conformance claim is accepted for the hash above.

## Threat-model disposition

**ACTIONABLE REJECTION.** Undefined word-run and row parsing can change both
the blocking verdict and the bytes selected for redaction. Undefined decoded
run selection and base64 grammar can create detection gaps for encoded JWTs or
private keys. Undefined source mapping and URI decoding can make a finding
unredactable or cause divergent raw-byte replacement. Undefined advisory
count/type/order can cause missed triage or incompatible automation; exposing
unredacted values would also violate the accepted v1 no-value diagnostic rule.
The offline, bounded, one-level-decoding, redacted-output goals are sound, but
they are not enforceable as a deterministic threat-model boundary until the
listed semantics are made normative.

## Decision

**REJECTED — actionable specification revision required.**

BR-T39 through BR-T44 must remain blocked. Before any of those beads can
proceed, revise the specification and its fixtures to define:

- the normative Q and exclusion truth table, word-run/maximal-consumption
  semantics, identifier and bead-ID grammar, assignment precedence, and exact
  value spans;
- decoded-run selection and over-limit behavior, independent-run boundaries,
  and an exact lenient-base64 grammar, with JWT and table-row truth rows;
- the derived-view source-span map, raw half-open envelope invariants, and the
  URI percent-decoding relationship;
- the exact advisory JSON type/location/count semantics and stable redacted
  stderr record; and
- a new exact-hash independent review after those bytes change.

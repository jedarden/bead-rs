# BR-T35 decoded bounds and structured blocking review — 2026-10-05

**Decision: ACTIONABLE REJECTED**

This is a focused clean-room review of sections 3.2, 4.1, and 4.4 of the
ruleset-v4 proposal. The review uses only the named ADRs, the accepted
secret-rejection-v1 artifact and acceptance record, and the permitted fixture
material. No other implementation was consulted. Candidate credentials are
not recorded; boundary cases below describe shapes and expected parser
decisions only.

## Inputs and exact hashes

These hashes identify the exact bytes reviewed. A change to any input requires
a new review.

| Input | SHA-256 |
|---|---|
| `docs/adr/023-widen-blocking-ruleset-to-observed-credential-formats.md` | `39d9174173df3aadc7c268bdd27d139e1091cc12559e20b9d098037683a7359a` |
| `docs/adr/024-match-on-a-normalized-view.md` | `508989f43c984b3a69ef529eaaf0d96ed1caa44873857e91b14aeaaa61de9c86` |
| `docs/adr/025-surface-advisory-findings-and-partial-scan-coverage.md` | `e13bda45279d2b875c9c2dce6d2bdc3e0471e91512cff22ec40487f1a0de4a33` |
| `research/specs/secret-ruleset-v4.md` | `bad7c8102086d8128465e284b839673ceceb84b28e3ea7bf55835dfaee5ad5a9` |
| `research/specs/secret-rejection-v1.md` (accepted v1 baseline) | `f6aa7639a8ef1dd509b431853abf64db78d0923ef9e9d59b09e0a4e0e55df231` |
| `docs/reviews/r038-specification-acceptance-2026-09-03.md` | `a7622b292729a33788ea36340946fd8be5495554510ecc839aebf8763903da8a` |
| `research/fixtures/README.md` | `23af7f702d57f71cce99ec9e13807cce66285be996ba277f8c2074c8c9bc5b7d` |
| `research/fixtures/recovery-finding-quarantine-v1.json` (accepted, value-free) | `aa50162029c0ec3899b288a1006981e05d745038ba47f16c186be0c7ebb03d6a` |
| `research/fixtures/secret-write-boundary-v1.json` | `c75f4dbf6242ced3c7e21e501e1de7a0fbef2a5371c601957c809043f0996710` |

The qualifier review is closed before this review, but its disposition does
not resolve the surfaces here. Section 4.4 calls the still-reviewed `Q` at
`m=20`, so table-row blocking cannot be accepted independently of a
deterministic qualifier contract.

## Section 3.2 — decoded-view bounds

The intended selection is understandable but not executable to one result:

- Scan maximal runs in the normalized view whose bytes belong to the
  base64/base64url alphabet and whose run length is at least 40.
- Decode each candidate once, retain it only when at least nine tenths of the
  decoded bytes are printable ASCII or ASCII whitespace, and expose only the
  provider-format and private-key rules to that view.
- Limit the work to 64 runs per field and 65,536 bytes per run.

The caps do not define what happens at their boundaries. In particular, the
specification does not say:

1. Whether the 64-run cap is the first 64 candidates in left-to-right order,
   the 64 longest candidates, or another deterministic selection.
2. Whether the cap counts every qualifying alphabet run before decoding, only
   successfully decoded runs, or only runs retained by the printable-byte
   test. A field with 65 candidates therefore has no specified scan result.
3. Whether a run longer than 65,536 source bytes is skipped, truncated to a
   prefix, rejected as incomplete coverage, or handled another way. A
   credential in the suffix of such a run can consequently be either missed
   or unexpectedly considered, depending on an implementation choice.
4. Whether the 65,536-byte limit is measured on the encoded source run or the
   decoded output, and how an invalid-length or padded run is handled by
   “lenient base64 decoding.” The overlapping base64/base64url alphabets and
   padding grammar are not closed by that phrase.
5. How skipped or over-limit decoded regions are represented in coverage. ADR-
   025 defines coverage for diagnostic sources, but section 3.2 does not
   provide a derived-view coverage disposition.

The one-level rule and the provider/private-key-only rule are compatible with
ADR-024: arbitrary decoded bytes do not acquire a source label, so
`credential-assignment` must not run there. The bounded work also supports the
accepted v1 requirement for an offline deterministic scanner. The missing
selection and over-limit policy prevent the proposal from meeting either
property in practice.

## Section 4.1 — JSON web token blocking

The intended positive is a provider-format finding: three body-alphabet
segments, each at least eight bytes, with the first two beginning `eyJ`, and a
first segment that base64url-decodes to a JSON object containing an `alg`
member. It is therefore eligible on raw, normalized, and dewrapped views, and
on the decoded view when an outer encoded run produces the token. Labelled
assignment qualification is not involved.

That intent does not close the following boundaries:

- “JSON object with an `alg` member” does not specify JSON parser strictness,
  duplicate-member handling, whether `alg` must be a string, whether null or an
  empty value is accepted, or whether trailing decoded bytes are forbidden.
- The shape says three segments, but the generic after-match boundary rejects
  only a following byte in the rule body alphabet. A fourth dot is outside
  that alphabet, so a parser that searches for the stated three segments can
  accept the first three segments of a four-segment near miss. Exact
  three-segment matching must be stated independently of the generic boundary
  predicate.
- Section 3.2 says “lenient base64” for decoded runs while section 4.5 refers
  to base64url for the header. Padding, canonical encoding, and non-zero
  unused bits are not specified for the JWT header. The second segment is
  required to begin `eyJ`, but no payload JSON validation is stated; that
  distinction should be explicit.
- A direct token's raw finding should cover precisely the three segments and
  the two separators, excluding surrounding bytes. When the token is produced
  by a decoded run, section 3.4 instead requires the raw range of the whole
  encoded run. The latter is clear, but the former and the exact treatment of
  normalized/dewrapped delimiters need a value-free fixture with expected
  start/end offsets.

These are compatibility boundaries, not merely parser preferences. Different
answers change whether a mutation is rejected, change the rule/range pair used
in the v1 fingerprint, and can make an acknowledgment or `bead redact`
selection fail to identify the same stored bytes.

## Section 4.4 — labelled assignments and table rows

Forms 1 and 2 have a plausible assignment/long-option interpretation. Form 3
is not closed enough to verify the requested table-row blocking semantics:

- A row requires a leading marker (`-`, `*`, or `|`), an identifier, a tab,
  at least two spaces, or `|`, then a value and an optional trailing delimiter.
  The precedence between a `|` marker, the `|` identifier/value separator, and
  a trailing `|` is not stated.
- `|` is not excluded by the value-run grammar. A no-space row terminator can
  therefore be consumed as part of the maximal value rather than recognized
  as the optional trailing table marker. Quoted/backtick values are likewise
  not parsed as a separate form; the value simply stops at those bytes.
- The value has trailing `.`, `)`, `]`, and `}` removed, but the specification
  does not say whether those bytes are outside the blocking raw range or only
  excluded from `Q`. Section 4.3 says structural findings report the value,
  while section 3.4 requires exact raw ranges; a punctuation boundary fixture
  is needed to make the fingerprint and redaction span unambiguous.
- Line ending grammar is not stated for the end-of-line requirement. CRLF,
  lone CR, and a final line without a terminator need deterministic treatment.
- Table rows use `Q(value, 20)`. The qualifier review is closed with an
  actionable rejection, and the current Q prose still does not provide a
  deterministic step-5 cursor algorithm or the complete truth table. Thus a
  table row can have different blocking outcomes even after the row parser is
  chosen.

The intended disposition is that a qualifying table-row value produces one
`credential-assignment` blocking finding whose raw range is the value alone;
the marker, identifier, separator, whitespace, and optional row terminator are
not part of that range. A Q-failing non-placeholder value of at least eight
bytes falls to the named advisory rule, subject to the qualifier and exclusion
semantics. The current text does not state enough about delimiter precedence
and punctuation spans to make that intended disposition interoperable.

## Compatibility and threat-model disposition

**Compatibility: actionable rejection.** The proposal preserves the accepted
v1 mutation boundary, exit behavior, value-free diagnostics, ruleset-versioned
fingerprints, acknowledgments, and raw-byte coordinate model in intent. Ruleset
4 is correctly a new version, so ruleset-3 membership equivalence is not
required. However, unspecified decoded selection, JWT grammar, table-row
spans, and unresolved `Q(value, 20)` outcomes mean two conforming readers can
reject different requests or compute different fingerprints. Compatibility
cannot be claimed for this exact ruleset hash.

**Threat model: actionable rejection.** An attacker can place a credential in
the 65th decoded candidate or beyond an oversized candidate run if the
implementation skips those regions; no partial-coverage signal is currently
required. A permissive JWT parser can block malformed lookalikes or accept a
prefix of a four-segment value, while a strict parser can miss a value another
reader blocks. Table-row delimiter ambiguity can similarly create false
negatives or false positives, and an unresolved `Q20` lets the same labelled
value cross the blocking boundary by implementation choice. These outcomes
undermine both the v1 fail-before-commit guarantee for detectable credentials
and ADR-023's near-zero-false-positive requirement for blocking membership.

## Required corrections and decision

Before this slice can be accepted:

1. Specify decoded-run alphabet, padding/decoding grammar, source-versus-
   decoded-byte measurement, deterministic run order, the meaning of the
   64-run cap, and an explicit over-limit/partial-coverage disposition. Add
   boundary cases for 39/40 bytes, 65,535/65,536/65,537 source bytes, and the
   64/65 candidate transition.
2. Close JWT parsing: require exactly three segments; define canonical
   base64url and JSON object/`alg` rules, including duplicate and type
   handling; state whether payload JSON is intentionally not validated; and
   add near-miss cases for a fourth segment, malformed header, invalid `alg`,
   and boundary-adjacent bytes.
3. Define table-row delimiter precedence, whether a no-space trailing `|` is
   a delimiter, line endings, quote/backtick handling, punctuation trimming,
   and the exact raw span. Add value-free/runtime-assembled rows for `-`, `*`,
   and `|` markers, each separator, a trailing delimiter, punctuation, and
   near misses.
4. Resolve the qualifier review's deterministic `Q` algorithm and truth-table
   gap before claiming table-row blocking compatibility. Re-review this slice
   at the new exact input hashes after the corrections.

**Final decision: REJECTED at the exact ruleset SHA-256 recorded above.** The
review is actionable rather than an acceptance; implementation and the
dependent ruleset release work remain blocked until the listed semantics and
fixtures are specified and independently reviewed.

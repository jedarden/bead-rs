# Focused review — ruleset 4 randomness and excluded identifiers

Review bead: `beadrs-72e35e21`

Date: 2026-10-04

Decision: **ACTIONABLE REJECTION.** The reviewed ruleset-v4 bytes are
compatible with the accepted v1 mutation and finding model in principle, but
sections 2.2 and 4.4 are not deterministic enough to accept as an
implementation contract. The rejection is limited to the qualifier and
labelled-assignment slice; it does not invalidate the accepted v1 contract.

## Reviewed inputs

The primary reviewed input is `research/specs/secret-ruleset-v4.md` at the
exact SHA-256 below. Supporting inputs were read at the same checkout and are
bound here so that a later review cannot silently change the comparison.

| Input | Role | SHA-256 |
|---|---|---|
| `research/specs/secret-ruleset-v4.md` | sections 2.2, 4.4, and section 7 acceptance obligations | `bad7c8102086d8128465e284b839673ceceb84b28e3ea7bf55835dfaee5ad5a9` |
| `docs/adr/023-widen-blocking-ruleset-to-observed-credential-formats.md` | blocking-tier membership and deterministic evidence decision | `39d9174173df3aadc7c268bdd27d139e1091cc12559e20b9d098037683a7359a` |
| `docs/adr/024-match-on-a-normalized-view.md` | view selection and raw-range compatibility | `508989f43c984b3a69ef529eaaf0d96ed1caa44873857e91b14aeaaa61de9c86` |
| `docs/adr/025-surface-advisory-findings-and-partial-scan-coverage.md` | Q-false advisory behavior and observability | `e13bda45279d2b875c9c2dce6d2bdc3e0471e91512cff22ec40487f1a0de4a33` |
| `research/specs/secret-rejection-v1.md` | accepted mutation, finding, fingerprint, and redaction baseline | `f6aa7639a8ef1dd509b431853abf64db78d0923ef9e9d59b09e0a4e0e55df231` |
| `docs/reviews/r038-specification-acceptance-2026-09-03.md` | exact-hash record for the unconditional v1 acceptance | `a7622b292729a33788ea36340946fd8be5495554510ecc839aebf8763903da8a` |
| `research/fixtures/secret-write-boundary-v1.json` | available independent fixture inventory | `c75f4dbf6242ced3c7e21e501e1de7a0fbef2a5371c601957c809043f0996710` |

The available checked-in fixture is a nine-case write-boundary manifest. It
contains no Q values, truth-table rows, identifier lexemes, or assignment
forms, so it cannot establish section 2.2 or 4.4 conformance. The rows below
are hand-authored synthetic review probes, not copied implementation samples;
no other bead implementation was consulted.

## Randomness qualifier truth table

These probes apply the natural reading that a matched word-like run is
maximal and advances the scan past the matched run. `a`, `d`, `w`, and `t`
are the section 2.2 counters. A dash means the row fails before that counter
is reached. The result column is the expected result under that reading, not
an assertion that the current prose uniquely defines the reading.

| Named class | Synthetic value | `a` | `d` | `w` | `a-w` | `t` | First failing step / result |
|---|---|---:|---:|---:|---:|---:|---|
| Bead identifier | `beadrs-11223344` | 14 | 8 | 6 | 8 | — | step 5 (`8 < 12`); `Q(v,12)=false` |
| Name plus short hash | `worker-name-1a2b3c4d` | 18 | 4 | 10 | 8 | — | step 5 (`8 < 12`); `Q(v,12)=false` |
| Timestamp | `2026-10-03T12:34:56Z` | 16 | 14 | — | — | — | step 4 (`70 < 64` is false); `Q(v,12)=false` |
| Version string | `v1.2.3` | 4 | 3 | 0 | 4 | — | step 5 (`4 < 12`); `Q(v,12)=false` |
| Path | `/srv/app/config` | 12 | 0 | — | — | — | step 3 (no digit); `Q(v,12)=false` |
| Camel-case type name | `AccessTokenValue` | 16 | 0 | — | — | — | step 3 (no digit); `Q(v,12)=false` |
| Snake-case name | `secret_scan_job` | 13 | 0 | — | — | — | step 3 (no digit); `Q(v,12)=false` |
| 40-byte base62 | `a1B2c3D4e5F6g7H8i9J0k1L2m3N4o5P6q7R8s9T0` | 40 | 20 | 0 | 40 | 39 | all steps pass; `Q(v,12)=true` and `Q(v,20)=true` |
| 40-byte one-case hexadecimal | `a1b2c3d4e5f60718293a4b5c6d7e8f901a2b3c4d` | 40 | 24 | 0 | 40 | 30 | hex exception and all later steps pass; `Q(v,12)=true` and `Q(v,20)=true` |
| Base64-shaped value containing `+` and `/` | `A1+b2/C3+d4/E5+f6/G7+h8/I9+j0/K1+l2/M3+n4/O5+p6/` | 32 | 16 | 0 | 32 | 47 | all steps pass; `Q(v,12)=true` and `Q(v,20)=true` |

The exact threshold matters to assignment semantics. The synthetic value
`a1B2c3D4e5F6g7H8` has `a=16`, `d=8`, `w=0`, and `t=15`. It passes
`Q(v,12)` and fails `Q(v,20)`. Therefore, if the same value is parsed
successfully as an assignment it blocks, while if it is parsed as a table row
it is advisory (assuming `P(v)` is false). That distinction is intended by
the two thresholds, but section 7 supplies no exact boundary fixture and
section 2.2 does not state enough scanning detail to make all implementations
produce the same counters.

### Qualifier defects

1. Step 5 says to take the first matching alternative and says to advance one
   byte only when no alternative matches. It does not say that a successful
   match consumes its full run. A one-byte advancement interpretation counts
   overlapping suffixes of `abcdefgh` and gives a different `w` from the
   maximal-run interpretation. It also does not say whether “four or more”
   means a maximal run or exactly the first four bytes.
2. The listed consequences are classes, not normative vectors. There are no
   exact inputs for the bead-ID grammar, short-hash boundary, timestamp
   grammar, version grammar, path grammar, or the Q(12)/Q(20) boundary. A
   truth table cannot be independently replayed from the reviewed contract.
3. Because Q is used both for blocking and advisory selection, this missing
   run-consumption rule changes not only rejection but also the advisory count
   and the write-time notice required by ADR-025.

## Excluded identifiers and assignment semantics

The exclusion inventory is observable, but its lexical and precedence rules
are not complete enough to give it a stable meaning. The following is the
minimum expected exclusion table derived from the text; each row must suppress
`credential-assignment` when the identifier is actually parsed as that rule's
identifier.

| Exclusion class | Required examples | Review result |
|---|---|---|
| Exact identifier | `acknowledge-secret` | excluded, but the prose does not say whether the advisory fallback is also suppressed |
| Prefix | `secret_scan_job`, `secret-scan-job` | excluded; “beginning” does not define component or byte-prefix matching |
| Fencing token | `fencing-token`, `fencing_token` | excluded |
| Exact max option | `max_tokens` | excluded; treatment of `max-tokens`, `maxTokens`, or a suffix is unstated |
| File/path/name/ref/label/count/limit/budget/usage suffix | `api_token_file`, `api-token-path`, `secret_value_name`, and the remaining listed suffixes | excluded with `_` or `-` according to the text; periods, spaces, and camel case are unstated |
| Plural keyword | `passwords`, `credentials` | not a keyword, but “plural” has no lexical definition |

The same hand-authored Q(12)-passing value should be tested against every
listed exclusion, both with `=` and `:`, with and without quotes, and in all
three forms. The expected scope for this review is that an exclusion suppresses
only the labelled-assignment rule. A provider-format, context-bound, or
structural finding on the same bytes must remain eligible, and ADR-024's
decoded view must not run labelled assignment at all. Section 4.4 does not say
this explicitly, nor does it say whether an excluded identifier with a
Q-failing value still produces `advisory-keyword-assignment`.

The assignment grammar has additional actionable gaps:

- “Components joined by” a separator or a lower-to-upper transition does not
  define the component lexer, empty components, repeated separators, digits,
  or whether camel boundaries are found before or after ASCII case folding.
  This matters because matching is case-insensitive but camel boundaries are
  case-sensitive evidence.
- `key` is special only when immediately preceded by one of ten components,
  while `pat` is a whole component. The contract does not specify whether
  `apiKey`, `api_key`, `apikey`, and `API-KEY` are equivalent, or how a
  keyword followed by a non-suffix component is rejected.
- The three forms overlap. There is no precedence for assignment versus long
  option versus table row, no longest-match rule for `:` versus `:=`, and no
  complete rule for whitespace after a list/table marker. Conventional
  spaced rows such as `| token | value |` are not literally accepted by the
  stated marker-plus-identifier grammar, while compact pipe rows can make the
  value delimiter ambiguous.
- The value definition does not say whether `|` terminates a form-3 value,
  whether trailing punctuation is stripped once or repeatedly, or whether
  length and Q are evaluated before or after that stripping. Those choices
  change the raw range and therefore the v1 fingerprint.

## Compatibility and threat-model dispositions

| Area | Disposition |
|---|---|
| Accepted v1 mutation boundary | Compatible in principle: v4 retains complete pre-transaction scanning, atomic rejection, value-free diagnostics, and exact acknowledgments. No v1 change is authorized by this review. |
| Findings, offsets, fingerprints, redaction | Not yet compatible as an implementable contract: unresolved value spans and parser precedence make the raw range and fingerprint non-deterministic, despite the v1 requirement that those be exact. |
| ADR-023 | Direction is compatible, but the deterministic/evidence bar is not met. The missing Q truth vectors and identifier lexer prevent repeatable near-miss and false-positive evidence. |
| ADR-024 | View selection is compatible at a high level: labelled assignment is excluded from decoded views. Assignment parsing on raw, normalized, and dewrapped views still needs explicit tokenization and deduplication rules. |
| ADR-025 | The advisory fallback is compatible in principle, but Q-run ambiguity and exclusion scope can change advisory counts, stderr notices, and the one-tenth volume gate. |
| False negatives | High risk. A case-folding/prefix mismatch, a conventional table row not parsed, or an overbroad exclusion can leave a real labelled credential outside the blocking tier. Normalization creates additional representations that must use the same precise parser. |
| False positives | Material risk. A different component split or value span can turn identifiers, documentation, paths, or table prose into blocking assignments; this undermines ADR-023's trust rationale. |
| Audit and remediation | High impact if unresolved. Different spans select different fingerprints, so operators may be unable to acknowledge or redact the finding that another conforming parser produced. |

## Required action before acceptance

1. Specify Q step 5 as a maximal-run lexer: define each alternative's exact
   consumption, advancement after a match, tie behavior, and whether runs may
   overlap. Add generated-at-test-time exact rows for every section 2.2 class,
   including Q(12) versus Q(20) boundary pairs and both sides of every
   inequality.
2. Define the identifier grammar as an ASCII lexer, including component
   boundaries, case-folding order, empty/repeated separators, digit handling,
   keyword/suffix/plural rules, and the exact exclusion precedence.
3. Define form precedence and delimiters, including whitespace after markers,
   `:`/`:=`/`=>` longest matching, pipe-row termination, quoted values,
   punctuation stripping, line boundaries, and the exact raw span used for Q,
   advisory reporting, and fingerprints.
4. State that exclusions suppress only `credential-assignment` (or explicitly
   choose a different scope), while provider/context/structural rules remain
   eligible; state whether the advisory fallback is suppressed for excluded
   identifiers.
5. Add independent generated conformance coverage for every exclusion in both
   blocking and Q-failing advisory cases, without committing format-valid
   secret samples.

**Final decision: ACTIONABLE REJECTION** of
`research/specs/secret-ruleset-v4.md` at SHA-256
`bad7c8102086d8128465e284b839673ceceb84b28e3ea7bf55835dfaee5ad5a9` for the
reviewed sections. The qualifier and labelled-assignment work remains blocked
until the listed semantics and synthetic truth table are made normative.

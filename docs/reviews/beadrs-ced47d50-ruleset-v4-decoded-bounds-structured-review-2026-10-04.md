# Ruleset 4 decoded bounds and structured blocking review — 2026-10-04

Reviewer: Codex, independent of the specification drafting and implementation
activity. This is a clean-room review: it uses the normative files, accepted
baseline, ADRs, and independent fixture material named below. It does not
consult another bead implementation or use implementation behavior as
evidence.

## Decision

**ACTIONABLE REJECTION** of `research/specs/secret-ruleset-v4.md` at the exact
SHA-256 recorded below. The provider-format shape and raw-range intent are
understandable, but sections 3.2 and 4.4 leave security-relevant selection and
span behavior unspecified. The independent fixture set also does not exercise
the reviewed surfaces. Ruleset 4 cannot be accepted or released until the
actions in this review are resolved against a new exact hash.

This is a scoped rejection of the reviewed ruleset candidate, not a rejection
of the accepted `secret-rejection-v1` contract.

## Inputs and exact identities

The reviewed ruleset bytes are:

| Artifact | SHA-256 |
| --- | --- |
| `research/specs/secret-ruleset-v4.md` | `bad7c8102086d8128465e284b839673ceceb84b28e3ea7bf55835dfaee5ad5a9` |
| `research/specs/secret-rejection-v1.md` | `f6aa7639a8ef1dd509b431853abf64db78d0923ef9e9d59b09e0a4e0e55df231` |
| `docs/reviews/r038-specification-acceptance-2026-09-03.md` | `a7622b292729a33788ea36340946fd8be5495554510ecc839aebf8763903da8a` |
| `docs/adr/014-hard-reject-secret-bearing-mutations.md` | `1bd43644e26acc0087925067654e93a9ddbbc5a313aff44144d23da205f2fd88` |
| `docs/adr/023-widen-blocking-ruleset-to-observed-credential-formats.md` | `39d9174173df3aadc7c268bdd27d139e1091cc12559e20b9d098037683a7359a` |
| `docs/adr/024-match-on-a-normalized-view.md` | `508989f43c984b3a69ef529eaaf0d96ed1caa44873857e91b14aeaaa61de9c86` |
| `docs/adr/025-surface-advisory-findings-and-partial-scan-coverage.md` | `e13bda45279d2b875c9c2dce6d2bdc3e0471e91512cff22ec40487f1a0de4a33` |
| `research/fixtures/README.md` | `dfd99a8d7a5272dc92e5ec5be16a5f50a426e507bcf3ea866e41bbd910284b64` |
| `research/fixtures/secret-write-boundary-v1.json` | `c75f4dbf6242ced3c7e21e501e1de7a0fbef2a5371c601957c809043f0996710` |

The R038 record unconditionally accepts the v1 specification at the exact
hash above. Its mutation boundary, value-free finding shape, raw byte offsets,
fingerprint, and acknowledgment rules are therefore the compatibility
baseline for this review.

## Section 3.2 — view selection and bounds

The intended view routing is clear and compatible with ADR-024:

| View | Source and eligible rules |
| --- | --- |
| Raw | The original field UTF-8 bytes; all blocking rules may run. |
| Normalized | ANSI removal, one-pass escape decoding, then printable percent decoding; all blocking rules may run. |
| Dewrapped | Normalized bytes with the specified continuation span removed; all blocking rules may run. |
| Decoded | Candidate base64/base64url runs from the normalized view; provider-format and private-key rules only. |

The decoded view is consequently not a place to run `credential-assignment`.
That preserves ADR-024's rationale: arbitrary decoded bytes did not carry a
label supplied by the operator, so applying `Q` there would turn a structural
qualification into an unlabelled detector. Advisory rules remain raw-only.

The candidate condition is also stated: a maximal run in normalized bytes must
be at least 40 bytes, it is subject to a 64-run-per-field cap and a
65,536-byte-per-run cap, and it is retained only when at least nine tenths of
its decoded bytes are printable ASCII or ASCII whitespace. A decoded finding
uses the raw range of the whole selected encoded run, as section 3.4 requires.

Those statements do not define a deterministic decoded view at the limits:

1. If a field has more than 64 qualifying maximal runs, the specification
   does not say whether the first 64 in raw order are selected, whether the
   runs are sampled, or whether the field becomes partially scanned. An
   attacker can put benign qualifying runs before a credential-bearing run;
   different selection policies produce different blocking verdicts.
2. If one maximal run exceeds 65,536 bytes, the specification does not say
   whether it is skipped, truncated, split into chunks, or causes a scan
   failure. Splitting changes the definition of a maximal run; truncating
   changes the raw range that section 3.4 says must cover the whole encoded
   run; skipping creates a deliberate blind spot. Each choice has a different
   threat and redaction consequence.
3. “Base64 or base64url alphabet” does not say whether the candidate alphabet
   is the union, how a run containing both `+`/`/` and `-`/`_` is classified, or
   which lenient padding and residual-length rules are used. “At least nine
   tenths” likewise needs an integer comparison and a defined ASCII-whitespace
   set for cross-implementation parity.

The count and size phrases are resource bounds, but not over-limit behavior.
That is insufficient for a closed, compiled, deterministic blocking ruleset.
It also leaves no coverage status for a skipped decoded region, whereas
ADR-025 establishes that partial scanning must be visible in diagnostics.

## Section 4.1 — JSON web token blocking

The intended positive is deterministic up to the section 3.2 gap: exactly
three dot-separated segments, each at least 8 bytes and made of the stated
base64url alphabet, with the first two beginning with the stated header
prefix. Before blocking, the first segment must decode to a JSON object with
an `alg` member. This is an offline shape check, not signature verification,
which is consistent with ADR-014's prohibition on network validation.

The rule is eligible on raw, normalized, and dewrapped views, and also on a
selected decoded view because it is a provider-format rule. It is not eligible
on the advisory raw-only path and it is not eligible as a labelled assignment.
Near misses that must not block include a short segment, a missing required
header prefix, a first segment that is not the required JSON object, a missing
`alg` member, a non-three-segment shape, and a candidate whose surrounding
bytes fail the section 3.1 span predicate.

The raw-span consequences are:

- A raw, normalized, or dewrapped match reports the smallest raw half-open
  range that produced the JWT bytes. The identifier, delimiters, and unrelated
  surrounding text are outside `start..end`.
- A match in the decoded view reports the raw half-open range of the entire
  selected encoded run, not the smaller raw range corresponding to the JWT
  substring inside the decoded payload.
- The v1 fingerprint consequently hashes the reported raw bytes, and
  acknowledgment and redaction continue to use raw coordinates. A decoded
  over-limit policy must not silently produce a range that contradicts this
  rule.

The contract should additionally state the exact base64url decoder behavior
for the header (including padding and impossible residual lengths), the JSON
member/value acceptance rule, and the JWT body alphabet used by the trailing
boundary check. These are smaller issues than the missing run-limit policy,
but leaving them to libraries risks different near-miss verdicts.

## Section 4.4 — labelled assignments and table rows

The assignment rule's high-level tiering is compatible with ADR-023 and the
accepted v1 boundary: forms 1 and 2 use `Q(value, 12)`, form 3 uses the
stricter `Q(value, 20)`, and a non-placeholder value of at least 8 bytes that
fails `Q` is advisory rather than blocking. The identifier keyword, suffix,
and exclusion lists are applied only where the assignment form has first
located a label; they do not make arbitrary decoded text block.

The table-row form is intended to be:

```
optional indentation + one marker + identifier + row separator + value + optional row terminator + end of line
```

Its blocking raw range should begin at the first byte of the value after the
row separator and end immediately before trailing row whitespace or the
trailing table marker. The marker, identifier, separator, and line ending are
not part of the finding. If the value has terminal `.`, `)`, `]`, or `}`
punctuation, those bytes are removed from the value before `Q` and are outside
the reported range. This is the only raw-span result consistent with the v1
field-byte range and fingerprint contract.

However, the current value grammar does not implement that result
unambiguously. It defines the value as the maximal run excluding whitespace,
quotes, backticks, commas, and semicolons, but `|` is not excluded. In the
same form, `|` is an optional marker, a valid identifier/value separator, and
an optional trailing table terminator. Therefore a table row with a trailing
table marker can either:

- include that marker in the maximal value and potentially change `Q` and the
  fingerprint; or
- stop before it by an unstated special case.

The two readings produce different blocking decisions and different raw
spans. The identifier's allowance for a single space, combined with a
separator of two or more spaces, also needs a specified longest-match rule so
that the same row cannot be tokenized in two ways. These are not merely parser
style choices: an included delimiter can make a credential miss the qualifier,
or make a successful finding's redaction range include table syntax.

## Independent fixture assessment

`research/fixtures/secret-write-boundary-v1.json` is valid JSON and has nine
independently named cases for service mutation, policy fail-closed behavior,
recovery reporting, publication quarantine, and sanitized recovery. Its
provenance README is the governing fixture guidance. None of its cases
specifies a decoded-run count or size boundary, a JWT positive/near miss, a
table-row positive/near miss, or an expected raw `start..end` span. It therefore
supports the pre-transaction and publication invariants only; it cannot serve
as independent evidence for sections 3.2, 4.1, or 4.4.

Before acceptance, add independent, value-free fixture descriptions or
test-time constructors for at least:

- runs 1, 64, and 65, plus a run at 65,536 and one byte beyond it, with the
  chosen left-to-right/skip/split behavior recorded;
- a qualifying decoded run containing a JWT and its complete encoded-run raw
  span, alongside normalized and dewrapped JWT spans;
- JWT structural and boundary near misses; and
- table rows with each marker/separator, a trailing table marker, a quoted or
  punctuation-ended value, Unicode elsewhere in the field, and the exact
  half-open raw span.

As required by the v4 contract, constructors should assemble any
format-valid candidate at test time and committed fixture text should not
contain a live-looking format-valid sample.

## Compatibility and threat-model disposition

The proposed extension has a sound compatibility direction. It keeps the v1
mutation outcome, redacted finding shape, raw byte offsets, and fingerprint
coordinate system; advertises a new `ruleset_contract`; and uses a ruleset
version so newly detected values do not silently reuse old acknowledgments.
That matches ADR-014 and ADR-023. A v4 implementation may reject text that a
v1 implementation accepted, but it must not silently emulate a legacy profile
or reinterpret a stored raw range.

The unresolved limits and table delimiter have material security impact. A
credential-bearing run after the unspecified 64-run cutoff or inside an
unspecified over-limit run may evade the decoded provider rules. A delimiter
included in a table value can cause a false negative or an incorrect
fingerprint-selected redaction range. Conversely, an implementation that
chooses different implicit rules can reject different operator mutations,
violating deterministic fleet behavior. These outcomes contradict the
closed/offline/deterministic intent of ADR-023 and the bounded, observable
partial-scan requirement of ADR-025, even though v1 correctly does not claim
complete detection of all unstructured secrets.

## Required resolution

1. Amend section 3.2 with a deterministic run order, exact behavior for the
   65th run and runs beyond 65,536 bytes, decoder/alphabet rules, printable
   count arithmetic, and the coverage/reporting disposition of skipped input.
2. Amend section 4.1 with the exact header decoder and JSON `alg` predicate,
   and define the JWT body alphabet used by section 3.1.
3. Amend section 4.4 so the table-row lexer treats `|` as a delimiter rather
   than value data, defines marker/separator precedence and longest-match
   behavior, and states the exact half-open raw span after punctuation trim.
4. Add the independent section-specific vectors listed above, recompute all
   affected hashes, and repeat the exact-hash review. Do not carry this
   decision forward to a changed ruleset file.

Until those changes and a new review are complete, the explicit disposition is
**not accepted**; BR-T39 through BR-T44 remain blocked by the ruleset-4
acceptance gate.

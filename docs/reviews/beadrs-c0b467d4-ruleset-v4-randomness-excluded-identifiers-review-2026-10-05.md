# BR-T35 focused review — Q qualifier and excluded identifiers

**Decision: ACTIONABLE REJECTED**

This is an exact-input, clean-room review of the ruleset-v4 randomness
qualifier and labelled-assignment exclusions. It covers only the named ADRs,
the named specifications, and the permitted fixture material. No
implementation, other bead implementation, or pre-existing review artifact
was used to derive behavior.

## Inputs and exact hashes

The hashes below identify the bytes reviewed. A change to any input requires a
new review hash.

| Input | SHA-256 |
|---|---|
| `docs/adr/023-widen-blocking-ruleset-to-observed-credential-formats.md` | `39d9174173df3aadc7c268bdd27d139e1091cc12559e20b9d098037683a7359a` |
| `docs/adr/024-match-on-a-normalized-view.md` | `508989f43c984b3a69ef529eaaf0d96ed1caa44873857e91b14aeaaa61de9c86` |
| `docs/adr/025-surface-advisory-findings-and-partial-scan-coverage.md` | `e13bda45279d2b875c9c2dce6d2bdc3e0471e91512cff22ec40487f1a0de4a33` |
| `research/specs/secret-ruleset-v4.md` | `bad7c8102086d8128465e284b839673ceceb84b28e3ea7bf55835dfaee5ad5a9` |
| `research/specs/secret-rejection-v1.md` (accepted v1 baseline) | `f6aa7639a8ef1dd509b431853abf64db78d0923ef9e9d59b09e0a4e0e55df231` |
| `research/fixtures/README.md` | `23af7f702d57f71cce99ec9e13807cce66285be996ba277f8c2074c8c9bc5b7d` |
| `research/fixtures/secret-write-boundary-v1.json` | `c75f4dbf6242ced3c7e21e501e1de7a0fbef2a5371c601957c809043f0996710` |

## Q qualifier assessment

Q is integer-only, bounded to printable ASCII, rejects placeholders before
the class tests, requires both a digit and a letter, limits digit density,
limits word-like runs, and requires frequent class changes. That construction
is compatible with ADR-023's structural/labelled use and with the accepted
v1 rule that statistical entropy is advisory rather than blocking. ADR-024's
decoded view also correctly excludes labelled-assignment matching, avoiding a
label-free decoded byte stream being treated as an assignment.

The stated consequences do not, however, constitute the required truth table.
The following is the result of applying the written predicates to
representative shapes, with `m` shown because the caller supplies it. These
are shape-level checks, not committed secret fixtures.

| Named class from section 2.2 | Q at `m=8` | Q at `m=12` | Q at `m=20` | Written-predicate disposition |
|---|---:|---:|---:|---|
| Bead identifier | may pass | fails | fails | The usual short hash suffix leaves too little `a-w` for 12 or 20; it is not universally false for `m=8`. |
| Name followed by a short hash | may pass | fails | fails | Same `m` sensitivity as a bead identifier. |
| Timestamp | fails | fails | fails | Digit-density inequality fails for a normal timestamp. |
| Version string | fails | fails | fails | It reaches the word-material threshold with too little non-word material. |
| Path | fails | fails | fails | A normal path has no digit and fails step 3. |
| Camel-case type name | fails | fails | fails | A normal type name has no digit and fails step 3. |
| Snake-case name | fails | fails | fails | A normal name has no digit and fails step 3. |
| 40-character base62 string with mixed classes | passes | passes | passes | A balanced alternating representative satisfies steps 3–6. |
| Labelled 40-character hexadecimal string | passes | passes | passes | The long, single-case hexadecimal exception permits step 4. |
| Base64 string containing `+` and `/` with mixed classes | passes | passes | passes | A mixed-class representative satisfies steps 3–6. |

Two acceptance blockers follow.

1. Section 2.2 supplies `m=8`, `m=12`, and `m=20` at different call sites,
   but section 7 does not map each truth-table row to its applicable `m` or
   publish the boundary counts (`a`, `d`, `w`, `t`). The prose consequence
   that a bead identifier fails is therefore false for some permitted calls,
   while the section 5 advisory call uses `m=16` and is not represented at
   all.
2. Step 5 says to take the first matching alternative but does not define
   whether “four or more” consumes the maximal run or only four bytes, nor
   explicitly says that the scan cursor advances to the end of the selected
   run. Overlapping lower/upper/title-case candidates can therefore produce
   different `w` values in independent implementations. The resulting
   blocking verdict is not yet a fully specified deterministic function.

The hexadecimal exception also needs a row that states the exact boundary:
length 31 versus 32, mixed letter case versus one case, and the required
letter/digit presence from step 3. Broad labels such as “40-character base62”
cannot substitute for exact runtime-assembled candidates because many strings
in that class fail step 3 or the word-run test.

## Excluded-identifier assessment

The intended result is that an excluded identifier does not produce a
`credential-assignment` finding, so Q is not consulted for that assignment.
The written list identifies these classes:

| Identifier class | Intended result |
|---|---|
| Exact `acknowledge-secret` | excluded |
| Any identifier beginning `secret_scan` or `secret-scan` | excluded |
| Exact `fencing-token` or `fencing_token` | excluded |
| Exact `max_tokens` | excluded |
| Any identifier ending in the listed `_file`, `_path`, `_name`, `_ref`, `_label`, `_count`, `_limit`, `_budget`, or `_usage` suffixes, with `-` accepted for `_` | excluded |
| A plural keyword such as a plural form of `token` or `password` | not a keyword; no assignment match on that keyword alone |
| A non-excluded singular credential label such as `secret` plus an allowed suffix component | eligible for the applicable form and Q threshold |

This intent is not sufficiently normative for acceptance:

- “Component”, “beginning”, and “ending” are not defined against one
  canonical tokenization. The identifier grammar permits `_`, `-`, `.`, a
  single space, and lower-to-upper transitions, but the exclusions only
  spell out underscore and hyphen suffixes. Space-separated and camel-case
  variants consequently have unresolved outcomes; consecutive capitals and
  digits are also not resolved.
- It is not stated whether an exclusion is exact, component-bounded, or a
  raw prefix/suffix test. In particular, the interaction of
  `secret_scan`-style prefixes with a later suffix exclusion is undefined.
- The section does not say whether exclusions suppress only blocking,
  suppress both blocking and `advisory-keyword-assignment`, or apply to any
  other rule. They must be scoped to `credential-assignment`; provider-format
  and structural rules must remain eligible when the same bytes occur in
  their own structures.
- “A plural keyword is not a keyword” needs a token-boundary rule. It should
  not accidentally turn a plural component into a substring match for a
  singular keyword or permit a keyword hidden inside an unrelated component.

These are security-relevant distinctions, not editorial preferences. A false
negative can be created by a credential-bearing label that lands in an
over-broad exclusion, while a false positive can make operators acknowledge
or reword real data. The explicit exclusion list is a reasonable threat-model
control only if its matching scope and precedence are fixed and tested.

## Compatibility and threat-model disposition

**Compatibility:** conditionally compatible with the accepted
`secret-rejection-v1` baseline. Q changes blocking/advisory membership, but it
does not require changing v1 mutation atomicity, exit semantics,
fingerprints, raw byte ranges, acknowledgment shape, or output redaction.
The v4 ruleset identity can version the membership change. Compatibility
cannot be claimed until Q and exclusion parsing have one unambiguous result
for every required truth-table row. ADR-025's `Q(run,16)` advisory path must
also be included in that definition.

**Threat model:** the bounded, offline, deterministic design is sound in
principle, and the deliberate exclusion of hash-shaped unlabelled strings
avoids treating ordinary identifiers as credentials. The current ambiguity
leaves both false-negative and false-positive paths open, and an excluded
suffix can become an evasion label if exclusions are applied outside the
credential-assignment rule. Disposition is therefore **not acceptable for
release**.

## Fixture and provenance disposition

`research/fixtures/README.md` permits independently invented fixtures and
requires candidate values to be assembled at runtime rather than committed.
The only named secret-related fixture, `secret-write-boundary-v1.json`,
covers write/recovery boundary scenarios and contains no Q or excluded-label
truth-table cases. It is therefore compatible with the clean-room boundary
but cannot provide the required BR-T35 evidence. No format-valid secret or
implementation-derived fixture is added by this review.

## Actionable rejection

Before BR-T35 can be accepted:

1. Specify step-5 matching as a cursor algorithm: define maximal versus
   minimum run length, alternative priority, and cursor advancement, including
   overlapping title-case and upper/lower runs.
2. Add an exact runtime-assembled truth table for every section-2.2 class,
   the hexadecimal boundaries, and the `m=8`, `m=12`, `m=16`, and `m=20`
   call sites. Record enough integer counts to make each verdict auditable.
3. Define identifier tokenization and ASCII case-folding for separators,
   camel case, consecutive capitals, digits, and spaces. Define exact versus
   prefix/suffix boundaries and exclusion precedence.
4. State that exclusions apply only to `credential-assignment` (including or
   excluding its advisory fallback explicitly), while provider and structural
   rules remain independent. Add runtime synthetic cases for every exclusion,
   plural near miss, separator/case variant, and an adversarial excluded-label
   credential case; keep values out of committed fixtures as required by the
   fixture guide.

Until those changes and fixture results are recorded against a new exact input
hash, the BR-T35 decision is **ACTIONABLE REJECTED**.

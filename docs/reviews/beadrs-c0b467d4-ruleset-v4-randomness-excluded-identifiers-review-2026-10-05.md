# BR-T35 focused review — Q qualifier and excluded identifiers

**Decision: ACTIONABLE REJECTED**

This is an exact-input, clean-room review of the ruleset-v4 randomness
qualifier and labelled-assignment exclusions. It covers only the named ADRs,
the named specifications, and the permitted fixture material. Behavior below
was evaluated from those texts. Synthetic byte strings were assembled in a
transient calculation and only their shape and integer counts are recorded;
no candidate value is committed.

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
| `research/fixtures/recovery-finding-quarantine-v1.json` (accepted, value-free scenario manifest) | `aa50162029c0ec3899b288a1006981e05d745038ba47f16c186be0c7ebb03d6a` |
| `research/fixtures/secret-write-boundary-v1.json` | `c75f4dbf6242ced3c7e21e501e1de7a0fbef2a5371c601957c809043f0996710` |

## Q qualifier assessment

Q is bounded to printable ASCII and uses integer-only tests. It rejects
placeholders, requires a digit and a letter, limits digit density and
word-like material, and requires frequent class changes. This is a coherent
shape filter for an already labelled or structural candidate, and it does not
turn the v1 advisory entropy rule into a blocking rule. ADR-024 also excludes
labelled-assignment matching from the decoded view, where no source label is
present.

I evaluated the informative classes with synthetic representatives. `w`
below uses the natural greedy reading of “four or more” in step 5: consume the
whole matching run. The table gives `(n, a, d, w, t)`, then the predicate
result at every `m` used by sections 2.2–5.1. The candidate bytes were not
stored.

| Section-2.2 class; synthetic representative shape | `(n,a,d,w,t)` | `Q8` | `Q12` | `Q16` | `Q20` | First relevant observation |
|---|---:|---:|---:|---:|---:|---|
| Bead identifier: short alphabetic prefix, separator, four alternating digit/letter pairs | `(15,14,4,6,9)` | pass | fail | fail | fail | The informative “fails” consequence is true for `m=12/16/20`, but false at the URI/curl `m=8` call site. |
| Name plus short hash: five lowercase letters, separator, same four pairs | `(14,13,4,5,9)` | pass | fail | fail | fail | A short name/hash shape is not categorically excluded by Q at `m=8`. |
| Timestamp: ISO date-time with `T` and `Z` | `(20,16,14,0,11)` | fail | fail | fail | fail | Fails step 4: `5d=70`, while `4a=64`. |
| Version: `v` plus numeric release, prerelease letters, and numeric suffix | `(15,12,9,0,8)` | pass | pass | fail | fail | This ordinary version-like shape passes at `m=12`, contradicting an unconditional “version string ... fail” consequence. |
| Path: ordinary slash-separated alphabetic path | `(21,17,0,11,7)` | fail | fail | fail | fail | Fails step 3 because it has no digit. |
| Camel-case type name | `(15,15,0,15,3)` | fail | fail | fail | fail | Fails step 3 because it has no digit. |
| Snake-case name | `(16,15,0,15,2)` | fail | fail | fail | fail | Fails step 3 because it has no digit. |
| 40-byte base62: alternating lowercase letters and digits | `(40,40,20,0,39)` | pass | pass | pass | pass | Meets steps 3–6, including `m=20`. |
| Labelled 40-byte hex with lowercase letters only, alternating hex letters and digits | `(40,40,20,0,39)` | pass | pass | pass | pass | The one-case exception permits step 4; all later predicates pass. |
| 40-byte base64 alphabet including `+` and `/`, with short mixed-class runs | `(40,24,8,0,32)` | pass | pass | pass | pass | Meets step 3 and `a-w=24`, so it also passes `m=20`. |

The table does not establish a complete truth table: its result depends on a
greedy interpretation that the step-5 prose does not require, and section 7
contains no actual rows or counts. A runtime-assembled boundary check also
shows why the hex exception needs explicit evidence: a one-case 31-byte
hexadecimal witness with `(a,d,w,t)=(31,25,0,13)` fails step 4 (`125 < 124` is
false); the corresponding one-case 32-byte witness with `(32,26,0,13)` passes
step 4 only because the exception applies; a mixed-case 32-byte witness with
the same counts fails step 4 because the exception does not apply. Each has
both letter and digit bytes, and the passing 32-byte witness passes all four
`m` values. These counts are synthetic and no witness bytes are recorded.

There is also a direct determinism defect. A transient 21-byte synthetic
candidate has `(n,a,d,t)=(21,20,8,16)` and one lowercase run of length five.
If step 5 consumes the maximal run, `w=5`, so `a-w=15` and `Q(v,16)` fails.
If “four or more” consumes the minimum four bytes and resumes at the next
cursor, `w=4`, so `a-w=16` and `Q(v,16)` passes. All other predicates pass in
both readings. Thus the `m=16` advisory call can have different outcomes
under two readings of the current text. This 21-byte witness also fits the
section-5.1 token alphabet and is not hash-shaped, so the ambiguity can change
an advisory finding. Step 5 must say maximal run and advance-to-end (or define
another exact algorithm) before implementations can agree.

The required `m` call sites are `8` for URI passwords and curl credentials,
`12` for authorization values, Kubernetes `stringData`, and assignment/long
option forms, `20` for assignment table rows, and `16` for advisory runs. The
informative consequence list is not a substitute for this matrix: the
version-like witness passes at a blocking `m=12`, and identifier-like shapes
pass at `m=8`.

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

The positive identifier grammar defines the listed component separators and
ASCII case-insensitivity. I read each exclusion against the captured
identifier, case-folded: exact entries match the whole identifier, “beginning”
is a literal prefix, and “ending” is a literal suffix. Under that reading,
`acknowledge-secret`, `fencing-token`, `fencing_token`, and `max_tokens` are
excluded; the specified `secret_scan` and `secret-scan` prefixes are excluded;
listed suffixes match `_` and `-` spellings. A plural component such as
`tokens` does not match the whole keyword component `token`. Other separators
are distinct: `max.tokens`, `fencing.token`, and `acknowledge_secret` are not
listed exclusions. Since a keyword may have arbitrary preceding components,
their `token` or `secret` components can otherwise make them eligible for
`credential-assignment`. That near-miss behavior should be stated and tested
so callers know the exclusions are literal rather than separator-normalized.

The suffix exclusions for `_file`, `_path`, `_name`, `_ref`, `_label`,
`_count`, `_limit`, `_budget`, and `_usage` appear redundant under the positive
grammar: the component after a keyword must be one of the listed suffix
components, and none of these words is allowed there. For example,
`passwordFile` and `password_file` do not match the positive grammar because
`file` is not a suffix component, even before the exclusion check. The spec
should either record that this redundancy is intentional or align the
positive suffix grammar with the exclusion list.

Section 4.4 also gives an advisory fallback for Q-failing values but does not
say whether an excluded identifier suppresses that fallback as well as the
blocking `credential-assignment` finding. The exclusions are located in the
credential-assignment rule, so they do not suppress independent provider or
structural rules that match the same bytes in their own forms. The plural
sentence and positive grammar do define a component boundary; no plural
substring ambiguity remains on the literal reading above. The advisory
fallback's precedence remains an observable open point.

These are security-relevant distinctions, not editorial preferences. A false
negative can be created by a credential-bearing label that lands in an
over-broad exclusion, while a false positive can make operators acknowledge
or reword real data. The explicit exclusion list is a reasonable threat-model
control only if its matching scope and precedence are fixed and tested.

## Compatibility and threat-model disposition

**Compatibility:** the proposal preserves the accepted v1 mutation boundary,
exit behavior, finding/fingerprint contract, acknowledgment shape, raw byte
ranges, and output-redaction requirements. Its intended blocking/advisory
membership change is correctly versioned as ruleset 4, so behavioral
equivalence with ruleset 3 is not required. Compatibility with the v1 contract
is acceptable at that boundary, conditional on one deterministic v4 result
for every Q call site and on retaining the v1 invariants during conformance.
The current Q text does not yet meet that condition.

**Threat model:** the bounded offline qualifier and structural/label context
are appropriate controls for avoiding statistical-only blocking. However,
the `m=8` and `m=12` witnesses show that identifier/version-like material can
pass in blocking contexts, and the step-5 ambiguity makes the result
implementation-dependent. Exclusions intentionally create false-negative
exceptions for some assignment labels; their literal boundaries and advisory
scope must be fixed so those exceptions stay narrow and independent provider
or structural matches remain active. The current text is therefore **not
acceptable for release**.

## Fixture and provenance disposition

`research/fixtures/README.md` permits independently invented fixtures and
requires provenance and expected behavior. Its accepted
`recovery-finding-quarantine-v1.json` is explicitly value-free and asks tests
to assemble candidate values at runtime; it contains no Q or excluded-label
truth-table cases. The proposed `secret-write-boundary-v1.json` likewise only
lists write/recovery scenarios and has no such cases. The runtime witnesses
in this review were independently assembled from the written predicates;
they are not added to either manifest and no format-valid sample is
committed. The permitted fixture material is clean-room compatible but does
not supply BR-T35 truth-table evidence.

## Actionable rejection

Before BR-T35 can be accepted:

1. Specify step-5 matching as a cursor algorithm: define maximal versus
   minimum run length, alternative priority, and cursor advancement, including
   overlapping title-case and upper/lower runs.
2. Put an exact runtime-assembled truth table in the spec/conformance record
   for every section-2.2 class, the 31/32-byte hexadecimal boundaries,
   one-case versus mixed-case hex, and `m=8/12/16/20`. Include the integer
   counts needed to audit each verdict; specifically resolve the passing
   version-like shape at `m=12` and the identifier-like shapes at `m=8`.
3. State that exclusions are case-folded literal exact/prefix/suffix checks
   against the full identifier, or specify another algorithm. Resolve whether
   alternate separators are deliberately near misses, align the listed
   `_file`-style exclusions with the positive suffix grammar, and say whether
   excluded identifiers suppress the Q-fail advisory fallback. Keep
   independent provider and structural rules active. Add runtime synthetic
   cases for each exception, plural component, alternate separator/case, and
   an adversarial excluded-label credential; keep candidate values out of
   committed fixtures as required by the fixture guide.

Until those changes and fixture results are recorded against a new exact input
hash, the BR-T35 decision is **ACTIONABLE REJECTED**.

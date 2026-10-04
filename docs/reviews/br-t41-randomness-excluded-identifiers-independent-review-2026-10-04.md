# Focused independent review — ruleset 4 qualifier and exclusions

Date: 2026-10-04.

Reviewer: OpenAI Codex, independent of the ADR/specification authors and
implementation owners.

Decision: **REJECTED — actionable specification blockers remain.** This review
covers only the randomness qualifier in section 2.2 and the excluded-identifier
semantics in section 4.4. It does not accept the other ruleset-v4 changes.

## Exact reviewed input

The review used only the following repository artifacts. No other bead
implementation, source tree, scanner output, test corpus, or fixture corpus
was inspected or used.

| Artifact | SHA-256 |
| --- | --- |
| `docs/adr/023-widen-blocking-ruleset-to-observed-credential-formats.md` | `39d9174173df3aadc7c268bdd27d139e1091cc12559e20b9d098037683a7359a` |
| `docs/adr/024-match-on-a-normalized-view.md` | `508989f43c984b3a69ef529eaaf0d96ed1caa44873857e91b14aeaaa61de9c86` |
| `docs/adr/025-surface-advisory-findings-and-partial-scan-coverage.md` | `e13bda45279d2b875c9c2dce6d2bdc3e0471e91512cff22ec40487f1a0de4a33` |
| `research/specs/secret-ruleset-v4.md` | `bad7c8102086d8128465e284b839673ceceb84b28e3ea7bf55835dfaee5ad5a9` |
| `research/specs/secret-rejection-v1.md` | `f6aa7639a8ef1dd509b431853abf64db78d0923ef9e9d59b09e0a4e0e55df231` |
| `research/fixtures/secret-write-boundary-v1.json` | `c75f4dbf6242ced3c7e21e501e1de7a0fbef2a5371c601957c809043f0996710` |
| `docs/reviews/r038-specification-acceptance-2026-09-03.md` | `a7622b292729a33788ea36340946fd8be5495554510ecc839aebf8763903da8a` |

The exact input-set digest is
`98f4e8fe3d4223ee32aef8cf71c404753cb6dd494b2739ccf2b217a243119d02`.
It is SHA-256 over the newline-delimited `sha256sum` output for the seven
paths above after lexicographic sorting of the complete output lines. The
target decision is bound to the ruleset-v4 hash above; changing any reviewed
byte requires a new review.

The supplied independent fixture is a nine-case write-boundary manifest. It
contains no qualifier values, qualifier truth-table rows, identifier samples,
or excluded-identifier cases. It therefore cannot provide the section 7 item
2 evidence required by the target specification.

## Randomness qualifier review

Section 2.2 gives deterministic integer predicates for printable range,
placeholder rejection, alphanumeric composition, digit proportion, word-like
runs, and class transitions. The first four predicates are sufficiently
direct to implement, subject to ordinary byte-level definitions. The
qualifier as a whole is not yet deterministic enough for independent
conformance:

| Required truth-table class | Result at this input hash |
| --- | --- |
| Out-of-range, placeholder, missing letter/digit, and ordinary digit-heavy values | The intended false branches are stated, but no exact rows exercise them. |
| Bead identifier and name-plus-short-hash | Required to fail, but neither class has a normative grammar or exact sample. |
| Timestamp, version, path, camel-case type name, and snake-case name | Required to fail, but the accepted shapes and boundary lengths are undefined. |
| 40-byte base62, labelled 40-byte hexadecimal, and base64 containing `+`/`/` | Required to pass, but the statement supplies no exact values and does not say whether it describes one fixture or the whole shape class. |
| `m`-dependent assignment/structural cases | The call sites use different thresholds, but no rows show the same value at the relevant thresholds. |

The principal normative defect is step 5. “Four or more” lowercase or
uppercase letters and “one uppercase followed by three or more lowercase
letters” do not specify match length. The scan also does not say whether a
match consumes its whole run, consumes only four bytes, or permits overlapping
matches before advancing. Those choices change `w`, and therefore can change
both `10 * w < 7 * a` and `a - w >= m`. A change in `w` changes a blocking
verdict and, for an advisory finding, can change the selected run and
fingerprint.

The informative consequences are not a substitute for a truth table. For
example, “timestamp”, “path”, and “base62 string” each describe many byte
strings with different run lengths, class transitions, and digit ratios. The
specification must give exact, non-secret synthetic rows and expected results,
including the boundary values for every predicate and each `m` used by a
rule. Section 7 currently requires such a table but neither the specification
nor the supplied fixture contains it.

## Excluded-identifier review

The exclusions are a reasonable false-positive control for metadata labels:
the scanner-control prefix, fencing-token labels, the token-budget label, and
file/path/name/reference/diagnostic suffixes are plausible non-credential
contexts. They also create deliberate detection gaps if an actual credential
is assigned to one of those names. That trade-off is acceptable only if it is
explicitly scoped to `credential-assignment`; provider-format and structural
rules must remain eligible, and the threat model must treat excluded labels as
evidence suppression rather than proof that the value is safe.

The current text leaves the following semantics unresolved:

1. “Components” has no formal alphabet or Unicode rule. The lower-to-upper
   boundary is described alongside ASCII case-insensitive matching without
   saying whether splitting happens before or after case folding.
2. “Contains a keyword and, after the keyword, only suffix components” does
   not define keyword selection when several components qualify, or the
   precedence between suffix recognition and exclusions.
3. “Any beginning `secret_scan`” and “any ending in `_name`” do not define
   component boundaries. It is unclear whether an immediately following
   letter, a dot, a camel-case transition, or a hyphen has the same meaning.
4. The explicit `max_tokens` spelling does not say whether the hyphenated form
   is equivalent, while the suffix sentence separately says that `-` is
   accepted for `_`. The same question applies to case variants and the other
   listed spellings.
5. The rule does not state that exclusions are checked before a keyword match
   and before `Q`, nor does it state whether they apply identically in
   assignments, long options, and table rows after ADR-024 normalization.
6. “A plural keyword is not a keyword” has no defined plural boundary or
   interaction with suffix components. Near-misses such as a plural keyword
   followed by an allowed suffix need explicit expected outcomes.

Without these decisions, two independent scanners can disagree about whether
the same label reaches `Q`, and can consequently disagree about blocking,
advisory classification, and fingerprint/redaction reach. The supplied
fixture has no case that can detect any of these disagreements.

## Compatibility consequences

The accepted v1 artifact is the exact `secret-rejection-v1.md` hash listed
above. It requires a closed, offline, versioned scanner; complete pre-
transaction scanning; value-free diagnostics; exact-fingerprint
acknowledgment; and atomic rejection. Ruleset 4 can preserve those invariants
as an explicitly versioned extension: it records ruleset version 4, keeps raw
byte coordinates for findings, and does not claim that detection is complete.

However, v4's label-based blocking is not itself a v1 provider-format or
private-key rule. It is compatible only as the separately identified v4
contract, with capabilities and fingerprints changing at the version boundary.
The unresolved `Q` and identifier semantics prevent independent
implementations from reproducing that boundary. They also make it impossible
to determine reliably which existing advisory findings become blocking,
which are intentionally excluded, and which stored fingerprints remain
redactable after the ruleset-version change.

## Threat-model consequences

The qualifier is aimed at accidental disclosure in credential-labelled or
credential-bearing contexts, not at proving that arbitrary text contains no
secret. An operator can still place a secret in an excluded label, use a value
that fails `Q`, or use an unlabelled format-free value; provider and structural
rules may catch some of those cases. This is consistent with a bounded
accidental-disclosure control only if the exclusions are narrowly scoped and
their false-negative behavior is documented.

As written, ambiguous run consumption and label parsing create two concrete
risks: a false negative can leave a credential outside the blocking tier, and a
false positive can reject ordinary metadata. Because v1 fingerprints include
the rule, field, byte range, and matched bytes, divergent parsing also changes
redaction reach and acknowledgment portability. The absence of synthetic
negative and positive rows means neither risk is bounded by the reviewed
evidence.

## Required actions and disposition

Before this slice can be accepted:

- define maximal word-run matching and scan advancement in section 2.2;
- add a normative, exact-value Q truth table covering every named class,
  predicate boundary, and each `m` used by the rules;
- formalize identifier components, case folding, component boundaries,
  keyword/suffix precedence, exclusion precedence, separator equivalence, and
  plural handling;
- state that exclusions suppress only the labelled-assignment rule (unless a
  broader suppression is deliberately intended) and document the resulting
  false-negative boundary; and
- add independent synthetic fixture rows for every exclusion and near-miss,
  every assignment form, and at least one value whose result differs between
  the `m = 12` and `m = 20` thresholds.

**Final disposition: ACTIONABLE REJECTION.** The ruleset-v4 direction is
compatible in principle with the accepted v1 security and compatibility
invariants, but the exact input set is not accepted for this slice. BR-T41 and
the ruleset-v4 release gate must remain blocked until the specification and
fixture hashes change and receive a new independent review.

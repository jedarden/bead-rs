# Focused review — ruleset v4 Q qualifier and excluded identifiers

Review bead: `beadrs-b60f83c5`
Date: 2026-10-05
Reviewer: OpenAI Codex, acting as an independent reviewer for this bead. The
reviewer is distinct from the `bead-rs maintainers` named as the specification
authors and decision-makers.

Decision: **ACTIONABLE REJECTION for this slice.** The v4 direction is
conditionally compatible with the accepted `secret-rejection-v1` boundary,
but the reviewed Q and identifier contract is not deterministic or
fixture-backed enough for acceptance.

## Scope and reviewed artifacts

This review is limited to section 2.2 (Q, its word-run semantics, and the
named truth-table classes), section 4.4 (credential-assignment identifiers and
exclusions), the related bead-ID/identifier shapes in section 5.1, and their
interaction with the three ADRs. No other implementation, scanner output, or
foreign fixture corpus was consulted.

The hashes below were computed with `sha256sum` at review time. No
hash-verification failure occurred.

| Artifact | Role | SHA-256 |
| --- | --- | --- |
| `research/specs/secret-ruleset-v4.md` | Q, identifier, bead-ID, and conformance contract | `bad7c8102086d8128465e284b839673ceceb84b28e3ea7bf55835dfaee5ad5a9` |
| `research/specs/secret-rejection-v1.md` | accepted mutation/finding/fingerprint baseline | `f6aa7639a8ef1dd509b431853abf64db78d0923ef9e9d59b09e0a4e0e55df231` |
| `docs/adr/023-widen-blocking-ruleset-to-observed-credential-formats.md` | Q-backed blocking membership | `39d9174173df3aadc7c268bdd27d139e1091cc12559e20b9d098037683a7359a` |
| `docs/adr/024-match-on-a-normalized-view.md` | view in which labelled assignment is eligible | `508989f43c984b3a69ef529eaaf0d96ed1caa44873857e91b14aeaaa61de9c86` |
| `docs/adr/025-surface-advisory-findings-and-partial-scan-coverage.md` | Q-backed advisory selection | `e13bda45279d2b875c9c2dce6d2bdc3e0471e91512cff22ec40487f1a0de4a33` |
| `research/fixtures/secret-write-boundary-v1.json` | available checked-in independent fixture inventory | `c75f4dbf6242ced3c7e21e501e1de7a0fbef2a5371c601957c809043f0996710` |
| `research/fixtures/README.md` | fixture provenance requirements | `dfd99a8d7a5272dc92e5ec5be16a5f50a426e507bcf3ea866e41bbd910284b64` |
| `docs/reviews/br-t35-randomness-qualifier-truth-table-independent-review-2026-10-04.md` | prior independent synthetic Q probes consulted as review evidence | `bf77cd450a7ed887dddd4ddb468e2aee66b6162067b193cc3223e816396dda60` |
| `docs/reviews/br-t41-randomness-excluded-identifiers-independent-review-2026-10-04.md` | prior independent synthetic exclusion probes consulted as review evidence | `5d222327393f9155e5ee21fd2474952b5a5a5f7c30efb0ba33c6b068eb7e28e4` |

The checked-in fixture has nine mutation-surface cases, but no Q value,
qualifier outcome, identifier lexeme, assignment form, or exclusion case.
Therefore it cannot satisfy section 7.2's required truth table. The exact
synthetic probes below are review evidence for the resulting ambiguity; they
are not being presented as a replacement conformance fixture.

## Q truth-table evidence

The following rows use the literal predicates in section 2.2. `w` is shown
under the natural *maximal matching run, consumed on success* reading solely
to make the arithmetic reproducible. `F1` through `F6` identify the first
failed numbered step; `PASS` means all six steps pass.

| Required class | Exact synthetic value | `(n,a,d,w,t)` | Result at `m=12` |
| --- | --- | --- | --- |
| Out of range | `A1!` repeated 171 times | `(513,342,171,0,512)` | F1 |
| Placeholder | `x1example` | `(9,9,1,7,2)` | F2 |
| Missing letter | `12345678!` | `(9,8,8,0,1)` | F3 |
| Missing digit | `AbCd!EfGh!` | `(10,8,0,0,9)` | F3 |
| Ordinary digit-heavy value | `ab12345678` | `(10,10,8,0,1)` | F4 |
| Bead identifier | `beadrs-11223344` | `(15,14,8,6,2)` | F5 (`a-w=8`) |
| Name plus short hash | `WorkItem-a1b2c3` | `(15,14,3,8,10)` | F5 (`a-w=6`) |
| Timestamp | `2031-07-18T13:47:26Z` | `(20,16,14,0,11)` | F4 |
| Version string | `v1.2.3-rc4` | `(10,7,4,0,8)` | F5 |
| Path | `/var/log/2026` | `(13,10,4,0,5)` | F5 (`a-w=10`) |
| Camel-case type name | `CredentialPolicy` | `(16,16,0,16,3)` | F3 |
| Snake-case name | `secret_scan_job` | `(15,13,0,10,4)` | F3 |
| 40-character base62 | `a1B2c3D4e5F6g7H8j9K0m1N2p3Q4r5S6t7U8v9W0` | `(40,40,20,0,39)` | PASS |
| 40-character one-case hexadecimal | `1234a5678b9012c3456d7890e1234f5678a9012b` | `(40,40,32,0,15)` | PASS only via the hex exception |
| Base64 alphabet containing `+` and `/` | `a1B2+c3D4/e5F6+g7H8/j9K0+m1N2/p3Q4+r5S6/t7U8v9W0` | `(48,40,20,0,47)` | PASS |

This table exposes two evidence gaps. First, section 2.2 calls these classes
“informative consequences,” but gives neither an exact normative vector nor a
grammar for “bead identifier,” “short hash,” timestamp, version, path,
camel-case, snake-case, base62, or base64. For example, the reviewed text
does not determine whether `beadrs-11223344` is a bead ID because it never
specifies the prefix, hash alphabet, or width. The same issue appears in the
“bead identifier” exclusion from ADR-025's hash-shaped advisory candidates.
Second, the section 7 requirement for a truth table is not met by the
available fixture, which has no such rows.

The inclusive `m` boundaries are also unrepresented by the checked-in
fixture. Exact probes show why they matter:

| Probe | Counters | `Q(v,12)` | `Q(v,16)` | `Q(v,20)` |
| --- | --- | --- | --- | --- |
| `a1B2c3D4e5F6g7H8` | `a=16,d=8,w=0,t=15` | PASS | PASS | F5 (`a-w=16`) |
| `a!1!b!2!c!3!d!4!e!5!f` | `a=11,d=5,w=0,t=21` | F5 (`a-w=11`) | F5 | F5 |
| `a!1!b!2!c!3!d!4!e!5!f!g` | `a=12,d=5,w=0,t=22` | PASS | F5 | F5 |

Thus assignment/long-option (`m=12`), advisory (`m=16`), and table-row
(`m=20`) behavior cannot be checked from the committed fixture, including the
required one-value-different-result boundary.

## Deterministic word-run ambiguity

Step 5 says to take the first matching alternative at each position and to
advance one byte only when no alternative matches. It does not specify what a
successful match consumes. At least these readings are compatible with the
prose:

1. consume the entire contiguous lowercase/uppercase/title run;
2. consume exactly the four bytes that establish the “four or more” match;
3. count the matching run but advance by one byte, allowing overlapping
   suffixes; or
4. apply a regex-like leftmost-longest rule, whose interaction with the
   ordered alternatives is not stated.

The exact probe
`abcdefgh!1!2!3!4!5!6!7!8!9!0!1` has `n=30`, `a=19`, `d=11`, and `t=22`.
With maximal consumption, `w=8`, so `a-w=11` and `Q(v,12)` fails at step 5.
With four-byte consumption, `w=4`, so `a-w=15`; the word-run and transition
inequalities pass and `Q(v,12)` succeeds. This is a blocking-versus-advisory
and fingerprint-selection difference, not an implementation detail. The
available fixture contains no row that can detect it.

## Excluded identifiers and related shapes

Section 4.4 lists exclusions but leaves the following independent questions
unresolved. Each item is a separate conformance ambiguity, because the same
Q-passing value can be placed after the identifier in an assignment and yield
a different blocking/advisory result depending on the choice.

| Exclusion or shape | Synthetic evidence | Unresolved contract question |
| --- | --- | --- |
| Exact `acknowledge-secret` | `acknowledge-secret` paired with the exact Q12 boundary probe above | Is this exact exclusion checked before Q, and does it suppress only `credential-assignment` or also the advisory fallback? |
| `secret_scan` / `secret-scan` prefix | `secret_scan_job`, `secret_scanX`, `secret-scan-job` | Does “beginning” mean raw byte prefix, a complete component, or a component followed by a separator? |
| Fencing names | `fencing-token`, `fencing_token`, `fencing.tokens` | Are only the two spellings excluded, or are dot/camel variants equivalent under the undefined component lexer? |
| Exact `max_tokens` | `max_tokens`, `max-tokens`, `maxTokens` | Does the statement that `-` substitutes for `_` apply to this exact exclusion, and does ASCII case-insensitivity apply before or after exclusion matching? |
| Listed suffixes | `api_token_file`, `api-token-path`, `secret_value_name`, `api.token.ref`, `apiTokenName` | Are `_file`/`_path`/… suffixes byte suffixes only, separator-normalized components, or also dot/space/camel forms? |
| Plural keywords | `passwords`, `credentials`, `passwords_token` | What is a plural, where is its boundary, and can a plural keyword followed by an allowed suffix still qualify? |
| Components | `api_key`, `apiKey`, `API-KEY`, repeated separators, and digit-bearing components | What alphabet, empty-component rule, digit rule, and lower-to-upper split are used, and does splitting happen before ASCII case folding? |
| Keyword/suffix selection | `api_access_key_value` and `secret_token_value` | If several components are keywords or suffixes, which keyword is selected and in what order are keyword, suffix, and exclusion decisions applied? |
| Bead-ID/hash-shaped identifier | `beadrs-11223344`, `bead-11223344`, and a different-width hex suffix | What exact prefix, separator, case, alphabet, and width define a bead identifier in the Q truth table and ADR-025's advisory exclusion? |

The supplied independent fixture has no row for any of these lexemes. It
therefore cannot establish exclusion precedence, separator equivalence,
plural handling, component boundaries, or bead-ID shape. The required fixture
additions are not optional: section 7.2 explicitly requires every excluded
identifier, while ADR-023 makes the false-positive/near-miss evidence the bar
for promoting this rule into the blocking tier.

The contract also does not state the scope of an exclusion. The safe
compatibility reading is that it suppresses only `credential-assignment`; a
provider-format, context-bound, or structural rule must remain eligible, and
the decoded view must continue to omit labelled assignment as ADR-024 says.
That scope is not normative, nor is it stated whether a Q-failing value under
an excluded label still produces `advisory-keyword-assignment`. A broad
exclusion would create a silent false-negative boundary; a narrow exclusion
without an explicit advisory rule would cause independent scanners to differ
in diagnostics and stored finding fingerprints.

## Compatibility and disposition

`secret-rejection-v1` requires a closed, offline, versioned scanner; complete
pre-transaction scanning; atomic rejection; value-free diagnostics; and exact
fingerprints for acknowledgment and redaction. ADR-023 through ADR-025 can
preserve those invariants as a separately versioned v4 extension. The
direction is therefore **conditionally compatible** with v1 in principle.

The exact reviewed contract is **rejected** for this slice. Unspecified word
run consumption changes Q outcomes. Missing exact class and boundary vectors
prevent independent replay. Missing component, bead-ID, exclusion precedence,
and plural rules change whether a value is blocking, advisory, or absent. Any
of those changes can alter the raw finding span and therefore the v1
fingerprint, acknowledgment, and `bead redact` reach. ADR-025's advisory
count and volume gate are likewise not reproducible.

Before acceptance, section 2.2 must define successful-match consumption and
advancement, and section 7 must carry exact expected rows for every named
class, every predicate boundary, and `m=12/16/20`. Section 4.4 and the
related bead-ID text must define an ASCII component lexer, case-folding order,
keyword/suffix precedence, every exclusion's exact scope, plural handling,
and the exact bead-ID/hash shape. Independent fixtures must then cover each
exclusion, near miss, and assignment threshold. A new review is required
against the resulting hashes.

**Final disposition: ACTIONABLE REJECTION** of the Q/randomness and excluded-
identifier slice at ruleset SHA-256
`bad7c8102086d8128465e284b839673ceceb84b28e3ea7bf55835dfaee5ad5a9`.

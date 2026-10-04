# BR-T35 focused review — randomness qualifier truth table

Date: 2026-10-04.

Decision: **REJECTED for this slice at the reviewed ruleset hash.** This
supplement evaluates each class named in section 2.2 with exact synthetic
vectors and evaluates the `m` boundaries used by blocking and advisory rules.
It supplements the unchanged BR-T41 finding on qualifier and exclusion
semantics; it does not accept ruleset v4.

## Reviewed inputs

| Artifact | SHA-256 |
| --- | --- |
| `research/specs/secret-ruleset-v4.md` | `bad7c8102086d8128465e284b839673ceceb84b28e3ea7bf55835dfaee5ad5a9` |
| `docs/adr/023-widen-blocking-ruleset-to-observed-credential-formats.md` | `39d9174173df3aadc7c268bdd27d139e1091cc12559e20b9d098037683a7359a` |
| `docs/adr/024-match-on-a-normalized-view.md` | `508989f43c984b3a69ef529eaaf0d96ed1caa44873857e91b14aeaaa61de9c86` |
| `docs/adr/025-surface-advisory-findings-and-partial-scan-coverage.md` | `e13bda45279d2b875c9c2dce6d2bdc3e0471e91512cff22ec40487f1a0de4a33` |
| `research/specs/secret-rejection-v1.md` | `f6aa7639a8ef1dd509b431853abf64db78d0923ef9e9d59b09e0a4e0e55df231` |
| `docs/reviews/r038-specification-acceptance-2026-09-03.md` | `a7622b292729a33788ea36340946fd8be5495554510ecc839aebf8763903da8a` |
| `research/fixtures/secret-write-boundary-v1.json` | `c75f4dbf6242ced3c7e21e501e1de7a0fbef2a5371c601957c809043f0996710` |
| Existing finding: `docs/reviews/br-t41-randomness-excluded-identifiers-independent-review-2026-10-04.md` | `5d222327393f9155e5ee21fd2474952b5a5a5f7c30efb0ba33c6b068eb7e28e4` |

The reviewed ruleset SHA is unchanged from the BR-T41 review. The v1
acceptance record is bound to the exact v1 hash above. No other bead
implementation, source, scanner output, or copied corpus was consulted.

## Synthetic truth table

`F1` through `F6` mean the first failing numbered predicate in section 2.2;
`PASS` means all six predicates pass. Counts are `(n, a, d, w, t)`. To make
the arithmetic checkable, `w` below uses one plausible reading: consume the
entire matching contiguous letter run when an alternative matches. Section
2.2 does not settle whether that is the required advancement rule; the table
is review evidence, not a normative fixture or conformance oracle.

| Named class | Exact synthetic value | `(n,a,d,w,t)` | Q(·,12) | Q(·,16) | Q(·,20) |
| --- | --- | --- | --- | --- | --- |
| Out of range | `A1!` repeated 171 times | `(513,342,171,0,512)` | F1 | F1 | F1 |
| Placeholder | `x1example` | `(9,9,1,7,2)` | F2 | F2 | F2 |
| Missing letter | `12345678!` | `(9,8,8,0,1)` | F3 | F3 | F3 |
| Missing digit | `AbCd!EfGh!` | `(10,8,0,0,9)` | F3 | F3 | F3 |
| Digit-heavy | `ab12345678` | `(10,10,8,0,1)` | F4 | F4 | F4 |
| Bead identifier | `beadrs-11223344` | `(15,14,8,6,2)` | F5 | F5 | F5 |
| Name plus short hash | `WorkItem-a1b2c3` | `(15,14,3,8,10)` | F5 | F5 | F5 |
| Timestamp | `2031-07-18T13:47:26Z` | `(20,16,14,0,11)` | F4 | F4 | F4 |
| Version | `v1.2.3-rc4` | `(10,7,4,0,8)` | F5 | F5 | F5 |
| Path | `/var/log/2026` | `(13,10,4,0,5)` | F5 | F5 | F5 |
| Camel-case type name | `CredentialPolicy` | `(16,16,0,16,3)` | F3 | F3 | F3 |
| Snake-case name | `secret_scan_job` | `(15,13,0,10,4)` | F3 | F3 | F3 |
| 40-character base62 | `a1B2c3D4e5F6g7H8j9K0m1N2p3Q4r5S6t7U8v9W0` | `(40,40,20,0,39)` | PASS | PASS | PASS |
| 40-character single-case hex, digit-heavy | `1234a5678b9012c3456d7890e1234f5678a9012b` | `(40,40,32,0,15)` | PASS | PASS | PASS |
| Base64 alphabet with `+` and `/` | `a1B2+c3D4/e5F6+g7H8/j9K0+m1N2/p3Q4+r5S6/t7U8v9W0` | `(48,40,20,0,47)` | PASS | PASS | PASS |

The 40-byte hex row reaches the digit-ratio equality (`5d = 4a = 160`), so
its pass specifically depends on the stated long, single-case hexadecimal
exception. Its repeated synthetic structure also illustrates that Q is a
shape predicate, not a test of cryptographic unpredictability.

Section 4.4 uses `m=12` for assignment and long-option blocking and `m=20`
for table-row blocking. Section 5.1 uses `m=16` for the advisory candidate
rule. These exact boundary vectors have `w=0`; every other Q predicate passes.

| `a-w` | Exact synthetic value | Q(·,12) | Q(·,16) | Q(·,20) |
| ---: | --- | --- | --- | --- |
| 11 | `a!1!b!2!c!3!d!4!e!5!f` | F5 | F5 | F5 |
| 12 | `a!1!b!2!c!3!d!4!e!5!f!g` | PASS | F5 | F5 |
| 15 | `a!1!b!c!d!e!f!g!h!j!k!l!m!n!p` | PASS | F5 | F5 |
| 16 | `a!1!b!2!c!3!d!4!e!5!f!6!g!h!j!k` | PASS | PASS | F5 |
| 19 | `a!1!b!c!d!e!f!g!h!j!k!l!m!n!p!q!r!s!t` | PASS | PASS | F5 |
| 20 | `a!b!c!d!e!f!g!h!j!k!l!m!n!p!q!r!s!t!u!1` | PASS | PASS | PASS |

The exact values therefore pass at each inclusive boundary and fail one unit
below it. The existing write-boundary fixture is a nine-case mutation-surface
manifest; it contains no Q values or expected qualifier outcomes. Thus the
review vectors demonstrate the arithmetic but do not repair the missing
independent, committed Q fixture or the absent normative table required by
section 7.

## Compatibility and threat model

The Q decision is compatible in principle with accepted v1 invariants only
as an explicitly versioned v4 extension. ADR-023 confines blocking Q to
labelled assignments; ADR-024 excludes labelled-assignment matching from the
decoded view; ADR-025 keeps unlabelled Q-selected strings advisory. This
preserves v1's closed, offline scanner, value-free findings, exact-fingerprint
acknowledgment, and atomic rejection model. It does not make v4 detector
results compatible with v1: the ruleset version and finding fingerprints
change at the version boundary.

The threat model is bounded accidental-disclosure reduction, not proof that
arbitrary text is secret-free. Unlabelled, unstructured, unrecognized values
remain outside blocking detection, and Q does not establish entropy or
unpredictability. A credential can fail Q and escape the labelled blocking
rule; an identifier-like or repetitive value can pass Q when labelled. The
advisory tier does not block. Those limits are consistent with ADR-023–025
only if the predicate and its candidate boundaries are deterministic and
reviewed.

## Slice decision and required gap

**Slice decision: ACTIONABLE REJECTION** of the randomness-qualifier slice at
ruleset SHA-256
`bad7c8102086d8128465e284b839673ceceb84b28e3ea7bf55835dfaee5ad5a9`.
The sampled classes and inclusive `m` boundaries produce the stated results
under the review's explicit run-consumption reading, but that reading is not
normatively selected. The existing BR-T41 finding is unchanged and remains
actionable.

Before acceptance, section 2.2 must define exact word-run match length and
advancement; section 7 must contain exact normative rows for every named
class and each `m` call site; and an independent fixture must carry those
values and expected outcomes. The ruleset must then receive a new exact-hash
review. Do not claim v4 qualifier conformance or release acceptance on the
current hash.

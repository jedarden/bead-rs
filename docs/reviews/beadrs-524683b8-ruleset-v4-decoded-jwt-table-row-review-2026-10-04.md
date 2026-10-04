# Ruleset v4 decoded-view, JWT, and table-row review

Date: 2026-10-04.

Scope: ADR-023 through ADR-025; ruleset-v4 sections 3.2, 4.1, and 4.4; the
accepted `secret-rejection-v1`; and the independent fixture material available
under `research/fixtures/`. The conclusions below come from those normative
inputs. No implementation source, scanner output, or other implementation was
used as behavioral evidence.

## Exact input identities

The target contract is `research/specs/secret-ruleset-v4.md` at SHA-256
`bad7c8102086d8128465e284b839673ceceb84b28e3ea7bf55835dfaee5ad5a9`.
The complete reviewed input set is:

| Input | SHA-256 |
|---|---|
| `docs/adr/023-widen-blocking-ruleset-to-observed-credential-formats.md` | `39d9174173df3aadc7c268bdd27d139e1091cc12559e20b9d098037683a7359a` |
| `docs/adr/024-match-on-a-normalized-view.md` | `508989f43c984b3a69ef529eaaf0d96ed1caa44873857e91b14aeaaa61de9c86` |
| `docs/adr/025-surface-advisory-findings-and-partial-scan-coverage.md` | `e13bda45279d2b875c9c2dce6d2bdc3e0471e91512cff22ec40487f1a0de4a33` |
| `research/specs/secret-ruleset-v4.md` | `bad7c8102086d8128465e284b839673ceceb84b28e3ea7bf55835dfaee5ad5a9` |
| `research/specs/secret-rejection-v1.md` | `f6aa7639a8ef1dd509b431853abf64db78d0923ef9e9d59b09e0a4e0e55df231` |
| `docs/reviews/r038-specification-acceptance-2026-09-03.md` | `a7622b292729a33788ea36340946fd8be5495554510ecc839aebf8763903da8a` |
| `research/fixtures/README.md` | `dfd99a8d7a5272dc92e5ec5be16a5f50a426e507bcf3ea866e41bbd910284b64` |
| `research/fixtures/secret-write-boundary-v1.json` | `c75f4dbf6242ced3c7e21e501e1de7a0fbef2a5371c601957c809043f0996710` |

The R038 acceptance record accepts the v1 contract unconditionally at the exact
hash shown above. No ruleset-v4 detector fixture set is present in
`research/fixtures/`. The only secret-related fixture there is a nine-case
write-boundary scenario manifest; it contains no candidate values or expected
provider, decoded-view, JWT, or table-row findings. It cannot establish v4
detector behavior or near-miss rates.

## Findings

### Decoded-view limits and over-limit behavior

Section 3.2 scans the normalized view for maximal base64/base64url-alphabet
runs of at least 40 bytes. It permits at most 64 runs per field and at most
65,536 bytes per run. Each run is decoded once with a described only-as
“lenient” base64 decoder; decoded bytes are retained only when at least nine
tenths are printable ASCII or ASCII whitespace. Section 3.3 then applies only
provider-format and private-key blocking rules to that decoded view. These
caps nominally bound accepted decode work to 64 runs and 4 MiB of encoded input
per field, in addition to linear view construction.

The contract does not say which runs are selected when more than 64 qualify,
or what happens to a qualifying run longer than 65,536 bytes. It does not say
whether such runs are skipped, truncated, split, or make scanning incomplete.
“Lenient base64” also lacks a grammar defining padding, mixed standard and URL
alphabets, and invalid-byte handling. Thus the numerical caps are clear, but
over-limit verdicts and even run boundaries are not deterministic across
independent implementations. No behavior can safely be inferred from “at
most.”

### JSON web tokens

Section 4.1 requires a three-segment token, segments of at least eight bytes,
and `eyJ` prefixes on the first two segments. Section 4.5 further requires the
first segment to base64url-decode to a JSON object with an `alg` member.
Therefore a candidate satisfying those conditions is expected to block as
`json-web-token`, including when found in raw, normalized, or dewrapped views.
It is also eligible in the decoded view because §3.3 includes provider-format
rules there. The latter expectation depends on the unresolved decoded-run
grammar and limits above.

The contract does not define base64url padding acceptance or JSON duplicate
`alg` member handling, and the available independent fixture has no direct or
decoded JWT positive or near-miss. The text establishes the intended blocking
class, but the fixtures do not verify its boundary or the accepted v1
near-zero-false-positive criterion. This is shape detection, not verification
of signature, issuer, or expiry; that offline limitation should remain clear.

### Labelled assignments in table rows

Section 4.4 says a table row blocks when its identifier and value parse under
form 3 and `Q(value, 20)` succeeds. The form allows an optional leading `-`,
`*`, or `|` marker, then an identifier, a tab/two-or-more-spaces/`|`
separator, a value, and optional trailing spaces or `|`. So the intended
blocking expectation is explicit for qualifying parsed rows.

The row grammar does not define whitespace between a marker and identifier,
how leading and trailing pipes interact with cell separators, which cell is
the value, or how embedded/escaped pipes are treated. For example, ordinary
Markdown spacing after a leading pipe is not admitted by the written
identifier adjacency, while compact pipe rows can satisfy multiple delimiter
roles. The nine-case fixture has no table rows or `Q(value, 20)` outcomes.
Independent implementations can consequently disagree on both whether a row
blocks and which bytes its finding/redaction selects.

## Compatibility and threat-model assessment

The extension is compatible in principle with the accepted v1 contract: v1
keeps the scanner offline and closed, rejects a complete mutation before its
write transaction, requires atomic rejection and value-free diagnostics, and
defines findings and fingerprints in raw field-byte coordinates. Ruleset v4
keeps raw coordinates and advertises an additional contract identity. The
ruleset-version change intentionally changes fingerprints, so acknowledgments
and stored findings must be re-derived at the v4 boundary.

Compatibility is not established for this exact v4 input. Undefined decoded
selection and decoding can create bypasses or divergent findings. Ambiguous
table parsing can miss a labelled credential or reject unrelated row text;
different spans also mean different fingerprints and redaction targets. JWT
shape matching can identify expired or otherwise non-usable strings as
credentials, and no independent negatives quantify that false-positive risk.
The numeric caps support bounded work only once the over-limit disposition is
specified. These gaps weaken the accidental-disclosure control at precisely
the encoded-input and structured-text boundaries this slice is meant to add.

## Required revisions and disposition

Before this slice can be accepted:

1. Specify the exact base64/base64url run alphabet and decode grammar, run
   ordering/selection after the 64-run cap, and disposition of runs above
   65,536 bytes (including whether scanning reports incomplete coverage).
2. Define the JWT header decoding and JSON-member rules sufficiently for
   independent matching, and add synthetic direct/decoded positives plus
   malformed and near-miss cases.
3. Define table-row marker spacing, pipe/cell precedence, value selection,
   and escaping; add synthetic positive and near-miss rows with expected
   blocking outcomes and raw ranges.

**Disposition: ACTIONABLE BLOCKERS.** The intended JWT and table-row blocking
classes are stated, but this exact hash does not make decoded-view limits,
over-limit outcomes, or row/JWT edge behavior reproducible, and the available
independent fixtures do not verify this slice. This decision does not reject
the accepted `secret-rejection-v1` contract or approve ruleset-v4 compatibility.

# BR-T35 independent review — secret ruleset v4

Date: 2026-10-03.

Reviewer: OpenAI Codex, independent of the ADR/specification author and of
the implementation owners. This is a clean-room review using only the
repository's ADRs, accepted specifications, repository architecture, and
synthetic tests. No other bead implementation, source tree, fixture corpus,
or scanner output was inspected.

## Reviewed artifacts and exact identities

The review was first performed against the following bytes at review anchor
commit `565fd14`. The contract artifacts below are byte-identical at the
current committed `HEAD` `8417d8b`; the later v1 implementation corrections
are noted below and do not change the ruleset-v4 target hash:

| Artifact | SHA-256 |
| --- | --- |
| `research/specs/secret-ruleset-v4.md` | `584f1536495095c77dad818cce1b6b7bdd4414ef091970631ddc75fbd31f763c` |
| `docs/adr/023-widen-blocking-ruleset-to-observed-credential-formats.md` | `39d9174173df3aadc7c268bdd27d139e1091cc12559e20b9d098037683a7359a` |
| `docs/adr/024-match-on-a-normalized-view.md` | `508989f43c984b3a69ef529eaaf0d96ed1caa44873857e91b14aeaaa61de9c86` |
| `docs/adr/025-surface-advisory-findings-and-partial-scan-coverage.md` | `e13bda45279d2b875c9c2dce6d2bdc3e0471e91512cff22ec40487f1a0de4a33` |
| `research/specs/secret-rejection-v1.md` | `f6aa7639a8ef1dd509b431853abf64db78d0923ef9e9d59b09e0a4e0e55df231` |
| `research/specs/needle-cli-contract-v1.md` | `64a4057bdec893d81e42a3f23bff86f225a302fc3b5fec18d8ab5cf414ad7233` |
| `research/fixtures/secret-write-boundary-v1.json` | `c75f4dbf6242ced3c7e21e501e1de7a0fbef2a5371c601957c809043f0996710` |

The repository already contains the accepted R038 review for the
`secret-rejection-v1` hash above. The fixture is a write-boundary fixture; it
does not supply the ruleset-v4 qualifier, view, or output truth tables.

## Review method and architecture check

I compared the proposed contract with the accepted v1 finding identity,
acknowledgment, mutation, diagnostic, and redaction requirements. I traced
the committed scanner and service architecture in `src/scan/`,
`src/service/secret_diagnostics.rs`, `src/service/redaction.rs`, and
`src/cli_secret_scan.rs`. I also reviewed the repository's synthetic coverage
in `src/scan/tests.rs`, `tests/secret_rejection.rs`, and
`tests/secret_gap_contract.rs`, including normalized raw-range probes,
provider/JWT positives and near-misses, structural credentials, redaction,
diagnostic coverage, and the advisory notice. The dirty worktree contains
other workers' staged and untracked changes; none were staged, edited, or
used as review evidence.

## Current-tree revalidation

The two committed changes after the original review were also checked. Commit
`13d16c5` corrects the ruleset-3 npm checksum width and adds its boundary
coverage; commit `1f235fe` makes the v1 keyword prefilter enumerate
overlapping anchors and adds its regression coverage. Both are expressly
allowed v1 corrections in the proposed v4 document. Neither defines the
missing v4 qualifier table, derived-view limits, raw-range map, or advisory
schema. The committed tree still has no v4 implementation, so these changes
cannot be treated as conformance evidence for the proposed contract.

The reviewed provider table does name `json-web-token` as a blocking rule,
and section 4.5 requires an `alg` member in its decoded header. That intent is
not yet testable as a deterministic blocking rule because section 3.2 leaves
the base64url decoder grammar unspecified; no accepted v1 fixture supplies a
v4 JWT truth table. Likewise, table rows are intended to block through
`credential-assignment`, with `Q(value, 20)`, but the marker/separator
ambiguity prevents a deterministic claim about the reported value and raw
range. These are contract blockers, not a claim that the intended rules are
absent.

## Findings requiring correction

### 1. Q has no normative truth table and `w` is not deterministic

Section 2.2 says that the section-7 truth table is required, but the
specification supplies only a prose list of classes. It gives no concrete
input for a bead identifier, short hash suffix, timestamp, version, path,
camel-case name, snake-case name, 40-character base62 value, 40-character
single-case hexadecimal value, or base64 value containing `+` and `/`; it also
does not state which `m` is used for each row. The contract therefore cannot
be conformance-tested from the artifact alone.

More importantly, “four or more lowercase letters” and the other two word
alternatives do not say whether the match consumes exactly four bytes or the
maximal run. Since `w` is a byte count used in two inequalities, that choice
can change a verdict. “Non-word material” also does not name the quantity
actually used by `a - w` (remaining alphanumeric bytes, rather than all bytes
outside word-like runs).

Required correction: add a normative table with concrete non-secret fixture
values, each `m`, expected result, and the first failing predicate; define
maximal consumption and the exact meaning of “non-word material”. Keep the
integer formulas unchanged unless the corrected table shows they are not the
intended policy.

### 2. The advisory machine schema contradicts ADR-025

ADR-025 section “Decision” says machine output gains an additive **count**.
Ruleset-v4 §5.2 instead names a member `secret_scan.advisory_findings` but
does not define whether it is an integer count, a list of redacted findings,
or another value. The existing synthetic notice test treats it as a list,
while the ADR describes a count. NEEDLE permits additive JSON fields, but it
does not make an unspecified field type compatible. Routing the notice to
stderr is compatible with NEEDLE §5.2; the unresolved issue is what one
notice record and the additive machine member mean, not that stderr itself is
forbidden.

Required correction: choose and state one JSON type, its exact placement on
every machine-readable mutation result, and whether the count/list is
deduplicated by fingerprint. Also define the notice's stable rule-ID order and
count semantics. “Exactly one line” must mean one secret-scan notice record;
otherwise it conflicts with existing successful mutation diagnostics such as
publication messages while NEEDLE §5.2 only constrains routing to stderr and
preservation of stdout JSON.

### 3. Table-row grammar is ambiguous at the `|` boundary

Section 4.4 uses `|` both as an optional row marker and as a field separator,
while the value is defined as a maximal run excluding whitespace, quotes,
backticks, commas, and semicolons—but not `|`. The optional trailing pipe
therefore has no deterministic parse precedence for rows without spaces around
the separators. The same ambiguity affects the exact raw range to fingerprint
and redact. A parser can legally include a delimiter in the value or choose a
different identifier/value split while still satisfying the prose.

Required correction: define the row grammar with explicit delimiter
precedence, exclude the delimiter from the value alphabet (or require the
specified whitespace), and state the precise value span reported for both
pipe-delimited and whitespace-delimited rows. Add true-positive and negative
fixtures for all three marker forms, including no-space pipe rows and excluded
identifiers.

### 4. Decoded-view limits and lenient decoding are underspecified

Section 3.2 says “at most 64 runs per field” and “at most 65,536 bytes per
run”, but does not state whether an overlong run consumes one of the 64
slots, whether it is skipped or truncated, or that selection is left-to-right.
It also does not define “lenient base64”: accepted padding counts, non-zero
pad bits, mixed standard/URL alphabets, and invalid trailing material are all
observable choices. Different choices can expose different JWTs or private
keys and violate the deterministic/offline threat model.

Required correction: specify left-to-right selection of the first 64 maximal
runs, the exact treatment of runs outside the 40..65,536 bound (skip without
truncation is the safe choice), and a precise decoder grammar/algorithm. State
that each decoded run is an independent derived view so matches cannot cross
run boundaries.

### 5. Raw-range mapping is not formal enough to guarantee `bead redact`

Section 3.4 gives the desired result but not the mapping operation for a
derived match. ANSI removal, escape decoding, percent decoding, and dewrapping
can map multiple output bytes to one raw span and can delete raw bytes between
two output bytes. “Smallest raw range that produced its bytes” does not define
the half-open envelope, empty-match behavior, or the required raw-boundary
invariants. Those details are part of the v1 fingerprint input and determine
whether a fingerprint found by `doctor` can be resolved by `bead redact`.

Required correction: define a per-output-byte source-span map and report the
half-open envelope of the first and last mapped spans, with no empty findings;
require `0 <= start < end <= field.len()` and valid UTF-8 slice boundaries in
the stored raw field. State that the same mapping and raw fingerprint are used
when redaction revalidates the finding. Add fixtures asserting exact raw
ranges for each transformation and successful `bead redact --dry-run`.

There is a related decoding mismatch: URI credentials are required to apply Q
“after percent-decoding”, while the normalized view only decodes `%HH` values
that produce printable ASCII. The contract must say whether the URI rule uses
the normalized printable-only view or performs a separate all-byte percent
decode before Q; otherwise non-printable escapes can produce different
blocking decisions.

### 6. “Bead identifier” is not a defined hash-shaped grammar

Section 5.1 excludes a “bead identifier”, but the repository accepts imported
issue IDs as arbitrary valid non-control text and generated IDs use a
workspace-configured prefix plus an eight-byte hexadecimal suffix. The phrase
does not say which of those forms is excluded. This matters because the
advisory tier is intentionally shape-based and its false-positive disposition
must be reproducible across workspaces; a broad “prefix plus eight hex” rule
also suppresses unrelated token-shaped values.

Required correction: define the exact native generated-ID grammar, say whether
imported IDs are included, and add positive/negative advisory fixtures for
configured prefixes and lookalikes.

## Compatibility disposition

The proposed view and raw-range design can preserve the accepted v1 finding
and redaction coordinate system, and the ruleset version bump correctly makes
ruleset-3 fingerprints and acknowledgments non-matching. The capability
addition `ruleset_contract` is additive and compatible with NEEDLE consumers.
That compatibility claim is conditional: the unresolved raw-map, machine
field-type, table-row, and decoder definitions must be fixed before a v4
implementation can claim conformance. The v1 contract's no-value diagnostics,
pre-transaction rejection, exact-fingerprint acknowledgment, and deterministic
selector/range identity remain mandatory.

## Threat-model disposition

The closed compiled inventory, offline matching, integer-only qualifier, no
recursive decoding, explicit decoded-size limits, raw-byte fingerprints, and
redacted output are sound security goals. They are not yet a complete threat
model because ambiguous Q parsing can diverge between implementations,
underdefined decoded-run selection can create detection gaps, and the URI
percent-decoding mismatch can create a false-positive/false-negative split.
The missing advisory JSON type and table-row grammar also create consumer and
redaction failures rather than merely editorial differences.

## Decision

**REJECTED — actionable specification revision required.** The exact hash
above is recorded for the rejected artifact, not accepted as a release
contract. BR-T39 through BR-T44 must remain blocked. After the authors add the
normative Q truth table and grammar, resolve the ADR-025 machine-schema
conflict, specify table-row parsing, decoded-run selection/decoding, raw-range
envelopes, URI percent-decoding, and bead-ID shape, the changed
`secret-ruleset-v4.md` must receive a new exact-hash independent review.

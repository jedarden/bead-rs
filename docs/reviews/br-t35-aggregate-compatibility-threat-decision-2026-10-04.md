# BR-T35 aggregate compatibility and threat decision — secret ruleset v4

Date: 2026-10-04.

Reviewer: OpenAI Codex. This reviewer is distinct from the authors of the
ruleset, ADRs, specifications, fixtures, and implementation, and from the
reviewers named in the three current focused child records below.

Decision: **REJECTED** for the exact ruleset-v4 artifact identified below.

## Review boundary and provenance

This is the authoritative BR-T35 aggregate for the three current focused
reviews. It uses the normative ADRs and specifications, independently
authored fixture material, the cited repository architecture evidence, and the
review records named below. No other bead implementation's source, tests,
fixtures, output, SQL, comments, or prose was inspected or used. The decision
is bound to complete files and hashes, not to extracted sections. Any byte
change to a reviewed artifact requires a new hash and a new review.

The earlier BR-T35 qualifier and excluded-identifier findings remain in scope;
they are carried forward explicitly below and are not waived by the newer
focused records.

## Exact reviewed artifacts

### Normative, baseline, and fixture inputs

| Artifact | SHA-256 |
| --- | --- |
| `research/specs/secret-ruleset-v4.md` | `bad7c8102086d8128465e284b839673ceceb84b28e3ea7bf55835dfaee5ad5a9` |
| `docs/adr/023-widen-blocking-ruleset-to-observed-credential-formats.md` | `39d9174173df3aadc7c268bdd27d139e1091cc12559e20b9d098037683a7359a` |
| `docs/adr/024-match-on-a-normalized-view.md` | `508989f43c984b3a69ef529eaaf0d96ed1caa44873857e91b14aeaaa61de9c86` |
| `docs/adr/025-surface-advisory-findings-and-partial-scan-coverage.md` | `e13bda45279d2b875c9c2dce6d2bdc3e0471e91512cff22ec40487f1a0de4a33` |
| `research/specs/secret-rejection-v1.md` | `f6aa7639a8ef1dd509b431853abf64db78d0923ef9e9d59b09e0a4e0e55df231` |
| `research/specs/needle-cli-contract-v1.md` | `64a4057bdec893d81e42a3f23bff86f225a302fc3b5fec18d8ab5cf414ad7233` |
| `research/specs/secret-write-boundary-v1.md` | `3ef8948c1ad5b94c911780070e5ffee5f26856276d31ccb1b6b845c2eaae4f8e` |
| `research/specs/historical-redaction-v1.md` | `5658ac80cf9594283ddf65742aff2f4b2020a0a7869a612ee2230ed99033a016` |
| `man/man1/bead-redact.1` | `957b8b72fee4ecbd0a71520b045794b219dbaae5c5d2d0d96eef3dd60f746153` |
| `research/fixtures/README.md` | `dfd99a8d7a5272dc92e5ec5be16a5f50a426e507bcf3ea866e41bbd910284b64` |
| `research/fixtures/secret-write-boundary-v1.json` | `c75f4dbf6242ced3c7e21e501e1de7a0fbef2a5371c601957c809043f0996710` |
| `docs/reviews/r038-specification-acceptance-2026-09-03.md` | `a7622b292729a33788ea36340946fd8be5495554510ecc839aebf8763903da8a` |

The target contract is therefore `research/specs/secret-ruleset-v4.md` at
SHA-256
`bad7c8102086d8128465e284b839673ceceb84b28e3ea7bf55835dfaee5ad5a9`.
The available secret fixture is a nine-case v1 write-boundary manifest; it has
no v4 detector truth table, JWT or table-row cases, decoded-view labels, or
redaction-coordinate expectations.

### Architecture evidence reported by the stderr child review

These hashes identify the complete tracked files cited as implementation
architecture evidence by `beadrs-2f7ad38d`; they do not replace the normative
contract or establish v4 conformance:

| Artifact | SHA-256 |
| --- | --- |
| `src/cli_secret_scan.rs` | `a8de67e47ac635e686c3f0b546f888654d07b79f1c1749bc7a667360a511b98c` |
| `src/main.rs` | `ec27e27bc71402c717aa32fb14c22fca95692dc81b6df09c469d582c9a470705` |
| `src/scan/mod.rs` | `7ddba2e97093c9548b1547f466f4bd8fecab4a7ca58dc30a09393bb9c4e5d63b` |
| `src/error.rs` | `c85e65d862b69f490ab8d3c3c9cb0b8e70ff34d90bd2cf1c4e537d9dc1c1ce13` |
| `tests/secret_gap_contract.rs` | `d691931edb1828f18c7bfdfc0ac8726705b07ecea099ffbe51972aacd08b06c1` |

### Three current focused child records

| Focused child record | SHA-256 | Finding retained in this aggregate |
| --- | --- | --- |
| `docs/reviews/beadrs-524683b8-ruleset-v4-decoded-jwt-table-row-review-2026-10-04.md` | `a0b8293c6152647ce9807fb6afcf5f65ad1b8b0320c3ab20511b50cc830ad8be` | Actionable blockers for decoded-view limits, JWT semantics, and table-row parsing |
| `docs/reviews/beadrs-769db4b9-decoded-blocking-raw-range-review-2026-10-04.md` | `8e4f69e77581cfd630771745975d149211ba8ad494f358c78cbb6c517de25690` | Actionable rejection of decoded blocking and conditional raw-range/redaction compatibility |
| `docs/reviews/beadrs-2f7ad38d-write-time-stderr-compatibility-review-2026-10-04.md` | `0f79ef8d4e7f0d237187840de1fc402b9afd126c0696c129b576621baf84614a` | Actionable rejection of stderr, advisory JSON, and mutation/checkpoint edge semantics |

The overlapping decoded-view findings are intentionally retained from both
`beadrs-524683b8` and `beadrs-769db4b9`: the former records the missing
contract and fixture coverage, while the latter supplies the raw-coordinate
consequences and private-key details.

### Carried-forward BR-T35 findings

The prior BR-T35 records below are not silently dropped by this aggregate:

| Prior record | SHA-256 | Finding retained |
| --- | --- | --- |
| `docs/reviews/br-t41-randomness-excluded-identifiers-independent-review-2026-10-04.md` | `5d222327393f9155e5ee21fd2474952b5a5a5f7c30efb0ba33c6b068eb7e28e4` | Q run consumption and excluded-identifier/component semantics are not deterministic |
| `docs/reviews/br-t35-randomness-qualifier-truth-table-independent-review-2026-10-04.md` | `bf77cd450a7ed887dddd4ddb468e2aee66b6162067b193cc3223e816396dda60` | Required exact Q boundary rows and an independent conformance fixture are absent |
| `docs/reviews/br-t35-secret-ruleset-v4-independent-review-2026-10-03.md` | `d08b9c51143f3b3c89e63b047dfcd8ac263c9a142f418aef01859ad593bbc396` | Earlier integrated BR-T35 rejection and its v1-boundary preservation remain the predecessor decision |

## Focused findings and reconciliation

1. **Decoded views, JWTs, and table rows — rejected.** Section 3.2 does not
   select the 64 runs, define over-65,536-byte behavior, expose incomplete
   coverage, or specify a complete lenient base64 grammar. Section 4.1 does
   not close base64url padding, duplicate/typed `alg`, invalid UTF-8, trailing
   JSON, or decoded private-key grammar. Section 4.4 does not define marker
   spacing, pipe/cell precedence, embedded escapes, or the value span. These
   gaps can cause independent scanners to miss or invent blocking findings.

2. **Raw coordinates and redaction — conditional only.** The `beadrs-769db4b9`
   probes show that a selected derived match can map to a raw half-open cover,
   and the v1 fingerprint plus live-byte revalidation can safely resolve a
   `bead redact` request without caller-supplied offsets. This is compatible
   in principle, but it is not full v4 acceptance: whether padding belongs to
   a decoded run changes the whole-run cover (and therefore the fingerprint
   and redaction target), and ambiguous table or decoded parsing can select a
   different cover. The raw-range mechanism is accepted only as a conditional
   construction after the missing parser and fixture rules are fixed.

3. **Write-time diagnostics and machine output — rejected.** NEEDLE permits
   value-free diagnostics on stderr and the v1 pre-write exit-2,
   acknowledgment, atomicity, and durability direction is compatible. The
   v4 contract does not define the exact UTF-8 line grammar or newline,
   advisory JSON type/location, count and deduplication identity, stable rule
   ordering, acknowledgment/no-op/rollback behavior, `--no-auto-flush`,
   post-commit publication failure, or whether advisory mode reports
   blocking-tier findings. “Unchanged stdout” also conflicts unless limited to
   human-readable output. Filling these gaps with ranges or matched values
   would violate the v1 redaction boundary.

4. **Qualifier and excluded identifiers — carried-forward rejection.** The
   Q word-run consumption/advancement and exact boundary rows remain
   unspecified. Identifier components, case folding, component boundaries,
   keyword/suffix and exclusion precedence, separator equivalence, plural
   handling, assignment precedence, and exact value spans remain unresolved.
   The absence of synthetic positive, negative, boundary, and near-miss rows
   means this full-ruleset blocker is not waived by the three current child
   records.

## Compatibility disposition

**ACTIONABLE REJECTION.** Ruleset 4 is compatible in principle as an explicit
versioned extension of the accepted v1 contract. The review can preserve the
closed/offline scanner, complete pre-write scanning, atomic rejection, raw
field-byte coordinates, fingerprint-scoped acknowledgment and redaction
revalidation, value-free diagnostics, permitted stderr routing, and additive
machine output. A ruleset-version boundary also makes changed fingerprints
explicit rather than silently changing v1 behavior.

Compatibility is not established for the exact ruleset-v4 hash above.
Undefined run selection and parsing can change verdicts, fingerprints, raw
redaction reaches, and coverage. Undefined output and mutation edge semantics
can make clients miscount or misclassify findings and publication outcomes.
No v4 compatibility or conformance claim may be made at this hash.

## Threat-model disposition

**ACTIONABLE REJECTION.** A credential can be missed in an unselected or
over-limit decoded run, an unsupported base64 spelling, a malformed/ambiguous
JWT, a private-key boundary, a table-row delimiter boundary, an excluded
identifier, or a value that falls on the unresolved Q boundary. Conversely,
ambiguous parsing can reject ordinary metadata. Divergent derived spans alter
fingerprints and can redact delimiters or too much/too little source text.
Ambiguous advisory counts and output can cause fleet automation to underreact
or mis-triage a finding. Diagnostics must continue to omit matched bytes and
locations; redaction revalidation prevents stale mutation but cannot repair a
missed finding or make divergent fingerprints converge.

## Actionable blockers before acceptance

1. Specify maximal Q run matching/advancement, exact truth-table rows for all
   named classes and predicate boundaries, and every `m` call site.
2. Formalize identifier components, case folding and boundaries, keyword,
   suffix, exclusion and assignment precedence, separator/plural behavior,
   and exact value/quote/trimming spans.
3. Specify base64/base64url alphabet and grammar, invalid-byte and canonical
   handling, run order and 64-run selection, over-limit disposition, and an
   explicit incomplete-coverage result where scanning is bounded.
4. Define JWT header decoding and JSON-member policy, and the private-key rule
   identifier and armor grammar across raw, normalized, dewrapped, and decoded
   views.
5. Give form 3 an unambiguous row grammar for marker spacing, pipe/cell
   precedence, escaping, and value selection.
6. Define the raw-source mapping and whole-run fingerprint/redaction rule once
   decoded and URI/escape parsing are fixed; add harmless exact-range and
   `bead redact --dry-run` revalidation cases.
7. Define the exact stderr grammar, advisory JSON type/location, count and
   deduplication basis, unique sorted rule IDs, output coexistence, mode and
   acknowledgment behavior, no-op/rollback/`--no-auto-flush` semantics, and
   post-commit publication failure behavior.
8. Add independent synthetic positive, negative, boundary, and near-miss
   fixtures for all of the above, then obtain a new independent review at a
   new exact ruleset hash.

## BR-T39 through BR-T44 release gate

**BR-T39 through BR-T44 remain blocked.** The rejection applies to the full
ruleset-v4 release gate; the conditional raw-range result does not release or
bypass it. No one of BR-T39, BR-T40, BR-T41, BR-T42, BR-T43, or BR-T44 may be
treated as accepted on the current hash. They can proceed only after the
specification and independent fixtures address the blockers above and a new
independent review accepts the revised exact artifact.

## Final decision

**REJECTED — actionable specification, fixture, parsing, redaction-contract,
and output-contract revisions are required for**
`research/specs/secret-ruleset-v4.md` at SHA-256
`bad7c8102086d8128465e284b839673ceceb84b28e3ea7bf55835dfaee5ad5a9`.

The compatibility and threat-model dispositions are both actionable
rejection. The raw-range result is accepted only conditionally for its
construction and v1 revalidation properties. The BR-T39 through BR-T44 gate
remains closed.

# BR-T35 aggregate compatibility and threat decision

Date: 2026-10-05.

Aggregate reviewer: OpenAI Codex. This reviewer is distinct from the
specification, ADR, and fixture authors and from the implementation owners.
The review is an independent synthesis of the permitted repository
specifications, fixture material, provenance record, and focused review
artifacts listed below.

**Decision: REJECTED — actionable specification and fixture revisions are
required.** This decision is for the complete
`research/specs/secret-ruleset-v4.md` artifact at the exact SHA-256 below. The
v4 direction is conditionally compatible with the accepted v1 boundary, but
this exact contract is not an acceptable implementation or release baseline.

**Release gate: BR-T39 through BR-T44 remain blocked.** They may proceed only
after the blockers in this record are resolved, the changed contract and
independent fixtures receive a new exact-hash review, and the applicable
conformance gate passes.

## Review boundary and clean-room provenance

This synthesis uses the ruleset and normative inputs below, independently
authored fixture material, and the focused records below. Synthetic vectors
are review evidence, not a substitute for normative fixtures. No source,
tests, fixtures, SQL, comments, output, or prose from another bead
implementation was consulted or used. The aggregate reviewer did not inspect
implementation source. The stderr child review's first-party architecture
observations remain that review's scoped evidence and do not replace normative
requirements. The repository's clean-room protocol and provenance record are
included in the rechecked identities below.

Hashes are SHA-256 over complete file bytes. The reviewed ruleset identity is
not inferred from section excerpts; any byte change requires a new review.

## Rechecked normative, baseline, and fixture inputs

| Artifact | SHA-256 |
| --- | --- |
| `research/specs/secret-ruleset-v4.md` | `bad7c8102086d8128465e284b839673ceceb84b28e3ea7bf55835dfaee5ad5a9` |
| `research/specs/secret-rejection-v1.md` | `f6aa7639a8ef1dd509b431853abf64db78d0923ef9e9d59b09e0a4e0e55df231` |
| `research/specs/secret-write-boundary-v1.md` | `3ef8948c1ad5b94c911780070e5ffee5f26856276d31ccb1b6b845c2eaae4f8e` |
| `research/specs/needle-cli-contract-v1.md` | `64a4057bdec893d81e42a3f23bff86f225a302fc3b5fec18d8ab5cf414ad7233` |
| `research/specs/historical-redaction-v1.md` | `5658ac80cf9594283ddf65742aff2f4b2020a0a7869a612ee2230ed99033a016` |
| `docs/adr/014-hard-reject-secret-bearing-mutations.md` | `1bd43644e26acc0087925067654e93a9ddbbc5a313aff44144d23da205f2fd88` |
| `docs/adr/023-widen-blocking-ruleset-to-observed-credential-formats.md` | `39d9174173df3aadc7c268bdd27d139e1091cc12559e20b9d098037683a7359a` |
| `docs/adr/024-match-on-a-normalized-view.md` | `508989f43c984b3a69ef529eaaf0d96ed1caa44873857e91b14aeaaa61de9c86` |
| `docs/adr/025-surface-advisory-findings-and-partial-scan-coverage.md` | `e13bda45279d2b875c9c2dce6d2bdc3e0471e91512cff22ec40487f1a0de4a33` |
| `research/fixtures/README.md` | `dfd99a8d7a5272dc92e5ec5be16a5f50a426e507bcf3ea866e41bbd910284b64` |
| `research/fixtures/secret-write-boundary-v1.json` | `c75f4dbf6242ced3c7e21e501e1de7a0fbef2a5371c601957c809043f0996710` |
| `docs/reviews/r038-specification-acceptance-2026-09-03.md` | `a7622b292729a33788ea36340946fd8be5495554510ecc839aebf8763903da8a` |
| `man/man1/bead-redact.1` | `957b8b72fee4ecbd0a71520b045794b219dbaae5c5d2d0d96eef3dd60f746153` |
| `research/specs/clean-room-protocol.md` | `8d9cf1eab817fad10311dd19a5c92b0d4ffbfae4aed40f336bd78268e5081e0e` |
| `PROVENANCE.md` | `549c3f5f43d05f5e0d33dd0466536939cd009f013e005143024ce77b5ffab295` |

The fixture set cited by the focused reviews contains nine v1 write-boundary
cases. It has no v4 Q truth table, identifier/exclusion cases, decoded-run
limits, JWT or table-row vectors, or expected v4 raw-range/redaction outcomes.
It therefore cannot provide v4 conformance evidence.

## Focused records synthesized

These are the three current focused records for the Q/exclusion contract,
decoded and structured blocking, and raw-range/redaction behavior. Their
complete bytes were rehashed for this decision.

| Focused record | SHA-256 | Aggregate disposition |
| --- | --- | --- |
| `docs/reviews/beadrs-b60f83c5-ruleset-v4-randomness-excluded-identifiers-review-2026-10-05.md` | `b0a4cb137136e08772a81cf9d32df8481069c9b56f619ee73e45dcb3991c3a48` | Actionable rejection of Q determinism, required vectors, identifier parsing, and exclusion semantics |
| `docs/reviews/beadrs-ced47d50-ruleset-v4-decoded-bounds-structured-review-2026-10-04.md` | `ecee4cf5aefa411b35be612efaf205c5545c87e03b84cbd2fbac91ad21a65c05` | Actionable rejection of decoded bounds/selection, JWT, table-row, and fixture gaps |
| `docs/reviews/beadrs-db0131b7-ruleset-v4-raw-range-redact-review-2026-10-04.md` | `0746141e99d947464e3233d1da8174e7698c8c8b7d78215da4fd8245d72a17c0` | Accepts section 3.4's raw covering-range construction and v1 revalidation only |

The separate BR-T35 write-time-output review is also retained because it is a
release blocker and was an explicit dependency of this aggregate:

| Additional focused record | SHA-256 | Finding retained |
| --- | --- | --- |
| `docs/reviews/beadrs-2f7ad38d-write-time-stderr-compatibility-review-2026-10-04.md` | `0f79ef8d4e7f0d237187840de1fc402b9afd126c0696c129b576621baf84614a` | Actionable rejection of underspecified stderr/JSON semantics and mutation/checkpoint edge behavior |

The following earlier records are supporting evidence carried forward by the
newer focused reviews; none is treated as waiving an open finding:

| Supporting review | SHA-256 | Finding retained |
| --- | --- | --- |
| `docs/reviews/br-t35-randomness-qualifier-truth-table-independent-review-2026-10-04.md` | `bf77cd450a7ed887dddd4ddb468e2aee66b6162067b193cc3223e816396dda60` | Exact Q truth-table and inclusive boundary rows are absent from the v4 fixtures |
| `docs/reviews/br-t41-randomness-excluded-identifiers-independent-review-2026-10-04.md` | `5d222327393f9155e5ee21fd2474952b5a5a5f7c30efb0ba33c6b068eb7e28e4` | Excluded-identifier and Q semantics remain under-specified |
| `docs/reviews/beadrs-524683b8-ruleset-v4-decoded-jwt-table-row-review-2026-10-04.md` | `a0b8293c6152647ce9807fb6afcf5f65ad1b8b0320c3ab20511b50cc830ad8be` | Earlier decoded/JWT/table-row blockers, reaffirmed and extended by `beadrs-ced47d50` |
| `docs/reviews/beadrs-769db4b9-decoded-blocking-raw-range-review-2026-10-04.md` | `8e4f69e77581cfd630771745975d149211ba8ad494f358c78cbb6c517de25690` | Earlier decoded-selection and raw-coordinate concerns, reconciled below with the scoped section 3.4 acceptance |
| `docs/reviews/beadrs-3ecefbce-ruleset-v4-stderr-final-review-2026-10-04.md` | `318228a896d30b18c608f9bf1055bb38ec7ca0b06d2eb12eb03d37cd6270757f` | Earlier stderr/advisory-output blockers, reaffirmed by `beadrs-2f7ad38d` |
| `docs/reviews/br-t35-secret-ruleset-v4-independent-review-2026-10-03.md` | `d08b9c51143f3b3c89e63b047dfcd8ac263c9a142f418aef01859ad593bbc396` | Predecessor rejection and preservation of the accepted v1 boundary |

## Reconciled findings

### Q qualifier and excluded identifiers — rejected

Section 2.2 does not define successful word-run consumption/advancement, so
`w` and the Q result can differ between implementations. The named Q classes
and the inclusive `m=12`, `m=16`, and `m=20` boundaries lack exact normative
rows. Section 4.4 and the related bead-ID text do not fix the identifier
component grammar, case-folding order, boundaries, keyword/suffix precedence,
separator and plural handling, or bead-ID/hash shape. The scope and ordering
of exclusions are not explicit, including `secret_scan`, fencing names,
`max_tokens`, listed suffixes, and `acknowledge-secret` across assignment,
provider, decoded, and advisory rules. The checked-in independent fixture has
none of the required positive, negative, boundary, exclusion, and near-miss
cases. These are distinct findings from `beadrs-b60f83c5` and the earlier Q
reviews; none is waived by the raw-range acceptance.

### Decoded views, JWTs, and table rows — rejected

Section 3.2 gives 64-run and 65,536-byte bounds without deterministic run
selection, over-limit behavior, or an observable incomplete-coverage result.
It also leaves base64/base64url alphabet classification, padding and residual
length rules, and printable-byte arithmetic open. Section 4.1 does not fully
define JWT header decoding or the `alg` JSON-member predicate; decoded private
key grammar remains incomplete. Section 4.4 leaves table marker/pipe/value
precedence, separator matching, escaping, punctuation trimming, and exact
value span ambiguous. The fixture set lacks decoded-limit, JWT, table-row, and
raw-span cases. A value can consequently be missed or spuriously blocked, and
two implementations can select different finding ranges. These findings
retain both the earlier `beadrs-524683b8`/`beadrs-769db4b9` results and the
more recent, independently hashed `beadrs-ced47d50` review.

### Raw ranges and redaction — narrowly accepted, conditional for v4

`beadrs-db0131b7` supports section 3.4's mapping of transformed bytes to the
smallest raw half-open covering range, use of that raw range for the v1
fingerprint, and live-byte revalidation when `bead redact` resolves the
fingerprint. Its harmless percent, escape, ANSI, dewrap, and whole-decoded-run
vectors support that construction. The whole encoded run may be redacted even
when only part decodes to a match; this is an explicit privacy/content-loss
tradeoff, not a literal view-relative span. This scoped acceptance does not
resolve which decoded run, JWT span, table value, or Q-qualified value is
selected. The open parser and coverage ambiguities can still change a
fingerprint or redact too much, too little, or the wrong source bytes.
Consequently, redaction mechanics are accepted only for the stated section
3.4 construction; overall v4 redaction compatibility remains conditional and
unproven.

### Advisory output and mutation behavior — rejected

The additional `beadrs-2f7ad38d` review finds no exact one-line UTF-8 stderr
grammar; no normative `secret_scan.advisory_findings` type, location, count,
deduplication, or rule ordering; and an unresolved distinction between
advisory-tier and mode-demoted blocking findings. Acknowledgment, no-op,
rollback, `--no-auto-flush`, and post-commit checkpoint-publication failure
behavior are not specified for the notice. “Unchanged stdout” is ambiguous
alongside additive machine output. NEEDLE permits diagnostics on stderr and
the v1 value-free, pre-write rejection and atomic acknowledgment direction is
compatible, but it does not fill these v4 serialization and lifecycle gaps.
Diagnostics must remain value-free and location-free.

## Compatibility and threat-model dispositions

**Compatibility: ACTIONABLE REJECTION for this exact hash.** A separately
versioned v4 extension can preserve the accepted v1 closed/offline scanner,
complete pre-write scanning, atomic rejection, raw field-byte coordinates,
fingerprint-scoped acknowledgment and redaction revalidation, and value-free
diagnostics. But the unresolved rules above permit different verdicts,
coverage, output counts, fingerprints, and redaction ranges. No v4
compatibility or conformance claim is supported at this hash.

**Threat model: ACTIONABLE REJECTION.** A credential can evade blocking after
an unspecified decoded-run cap, inside an over-limit run, in a JWT/private-key
or table-row boundary, under an excluded identifier, or at an unresolved Q
boundary. Ambiguous parsing can also reject ordinary metadata. Different
selected ranges change exact fingerprints and can redact delimiters or
unrelated bytes. Ambiguous counts and output can cause consumers to underreact
or mis-triage. Redaction revalidation prevents stale mutation; it cannot
recover a missed finding or reconcile divergent parsers.

## Actionable blockers before acceptance

1. Define Q run consumption and advancement; publish exact expected rows for
   every named class and each `m=12/16/20` predicate boundary.
2. Specify identifier components, normalization and case handling,
   keyword/suffix precedence, separator and plural behavior, every exclusion's
   scope and order, and exact bead-ID/hash shapes.
3. Specify decoded alphabet/grammar, run ordering and the 65th-run behavior,
   over-65,536-byte behavior, invalid-byte handling, printable-count
   arithmetic, and visible partial-coverage semantics.
4. Define JWT header/base64url and JSON-member rules, private-key armor
   matching, and table-row marker/separator/value/escape/punctuation rules
   with exact raw half-open spans.
5. Define the exact stderr line and newline, advisory JSON schema/location,
   count/dedup basis and stable rule ordering, mode/tier behavior, and
   acknowledgment/no-op/rollback/`--no-auto-flush`/post-commit publication
   semantics without exposing matched bytes or locations.
6. Add independently authored, harmless v4 fixtures or test-time constructors
   for the Q classes and boundaries, identifiers and exclusions, decoded-run
   limits/coverage, JWT and table-row positives/near misses, raw ranges and
   `bead redact --dry-run`, and stderr/machine-output cases. Recompute all
   affected hashes and obtain a new independent exact-hash review before
   changing the release gate.

Until those blockers and the applicable conformance evidence are satisfied,
the complete ruleset remains **REJECTED**, and **BR-T39, BR-T40, BR-T41,
BR-T42, BR-T43, and BR-T44 remain blocked**. The section 3.4 raw-range
acceptance is limited to its construction and does not authorize release.

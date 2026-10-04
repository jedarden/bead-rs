# Ruleset v4 write-time stderr and final disposition review

Date: 2026-10-04.

Reviewer: OpenAI Codex, independent of the specification authors and
implementation owners.

Decision: **ACTIONABLE REJECTION.** The reviewed ruleset-v4 bytes are
compatible with the accepted v1 process and mutation invariants in principle,
but section 5.2 does not define a reproducible stderr or machine-output
contract. The full ruleset therefore remains unaccepted at this exact hash.

## Review boundary and method

This is the final review for the write-time notice and the consolidated review
disposition after the three focused ruleset-v4 reviews. I compared section 5.2
with `needle-cli-contract-v1`, ADR-025, the accepted `secret-rejection-v1`,
and the prior review records listed below. I used no other bead
implementation's source, tests, fixtures, output, or prose, and did not use
implementation behavior as evidence.

## Exact reviewed inputs

The target contract is the complete file below, not a section extracted from
it:

| Artifact | SHA-256 |
| --- | --- |
| `research/specs/secret-ruleset-v4.md` | `bad7c8102086d8128465e284b839673ceceb84b28e3ea7bf55835dfaee5ad5a9` |
| `research/specs/needle-cli-contract-v1.md` | `64a4057bdec893d81e42a3f23bff86f225a302fc3b5fec18d8ab5cf414ad7233` |
| `research/specs/secret-rejection-v1.md` | `f6aa7639a8ef1dd509b431853abf64db78d0923ef9e9d59b09e0a4e0e55df231` |
| `docs/adr/023-widen-blocking-ruleset-to-observed-credential-formats.md` | `39d9174173df3aadc7c268bdd27d139e1091cc12559e20b9d098037683a7359a` |
| `docs/adr/024-match-on-a-normalized-view.md` | `508989f43c984b3a69ef529eaaf0d96ed1caa44873857e91b14aeaaa61de9c86` |
| `docs/adr/025-surface-advisory-findings-and-partial-scan-coverage.md` | `e13bda45279d2b875c9c2dce6d2bdc3e0471e91512cff22ec40487f1a0de4a33` |
| `docs/reviews/r038-specification-acceptance-2026-09-03.md` | `a7622b292729a33788ea36340946fd8be5495554510ecc839aebf8763903da8a` |

The accepted v1 baseline is therefore the exact `secret-rejection-v1` hash
above; R038 records its unconditional acceptance. The prior review artifacts
used to consolidate this decision are:

| Review artifact | SHA-256 | Disposition relevant here |
| --- | --- | --- |
| `docs/reviews/br-t30-secret-write-boundary-v1-independent-review-2026-10-03.md` | `12b9228bad0aef25a1b91180472c9408a41857b5cc7c47ee6b99a64a079b92db` | Accepted v1 boundary; v4 must retain its value-free and atomic-write invariants. |
| `docs/reviews/br-t35-secret-ruleset-v4-independent-review-2026-10-03.md` | `d08b9c51143f3b3c89e63b047dfcd8ac263c9a142f418aef01859ad593bbc396` | Integrated actionable rejection; T39–T44 remain blocked. |
| `docs/reviews/br-t41-randomness-excluded-identifiers-independent-review-2026-10-04.md` | `5d222327393f9155e5ee21fd2474952b5a5a5f7c30efb0ba33c6b068eb7e28e4` | Actionable rejection of qualifier and exclusion semantics. |
| `docs/reviews/beadrs-524683b8-ruleset-v4-decoded-jwt-table-row-review-2026-10-04.md` | `a0b8293c6152647ce9807fb6afcf5f65ad1b8b0320c3ab20511b50cc830ad8be` | Actionable rejection of decoded-run, JWT, and table-row boundaries. |
| `docs/reviews/beadrs-db0131b7-ruleset-v4-raw-range-redact-review-2026-10-04.md` | `0746141e99d947464e3233d1da8174e7698c8c8b7d78215da4fd8245d72a17c0` | Narrow acceptance of section 3.4 range construction only. |

For auditability, the SHA-256 of the lexicographically sorted `sha256sum`
lines for the twelve paths in the two tables above is
`c3a81d2c5e02d4ea25f65d69b2da42c0c97fe3fc5a276c96a0f9c530dd8570d5`.

## Section 5.2 stderr determination

Section 5.2 and ADR-025 require a successful mutation that admits advisory
findings to emit exactly one line on standard error. The line must name the
count, rule identifiers, and `bead doctor --scope secrets`; it must not contain
matched bytes, must not alter exit status or standard output, and must be
absent in `off` mode. `needle-cli-contract-v1` makes this channel choice
compatible: successful machine-readable output stays valid on stdout, and
diagnostics are allowed on stderr. `secret-rejection-v1` independently keeps
matched bytes out of diagnostics and requires a complete pre-transaction scan
with atomic rejection for unacknowledged blocking findings.

That is a **PASS for channel and routing compatibility only**. It is not an
acceptance of the exact notice contract. The reviewed inputs do not determine:

- whether `secret_scan.advisory_findings` is a number, an array, or another
  object, or where it is placed on each machine-readable mutation result;
- whether the count means findings, distinct rule/range pairs, fingerprints,
  or something else after the section 5.1 per-field cap is applied;
- how duplicate findings from multiple views are counted, and whether rule
  identifiers are unique;
- the ordering and serialization of rule identifiers; or
- the literal one-line grammar, including its fixed prefix, separators,
  pluralization, newline boundary, and interaction with other success
  diagnostics.

The earlier focused write-time review recorded the same split result: stderr
routing is compatible with NEEDLE, while the exact serialization, count basis,
stable ordering, and JSON type/location are actionable gaps. The section 3.4
review's narrow acceptance does not resolve these section 5.2 questions.

## Consolidated compatibility disposition

**ACTIONABLE REJECTION.** The v4 direction can be an additive, explicitly
versioned extension of the accepted v1 contract. The reviewed material retains
raw field coordinates, fingerprint-scoped acknowledgment and redaction,
value-free diagnostics, complete pre-transaction scanning, and atomic
blocking rejection. The raw-range review accepts section 3.4's mapping for
that section only. Those compatible parts do not supply the missing v4
qualifier, decoded-view, table-row, URI, or advisory-output semantics.

In particular, two conforming implementations can emit different advisory
counts or rule lists while both satisfy the current prose. A consumer cannot
reliably parse or compare `secret_scan.advisory_findings`, and a NEEDLE client
cannot treat the notice as a stable machine-facing diagnostic. The exact
ruleset-v4 hash consequently cannot claim compatibility or conformance.

## Consolidated threat-model disposition

**ACTIONABLE REJECTION.** The unresolved output contract can cause an agent
or fleet sweep to miss an admitted advisory finding, miscount it, or treat a
successful mutation as a different event. Unspecified ordering and duplicate
handling make triage evidence non-reproducible. An implementation that fills
the gap by emitting matched values, locations, or overly broad diagnostic
context would violate the v1 value-redaction boundary. The other focused
reviews add independent detection and redaction risks: ambiguous Q and label
parsing can miss or falsely reject values, decoded-run and table-row gaps can
diverge on encoded credentials, and URI-view disagreement can change the
finding fingerprint and redaction reach.

The intended one-line redacted notice is a useful accidental-disclosure
control, but its security value is not bounded until its fields, count basis,
ordering, and grammar are normative.

## Required actions before acceptance

1. Define the exact JSON type and location of `secret_scan.advisory_findings`
   for every machine-readable successful mutation result.
2. Define the aggregate count and deduplication identity, including view
   duplicates, per-field caps, and stable ordering of unique rule identifiers.
3. Define one stable UTF-8 stderr notice grammar, its exact newline behavior,
   coexistence with unrelated diagnostics, and its absence in `off` mode;
   require that it contains no matched bytes or locations.
4. Resolve the actionable blockers recorded by the qualifier/exclusion and
   decoded/JWT/table-row reviews, and the URI ambiguity retained by the
   integrated review.
5. Add the required independent synthetic boundary fixtures, then review the
   changed specification and fixture bytes at a new exact SHA-256.

## BR-T39 through BR-T44 gate

The release gate is preserved. At review time the live dependency graph
reported the following blocked frontier:

| Work | Bead | Existing blocking path |
| --- | --- | --- |
| BR-T39 | `beadrs-5cf44cfe` | `beadrs-1c110609` (BR-T35 review) |
| BR-T40 | `beadrs-7740733d` | BR-T35 and BR-T39 |
| BR-T41 | `beadrs-1d8b4c78` | BR-T35 and BR-T39 |
| BR-T42 | `beadrs-3cf43a16` | BR-T41 |
| BR-T43 | `beadrs-dfe88716` | BR-T41 (and therefore BR-T35) |
| BR-T44 | `beadrs-de9b30a6` | BR-T39 through BR-T43 |

Because this review is an actionable rejection, none of BR-T39 through BR-T44
may be treated as accepted or released by this record. The edges must remain
in place until the revised specification and fixtures receive a new
independent acceptance review.

## Final decision

**ACTIONABLE REJECTION of `research/specs/secret-ruleset-v4.md` at
SHA-256 `bad7c8102086d8128465e284b839673ceceb84b28e3ea7bf55835dfaee5ad5a`.**

The stderr channel is compatible with the NEEDLE process contract, but the
write-time notice and additive machine member are not exact enough for
independent compatibility. The consolidated compatibility and threat-model
dispositions are both actionable rejection. BR-T39 through BR-T44 remain
blocked.

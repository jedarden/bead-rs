# Focused independent review: write-time stderr and compatibility boundary

Date: 2026-10-04.

Reviewer: OpenAI Codex (`beadrs-2f7ad38d`), independent of the listed
specification authors, decision-makers, and implementation owners.

Decision: **ACTIONABLE REJECTION.** The channel choice, redaction boundary,
pre-write rejection, acknowledgment atomicity, and nonzero-exit direction are
compatible with the normative NEEDLE and secret-write contracts. The exact
write-time notice and additive machine-output contract are not reproducible at
the reviewed hashes. The repository architecture also exposes a tier/mode
classification gap that must be resolved before claiming section 5.2
conformance.

## Review boundary and method

I reviewed section 5.2 of ruleset v4, ADR-025, the complete NEEDLE CLI
contract, the complete secret-write and secret-rejection contracts, and the
tracked CLI/write-path architecture and focused contract test. Architecture
files are implementation evidence, not a replacement for normative wording.
No other bead implementation's source, fixtures, SQL, output, or prose was
used as behavioral evidence. The review does not accept any other ruleset-v4
section.

## Exact reviewed artifacts

The hashes below are SHA-256 values of the complete files reviewed, including
their line endings and bytes outside the cited sections.

| Artifact | SHA-256 |
|---|---|
| `research/specs/needle-cli-contract-v1.md` | `64a4057bdec893d81e42a3f23bff86f225a302fc3b5fec18d8ab5cf414ad7233` |
| `research/specs/secret-write-boundary-v1.md` | `3ef8948c1ad5b94c911780070e5ffee5f26856276d31ccb1b6b845c2eaae4f8e` |
| `research/specs/secret-rejection-v1.md` | `f6aa7639a8ef1dd509b431853abf64db78d0923ef9e9d59b09e0a4e0e55df231` |
| `research/specs/secret-ruleset-v4.md` | `bad7c8102086d8128465e284b839673ceceb84b28e3ea7bf55835dfaee5ad5a9` |
| `docs/adr/025-surface-advisory-findings-and-partial-scan-coverage.md` | `e13bda45279d2b875c9c2dce6d2bdc3e0471e91512cff22ec40487f1a0de4a33` |
| `src/cli_secret_scan.rs` | `a8de67e47ac635e686c3f0b546f888654d07b79f1c1749bc7a667360a511b98c` |
| `src/main.rs` | `ec27e27bc71402c717aa32fb14c22fca95692dc81b6df09c469d582c9a470705` |
| `src/scan/mod.rs` | `7ddba2e97093c9548b1547f466f4bd8fecab4a7ca58dc30a09393bb9c4e5d63b` |
| `src/error.rs` | `c85e65d862b69f490ab8d3c3c9cb0b8e70ff34d90bd2cf1c4e537d9dc1c1ce13` |
| `tests/secret_gap_contract.rs` | `041c68d98f74603158a36fc702dc25dcdd305eb3a5186df902957670ce0c86a3` |

## Contract comparison

### What is compatible

NEEDLE section “Process rules” requires valid UTF-8 machine output on stdout,
diagnostics on stderr without corrupting JSON, nonzero exit for failures, and
durability before exit 0. It does not prohibit a successful command from
writing a redacted advisory diagnostic to stderr. Thus the proposed channel is
compatible in principle.

The v1 rejection contract requires one unacknowledged blocking finding to
reject the complete request with exit 2 and leave SQLite, events, checkpoint
state, and files unchanged. The write-boundary contract repeats the same
pre-write guarantee and requires an exact-fingerprint acknowledgment, when
permitted, to be audited in the same transaction as the semantic mutation.

The reviewed write path follows those invariants: `prepare` resolves policy
and scans before dispatch; `finalize` returns a CLI-usage error for an
unacknowledged blocking finding; and the top-level error mapping assigns that
family exit 2. The acknowledgment and advisory guards are armed before the
mutation, while `report_advisories` runs only after a successful dispatch. The
checkpoint publication probe runs after that semantic result, so an advisory
notice itself does not become a semantic row, event, acknowledgment, or
checkpoint mutation. These are compatibility-positive observations, not
acceptance of the unresolved output syntax.

### Finding 1 — no exact stderr contract (actionable)

Ruleset v4 section 5.2 and ADR-025 say that exactly one stderr line names the
count, rule identifiers, and `bead doctor --scope secrets`, and that matched
bytes are omitted. They do not define the literal line grammar. In
particular, they leave unspecified the fixed prefix, separators, quoting,
pluralization, required final newline, UTF-8/line-control restrictions,
identifier ordering, duplicate handling, and coexistence with unrelated
success diagnostics.

The tracked implementation currently chooses a line of the following shape:

```text
secret_scan advisory: N finding(s), rules R1, R2; inspect bead doctor --scope secrets. Matched bytes are not shown.
```

That choice is not established by NEEDLE. The focused test asserts only one
line beginning with `secret_scan advisory:`, that one rule identifier occurs,
and that the candidate bytes do not occur. Two conforming implementations can
therefore emit different lines while satisfying the reviewed prose. NEEDLE
confirms stderr routing, not this serialization.

### Finding 2 — advisory type and count semantics are contradictory or absent

Ruleset v4 says machine output gains `secret_scan` with `advisory_findings`,
while ADR-025 calls the addition an “additive count.” Neither document says
whether `advisory_findings` is an integer, an array of finding objects, or an
object containing a count; neither says which mutation result types carry it.
The current implementation emits an array of redacted finding objects and
uses its length for the stderr count. That is an implementation convention,
not a normative result.

The count basis is also not fixed. Section 5.1 caps findings per field and
section 3.4 deduplicates a rule/range pair across views, but section 5.2 does
not say whether the notice and JSON count fingerprints, distinct rule/range
pairs, array elements after view deduplication, or distinct rule identifiers.
It also does not define whether the rule list is unique and sorted. The
implementation currently deduplicates by fingerprint for the invocation,
counts those findings, and emits a sorted unique rule set, while the JSON
array retains each finding. This must be specified, including ordering, before
machine consumers can compare results.

There is an additional semantic mismatch in the write architecture. The
collector filters `report.findings` by `!is_blocking_match()`. In `advisory`
workspace mode, scanning still retains a confirmed blocking-tier finding in
`report.findings` and rejection is merely disabled; that finding consequently
enters `advisory_findings`. The v1 contract reserves “advisory” for the
advisory rule tier, while section 5.2 says “advisory findings.” The contract
must either explicitly define mode-demoted blocking findings as notice
advisories or require collection by `tier == Advisory`. The current focused
test covers an unlabelled advisory candidate only and does not exercise this
boundary.

### Finding 3 — acknowledgment and mutation/checkpoint edge cases are not observable

The v1 contracts define acknowledgment scope and same-transaction audit
atomicity, but section 5.2 does not state whether an exact acknowledgment of a
blocking finding changes the advisory count or notice. The architecture
correctly keeps acknowledged blocking findings out of the current notice
collector, but that exclusion is not an explicit compatibility rule.

The phrase “when a mutation succeeds” is also underspecified for the existing
publication architecture:

* A semantic no-op can return success without changing a row. The current
  success path still exposes the prepared advisory summary; the contracts do
  not say whether a notice is required for that invocation.
* A semantic transaction can commit and then automatic checkpoint publication
  can fail. The current path emits the notice and may already have emitted
  machine stdout before returning the defined post-commit publication error
  (exit 1). The contracts do not say whether “succeeds” means semantic commit
  or final process exit, or how consumers distinguish this split outcome.
* `--no-auto-flush` suppresses checkpoint publication but does not suppress the
  semantic mutation. The advisory line and additive field need an explicit
  statement that they do not imply checkpoint publication.

These cases do not weaken the v1 pre-write rejection guarantee, but without
observable rules they permit incompatible automation and ambiguous audit
interpretation at the mutation/checkpoint boundary.

### Finding 4 — “unchanged stdout” conflicts with additive machine output

Section 5.2 says both that machine output gains `secret_scan` and that
standard output text is unchanged. If “standard output text” includes JSON,
those requirements conflict; if it means human-readable text only, that scope
must be stated. NEEDLE permits additive fields only where the consumer contract
allows them; its issue-record rule allows additional fields, but it does not
define every mutation result envelope. The v1 statement that older clients
ignore additive fields is not enough to establish the type and location of
this new member for every machine-readable mutation.

The current decorator adds the object only when a serialized mutation result
is a top-level JSON object, and the focused test covers only manifest JSON.
The specification needs a per-command/output-format rule or a versioned
capability declaration so an older NEEDLE adapter cannot mistake an omitted,
array-shaped, or differently nested field for “no advisory findings.”

## Compatibility and threat-model impact

The intended extension is compatible in principle if it remains optional and
additive: stderr is a diagnostic channel allowed by NEEDLE, blocking findings
still fail before the transaction with exit 2, exact acknowledgments remain
fingerprint-scoped and atomic, and advisory reporting never places matched
bytes in stdout, stderr, events, or checkpoints. Ruleset version 4 also needs
to remain an explicit fingerprint boundary rather than silently changing v1
behavior.

Compatibility is **not established for the reviewed hashes**. An older or
independent consumer cannot know whether the JSON value is a count or a list,
or whether a count is per finding or per rule. It cannot reliably parse the
stderr notice, and it cannot distinguish a successful semantic mutation with
failed publication from an ordinary success. The advisory/mode mismatch can
also label a confirmed credential as merely advisory, causing a fleet
triage/automation consumer to underreact. Conversely, an implementation that
fills the prose gap with locations or matched content would violate the v1
redaction boundary. These are accidental-disclosure and audit-integrity risks,
not just formatting preferences.

## Required revision before acceptance

1. Specify one exact UTF-8 stderr grammar, including fixed tokens, count
   basis, unique sorted rule identifiers, newline behavior, and the rule that
   no matched bytes, ranges, or caller-controlled text appear in the line.
2. Specify the exact JSON type and location of `secret_scan.advisory_findings`
   for every machine-readable successful mutation result, and clarify that
   human-readable stdout is the only output whose text remains unchanged.
3. Define finding identity and count/deduplication across fields, views,
   repeated operations, per-field caps, and acknowledged blocking findings.
   Explicitly resolve whether `advisory` mode demotes blocking-tier findings
   or whether only `Tier::Advisory` findings are reported by section 5.2.
4. Define notice/JSON behavior for semantic no-ops, rollback or validation
   failure, `--no-auto-flush`, and post-commit checkpoint publication failure.
5. Add independent synthetic tests that assert exact stderr bytes, exit code,
   stdout JSON shape, off/advisory/enforce modes, exact acknowledgment, no-op,
   rollback, and publication-failure outcomes without committing candidate
   values.

## Final disposition

**ACTIONABLE REJECTION** of the write-time stderr and machine-output portion
of `research/specs/secret-ruleset-v4.md` at SHA-256
`bad7c8102086d8128465e284b839673ceceb84b28e3ea7bf55835dfaee5ad5a` and ADR-025
at SHA-256 `e13bda45279d2b875c9c2dce6d2bdc3e0471e91512cff22ec40487f1a0de4a33`.
NEEDLE compatibility is established for channel routing and failure direction
only. A revised exact contract and new independent review are required before
claiming section 5.2 compatibility or conformance.

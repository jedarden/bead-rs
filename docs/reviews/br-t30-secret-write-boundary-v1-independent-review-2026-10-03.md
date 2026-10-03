# BR-T30 independent review — secret write boundary v1

Date: 2026-10-03.

Reviewer: OpenAI Codex (GPT-6), independent of the ADR/specification authors
and implementation owners. This review used only bead-rs repository materials
and the committed bead-rs architecture. No other bead implementation, source
tree, fixture corpus, or scanner output was inspected.

## Reviewed artifacts and exact identities

The reviewed bytes are those at repository anchor commit
`a414b4ef4764ce08adb51d5929a9f8c424bb1958`. The target artifacts were
byte-identical in the working tree and at that commit.

| Artifact | SHA-256 |
| --- | --- |
| `docs/adr/020-enforce-secret-scanning-at-service-writes.md` | `00a26d591f5e106bc693f086e488c0c2caab3808c9a7afe98e3ae4cc322d28ca` |
| `docs/adr/021-lock-fleet-secret-scan-policy.md` | `22a5fdef179993db065d9c2c45fa870a782d47f12c0d312de242d98e7a01a014` |
| `docs/adr/022-quarantine-secret-bearing-recovery.md` | `974e9bee4bb48846808c5fd55f03b6660327bfe9aa4d250a92bda8b14fa17253` |
| `research/specs/secret-write-boundary-v1.md` | `3ef8948c1ad5b94c911780070e5ffee5f26856276d31ccb1b6b845c2eaae4f8e` |
| `research/fixtures/secret-write-boundary-v1.json` | `c75f4dbf6242ced3c7e21e501e1de7a0fbef2a5371c601957c809043f0996710` |

## Review method and architecture fit

I read ADR-020 through ADR-022, the v1 contract and fixture, and the accepted
secret rejection, historical redaction, checkpoint publication, interchange,
and verified recovery contracts. I compared them with the committed public
library surface and service modules, including issue and comment writes,
structured data, manifests, recurrence, lifecycle, dependency, external
reference, attempt, claim, and checkpoint operations. The crate exposes
`service` publicly and several mutation APIs take a store or SQLite connection
directly, so CLI-only preflight cannot provide the contract's guarantee.
Requiring a private, workspace-bound write context (or an equivalent gated
compatibility shim) addresses that boundary. Keeping arbitrary caller SQL and
alternative executables outside the guarantee is consistent with the stated
accidental-disclosure threat model.

The contract preserves the repository's store/checkpoint equivalence rule:
new semantic writes are rejected before commit, while recovery that encounters
old input may commit locally and durably quarantine publication. Its publisher
checks, redacted status, restart persistence, and fingerprint-selected
redaction path align with the existing checkpoint and historical-redaction
semantics. The quarantine is a publication hold, not a claim that prior source
artifacts, local inputs, or Git history have been sanitized; incident
containment and credential rotation remain separate.

Compatibility is adequately bounded. The general artifact retains ADR-014
modes and exact-fingerprint acknowledgments; the managed artifact narrows that
policy to enforced scanning with no worker acknowledgment path. Existing
library callers may need a source-level API change, and the contract makes
that explicit for a release. A compatibility shim is allowed only when it
applies the same gate. The broad requirement to scan every persisted
operator-supplied string applies to the full canonical persisted record,
including ancillary text and extension data, rather than only the examples
listed in the inventory; future mutators remain subject to the same gate.

## Threat model and synthetic fixtures

The protected case is accidental submission of a detectable credential
through a public bead-rs mutation API, or creation of a new Git-trackable copy
when a recovery input contains one. The contract explicitly does not claim to
stop arbitrary code running as the database owner, direct SQL, another
executable, process-argument exposure, or secrets already present in
published history. The managed build plus the separate Forgejo transport gate
is a layered accidental-disclosure control, not a hostile-code sandbox.

The fixture contains nine scenario identifiers and expected outcomes; it
contains no candidate credential values. This is compatible with the
requirement to assemble synthetic candidates at runtime and never emit the
matched bytes. I accept it as a compact scenario index, not as a complete
executable conformance suite: implementation conformance must cover every
scenario required by contract section 6, including malformed/downgraded
policy, restart and retained-generation recovery, audit atomicity, and
concurrent publication.

## Managed-policy false positives — section 4.6 disposition

I resolve the review question in section 4.6 in favor of the specified initial
path: under the managed policy, an operator must redact the exact scanner
finding and publish a sanitized generation. That can remove legitimate
text, but it is bounded, fingerprint-selected, auditable, and does not create
a worker-controlled bypass. No blanket acknowledgment or local exception is
accepted. Any future administrator-controlled false-positive resolution
needs its own exact, fingerprint-scoped, audited contract and independent
review before implementation.

## Disposition

**ACCEPT** the exact five-artifact byte set identified above: the three ADRs,
`secret-write-boundary-v1.md` at SHA-256
`3ef8948c1ad5b94c911780070e5ffee5f26856276d31ccb1b6b845c2eaae4f8e`, and
its fixture at SHA-256
`c75f4dbf6242ced3c7e21e501e1de7a0fbef2a5371c601957c809043f0996710`.
Any change to those bytes requires review of the new hash. This disposition
accepts the contract and fixture only; it is not implementation conformance,
managed-binary, or fleet-release approval.

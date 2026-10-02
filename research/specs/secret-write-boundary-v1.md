# Secret Write Boundary Contract v1

Status: proposed normative specification; independent exact-hash review required
before implementation.

Artifact identity: `urn:bead-rs:spec:secret-write-boundary:v1`.

Date: 2026-10-02.

This contract extends `secret-rejection-v1` and `historical-redaction-v1`.
Their ruleset, fingerprint, redacted output, and anti-resurrection contracts
remain authoritative where this document does not narrow them.

## 1. Threat and scope

The protected action is a bead-rs public API committing a newly supplied
high-confidence credential or publishing one newly encountered during
recovery. A detector cannot prove that every possible secret is absent.
Arbitrary caller SQL, an alternative executable, process argv, shell history,
and already-published Git history are outside this API guarantee. Those
boundaries require separate controls and credential rotation after exposure.

## 2. Service mutation gate

1. Each public mutating service entry must resolve its effective scan policy
   from its workspace and scan every persisted operator-supplied string before
   the first semantic write. Generated strings are scanned after expansion
   and before their transaction commits. A caller cannot provide an arbitrary
   `Mode`, report, or preapproved result in place of that resolution.
2. The write inventory includes issues, labels, resources, dependencies,
   comments, structured data, external references, attempts, recurrence
   templates and materializations, and bulk manifests. An internal helper
   called by an already guarded transaction does not independently commit.
3. For a blocking finding in enforce mode, the complete operation fails with
   `secret_detected` and the existing validation exit family. No semantic
   row, event, receipt, checkpoint generation, or Git index entry from that
   operation changes. The diagnostic includes only rule, selector, field,
   byte range, and fingerprint.
4. An exact-fingerprint acknowledgment permitted by policy is recorded in
   the same transaction as the semantic write. A no-op, validation failure,
   or rollback records no acknowledgment event. The CLI and public library
   use one canonical scan result so their decisions cannot diverge.
5. A public library caller must provide or obtain a workspace-bound write
   context. The context's fields are private to bead-rs and it is invalid
   after a different workspace or policy generation is substituted. Raw
   mutating connection helpers become private or are marked as guarded-only
   with an unforgeable context. A compatibility shim may exist only if it
   performs the same gate before writing.
6. Policy resolution, scan, and commit occur under the workspace operation
   lock or an equivalent serialization rule. A policy downgrade racing a
   write cannot change the verdict after scanning and before commit.

## 3. Managed-fleet policy

The proposed managed artifact uses a compiled `managed-secret-policy` feature.
It requires `enforce`, refuses workspace `advisory` or `off`, and refuses all
invocation or workspace fingerprint acknowledgments before mutation. Absence
or malformed workspace scanner configuration still fails closed. The feature
cannot be disabled by a workspace file or environment variable. A general
artifact retains ADR-014 behavior. Capabilities and doctor identify the
compiled policy and effective mode so consumers can require the managed
artifact. The release process pins the exact managed binary used by NEEDLE.

This is an accidental-disclosure control, not a hostile-code sandbox: a
worker able to execute arbitrary code as the database owner could use a
different binary or write SQLite directly. The independent Forgejo gate
covers Git publication, including checkpoint files.

## 4. Recovery quarantine

1. `restore`, `sync import-only`, and `sync reconcile` inspect the resulting
   semantic state and retained generation set for unacknowledged blocking
   findings under the effective ruleset. Existing input may be admitted to
   the local store, but a finding commits a durable quarantine state in the
   same transaction as recovery. Recovery's machine result states that local
   recovery succeeded and checkpoint publication was withheld.
2. While quarantined, ordinary semantic writes, automatic checkpoint
   publication and staging, `sync flush-only`, and `sync commit` refuse with
   the redacted `secret_quarantined` reason. Read-only commands, further
   explicit recovery, and `bead redact` remain available. The redaction
   publisher may write only a verified sanitized generation set.
3. Every publisher checks quarantine while holding the checkpoint publication
   lock. A restart, another process, or `--no-auto-flush` cannot clear it.
   Doctor and `sync status` show the count, rule identities, and remedy but
   never matched bytes.
4. After redaction, the scanner rechecks live state plus every retained
   generation that the next publication would reference. Only a zero
   unacknowledged blocking verdict clears quarantine. A ruleset upgrade
   triggers revalidation, not silent clearance.
5. A clean recovery follows the existing publish behavior. Existing Git
   artifacts that already contain a finding remain exposed until separate
   incident containment; quarantine prevents a new bead-rs publication.
6. False-positive handling under the managed policy is a review question.
   The initial implementation may require exact redaction; any alternative
   resolution must be administrator controlled, fingerprint scoped, audited,
   and specified before implementation. No worker-facing blanket bypass is
   permitted.

## 5. Git transport layer

The built-in scan is authoritative for bead-rs writes. The separate
`secret-scanner` checks staged additions and novel server-side commits; the
broader Gitleaks gate remains in the pre-receive path. Neither Git-layer
scanner substitutes for the service gate because it runs after SQLite has
accepted content. Existing rollout beads in the fast-secret-scanner repo own
installation and cross-agent verification.

## 6. Conformance

Independent fixtures and tests must use synthetic candidates assembled at
runtime and never emit matched values. Required scenarios include direct
library writes for every public mutation family, recurrence text generated
from an imported template, manifest all-or-none rejection, acknowledgment
audit atomicity, policy downgrade and malformed policy, both build profiles,
clean recovery, secret-bearing local artifact recovery, repeated recovery,
restart during quarantine, explicit flush/commit refusal, redaction clearing
quarantine, retained-generation scanning, and concurrent publication.

The reviewer must resolve section 4.6 and record an exact SHA-256 acceptance
or actionable rejection. BR-T31 through BR-T33 remain blocked until then.

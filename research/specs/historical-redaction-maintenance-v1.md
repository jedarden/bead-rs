# Historical redaction maintenance v1

Implementation authorized by the repository owner's 2026-10-03 request to
close the reviewed gaps. Independent review and release acceptance remain
separate; this document is not a self-approval record.

This contract extends historical-redaction-v1 with local maintenance safety.
It is derived from that contract, secret-write-boundary-v1, the owner's
request, and SQLite's public secure_delete, VACUUM and WAL checkpoint APIs.

## Durable hold and local cleanup

1. Any redaction epoch whose publication state is committed holds ordinary
   checkpoint publication, Git staging and sync commit. Every publication
   entry checks the hold while holding the publication lock. Restart and
   alternate public service calls cannot bypass it. Diagnostics name only
   receipt IDs and the redact --resume remedy.
2. Secure deletion is enabled and checked before the redaction write. The
   replacement, hashes, revisions, nonsecret receipts, epoch and recovery
   tombstones commit in one IMMEDIATE transaction as before.
3. The exceptional publisher resets both retained checkpoint pointers to
   sanitized content and removes superseded artifacts. While the epoch is
   still committed it compacts the sanitized live database using VACUUM,
   then runs a checked WAL checkpoint with TRUNCATE. Busy readers or an I/O
   failure leave the epoch committed and resumable. No process deletes a
   WAL or replaces a live database file directly.
4. Only after sanitized publication and local cleanup succeed does a separate
   transaction mark the epoch and its receipts published. This last
   transaction contains only nonsecret maintenance metadata. Published
   checkpoint projections and local receipt state converge on resume.
5. The local guarantee concerns SQLite-readable content, database free space
   and the current WAL, not disk-controller remnants, filesystem snapshots,
   old Git objects, backups or another process's prior read snapshot.

## Metadata and selected batches

6. Non-key diagnostic metadata fields may use the same exact byte replacement
   as prose. Any integrity hash and affected issue revision are maintained.
   A finding in text that also identifies a row must either use explicitly
   specified collision-safe rekeying with all related references, or report
   a typed unsupported-identity disposition. The API never permits callers
   to choose replacement bytes or SQL. No remaining unsupported disposition
   may be represented as successful erasure.
7. A batch accepts only scanner fingerprints, actor and nonsecret reason.
   Every fingerprint is resolved and revalidated against one IMMEDIATE
   transaction snapshot before changes. Duplicate inputs, overlapping spans
   and stale selections fail without changing any row or audit fact. Ranges
   in the same field are replaced from greatest offset to least.
8. Every finding has its own nonsecret receipt and tombstone. All receipts
   share one epoch. Associated issue revisions advance once per affected
   issue. An identical complete replay returns the existing receipts; a
   partial or conflicting replay refuses. The CLI publishes that epoch once
   and accepts any of its receipt IDs for resume.
9. A complete private-key PEM finding includes the body through its closing
   delimiter, not only the opening line. Derived-view findings replace their
   mapped original byte spans, never a decoded copy alone.
10. Recovery exemption applies only to existing artifact content. Newly
    supplied recovery actors and reasons still pass the write gate. Restoring
    historical resource declarations uses a private recovery helper and the
    resulting state is inspected in the recovery transaction; the ordinary
    declaration API cannot use that exemption.
11. Standalone compatibility writes own an IMMEDIATE transaction; helpers
    inside caller transactions use a savepoint and obtain the database write
    lock before rechecking maintenance holds. Failure rolls back both the
    semantic write and its audit. Legacy exports also serialize with the
    native workspace publication lock.
12. Redaction lock proofs are workspace-bound. Public apply, batch and preview
    entries refuse if the live database belongs to another checkpoint
    directory; callers cannot serialize a foreign store with unrelated locks.

## Verification

Use synthetic values assembled at runtime. Verify pending publication and
commit refusal, publication failure/restart, active-reader cleanup failure
and resume, absence of removed bytes in database and WAL, metadata coverage,
same-field batch offsets, stale/overlapping batch rollback, concurrent replay,
one publication epoch, preservation of unaffected state and recovery
precedence. Evidence and diagnostics never contain matched values.

## Controlled identity rekeying (owner admitted 2026-10-03)

The owner selected controlled atomic rekeying after the initial safe refusal.
Identity replacement is a whole-key rewrite to `redacted-` followed by the
first 32 lowercase hexadecimal characters of a domain-separated SHA-256 over
the identity family and old value. This bounded opaque name fits namespace,
label and resource constraints. It is not caller selectable. All collisions
refuse and roll back; rows are never merged, dropped or overwritten.

Supported families and explicit relational scope:

- A label or data namespace changes its selected issue-scoped row.
- A reference namespace changes all ordinary references and uniqueness
  bindings in that namespace on the selected issue.
- An ordinary reference key changes every identical key on the selected issue,
  across its reference namespaces.
- A uniqueness-binding key changes every identical binding key on the selected
  issue and each associated `unique-ref` value mirror. Selecting a mirror's
  value uses the same operation.
- A resource key changes every declaration of that key and its active lock,
  preserving contention and lease/fencing metadata across issues.
- A saved view's unique name changes the selected view, not its query.

Every affected issue advances once in the enclosing redaction transaction.
An additive `key_rewrite_v1` receipt/tombstone member records only the family,
scope digests, old-value digest and new opaque name. Recovery checks these
durable digests and refuses resurrection of any affected identity alias even
after the original row selector or scanner version changes. These records
survive checkpoint round trips through the existing additive-field contract.

Batch preflight refuses coupled selections whose row identity/value another
selected rekey would change; choose one of their fingerprints for the coupled
operation. Independent key and prose selections remain all-or-none. Dry-run
uses a rolled-back transaction and reports the actual replacement identity.
Arbitrary prose, saved-query text and external systems are not relational
references and are not rewritten implicitly.

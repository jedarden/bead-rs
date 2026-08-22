# Native workspace-local resource locks v1

R031 adds resource declarations and atomic claim exclusion to the native
bead-rs store. This specification is independent of every interchange profile.

## Scope and vocabulary

An issue may declare zero or more `resource_keys`. A key names a local resource
that must not be used concurrently by two claimed issues. Keys and locks belong
to one native SQLite store (one `.beads` workspace). This is workspace-local
scheduling exclusion, never distributed locking: two stores can claim the same
textual key without observing one another, and bead-rs does not provide a
network, host-wide, or cross-workspace lock service.

`issue_resource_keys` is durable issue metadata. `resource_locks` is live
derived state and is not portable ownership. A lock is held only by an issue in
`in_progress` with an assignee; an unassigned, open, deferred, or closed issue
holds no resource lock.

## Key validation and canonical form

The canonical form of a key is the original UTF-8 string after trimming Unicode
whitespace at both ends. Keys are case-sensitive. A canonical key MUST be
non-empty, no more than 255 UTF-8 bytes, and contain no Unicode control
character. Path separators and punctuation are permitted because paths,
sockets, and other local names are valid resource names.

Declarations are sorted lexically by canonical key. A declaration containing a
duplicate after normalization is invalid. Validation happens before any row is
changed, including when a checkpoint is imported.

The native issue JSON projection uses an additive `resource_keys` array. An
omitted array means no declaration; when present it contains the sorted,
canonical strings. Unknown fields continue to round-trip normally.

## Acquisition and lifecycle

Claim starts an immediate SQLite write transaction. It releases locks whose
recorded lease fencing token has expired, evaluates the ready frontier, and
selects the highest-ranked candidate whose complete declared key set has no
effective lock held by another issue. Selection, transition to `in_progress`,
lease creation (when requested), acquisition of every declared key, and the
claim event commit or roll back together. Keys are inserted in canonical order.
Consequently a partial key set is never visible after a failed claim, and a
concurrent claim in this workspace cannot acquire an overlapping key.

The same transaction releases all locks when an issue is released or closed.
An update that changes an issue into or out of `in_progress` reconciles its
locks atomically; adding a key to already claimed work fails with a conflict if
another issue owns it. Renewing a lease updates the lock's fencing token.
When the lease epoch recorded on a lock expires, the next claim in the store
returns those keys to the ready scheduler. Expiry does not automatically change
the issue's lifecycle status, and no stale worker may renew or mutate the
expired leased issue.

Resource declaration changes use `bead resource add|remove`; adding the same
normalized key twice in one declaration is invalid, while adding an already
declared key through a separate idempotent command leaves the declaration
unchanged. The commands are transactional and audited.

## Readiness and explanations

The reason code `resource_conflict` is emitted by claim decision traces and
`bead why --json` when an otherwise ready issue needs a key effectively held by
another issue in this workspace. `bead list --ready` uses the same effective
lock predicate as claim and therefore does not advertise work that claim will
refuse. An expired leased lock is not an effective conflict even if cleanup has
not yet run.

## Migration

Migration 10 creates `issue_resource_keys` and `resource_locks` with foreign
keys, a unique declaration key per issue, and a unique active owner per key.
Existing issues have empty declarations and therefore require no backfill.
Existing claims have no R031 keys, so migration creates no locks. The migration
is idempotent and leaves all pre-R031 rows unchanged. Checkpoint import validates
and restores declarations, and reconstructs non-leased local locks for imported
`in_progress` issues in the same activation transaction; a conflicting imported
claim rejects the activation rather than creating partial ownership.

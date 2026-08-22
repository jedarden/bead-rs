# Fork identity v1 specification

Status: normative.

Implements plan section 12 R028. This specification defines the explicit
workspace-identity fork and its interaction with the forensic event stream and
R027 reconcile.

## Scope and safety

`bead sync fork --actor ACTOR [--reason REASON]` is the only operation that
creates a fork identity. A clone, divergent history, foreign event, doctor
diagnostic, import, or reconcile MUST NOT infer or perform a fork.

Fork requires a recorded checkpoint generation that covers the current live
event sequence. A workspace with no recorded generation or with uncheckpointed
events is refused without mutation. The actor is nonempty, at most 255 UTF-8
bytes, and contains no control character. A reason, when present, is at most
4096 UTF-8 bytes.

## Identity and event sequences

Let `P` be the parent workspace UUID and `L` the live event sequence at the
fork point. The new identity has this form:

```text
<first-8-characters-of-P>-fork-<L>-<16-random-hex-characters>
```

The random suffix makes repeated forks at one point distinct; the parent
prefix and decimal fork point preserve inspectable provenance. Existing event
wire identities are never rewritten.

The fork summary is the first event owned by the new origin. Its
`origin_event_sequence` is `L + 1`, and every later event for that origin is
contiguous from there. This deliberately preserves the sequence design shipped
by R028. It does not restart the new origin at 1.

The SQLite `events.sequence` column is a local ingestion key, not a wire
identity. SQLite allocates it through the table's AUTOINCREMENT mechanism; it
MUST NOT be assigned from `origin_event_sequence`. Thus a restore or import may
leave the local allocator's high-water mark different from the fork point
without causing identity reuse.

Forensic validation normally requires an origin to start at 1. The only
exception is an identity in the format above whose staged records include a
`fork` provenance receipt with `target_store_uuid` equal to that identity. Its
required start is the encoded `L + 1`. A fork-looking UUID without that receipt,
a receipt targeting another UUID, or any gap after the accepted start fails
closed. This rule is also normative for R027's valid-staged-stream qualifier.

## Atomic database effect

One immediate transaction:

1. records a `fork` provenance receipt from `P` to the new identity, attributed
   to the actor and carrying the pre-fork issue/event/receipt counts;
2. changes the live workspace UUID to the new identity; and
3. appends one `workspace_forked` summary event containing the parent UUID, new
   UUID, receipt ID, and optional reason.

Any failure rolls back all three database effects. The report returns both
UUIDs, the receipt identity and digest, the allocated live summary sequence,
the parent generation, counts, actor, time, and optional reason. The command
then updates the workspace configuration identity and enters the standard
post-commit checkpoint-publication path.

R028's originally published dedicated fork-receipt projection omitted fields
needed to recompute its embedded digest after SQL projection. A verified
checkpoint may retain that exact dedicated-schema shape: the selected
checkpoint root authenticates its bytes, while validation still requires a
lowercase 64-character receipt digest and a structurally valid target fork
identity. Generic provenance receipts, including generic-schema fork receipts,
remain subject to the complete receipt-hash validation.

## Reconcile and merge

A checkpoint advanced from a forked clone remains eligible for R027
remote-advanced classification when all other qualifiers hold. Reconcile
replays the common prefix idempotently: existing events match by wire identity,
existing dependency and label keys are retained, and an existing receipt ID is
retained only when every projected receipt field matches. A reused receipt ID
with different content is an integrity conflict. The checkpoint suffix and the
reconcile summary/receipt are then committed through the normal merge path.

## Conformance scenarios

1. A clean, checkpointed workspace forks to a distinct UUID and records one
   receipt and one summary event; actor and reason validation fail inertly.
2. No checkpoint and a dirty checkpoint are refused without changing UUID,
   receipts, or events.
3. Repeated forks produce distinct identities and each origin starts at its
   encoded fork point plus 1.
4. A local AUTOINCREMENT high-water mark greater than the live maximum does not
   change the wire fork point and does not collide with the summary event.
5. A cloned fork workspace can advance, publish, and reconcile that checkpoint
   into its lagging same-UUID peer; shared labels, dependencies, events, and
   receipts are not duplicated.
6. Removing the matching fork receipt or changing the encoded fork point makes
   forensic validation reject the non-1 origin start.

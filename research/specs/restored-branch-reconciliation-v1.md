# Explicit reconciliation of a restored branch

An operator may merge a named, verified checkpoint whose native origin reused
event sequences after a restore into a new store. Ordinary merge remains strict.
The explicit `sync import-only --merge --source-generation GEN
--reidentify-restored-branch-at N` operation supports this narrow recovery shape:

- Source and target have the same UUID. The source has one native event origin.
- Every source event before N exists identically in the target. At N the source
  has a `checkpoint_restored` event and a matching restore provenance receipt;
  its restored snapshot ends at N-1. The target has a different event at N.
- Following events create new issues or update/label those newly created issues.
  Changes to pre-existing issues, other workspace events, redaction history, or
  attempt outcomes not already present identically in the target are refused.
- The target retains every existing issue and projection verbatim. Only source
  issues created after N are inserted. ID collisions fail unless a prior receipt
  proves this exact immutable source was already merged.

The divergent source events are imported with a deterministic origin derived
from the source UUID and complete restore event. Their new sequence starts at 1.
Each event retains its public content and unknown fields and gains an additive
`restored_branch_origin` object naming its original UUID and sequence. A reserved
extension collision fails. Shared prefix events are never rewritten or duplicated.
Original source receipts are preserved without rewriting their historical fields.
A new merge receipt and summary event record the immutable generation/root hash,
old/new origin, boundary, actor, counts, and identity mapping rule. A repeated
merge of the same immutable source is a no-op, even after later local issue edits.
An extended source generation requires a separate decision and is refused if it
reuses already imported issue IDs.

All target inspection, conflict checks and changes run in one immediate SQLite
transaction, under the checkpoint publication lock. Dry-run executes the same
candidate writes and validations then rolls back, including provenance and
quarantine state. Failures leave native state unchanged. Existing secret scanning,
quarantine and publication rules apply. A successful merge publishes normally.
Source bytes and original native identities are never changed. The verified input
must be a named checkpoint generation; bare JSONL is not a recovery input.
Doctor recognizes only foreign events whose original identity mapping is covered
by this operation's durable merge receipt; unrelated origins remain errors.

Acceptance includes actual restore-generated divergence in monolithic and sharded
checkpoints, local labels/notes preserved, both histories retained, durable
provenance, verified export/restore, idempotent and concurrent retries, dry-run
rollback, wrong boundary and unsupported-edit refusal, and tampered source refusal.

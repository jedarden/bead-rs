# Verified restore v1 specification

Status: normative.

## Scope and safety rule

`bead restore` is the authoritative operator-facing recovery command. Restore
is always explicit. `bead doctor`, including `doctor --repair`, may diagnose a
missing, empty, or unusable live store and print a restore command, but it MUST
NOT initialize a target, select a generation, or perform restore itself.

The command is:

```text
bead restore --source PATH --generation GENERATION --actor ACTOR
             [--allow-non-empty] [--prefix PREFIX] [--format text|json]
```

`--input` is an alias for `--source`; `--force` and `--force-non-empty` are
aliases for `--allow-non-empty`. The long names above are canonical.

The implementation MUST verify the complete source before it creates,
initializes, clears, or activates target state. A failed source verification
leaves a previously absent target workspace absent.

## Source selection

`PATH` is either:

1. a checkpoint-set directory containing `current.json` and optionally
   `previous.json`; or
2. one generation pointer file named explicitly by the operator.

A directory source selects the `current.json` or `previous.json` whose
`generation_id` exactly equals `GENERATION`. The command never chooses a source
by modification time, filename ordering, file presence alone, or an implicit
meaning such as `latest`. If neither retained pointer names the requested
generation, restore fails and reports the retained generation IDs it could
read.

`GENERATION` begins with `gen-` and is copied exactly from a pointer. It names
the generation being authorized; the generation value is never inferred from
the root filename.

Bare monoliths, `forensic.jsonl`, issue-only exports, generation objects without
their pointer, and bare manifests are not verified-restore sources. They remain
eligible only for the lower-level import behavior that explicitly supports
them. A first-class restore requires a pointer plus its hash-verified root.
Current monolithic roots are content-addressed; retained native pointers from
the earlier checkpoint-set-v1 publisher may instead name
`objects/<generation>.jsonl`, and are accepted only when the filename equals
the selected generation and the pointer SHA-256 verifies the bytes.

## Verification

Before target inspection or mutation, restore MUST perform all of these checks:

- pointer JSON parses as schema version 1 and contains the requested generation,
  supported mode, nonempty store UUID, nonnegative snapshot sequence, active
  root path and lowercase SHA-256, and internally consistent issue/event/
  receipt/total counts;
- every pointer or manifest reference is a normalized slash-separated path
  beneath the checkpoint-set base, contains no empty, absolute, `.` or `..`
  component or backslash, traverses no symlink, and resolves inside the base;
- the monolithic root is `objects/<sha256>.jsonl` (or the legacy native
  `objects/<generation>.jsonl` form selected by that exact pointer), and the
  sharded root is `manifests/<sha256>.json`; every root's bytes agree with the
  pointer digest, and every content-addressed filename agrees with that digest;
- a sharded manifest is checkpoint-set-v1/native-v1, agrees with the pointer's
  store UUID, snapshot and counts, carries valid versioned thresholds, and
  lists each object once with the correct semantic role, byte length, record
  count, content-addressed filename, and SHA-256;
- every record parses under its declared role; issues, events, and receipts are
  unique and canonically ordered after staging; each event origin is continuous
  from sequence 1; dependencies are non-dangling and the blocking graph is
  acyclic; and staged counts equal the pointer counts.

The root and, for sharded mode, complete object closure are verified again
after staging. The CLI repeats complete verification after any target
auto-initialization and before activation. Any byte change across those checks
is an integrity failure.

## R029 archaeology boundary

Checkpoint archaeology materializes partial, read-only historical views. Such
artifacts are permanently non-importable. An archaeology artifact carries an
`artifact_kind`, `kind`, or `$schema` containing `archaeology`, and MUST carry
`"importable": false`. Restore and `sync import-only` MUST reject such an
artifact even when its records happen to resemble checkpoint records. An
operator cannot override this refusal with `--allow-non-empty` or another flag.

The refusal happens at source classification, before target initialization.
Only the original verified checkpoint generation from which a view was derived
can be restored.

## Target and activation

The target is the bead-rs workspace selected from the current directory. After
source verification:

- a tracked native `config.json` with a missing/uninitialized database is
  initialized while preserving its recorded identity and prefix;
- a directory with no workspace is initialized using `--prefix` (default
  `bead`);
- an unrecognized `.beads` directory remains protected by the workspace
  discovery rules and is never initialized or written;
- a usable initialized database is inspected for native semantic state.

The target is non-empty when it has any issues, audit events, provenance
receipts, saved views, or recurrence templates. Without
`--allow-non-empty`, a non-empty target is refused without mutation and the
error reports all five counts plus the explicit override remedy.

With `--allow-non-empty`, the command atomically replaces native semantic state
inside one SQLite transaction. It clears native issue children, events,
receipts, saved views, recurrence data, scheduling state, and checkpoint state;
adopts the source store UUID; activates every verified source record; and
creates the restore summary event and receipt. Unknown tables are not dropped,
cleared, interpreted, or rewritten. Failure rolls the transaction back.

The override is consequential but not an unverified-source escape hatch: all
source checks are identical with and without it.

## Attribution, receipt, and output

`ACTOR` is required, nonblank, at most 255 bytes, and contains no control
characters. A successful restore appends one local `checkpoint_restored` audit
event and one immutable `restore` provenance receipt. The receipt records the
actor, source and adopted target UUID, verified active-root SHA-256, restored
issue/event/prior-receipt counts, success result, receipt digest, and summary
event identity. The new receipt itself is not included in the count of prior
receipts restored.

Text and JSON output report exactly:

- selected generation and mode;
- pointer and active-root path plus verified root SHA-256;
- source and adopted target UUID;
- source snapshot sequence and actor;
- issues, source events, and prior provenance receipts restored;
- new restore receipt ID and SHA-256;
- new restore summary event sequence;
- whether the non-empty override was exercised; and
- when exercised, displaced issue/event/receipt/view/template counts.

Automatic checkpoint publication follows the normal post-commit rule. It
publishes the restore summary event and new receipt when enabled, including
when this command had to initialize the target. `--no-auto-flush` and
`checkpoint.auto_flush: false` retain their usual suppression semantics.

## Relationship to `sync import-only`

`bead restore` is the authoritative public disaster-recovery path.

`bead sync import-only` remains public in verified-restore-v1; it does **not**
become internal. It is the lower-level compatibility and reconciliation
primitive for standalone monoliths, external/import profiles, diagnostic dry
runs, restore-into-empty compatibility automation, and merge into a healthy
store. Its `--restore-into-empty` mode continues to require an already
initialized empty target and may accept standalone artifacts that have no
generation pointer. Therefore it MUST NOT be documented as equivalent to a
named verified recovery and MUST NOT be the command doctor recommends.

Both paths share the atomic staging/activation implementation and provenance
schema. The first-class command adds the stricter pointer selection, immutable
closure verification, target auto-initialization, non-empty replacement
override, and exact recovery report defined here.

## Required conformance scenarios

1. **Empty target:** one command verifies and restores a named generation,
   initializes the missing native target if necessary, adopts the source UUID,
   restores every record, and writes an actor-attributed receipt.
2. **Non-empty refused:** without the override, all target counts and visible
   state remain unchanged and no restore receipt is written.
3. **Override:** with `--allow-non-empty`, native state is atomically replaced,
   displaced counts are reported, and an unknown table survives unchanged.
4. **Unverified source refused:** a missing, malformed, miscounted, renamed, or
   hash-mismatched root/object fails before target initialization.
5. **R029 view refused:** an explicitly non-importable archaeology artifact is
   rejected by classification before target initialization.
6. **Doctor remains diagnostic:** doctor prints a command containing the exact
   retained generation when it can read one, exits nonzero for the broken live
   store, and neither initializes nor restores it.
7. **Concurrent empty-target guard:** two simultaneous restore processes aimed
   at one empty initialized target produce exactly one successful activation;
   the loser observes the winner's committed state under the write transaction
   and is refused as non-empty.

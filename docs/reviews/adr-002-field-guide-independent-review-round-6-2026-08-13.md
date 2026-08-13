# ADR-002 native field guide independent review — round 6 (R18 only)

Date: 2026-08-13
Reviewer: Claude (Anthropic). Authored the round-1 through round-5 reviews;
authored neither the original artifact nor any correction, is not the schema
implementation author, and did not author the checkpoint fixes offered as
evidence.
Artifact: `research/specs/native-field-guide-v1.md`, unchanged at commit
`805c7de`
Specification SHA-256:
`8d26bb1297d91e147cb60a230a2f3653bed6b78d4518b5bc02c3d2d07834ad0e`
Implementation under evidence: `1873da2`
Prior round: `145d78c` (accepted with required revisions, same hash)
Tracking bead: `bf-57wtd`

Scope: **R18 only**, per the review request. No other section was re-reviewed.
Every closure recorded in rounds 2 through 5 stands.

Decision: **accepted**. Unconditional. R18 is resolved, and it was the only open
item against this artifact.

## Independence and provenance

Method as in prior rounds: verified against `src/`, `tests/`, and empirical runs
of a build of `1873da2` in disposable workspaces. No source, tests, fixtures,
SQL, or internal documentation from any other bead implementation was inspected.
No clean-room contamination was found.

Repository state at review: working tree clean; `HEAD` = `origin/main` =
`1873da2`; Forgejo divergence `0 0`. `cargo fmt --check` clean.
`cargo clippy --all-targets -- -D warnings` clean. `cargo test` exit 0 with
**635 passed, 0 failed, 0 ignored across 36 suites**; no `#[ignore]` anywhere in
`src/` or `tests/`. All three conformance tests pass.

## R18 — repeated merges import only the new suffix: closed

The artifact's claim was fixed in the implementation rather than weakened in the
text, which is the option that keeps section 6's identity and contiguity
statements true. `1873da2` replaced the inert `INSERT OR IGNORE` in
`import_events` with an explicit existence check on
`(origin_store_uuid, origin_event_sequence)` that skips an already-present
identity before inserting. That is the correct shape: it does not depend on a
constraint the schema does not have, and it needs no dedupe migration for
workspaces that already merged twice.

Reproduced on a build of `1873da2` — the same probe that produced the round-five
finding, one source merged three times into one target as it gained one event
per round:

```text
after merge 1:  origin events in target = 1     seq 1 x1
after merge 2:  origin events in target = 2     seq 1 x1, seq 2 x1
after merge 3:  origin events in target = 3     seq 1 x1, seq 2 x1, seq 3 x1
                (source has 3 events)
```

Round five measured 1 / 3 / 6 for the same sequence. Scalar content still
advances across every merge (`work 1` -> `work 2` -> `work 3`), so suffix-only
import did not come at the cost of the merge itself.

Origin identity is now genuinely unique in a merge target: 3 rows carrying an
origin identity for 3 distinct identities, and the partial unique index that
round five could not create on the duplicated data —
`CREATE UNIQUE INDEX ... ON events (origin_store_uuid, origin_event_sequence)
WHERE origin_store_uuid IS NOT NULL` — now creates and drops cleanly. The
distinct origin sequences in the target are `1,2,3`, so section 6's
"monotonically contiguous integer within the origin store" holds after repeated
merges, as does ordering by `(origin_store_uuid, origin_event_sequence)` as an
identity.

The consumer-visible symptom is gone. `bead changes --since 0 --json` on that
target now reports `total_available: 6` — three imported origin events and three
local checkpoint events, `has_gaps: false`, no duplicates. Round five reported
`9` on the same workspace.

`1873da2` also adds the count-equality assertion this needed
(`tests/checkpoint_round_trip_conformance.rs:1268-1288`): rows carrying an
origin identity must equal distinct origin identities, with the message
"repeated merge duplicated an identical event-history prefix". It is committed,
active, and passing, so the regression is guarded.

## Regression checks

The fix touches the event import path, so the round-five closures were
re-verified on the same build. All hold:

| Behavior | Result |
|---|---|
| Content-conflict rejection | `Event identity conflict: (..., 1) has different content`, rc=1, target title and event count unchanged |
| R15 forensic issue validation | Both invariant directions rejected, 0 issues written, on `--restore-into-empty` **and** `--merge` |
| R17 explicit-empty deletion propagation | Target 1/1/1 -> 0/0/0 after the source deleted all three and reflushed |
| Legacy omission preservation | Target held 1/1/1 through a checkpoint omitting all three, with scalar content advancing to `notes = "gen2"` |

## Decision

**Accepted**, unconditionally, against
`8d26bb1297d91e147cb60a230a2f3653bed6b78d4518b5bc02c3d2d07834ad0e` at commit
`805c7de`. There are no carve-outs and no tracked revisions. Section 10's
condition is met: a reviewer who authored neither the artifact nor any
correction has recorded an acceptance decision against the file's exact SHA-256.
Schema implementation may proceed against the whole artifact.

Two notes for whoever acts on this.

**Updating the status header changes the hash.** Section 10 says the header
becomes `accepted normative specification` only after unconditional acceptance,
but this acceptance is bound to the hash of the file as it stands, which still
reads `corrected proposed normative specification; awaiting independent
re-review`. Editing the header produces a different digest. Record that edit in
`PROVENANCE.md` as a header-only change citing this acceptance and giving the new
digest; it does not require another review round. Do not silently supersede the
accepted hash.

**`bf-57wtd` is not complete.** The bead covers implementing `schema
list|show|explain` from one typed source and updating capabilities, not only
obtaining approval. `bead schema` is still an unrecognized subcommand on
`1873da2`. This acceptance discharges the approval half and unblocks the
implementation half; the bead stays open. Its three recorded blockers are the
defects these reviews surfaced — `bf-1cwrc` (merge deleting live collections),
`bf-3siqo` (generic update to closed), and `bf-12iqb` (forensic import never
validating records) — and all three are fixed and verified on `main` at
`0375fdc`, `2ce61ce` and `92701b7` respectively. They should be closed by their
owners so `bf-57wtd` leaves `blocked`.

## Review history

| Round | Commit | Hash | Outcome |
|---|---|---|---|
| 1 | `f4a31c8` | `32dea941...` | rejected as written (`0009b32`) |
| 2 | `5c45293` | `3a5a5228...` | accepted with required revisions R1-R11 (`febf041`) |
| 3 | `e5141b9` | `6b7c6da0...` | accepted with required revisions R12-R14 (`78a3d32`) |
| 4 | `9953b66` | `819fd3c1...` | accepted with required revisions R15-R17 (`770a2d5`) |
| 5 | `805c7de` | `8d26bb12...` | accepted with required revisions R18 (`145d78c`) |
| 6 | `805c7de` | `8d26bb12...` | **accepted** (this document) |

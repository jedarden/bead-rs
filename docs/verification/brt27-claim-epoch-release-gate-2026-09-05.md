# BR-T27 claim-epoch release gate — executed 2026-09-05

Status: **GATE OPEN — release withheld.** The sub-gates that can be run at a
committed source were run and are green; the sub-gates that require the
claim-epoch capability to exist at a commit cannot run, because no commit
carries it. Nothing in this dispatch published a release or closed the
claim-epoch transition.

Validation bead: `beadrs-41b9130e` (BR-T27), dispatch
`claude-code-glm-5.3-flash-glm-roam-18`, 2026-09-05 08:47–09:30Z. Source
commit of record at dispatch start: `a4f8fbe0534d26f69ff31324760cd557450beed8`.
Three planner commits landed mid-dispatch (`aebbdd4`, `b86e0fa`, `82f5d73` =
plan revision 14 + bead-graph publication), moving HEAD to `82f5d732`; none of
them carries implementation source.

## 1. Acceptance items versus what exists

`plan.md` revision 14 states BR-T27's acceptance as: *"source/binary hashes,
archive-build proof, restore rehearsal, NEEDLE canary and rollback receipt
agree"*, and its status as *"blocked by BR-T23–BR-T26"*.

| Acceptance item | State at this dispatch |
|---|---|
| Source/binary hashes | **Partial.** Hashes recorded below for the artifact that does exist (`a4f8fbe`, no claim epoch). No claim-epoch artifact can be hashed because no claim-epoch commit exists. |
| Archive-build proof | **Green.** `scripts/build-from-archive.sh a4f8fbe` produced a pinned binary from a git-archive extraction; shared-checkout index/HEAD/worktree untouched. §5. |
| Restore rehearsal | **Green.** Full checkpoint→empty-workspace restore on the built artifact, semantic identities equal, receipt recorded. §6. |
| NEEDLE canary | **Green** for the old/new consumer matrix on the committed pins (21 tests). §4. |
| Rollback receipt | **Green.** Restore receipt `restore-18d26265a87ca53e`. §6. |
| Capability snapshots agree | **Open.** No committed artifact advertises a claim-epoch capability at all. §3. |
| Durable issuance, credential-guarded mutations, reason-bearing override, compare-and-reap, stale-worker rejection, concurrency fixtures, schemas, help, ADR | **Open.** The first six exist only as uncommitted working-tree changes; ADR-017 does not exist. §2–§3. |

## 2. There is no exact source commit for the capability

The gate's own precondition — everything agreeing *"at one exact source
commit"* — is unmet at the source level, not merely at the test level:

```console
$ git log -S mint_claim_epoch --all --oneline | wc -l
0
$ git rev-parse HEAD
82f5d73279919c5d8568fa0b7ef0bc91b14e34ee
```

`mint_claim_epoch` — the durable-issuance primitive every later transition
bead builds on — appears in **no commit reachable from any ref**. The
implementation exists only as uncommitted changes in the shared checkout:

- 48 tracked files modified, +1702 / −622 (measured against `a4f8fbe`,
  2026-09-05 ~09:15Z): `src/service/claim.rs`, `src/service/leases.rs`,
  `src/model/attempt.rs`, `src/store/migrations.rs`, `src/cli.rs`,
  `src/service/{lifecycle,issues,watchdog,checkpoint,attempt,query,doctor,resource_locks,manifest,mod}.rs`,
  `src/profile/{native_v1,needle_v1}.rs`, and 30+ test files.
- 16 untracked paths, including the transition's own new tests
  (`tests/concurrent_replay_fencing.rs`, `tests/resolve_attempt_e2e.rs`) and
  five `src/scan/*` modules.
- Newest non-`.beads` mtime in that set: **2026-09-04 06:16Z** — the tree was
  already >20 h stale at dispatch start, with no cargo/rustc/editor activity
  and no assignee on any implementing bead. This is orphaned work from a dead
  dispatch, not a live co-author's.

Running release conformance against that tree would measure an uncommitted,
unowned snapshot rather than a source commit, and committing it here would
close BR-T23–T26 by side effect without their own evidence. Neither was done.

## 3. Required beads are open, and the contract is unadvertised

All eight prerequisite beads are `open` and unassigned (revisions as read
2026-09-05 ~09:00Z):

| Plan item | Beads | Revision |
|---|---|---|
| BR-T23 durable claim-epoch issuance | `beadrs-bd985270`, umbrella `beadrs-8c343a7c` | 9, 23 |
| BR-T24 credential-guarded mutations + reason-bearing override | `beadrs-9d740f26`, `beadrs-dc8df464` | 1, 1 |
| BR-T25 compare-and-reap + stale-worker fencing | `beadrs-3be4bf40`, `beadrs-0d0cb036` | 2, 3 |
| BR-T26 ADR-017, spec, schemas, help, capability + concurrency fixtures | `beadrs-eec200d1`, `beadrs-24a3a27b` | 1, 1 |

ADR-017 — the contract freeze BR-T26 requires — is absent from `docs/adr/`,
which ends at `016-observational-workspace-probes.md`.

Capability snapshots, probed live from the binaries:

```console
$ <pin> capabilities | grep -ci epoch
bead-pre-feature                    → 0
bead-attempt-resolution-e115609     → 0
bead-attempt-resolution-f25ab5c     → 0
brt27-rollback-baseline-a4f8fbe     → 2   (redaction_epoch only)
```

The two `epoch` matches in the `a4f8fbe` build are
`"document_kind": "redaction_epoch"` and
`urn:bead-rs:schema:redaction-epoch:native-v1` — historical redaction, not
claim fencing. That is the whole capability delta between the 2026-09-02 pin
and `a4f8fbe`. **No committed artifact advertises any claim-epoch capability,
and no schema, help text, or concurrency fixture for the contract exists at a
commit.**

## 4. NEEDLE consumer canary — green on the committed pins

`cargo test --test needle_variant_dispatch_paths --test needle_variant_paths
-- --test-threads=1`, `TMPDIR=/run/user/1000/brt27-tmp`, exit 0, local
cgroup-limited build. Both suites byte-verify each pin's sha256 against
`pinned-binaries/commits.json` before driving it, so this measured committed
artifacts, never the dirty tree.

| Suite | Result |
|---|---|
| `tests/needle_variant_dispatch_paths.rs` | 11 passed, 0 failed, 9.19 s |
| `tests/needle_variant_paths.rs` | 10 passed, 0 failed, 6.41 s |

Pins driven: `bead-pre-feature`
(`bead 0.2.4 (af023ad 2026-09-01T19:14:12Z)`, sha256 `7e0e73de…`),
`bead-attempt-resolution-e115609` (`68fe8d53…`),
`bead-attempt-resolution-f25ab5c` (`9a8455f2…`, the hash recorded in
`BUILD_PROCEDURE.md`).

The duplicate-worker replay is covered twice over:
`atomic_claim_is_exclusive_under_parallel_invocation_on_both_variants`
(parallel claimant processes on a one-bead queue → exactly one winner, the
rest clean `bead_id: null` at exit 0) and
`concurrent_claims_receive_distinct_beads_on_both_variants`, plus
`stale_revision_fencing_rejects_cleanly_on_both_variants`. The old/new matrix
result is unchanged from the 2026-09-03 validation
([needle-variant-dispatch-validation.md](needle-variant-dispatch-validation.md)).

`tests/needle_v1_compatibility.rs` was deliberately **not** counted as canary
evidence: it links the working tree, so it would have measured the uncommitted
claim-epoch WIP.

## 5. Archive-build proof at the exact commit

```console
$ scripts/build-from-archive.sh a4f8fbe0534d26f69ff31324760cd557450beed8 \
    --name brt27-rollback-baseline-a4f8fbe \
    --out /home/coding/scratch/brt27-artifacts
build-from-archive.sh: pinned /home/coding/scratch/brt27-artifacts/brt27-rollback-baseline-a4f8fbe
build-from-archive.sh: sha256 5d610b318a7fd9b0ccf1d7abd57ca0446b591570efb4c5bb81695514262323f9
```

| Field | Value |
|---|---|
| Source | `git archive a4f8fbe` extraction (index, HEAD, reflog, stash, worktree untouched) |
| Binary sha256 | `5d610b318a7fd9b0ccf1d7abd57ca0446b591570efb4c5bb81695514262323f9` |
| Size / toolchain | 9.4 M / rustc 1.97.1 |
| Build command | `cargo build --release --locked` (default features) |
| Version string | `bead 0.2.6 (unknown 2026-09-05T09:16:56Z)` — `unknown` is the documented archive-build value; the source commit is the one passed to the script |

This is a **rollback-baseline artifact, not a release candidate**: it is
byte-honest about `a4f8fbe`, which contains no claim-epoch code. It was
written outside the repository and is disposable; the identity above is the
durable record. It was deliberately *not* added to `pinned-binaries/`, whose
settled four-pin registry would otherwise grow a pin that advertises nothing
this transition needs.

## 6. Restore rehearsal and rollback receipt

Driven entirely by the `a4f8fbe` artifact under a clean
`TMPDIR=/run/user/1000/brt27-tmp`:

```console
origin:    bead init --prefix r27; 3 × bead create; bead sync flush-only
           → generation gen-c468c62bd1e2db33c7926b5c498f27c0, covered sequence 3
restored:  bead init --prefix r27
           bead sync import-only --input …/forensic.jsonl --restore-into-empty \
             --actor brt27-canary
           → Restored 3 issues, 3 events
           → Receipt restore-18d26265a87ca53e
           → receipt hash c84e7025df1994afa97be8d3e08184ed1f895811e2658c795a7b19ffa1800a11
           → 3 inserted, 0 updated, 0 retained, 0 conflicted
```

Post-restore `bead list --json` is identical to the origin workspace on
`(id, status, title)` for all three issues. The rollback path works at this
commit.

## 7. What has to happen before BR-T27 can close

1. BR-T23 → BR-T26 close in order on real commits: `beadrs-bd985270` /
   `beadrs-8c343a7c`, then `beadrs-9d740f26` + `beadrs-dc8df464`, then
   `beadrs-3be4bf40` + `beadrs-0d0cb036`, then `beadrs-eec200d1` +
   `beadrs-24a3a27b` — which requires the orphaned working-tree changes to be
   resumed, finished, and committed by whoever takes those beads.
2. ADR-017 accepted and `docs/adr/017-*` committed.
3. A capability snapshot that actually advertises the claim-epoch contract,
   emitted by an artifact built from that commit.
4. Only then: re-run §4–§6 against a pin built from that exact commit, add the
   claim-epoch consumer cases to the canary, and close BR-T27.

## 8. Graph repair performed by this dispatch

`beadrs-41b9130e` was claimed out of graph order — it became visible on the
ready frontier between `create` and dependency insertion, the exact race
`beadrs-57c668be` (BR-T28) records, and its blocker set was missing the BR-T24
and BR-T26 edges entirely. This dispatch added them, so the frontier now
matches `plan.md`'s "blocked by BR-T23–BR-T26":

```console
$ bead dep add beadrs-41b9130e beadrs-9d740f26
$ bead dep add beadrs-41b9130e beadrs-dc8df464
$ bead dep add beadrs-41b9130e beadrs-eec200d1
$ bead dep add beadrs-41b9130e beadrs-24a3a27b
```

Pre-existing blockers `beadrs-8c343a7c`, `beadrs-3be4bf40`, `beadrs-0d0cb036`
were left as found. Note for planners: the four edges above were added by
separate `dep add` calls, not `bead manifest` — per the new *Planning Safety*
section in `AGENTS.md`, a graph shape like this one should be materialized
atomically once the manifest path is the default.

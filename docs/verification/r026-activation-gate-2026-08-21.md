# R026 activation gate evidence

Status: **PASS — automatic publication remains activated**.

The activation tree is `49e2b4bb9dc177c00ec5c79913044c2268065aca`. The
compiled-default/documentation flip is present in its activation ancestor
`ed4fb3dc983943916494e669cfde67bf338d18b0`, and the R026 implementation
prerequisites are retained in that ancestry. This report records the section
13 gate against the activation tree.

The host has a shared foreign `.beads` directory under its normal temporary
directory. Integration tests were run in a temporary mount with `TMPDIR=/tmp`
so their disposable workspaces could exercise normal fail-closed discovery
without touching that shared tracker.

## Criterion results

### Bounded objects and identical-content reuse — PASS

Fresh CLI workspaces used the compiled default with no `checkpoint.auto_flush`
override. Each `bead create` was one mutate-and-publish cycle.

| cycles | retained JSONL objects | checkpoint bytes | live workspace bytes | checkpoint/live |
| ---: | ---: | ---: | ---: | ---: |
| 100 | 2 | 232,589 | 331,776 | 0.701× |
| 1,000 | 2 | 2,326,849 | 897,024 | 2.594× |

Both runs ended with `live_sequence == covered_sequence`, `dirty: false`, an
empty `unresolved_tombstones` array, and `ready_to_commit: true`. The retained
object count stayed at two — the current and previous generation roots — and
did not grow with publication count. The two-size run records a constant
factor of 2.594× for this workload and host.

`cargo test --test f017_forensic test_f017_identical_flushes_reuse_one_object
-- --exact --test-threads=1` passed. It removes the current pointer between
two otherwise identical publications and verifies that two generations point
to exactly one content-addressed object.

### Applied tombstones — PASS

`checkpoint_tombstones` passed all 3 tests, including applied cleanup,
stray-object reclamation, and unresolved-tombstone status/reapplication.
`checkpoint_mode_selection` passed all 9 tests, including mode-transition
tombstones. The 1,000-cycle status above reported no unresolved tombstones;
the pointer did not declare `current.json` deleted.

### Sound dirtiness — PASS

The command-tree/event-contract suite passed 4 tests: exhaustive leaf
classification, section-5 classification, every mutating command advancing
the live sequence, and read-only non-advancement. The post-commit publication
suite passed 13 tests, including one covering generation for every mutating
command and the clean-checkpoint skip.

### Incremental cost and rapid-fire capacity — PASS under the recorded contract

The sharded incremental probe forced `mode: sharded`, built 30- and 300-issue
workspaces with publication suppressed only during setup, then performed one
plain automatic mutation:

| issues | object corpus before | object corpus after | new objects | new object bytes |
| ---: | ---: | ---: | ---: | ---: |
| 30 | 23,403 | 24,502 | one issue shard + one event tail | 1,099 |
| 300 | 235,413 | 244,090 | one issue shard + one event tail | 8,677 |

The object corpora are 9.96× apart, while the changed data objects remain
delta-sized. Each publication also writes one immutable manifest; the full
new-path counts were 3 and the full new-path byte counts were 11,490 and
54,641 respectively. The mode-selection suite passed all 9 tests, including
threshold selection, partition retention, mode transition, and monolithic
contrast.

The optimized section-3.5.10 smoke matrix ran 40 reports: 100 and 1,000 beads
across all five dataset families and all four workloads. All 40 processes
completed; the reports contained 0 busy failures and 0 claim conflicts, with
65,687 successful claims. Thirty-four lanes were `completed`. Six lanes were
explicitly reported as `resource_limited` by the harness (the 100/1,000
chains-mixed and diamonds-mixed lanes, plus the 1,000 wide-DAG claim-close and
mixed lanes); rerunning those six with a 10-second interval preserved the
structured `resource_limited` result and still produced 0 busy/conflict
failures. This is the resource-limited outcome required by section 3.5.10,
not an omitted scale or an unreported failure.

### Concurrency — PASS

`checkpoint_publication_lock` passed all 3 tests: eight bounded concurrent
workers left an intact covering generation, a mutation committed while the
publication lock was held, and a lost publication race exited successfully.
The test assertions cover torn pointers, partial tombstone sets, lost
mutations, and quiescent sequence coverage.

### Split-failure semantics — PASS

`post_commit_publication` passed the forced-publication-failure test. The
mutation remained visible, the failure named the post-commit split on stderr,
and the process returned exit status 1.

### Escape hatches — PASS

The same publication suite passed the one-shot `--no-auto-flush` hatch, the
durable `checkpoint.auto_flush` hatch, flag precedence, dirty status, invalid
configuration fail-closed behavior, and idempotent `sync flush-only` checks.

### Recovery equivalence — PASS

The 1,000-cycle workspace was built entirely from automatic publication with
no explicit flush. `bead doctor --rehearse` returned 0 and reported:

```text
Original: 1000 issues, 1000 events
Diagnostics: 9 checks, 0 errors, 0 warnings
Semantic comparison: EQUIVALENT
Recovery rehearsal completed successfully
```

The recovery and restore suites passed 5 rehearsal tests, 5 clone/recovery
tests, and 3 checkpoint round-trip tests. The sharded/monolithic restore
equivalence test also passed.

### Capability handshake and documentation reversal — PASS

`cli_capabilities` passed all 9 tests, including `auto_flush: true` for both
native-v1 and needle-v1, advertisement independence from both hatches, and
agreement between the handshake and plain mutation behavior. The committed
surfaces checked in the activation tree are README, root/subcommand help,
generated man pages, plan text, ADR-003, and `AGENTS.md`; they describe
automatic publication, both suppression hatches, and `sync flush-only` as an
idempotent check.

The final wording sweep reports exactly two intentional historical matches,
both outside the user-facing contract: ADR-003 decision history and ADR-009's
pull-before-flush ordering rule. The release-note inventory is phrased without
reproducing those legacy strings, so it no longer makes the sweep self-match.

## Reproduction commands

The behavioral gate run was:

```text
bwrap --ro-bind / / --bind /home/coding/bead-rs /home/coding/bead-rs
  --bind /home/coding/target /home/coding/target --tmpfs /tmp
  --chmod 1777 /tmp --dev /dev --proc /proc env TMPDIR=/tmp
  cargo test --test post_commit_publication
    --test mutating_command_event_contract
    --test checkpoint_publication_lock --test checkpoint_tombstones
    --test checkpoint_mode_selection --test cli_capabilities
    --test r015_recovery_rehearsal --test checkpoint_round_trip_conformance
    --test clone_recovery -- --test-threads=1
```

Result: **54 passed, 0 failed**, plus the exact identical-content test above:
**55 passed, 0 failed** for the R026 evidence run.

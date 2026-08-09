# F017 independent review

Date: 2026-08-09
Reviewer: OpenAI Codex, operating independently from the Marathon/Claude
implementation iteration
Specification SHA-256:
`91f73abf3c09f141b2c36529979ee1dcf27cec5091cf340d798b3a7fa29f234c`
Decision: specification accepted as the implementation baseline; implementation
activation rejected pending conformance work

## Independence and provenance

The reviewer did not author either `research/specs/checkpoint-set-v1.md` or the
F017 implementation and did not inspect source, tests, fixtures, SQL, or internal
documentation from bead-forge, beads_rust, or another bead implementation. The
review used only this repository. No evidence of upstream contamination was
found.

The earlier implementation-before-review sequence was a missed project gate. It
was not evidence of clean-room contamination. This review satisfies the missing
independent-review dependency; it does not retroactively claim that incomplete
code conforms.

## Specification decision

The specification is an acceptable independent native design baseline. It
defines record families, canonical ordering, pointer authority, content
addressing, validation, restore and merge semantics, equivalence, and required
conformance scenarios sufficiently to continue implementation. Ambiguities and
example inconsistencies may be clarified by later, versioned specification
changes, with corresponding tests. They do not require another external owner
before implementation resumes.

## Implementation conformance findings

F017 remains `passes: false`. The following are implementation work, not
governance blockers:

1. `publish_forensic_checkpoint` and its helpers are dead code and have no CLI
   call site. `bead sync --flush-only` still always publishes the pre-F017
   issue-only checkpoint.
2. `SyncImportOptions` exposes neither `--restore-into-empty` nor `--merge`, and
   `import_checkpoint` only parses the pre-F017 issue-per-line format. Forensic
   validation, event replay, UUID handling, merge conflict handling, and durable
   restore/merge receipts are absent.
3. The sharded publisher uses stable logical filenames such as
   `issue-<prefix>.jsonl`, rather than immutable content-addressed object paths.
   Existing objects can therefore be replaced.
4. Issue shards always use one hexadecimal prefix and do not split adaptively at
   count or byte thresholds. Event shards enforce only the count threshold.
5. Pointer metadata reports empty added/replaced/deleted path sets. Tombstone
   calculation and cleanup are not implemented.
6. Publication flushes buffered writers but does not sync object files and
   parent directories before pointer replacement. `previous.json` is copied,
   not atomically installed, and database/file authority can diverge after a
   crash.
7. No integration tests invoke the forensic publisher or cover the ten required
   conformance scenarios. Existing green tests therefore do not establish F017.
8. Final capability reporting and post-F017 doctor/status behavior must be wired
   to the activated format and verified.

## Required disposition

Marathon should resume implementation immediately. It must address the findings
above with tests, then run formatting, Clippy, the full test suite, and the F017
conformance scenarios before changing F017 to `passes: true`. Further governance
status documents do not advance this review.

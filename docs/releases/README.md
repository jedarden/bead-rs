# Secret-scrubbing release promotion

Release owner: `beadrs-b3059276`. Forgejo `origin` is authoritative; GitHub
receives its tags through the configured server-side mirror. Only the
`bead-rs-ci` Argo WorkflowTemplate publishes release assets. A normal sensor
run is candidate-only (`publish-release=false`); a Cargo version bump alone
must not create a tag, publish a draft, or deploy a fleet artifact.

The template verifies the exact requested revision, Rust 1.85/default and
managed-policy gates, and builds checksummed Linux artifacts for both
profiles. Default assets remain `bead-<target>`; fleet assets are explicitly
`bead-managed-<target>`. `provenance.json` records the source revision, version,
builder/toolchain identity and profile inventory. Optional Darwin builds are
included only when that invocation actually succeeds, never from stale cache.
Since the controller has no default artifact repository, candidates are
retained in the existing private `bead-rs-ci-cargo-cache` registry under
`<version>-candidate.<full-source-sha>` (not a Cargo cache key). Tags are not
overwritten; the workflow's `candidate-digest` output identifies the immutable
payload for retrieval with `crane export <repository>@<digest> -`. It contains
only release assets, never semantic bead stores or registry credentials.

Promotion additionally requires a committed
`docs/releases/v<VERSION>-secret-scrubbing.json` accepted by
`python3 scripts/verify-secret-release.py <VERSION>`. Do not invent or mark
passing evidence to satisfy this guard. The record has schema version 1,
version, owning bead, source-tree SHA-256, complete independent exact-hash
review, hashed evidence for BR-T35/BR-T44, managed write boundaries, redaction
conformance and fleet Git-layer parity, and complete lab/codinghome replay
counts, zero unresolved findings, advisory-volume and hostile-field metrics.
Review identity must differ from the implementation/specification authors.
Evidence paths must be repository documentation with exact SHA-256 hashes.
The mandatory `candidate` object binds the exact tested `source_commit`,
private `payload_ref` (`ronaldraygun/bead-rs-ci-cargo-cache@sha256:...`), and
`checksums` evidence (`path`, `sha256`) to the approval. Commit the candidate's
actual complete checksum table under `docs/releases/`; it must include both
Linux targets in both profiles, installer and provenance. Replay reports must
identify these exact artifact hashes, not a rebuilt or older managed pin.

The source identity is SHA-256 of `git ls-tree -r --full-tree HEAD` after
excluding `.beads/checkpoint/` and `docs/releases/` entries. It includes the
source, tests, specifications, review records, scripts, pins and build inputs;
task updates and the approval commit cannot invalidate otherwise identical
tested inputs. An approval for a different tree or specification is refused.
The guard checks receipt integrity, not the truth of a review; the release
owner must independently verify the cited evidence before committing approval.

After approval, submit the existing template at the full approval revision
with `publish-release=true`. Promotion verifies that the approved candidate is
an ancestor with the identical source-tree identity, restores its immutable
payload through checksum-validated safe staging, and reruns the native smoke.
It does not rebuild: wall-clock/build-commit changes cannot replace the fleet-
tested bytes. The version tag names the tested candidate source commit, not
the later evidence-only commit. Publication refuses a version tag already bound
to another source rather than checking out an untested old tag. The tag is
pushed only to `origin`; GitHub must already have the mirrored tag before
`gh release create --verify-tag`. Existing drafts/assets must be verified,
not published or overwritten blindly. Preserve all independent review and
fleet remediation holds.

Deployment is a separate completion gate: download the published managed
artifact, check its entry in the release manifest and provenance, retain a
recoverable backup, atomically replace the resolved host executable, and
verify its digest/version/capabilities on each host. Exercise incoming-secret
rejection and fingerprint-selected atomic scrub/recovery in isolated harmless
stores. Do not claim that testing a pin or building a candidate means either
host is running the versioned release, and do not alter real credentials or
stores without the owning remediation bead and its admitted scope.

Run `python3 scripts/smoke-secret-release.py /absolute/path/to/bead` on each
installed managed executable (use `--scratch-root /tmp` in an isolated CI
container). It creates and removes only its own private disposable workspace;
the historical provider-shaped fixture is invented and assembled at runtime,
and never enters command arguments or output. The smoke checks rejection and
rollback, fingerprint-selected text/key batches, one revision per affected
bead, preserved key bindings, a real reader-held cleanup interruption, write
fencing and receipt resume, replay idempotence, sanitized checkpoint freshness,
and absence of fixture bytes from database/WAL/checkpoint files. Its JSON report
binds the result to the tested binary hash. Organization scanning, real-store
replay, full conformance, and independent acceptance remain separate gates.

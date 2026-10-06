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

The source identity is SHA-256 of `git ls-tree -r --full-tree HEAD` after
excluding `.beads/checkpoint/` and `docs/releases/` entries. It includes the
source, tests, specifications, review records, scripts, pins and build inputs;
task updates and the approval commit cannot invalidate otherwise identical
tested inputs. An approval for a different tree or specification is refused.
The guard checks receipt integrity, not the truth of a review; the release
owner must independently verify the cited evidence before committing approval.

After approval, submit the existing template at the approved full revision
with `publish-release=true`. Publication refuses a version tag already bound
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

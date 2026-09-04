# BR-T18 ruleset-v3 remediation evidence

Date: 2026-09-04

This addendum records the Garage credential-identifier remediation performed
after the first BR-T18 pass. It contains identifiers, hashes, counts, and
outcomes only. No credential value was printed, placed in argv, or copied into
this report.

## Exact bead-rs artifact

- Source commit: `bcc91edc2d4efb67512c4fe125b4a84560b97621`
- Change: ruleset v3 adds the blocking
  `garage-access-key-id-assignment` detector. It requires both the Garage key-ID
  shape and an explicit `AWS_ACCESS_KEY_ID` assignment, including a namespaced
  assignment. The same shape in unlabelled prose does not block.
- Git-archive release build: pass, using `scripts/build-from-archive.sh`.
- Installed binary SHA-256:
  `c78b05b46c07d1e7b69428cddf1268f99bccac17ec7d23cb393e7ac27ce6d438`.
- `cargo package --locked`: pass; packaged 344 files; crate SHA-256
  `0b79805024c5b60233974141bde7b251679c456e036fe1602c733be07c4f7a79`.
- Isolated install from the packaged crate: pass; installed binary SHA-256
  `b1ef0d56d765a872123e307e57d18b1fc1b8a0c1b049822aaaa4032f8bf22406`.
- Installed capabilities: secret-rejection contract v1, ruleset version 3,
  enforce mode, and historical-redaction atomic-redact, anti-resurrection,
  sanitized-generation-set, and resumable-publication capabilities all present.

## Focused verification from the exact archive

| Command | Result |
| --- | --- |
| `cargo test --locked --lib scan::` | 30 passed |
| `cargo test --locked --test secret_rejection` | 9 passed |
| `cargo test --locked --test redaction_transaction` | 6 passed |
| ruleset capability test | 1 passed |

These tests cover context binding, rejection atomicity, output nondisclosure,
historical discovery, exact redaction, and the advertised ruleset version.

The full exact-commit gates are not green and BR-T18 must not close on this
evidence alone:

- `cargo fmt --check`: failed on pre-existing formatting drift outside the
  ruleset-v3 files.
- `cargo clippy --all-targets --locked -- -D warnings`: failed with 14
  pre-existing lints in scheduling/diagnostic code.
- `cargo test --locked`: library tests passed (268 passed and 5 ignored), then
  the run stopped at `build_from_archive_checkout_untouched` because both tests
  assume a `.git` directory that a Git archive deliberately does not contain.

## NEEDLE semantic remediation

The exact installed binary found three live Garage key-ID assignment findings:
one issue note, one issue close reason, and one historical event detail. Each
was removed with `bead redact` using only its finding fingerprint. The operation
produced three ruleset-v3 findings, receipts, epochs, and tombstones in addition
to the three ruleset-v2 records from the secret-value remediation.

- NEEDLE checkpoint commit accepted by Forgejo:
  `20137f935f38a120706474eecd8989273a95519d`.
- Current and previous generation:
  `gen-420ddf164b2f0454b62a68b9e79fc500`.
- Verified root:
  `98887e590b893e9494da35f96934f8b42ee69759fe40dd55bcc2e301b5b2816a`.
- Counts: 1,994 issues; 15,374 events; 2,195 dependencies; 0 attempt
  outcomes; 6 findings, 6 receipts, 6 epochs, and 6 tombstones.
- `bead sync status`: live and covered sequence both 15,374, view agreement
  true, zero unresolved tombstones, ready to commit.
- `bead doctor --scope secrets --format json`: ruleset v3, 0 blocking and 375
  advisory findings across live state plus current and previous generations.
- Working-tree gitleaks scan: 0 findings; empty report SHA-256
  `37517e5f3dc66819f61f5a7bb8ace1921282415f10551d2defa5c3eb0985b570`.
- Full-history gitleaks scan: 1,899 commits scanned; 35 findings across 15
  commits (34 `generic-api-key`, 1 `gcp-api-key`); redacted report SHA-256
  `4dadd645f9b03f538084de0f158c38551d4e96c583158f2cc643c811eca2cda9`.
  These are immutable-history findings, not current semantic-store findings.

The gitleaks binary used here is 8.21.2 while NEEDLE's vendored configuration
declares minimum version 8.25.0. The built-in bead-rs scan is therefore the
authoritative semantic gate for this remediation; the gitleaks result is
supporting evidence with an explicit version caveat.

## Credential containment

The historical credential had matched the then-active iad-ci secret in an
output-suppressed equality check, so rotation was mandatory. Rotation was
performed through GitOps and the owning ardenone-cluster OpenBao instance:

- `7e2b2a20`: stage the replacement Garage key;
- `7efaeec5`: trigger replacement-secret synchronization;
- `4fc5456d`: rotate the iad-ci SealedSecret;
- `f0dcd91b`: retain the required public S3 endpoint; and
- `2b4aed3d`: switch the bucket to the replacement and revoke the exposed key.

The Garage and sealed-secrets Argo applications reached Synced/Healthy state.
The replacement credential passed downstream `HeadBucket`; the historical
credential failed it. The old key manifest is absent and the replacement key
and secret references remain present in current GitOps desired state. Remote
Git history remains immutable under the repository's no-force-push policy, so
revocation—not history rewriting—is the containment boundary.

## Recovery rehearsal and learned follow-up

An empty-target restore from the ruleset-v3 NEEDLE forensic checkpoint passed
with the counts above and all 24 redaction records intact. A merge of a
pre-redaction checkpoint then exited nonzero on an event-identity conflict;
the SHA-256 of the complete sorted issue-semantic tuple set was identical
before and after:
`982f764809f343f28b8ad9127b73e1c061fa99bf210bb1dbaf6b271b9b5ae181`.
Neither redacted range was resurrected.

The rejection is fail-closed, but its generic event-identity reason does not
name the applicable historical-redaction tombstones when one event has been
redacted more than once. Follow-up bead `beadrs-b162fc90` owns a focused
regression test and merge-classification fix and blocks BR-T18. BR-T18 remains
open until that work and the full exact-release Rust gates pass.

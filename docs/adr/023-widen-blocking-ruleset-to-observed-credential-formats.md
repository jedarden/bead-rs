# ADR-023: Widen the blocking ruleset to the credential formats the fleet stores

**Status**: Proposed

**Date**: 2026-10-03

**Decision-makers**: bead-rs maintainers, pending independent review

**Narrows**: ADR-014's membership criterion for the blocking tier.

## Context

ADR-014 admits "provider-prefixed token patterns, context-bound credential
identifiers, and private-key armor" to the blocking tier and sends everything
else to an advisory tier. Ruleset 3 implements that with 22 blocking rules.
An audit on 2026-10-03 measured what the split does in practice.

- `bead doctor` over 82 live fleet workspaces reported 0 blocking and 37,547
  advisory findings, 37,449 of them from the entropy rule. Every workspace
  ran `enforce` with no acknowledgment. In 8 workspaces the check returned no
  verdict at all (ADR-025).
- A synthetic probe of the installed 0.2.6 binary, with every candidate
  assembled at run time, was admitted for: a container-registry access token,
  an object-storage application key, legacy and batch vault tokens, a
  network-mesh key, a JSON web token, a URI carrying a password, an
  `Authorization` header, a current-format project API key, and labelled
  assignments such as an environment variable whose name ends in a
  credential keyword. Several produced no finding of either tier.
- A conforming npm token was admitted as `checksum_failed`. Ruleset 3
  validates an 8-character checksum; the format, like GitHub's, carries 6.
- The fleet's Git-layer scanner blocks staged additions on labelled
  assignments and URI credentials that bead-rs admits. Because publication
  stages the checkpoint automatically (ADR-018), a record bead-rs accepted
  can leave the checkpoint uncommittable, and `bead redact` cannot select a
  range that has no bead-rs finding (`beadrs-1c110ec3`).

The advisory tier does not compensate. A real token and a commit hash carry
the same rule identifier, nothing is printed when the mutation is accepted,
and the only place the finding appears is a list tens of thousands long.

A prototype of the rules decided below, replayed over the same 82 stores,
selected 11 distinct values in 12 beads. Every identifier that triggered the
labelled-assignment rule named a credential.

## Decision

Ruleset 4 widens the blocking tier. It stays closed, compiled, offline, and
deterministic, and it stays one of exactly two tiers.

1. **Membership is earned by evidence, not by prefix.** A rule may block when
   it is a deterministic function of the field bytes, its true-positive and
   near-miss fixtures pass, and the fleet replay leaves no undispositioned
   blocking finding. A provider prefix is one way to meet that bar. Labelled
   context and credential-bearing structure are two more.
2. **Provider formats** are added for the credentials the fleet issues and
   uses: container-registry tokens, object-storage application keys,
   network-mesh keys, vault batch and recovery tokens, current project API
   keys, an API-router key, age secret keys, and JSON web tokens.
3. **Context-bound formats** follow the precedent of the Garage key-ID rule:
   a shape too short to trust alone blocks only beside its label.
4. **Structural credentials** block: a URI password, an `Authorization`
   header value, a curl user option, and a Kubernetes Secret data block.
5. **Labelled credential assignment** blocks when the value passes a
   specified randomness qualifier. The qualifier is integer arithmetic over
   the value's bytes: placeholder shape, digit and letter presence, the share
   of word-like runs, the amount of non-word material, and the density of
   character-class changes. It is not an entropy threshold and it is never
   applied to unlabelled text.
6. **The npm checksum width is corrected to 6.** This is a defect against the
   accepted contract and does not wait for review.
7. **Parity with the Git-layer scanner is a fixture obligation.** Every class
   that scanner blocks must block in bead-rs first; its documented false
   positives must not.
8. **Ruleset 4 is opened by its first change and frozen by the release
   gate.** Fingerprints hash the ruleset version, so every fingerprint
   changes once. Stored acknowledgments and unresolved redaction dry-runs
   must be re-derived under ruleset 4.

The normative text is `research/specs/secret-ruleset-v4.md`.

## Rationale

ADR-014's reason for a narrow blocking tier was trust: a gate that misfires
teaches its users to bypass it. That reason argues for measuring false
positives, not for restricting the evidence a rule may use. The replay
measures them directly on the text the gate will actually see.

The qualifier keeps the property ADR-014 protected. Statistical scoring was
rejected because bead text is dense with hashes; the qualifier is only
consulted where a credential label or a credential-bearing structure has
already been found, and a hash rarely sits there. Where one does, as with a
finding fingerprint after `--acknowledge-secret`, the excluded-identifier
list names it.

Detection also decides what can be repaired. `bead redact` selects by
fingerprint, so a stored credential with no finding cannot be destroyed in
the tool. Widening detection is what makes the existing redaction path reach
existing leaks.

## Consequences

### Benefits

- The formats this fleet actually leaks are rejected before commit instead of
  being found in a public mirror.
- Stored credentials that ruleset 3 cannot see become findings, so they
  become redactable.
- A record bead-rs accepts no longer wedges checkpoint publication at the
  Git layer.

### Drawbacks

- More text is rejected. A bead that quotes a labelled random string must
  reword it or acknowledge one fingerprint.
- The excluded-identifier list is a maintained artifact. A new non-credential
  identifier that resembles a credential label is a false positive until a
  release lists it.
- Every fingerprint changes at the version bump.
- An unlabelled, unstructured, format-free secret still passes. So does an
  unlabelled hexadecimal key, which is indistinguishable from a hash.

### Alternatives Considered

- **Promote the entropy tier to blocking**: rejected. It would have rejected
  mutations in every workspace; ADR-014's analysis stands.
- **Embed or invoke the Git-layer scanner**: rejected. ADR-014 excludes an
  external binary on the mutation path, and that scanner's generic rule
  matches diagnostic prose.
- **Add provider prefixes only**: rejected. It closes the registry-token and
  storage-key cases but not URI credentials or labelled assignments, and it
  leaves the Git-layer wedge in place.
- **Rely on the Git-layer gates**: rejected. They run after SQLite has
  accepted the value and after publication has staged it.

## Implementation

BR-T36 corrects the npm width. After the specification is accepted, BR-T40
adds provider and context-bound formats, BR-T41 the qualifier and labelled
assignment, BR-T42 the structural rules, and BR-T44 runs the fleet replay and
freezes the version. Fixtures assemble every candidate at test time.

## Related

- ADR-014; ADR-024; ADR-025; `research/specs/secret-rejection-v1.md`
- R040 and BR-T34 through BR-T44 in `docs/plan/plan.md`
- `beadrs-1c110ec3` (Git-layer parity and redaction reach)

## Supersedes

None. ADR-014 remains in force as narrowed here.

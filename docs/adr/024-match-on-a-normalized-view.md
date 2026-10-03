# ADR-024: Match on a normalized view and report raw byte ranges

**Status**: Proposed

**Date**: 2026-10-03

**Decision-makers**: bead-rs maintainers, pending independent review

**Narrows**: ADR-014's matching sketch (keyword prefilter, anchored regex,
placeholder check).

## Context

Ruleset 3 matches each rule against the field's raw bytes and delimits
tokens with a word-boundary assertion. Agents rarely paste a credential as a
clean, isolated token. They paste command output, logs, and JSON. A probe on
2026-10-03 placed one checksum-valid synthetic GitHub token in five such
settings. The scanner admitted all five:

- after a JSON-escaped newline, where the escape's `n` is a word character;
- between ANSI color sequences, where the sequence's final `m` is one;
- percent-encoded after `token%3D`, where `D` is one;
- wrapped across a line break; and
- base64-encoded, as a kubeconfig or Secret manifest carries key material.

The keyword prefilter has a related defect. It enumerates anchors without
overlap, so one anchor can hide another that overlaps it and the hidden rule
never runs. The rule table already carries two comments working around this.

## Decision

A field is scanned in a raw view and in bounded derived views, and every
finding is reported against raw bytes.

1. **Boundaries are explicit.** A match is valid when the byte before it is
   not alphanumeric and the byte after it is not in the rule's body alphabet.
   The check is a predicate on the match span. Word-boundary assertions are
   removed.
2. **Normalized view.** ANSI control sequences are removed, backslash escapes
   are decoded once, and percent-encoded printable bytes are decoded.
3. **Dewrapped view.** A line break between two token-alphabet bytes is
   deleted, with trailing indentation and a preceding continuation backslash.
4. **Decoded view.** Long base64 runs that decode to mostly printable text
   are decoded once, within fixed count and size bounds, and scanned by
   provider-format and private-key rules only.
5. **Raw reporting.** Each transformation keeps an offset map. A finding's
   range and fingerprint are computed over the raw bytes that produced the
   match, so `secret-rejection-v1` findings, acknowledgments, and
   `bead redact` are unchanged.
6. **Overlapping prefilter.** An anchor that occurs in a view always makes
   its rule eligible. This restores accepted behavior and does not wait for
   review.

## Rationale

The alternative to normalization is a rule variant for every encoding of
every format, which multiplies the rule table and still misses the next
encoding. Decoding the text once and reusing the same rules keeps the table
small, which ADR-014 set as a goal.

Reporting raw ranges is what keeps the change contained. If findings pointed
into a derived view, the fingerprint contract, the redaction selector, and
every stored receipt would need a second coordinate system. With an offset
map they need none.

The decoded view is limited to provider-format and private-key rules because
decoded text has no labels a reader wrote. Running the labelled-assignment
rule there would test the qualifier on arbitrary decoded bytes.

One level of decoding is a deliberate limit. Recursive decoding has
unbounded cost on hostile input and the observed cases are single-layer.

## Consequences

### Benefits

- A credential pasted inside logs, JSON, colored output, a query string, or
  a manifest is rejected like the bare token.
- One rule table serves every view.
- Redaction, acknowledgment, and fingerprints keep one coordinate system.

### Drawbacks

- Up to four passes over a field. The benchmark budget is three times the
  ruleset 3 measurement on the hostile 4 MiB field.
- The dewrapped view can join two unrelated tokens. Fixed-length provider
  bodies and the trailing-boundary check make an accidental join into a
  valid match improbable; the fleet replay measures it.
- Double encoding, compression, and archives are still not inspected.

### Alternatives Considered

- **Keep word boundaries and add per-encoding rule variants**: rejected. The
  rule table grows with every encoding and remains incomplete.
- **Report ranges in the normalized view**: rejected. It breaks the
  fingerprint and redaction contracts.
- **Recursive decoding**: rejected. Unbounded cost, no observed need.
- **Normalize in place and store the normalized text**: rejected. bead-rs
  stores what the operator supplied; R006 requires the checkpoint to
  represent the store exactly.

## Implementation

BR-T37 makes prefilter evaluation overlapping. After the specification is
accepted, BR-T39 implements boundaries, the three derived views, and the
offset map. Section 3 of `research/specs/secret-ruleset-v4.md` is normative.

## Related

- ADR-014; ADR-015; ADR-023
- R040, BR-T37, and BR-T39 in `docs/plan/plan.md`

## Supersedes

None.

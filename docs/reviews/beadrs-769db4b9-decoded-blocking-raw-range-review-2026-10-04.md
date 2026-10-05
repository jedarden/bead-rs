# Ruleset v4 decoded blocking and raw-range review

- Reviewer identity: `Codex-pluck-decoded-range-2026-10-04`
- Date: 2026-10-04
- Decision: **actionable rejection** for the reviewed ruleset-v4 input
- Scope: sections 3.2, 3.4, 4.1, and 4.4, compared with the accepted
  `secret-rejection-v1` coordinate/rejection contract, ADR-023 through ADR-025,
  and the repository's independent fixture catalog. This is an independent
  contract review; no implementation source or another implementation's
  fixtures were used.

## Exact input identities

These are SHA-256 hashes of the complete files reviewed:

| Artifact | SHA-256 |
|---|---|
| `research/specs/secret-ruleset-v4.md` | `bad7c8102086d8128465e284b839673ceceb84b28e3ea7bf55835dfaee5ad5a9` |
| `research/specs/secret-rejection-v1.md` | `f6aa7639a8ef1dd509b431853abf64db78d0923ef9e9d59b09e0a4e0e55df231` |
| `docs/adr/023-widen-blocking-ruleset-to-observed-credential-formats.md` | `39d9174173df3aadc7c268bdd27d139e1091cc12559e20b9d098037683a7359a` |
| `docs/adr/024-match-on-a-normalized-view.md` | `508989f43c984b3a69ef529eaaf0d96ed1caa44873857e91b14aeaaa61de9c86` |
| `docs/adr/025-surface-advisory-findings-and-partial-scan-coverage.md` | `e13bda45279d2b875c9c2dce6d2bdc3e0471e91512cff22ec40487f1a0de4a33` |
| `research/specs/historical-redaction-v1.md` | `5658ac80cf9594283ddf65742aff2f4b2020a0a7869a612ee2230ed99033a016` |
| `man/man1/bead-redact.1` | `957b8b72fee4ecbd0a71520b045794b219dbaae5c5d2d0d96eef3dd60f746153` |
| `research/fixtures/README.md` | `dfd99a8d7a5272dc92e5ec5be16a5f50a426e507bcf3ea866e41bbd910284b64` |
| `research/fixtures/secret-write-boundary-v1.json` | `c75f4dbf6242ced3c7e21e501e1de7a0fbef2a5371c601957c809043f0996710` |

The fixture catalog contains no ruleset-v4 detector truth set. Its only
secret-related JSON fixture is a nine-case write-boundary manifest; it has no
candidate bytes, JWT/table-row cases, view labels, expected raw coordinates, or
redaction fingerprints. Section 7 of the ruleset therefore remains a
requirement, not evidence that these cases pass.

## Synthetic coordinate probes

The following probes were assembled at review time and are not committed
fixtures or credential samples. They check the coordinate arithmetic without
placing a format-valid candidate in the repository.

1. **Direct JWT.** A runtime-generated token had segment lengths 19, 15, and 8;
   the first segment decoded to a JSON object containing `alg`. In the raw
   field layout `x=` + token + `;`, the candidate occupies raw byte range
   `[2,46)`. This exercises the three-segment/`eyJ` shape in section 4.1 and
   the header check in section 4.5. It is a blocking-shape probe, not proof of
   signature, issuer, or expiry validation.
2. **Decoded JWT.** The same token was standard-base64 encoded at runtime. The
   encoded run was 60 bytes including one padding byte (59 without it). In the
   raw layout `B64=` + encoded-run + `;`, section 3.4 requires the decoded
   finding to report the whole-run raw range `[4,64)`. If padding is excluded
   from the run but unpadded decoding is accepted, the equally plausible range
   is `[4,63)`. Those ranges produce different v1 fingerprints and redaction
   targets even though the decoded bytes are the same.
3. **Table row.** A runtime-generated 31-byte printable value passed the
   section 2.2 checks needed for `Q(value,20)`. In the compact form-3 layout
   `|api_token|V31|`, the value bytes are `[11,42)` (the complete row is 43
   bytes). This is the expected range if the second pipe is the separator and
   the final pipe is only a terminator. The written value rule also permits
   `|`, so without delimiter precedence an implementation can instead consume
   the final pipe or choose a different cell/value span.

These probes establish that a selected derived finding can be mapped to a
valid raw half-open interval. They do not establish that independent
implementations select the same interval.

## Findings

### 1. Decoded-view bounds do not define coverage

Section 3.2 says to consider maximal normalized runs, with at most 64 runs per
field and at most 65,536 bytes per run. It does not define:

- whether the 64 runs are the first 64 in left-to-right order, the longest 64,
  or another selection;
- whether a run over 65,536 bytes is skipped, truncated, split into decodable
  pieces, or makes the field incomplete; or
- whether a skipped/limited run creates a coverage status or diagnostic.

ADR-024 supplies the one-level and bounded-decoding intent, while ADR-025's
`coverage` statuses describe independent diagnostic sources, not fields or
discarded decoded runs. A credential placed in the 65th qualifying run or
outside the chosen portion of an over-limit run can therefore be either
blocked or silently missed depending on the implementation. “At most” is a
resource cap, not a reproducible selection rule or a fail-closed scan result.

### 2. “Lenient base64” is not an interoperable grammar

The contract does not say whether the run alphabet includes padding `=`, how
padding must be placed, whether standard and URL-safe alphabets may mix, which
non-alphabet bytes terminate a run, or whether invalid input is rejected,
skipped, or decoded after filtering. It also does not state whether a
non-canonical spelling is accepted. The decoded-JWT probe demonstrates a
concrete consequence: including the final padding changes the whole-run raw
range from `[4,63)` to `[4,64)`.

This is not just a parser nicety. Run boundaries determine whether a decoded
provider token or private key exists, and section 3.4 fingerprints the entire
encoded run. Different grammar choices can thus change both detection and the
bytes selected by `bead redact`.

### 3. JWT and private-key blocking is only partly closed

The intended JWT path is clear at a high level: section 4.1 identifies the
three segments, section 4.5 requires a base64url-decoded JSON object with an
`alg` member, and section 3.3 permits provider-format rules in the decoded
view. The exact contract still lacks the base64url padding/unpadding rule and
JSON member semantics (for example, duplicate `alg` members, member type,
invalid UTF-8, and trailing JSON). Consequently, a shape-only match can be
classified differently by independent parsers, especially at the
`checksum_failed` versus blocking boundary.

The same section 3.3 explicitly names a private-key rule for the decoded view,
and ADR-023 names private-key armor as a blocking class, but section 4.1 does
not define its rule identifier or armor grammar. `secret-rejection-v1` states
the category without supplying that v4 matching shape. The inheritance sentence
about ruleset-3 rules is not enough for an independent v4 implementation to
know which armor headers, line structure, footer, malformed cases, and raw
range are required. This leaves decoded private-key reach and parity
unverifiable.

### 4. Form-3 table rows have no unique parse

Section 4.4 leaves several choices that affect both blocking and the range:

- it does not state whether a list marker is adjacent to the identifier or may
  be followed by spaces;
- `|` is simultaneously a leading marker, a separator, and a possible row
  terminator, with no precedence for cell selection or leading/trailing pipes;
- escaped or embedded pipes are not defined; and
- the value character rule does not exclude `|`, even though the row grammar
  treats a trailing `|` as optional syntax.

Thus compact `|identifier|value|`, conventional spaced Markdown rows, and rows
with embedded pipes can produce different identifier/value spans. Since the
form-3 threshold is `Q(value,20)`, a parser that includes a delimiter can also
change the qualifier result and the reported fingerprint. The compact
coordinate probe above is one valid interpretation, not a contractually
forced one.

## Compatibility and threat-model disposition

**Compatibility: rejected for this exact v4 input; compatible in principle.**
The additive ruleset identity, raw field-byte coordinates, fingerprinted
findings, and caller-free `bead redact` request remain compatible with
`secret-rejection-v1` and historical redaction. Once a view and parser are
fixed, the minimum-cover rule gives a valid raw half-open range, and
historical-redaction's live-byte fingerprint revalidation can safely reject a
stale target. That conditional property does not make ambiguous v4 findings
portable between implementations or revisions.

**Threat model: actionable rejection.** An attacker can place a credential in
an unselected decoded run, exploit an over-limit run, use a padding/alphabet
spelling that another decoder rejects, or format a credential-bearing table
row at a parser boundary. The absence of per-field incomplete coverage can
turn those misses into an apparent clean result. A range chosen by a permissive
table parser can also redact delimiter syntax or a wider value than another
implementation would select. Revalidation prevents an arbitrary stale range
from silently changing current bytes, but it cannot repair a missed finding or
make divergent fingerprints converge.

## Required revision before acceptance

1. Specify a complete base64/base64url grammar, including padding, mixed
   alphabets, invalid-byte handling, canonicality, decode failure, run order,
   the 64-run selection, and the over-65,536-byte disposition. Expose a
   deterministic incomplete-coverage result when scanning is intentionally
   bounded.
2. Define the JWT header decoder and JSON-member policy, and define the
   private-key rule identifier and armor grammar across raw, normalized,
   dewrapped, and decoded views.
3. Give form 3 an explicit row grammar with marker spacing, pipe/cell
   precedence, escaping, and an unambiguous value span. Add harmless test-time
   JWT and table-row positives, near misses, view labels, exact raw ranges, and
   `bead redact --dry-run` revalidation expectations.

**Final disposition: ACTIONABLE REJECTION.** The raw-range mechanism is
acceptable as a conditional construction, but the current ruleset cannot
claim deterministic decoded blocking, private-key parity, table-row blocking,
or cross-implementation redaction compatibility.

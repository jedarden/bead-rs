# Ruleset v4 raw-range and redact compatibility review

- Date: 2026-10-04
- Decision: **accepted for section 3.4 at the exact input hash below**
- Scope: contract review only; this does not accept the rest of ruleset v4 or
  claim implementation conformance.

## Inputs and method

The review compared section 3.4 and its view definitions with the raw-byte
finding and fingerprint rules, historical-redaction revalidation, the public
`bead redact` manual, and the independent fixture catalog. It used no
implementation source or copied tests. The table's byte strings are
independently authored, harmless mapping examples; they contain no
format-valid credential.

| Input | SHA-256 |
|---|---|
| `research/specs/secret-ruleset-v4.md` (entire reviewed input) | `bad7c8102086d8128465e284b839673ceceb84b28e3ea7bf55835dfaee5ad5a9` |
| `research/specs/secret-rejection-v1.md` | `f6aa7639a8ef1dd509b431853abf64db78d0923ef9e9d59b09e0a4e0e55df231` |
| `research/specs/historical-redaction-v1.md` | `5658ac80cf9594283ddf65742aff2f4b2020a0a7869a612ee2230ed99033a016` |
| `man/man1/bead-redact.1` | `957b8b72fee4ecbd0a71520b045794b219dbaae5c5d2d0d96eef3dd60f746153` |
| `research/fixtures/README.md` | `dfd99a8d7a5272dc92e5ec5be16a5f50a426e507bcf3ea866e41bbd910284b64` |
| `research/fixtures/secret-write-boundary-v1.json` | `c75f4dbf6242ced3c7e21e501e1de7a0fbef2a5371c601957c809043f0996710` |

The committed fixture describes service and publication outcomes; it contains
no derived-view range cases. The ruleset v4 conformance section requires
synthetic candidates assembled at test time, including `bead redact --dry-run`
resolution. The following independent vectors check the range arithmetic
without embedding a valid finding.

## Raw-range check

Ranges below are zero-based, half-open byte offsets in the raw UTF-8 field. In
the ANSI row, `ESC` is one byte (`0x1b`). Each selected derived span maps to the
smallest contiguous raw interval covering the source bytes for its output;
deleted bytes between those source bytes are necessarily inside that interval.

| Transform | Raw synthetic input | Derived bytes | Selected bytes | Raw range |
|---|---|---|---|---|
| Percent decoding | `A%2FB` | `A/B` | `/B` | `[1,5)` |
| Backslash escape | `q\u0058r` (six literal escape bytes) | `qXr` | `X` | `[1,7)` |
| ANSI removal | `A` + `ESC[31m` + `B` + `ESC[0m` + `C` | `ABC` | `BC` | `[6,12)` |
| Dewrapping | `AB` + backslash + LF + two spaces + `CD` | `ABCD` | `BC` | `[1,7)` |
| Decoded run | `YWJjZGVmZ2hpamtsbW5vcHFyc3R1dnd4eXphYmNk` | `abcdefghijklmnopqrstuvwxyzabcd` | mapping-only | `[0,40)` |

The decoded example is 40 base64 alphabet bytes decoding to 30 printable ASCII
bytes. It is intentionally benign and does not exercise a production rule. For
an actual decoded-view finding, section 3.4 explicitly selects the entire
encoded run, so the returned offsets still identify one valid raw half-open
range even when a particular decoded match uses only part of the run.

These cases support the contract's range construction: transformed output
bytes retain their producing raw spans; taking the minimum source start and
maximum source end yields a valid field range. For dewrapped or ANSI-stripped
matches the cover includes deleted separators. For decoded matches the
whole-run rule provides a stable cover without requiring a partial base64
substring to be independently decodable.

## Redaction and compatibility

`secret-rejection-v1` defines finding offsets in raw field bytes and includes
the range and matched bytes in the fingerprint. Section 3.4 removes ambiguity
for derived views by defining the fingerprint input as the raw bytes at the
reported range. `historical-redaction-v1` resolves a finding through the
current scanner, recomputes its fingerprint against live field bytes under
the redaction transaction, and conflicts on stale content. Thus, for an
unchanged field and ruleset, each range above can be re-derived and
revalidated; offsets from a normalized or decoded buffer are never passed to
redact. The public CLI continues to select by fingerprint and does not accept
caller-supplied offsets.

The finding shape and raw-byte offset convention remain compatible. The
reported range is a redaction cover, not necessarily a literal substring
equal to the detected derived bytes. Ruleset version 4 makes that changed
matching behavior distinguishable. A consumer that displays or uses offsets
must therefore treat them as the bytes to redact, not as a view-relative
match span; this is a semantic clarification, not a new request or receipt
shape.

The privacy effect is conservative and has a content-loss cost. A cover can
include escape syntax, line breaks, ANSI controls, or other encoded content;
for a decoded finding the entire encoded run is replaced. The normalized run
limit is 65,536 bytes, but that is not a bound on its raw covering interval
when the source uses escapes or contains removed ANSI sequences. This is
intentional in the stated contract and favors removing all source material
that contributed to the decoded candidate. The current dry-run path lets an
operator inspect the selected effect before mutation. Fingerprints and
diagnostics still contain no matched bytes.

## Decision

**Accept section 3.4 for SHA-256 `bad7c8102086d8128465e284b839673ceceb84b28e3ea7bf55835dfaee5ad5a9`.** Its derived findings have valid raw half-open ranges, and the fingerprint and live-row revalidation contracts allow `bead redact` to resolve them without changing its request interface. The wider decoded-run replacement is a documented privacy-versus-content-preservation tradeoff, not a compatibility blocker for this slice.

This is not evidence that an implementation passes ruleset v4. The required
runtime fixtures must still show derived findings' offsets and fingerprints,
and that `bead redact --dry-run` resolves each one, before claiming
conformance.

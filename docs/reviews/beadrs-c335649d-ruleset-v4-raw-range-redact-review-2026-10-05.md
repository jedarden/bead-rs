# BR-T35 raw-range reporting and redact compatibility review

- Date: 2026-10-05 (after the decoded-view review)
- Decision: **ACCEPTED for section 3.4 only**, at the exact ruleset input hash below
- Scope: specification and public CLI contract review; no implementation
  conformance claim

## Inputs and method

The review compared ruleset v4 sections 3.2–3.4 with the accepted v1 finding
and fingerprint contract, the historical-redaction contract, the public
`bead redact` manual, and the permitted independent fixture material. It used
no implementation source and no real or format-valid credentials.

| Input | SHA-256 |
|---|---|
| `research/specs/secret-ruleset-v4.md` (entire reviewed input) | `bad7c8102086d8128465e284b839673ceceb84b28e3ea7bf55835dfaee5ad5a9` |
| `research/specs/secret-rejection-v1.md` | `f6aa7639a8ef1dd509b431853abf64db78d0923ef9e9d59b09e0a4e0e55df231` |
| `research/specs/historical-redaction-v1.md` | `5658ac80cf9594283ddf65742aff2f4b2020a0a7869a612ee2230ed99033a016` |
| `man/man1/bead-redact.1` | `957b8b72fee4ecbd0a71520b045794b219dbaae5c5d2d0d96eef3dd60f746153` |
| `research/fixtures/README.md` | `23af7f702d57f71cce99ec9e13807cce66285be996ba277f8c2074c8c9bc5b7d` |
| `research/fixtures/secret-write-boundary-v1.json` | `c75f4dbf6242ced3c7e21e501e1de7a0fbef2a5371c601957c809043f0996710` |

The fixture catalog has no derived-view range cases. Its secret-write-boundary
fixture records only service and publication outcomes. The following
independently authored, harmless byte strings check coordinate arithmetic;
the decoded string is ordinary alphabetic text, not a credential candidate.

## Raw half-open range check

Offsets are zero-based bytes in the raw UTF-8 field. For normalized matches,
the minimum source start and maximum source end of the selected output bytes
form the smallest contiguous raw cover. Deleted bytes between their sources
are inside that cover. A decoded match selects the cover of its entire
normalized encoded run, as section 3.4 requires.
Here `ESC` in the table denotes one raw `0x1b` byte.

| Transform | Raw synthetic input | Derived bytes | Selected bytes | Raw range |
|---|---|---|---|---|
| Percent decoding | `A%2FB` | `A/B` | `/B` | `[1,5)` |
| Backslash escape | `q\u0058r` (six literal escape bytes) | `qXr` | `X` | `[1,7)` |
| ANSI removal | `A` + `ESC[31m` + `B` + `ESC[0m` + `C` | `ABC` | `BC` | `[6,12)` |
| Dewrapping | `AB` + backslash + LF + two spaces + `CD` | `ABCD` | `BC` | `[1,7)` |
| Decoded run | `YWJjZGVmZ2hpamtsbW5vcHFyc3R1dnd4eXphYmNk` | `abcdefghijklmnopqrstuvwxyzabcd` | whole encoded run | `[0,40)` |

Each result is within the original field and is a valid raw `[start,end)`
range. The derived bytes need not equal the raw bytes covered: normalization
and dewrapping can include escape syntax or deleted separators. The 40-byte
base64 example decodes to 30 printable bytes; selecting the whole run avoids
requiring a partial base64 substring to decode independently.

## Compatibility disposition

**ACCEPTED for section 3.4's range and fingerprint contract.** The accepted
`secret-rejection-v1` contract locates findings in raw field bytes and includes
the range and matched bytes in the fingerprint. Section 3.4 specifies that a
derived finding fingerprints the raw bytes at its reported range. That gives
`bead redact` a raw selector it can reproduce using the same binary and
ruleset. The historical-redaction contract resolves checkpoint findings to
the live field and recomputes the fingerprint under the redaction transaction;
changed or stale live bytes conflict without mutation. The public command
accepts a fingerprint, not caller-supplied offsets, so its request and receipt
interfaces need no change.

The reported cover is the byte span to redact, not necessarily the literal
derived-view match. Consumers must interpret offsets as raw bytes to remove.
Ruleset versioning identifies that semantic clarification. This decision does
not accept other v4 parsing or view-selection semantics and does not establish
runtime conformance.

## Threat-model disposition

**ACCEPTED for source erasure, with bounded coverage and deliberate
over-redaction.** Replacing the raw cover removes the source syntax that
contributed to a derived finding; decoded findings replace the entire encoded
run. That may remove benign co-encoded content. The raw cover can also exceed
the normalized 65,536-byte run limit when escapes or removed control sequences
expand its source span. This chooses removal of contributing stored material
over preserving adjacent content. The disposition makes no claim that the
scanner detects every credential. Value-free output, dry-run review, and
stale-fingerprint conflict remain the applicable privacy and mutation
controls.

## Decision

**ACCEPTED — section 3.4 at SHA-256
`bad7c8102086d8128465e284b839673ceceb84b28e3ea7bf55835dfaee5ad5a9`.** The derived bytes
map to valid raw half-open covers, and the accepted fingerprint plus
historical-redaction contracts let `bead redact` revalidate them against
unchanged live bytes. The compatibility acceptance is limited to raw-range
reporting and redaction; it is not a waiver for the rest of ruleset v4.

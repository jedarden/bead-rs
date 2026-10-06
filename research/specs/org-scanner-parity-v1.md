# Organization secret-scanner parity v1

Status: implemented (beadrs-1c110ec3). Owner bead records verification.

Fully quoted JSON-view correction proposed 2026-10-06 under
`beadrs-b3059276`; independent exact-hash review required before activating
the correction. The framing protocol and all other behavior remain unchanged.

## Problem

The fleet's Git hooks and Forgejo pre-receive gate run the organization
scanner (`secret-scanner`, a separate repository) over `.beads/checkpoint/`
files. Before this contract, a bead text that scanner blocked but bead-rs's own
rules did not flag had no in-tool remedy: `bead redact` resolves only findings
bead-rs itself reports, and `.beads/` must never be hand-edited.

## Contract

1. **Source of truth.** bead-rs does not copy the organization rules. When
   `BEAD_ORG_SECRET_SCANNER` names an executable, bead-rs starts it once per
   process as `<scanner> --serve` and asks it, for every scanned field, where
   its findings sit. Unset, empty, or `off` disables parity; hermetic builds
   and tests never depend on a host binary.
2. **Protocol.** Request: 4-byte big-endian length, then the document bytes.
   Response: one line, a JSON array of `{line, rule, start, end}` half-open
   byte offsets into the document, or `{"skipped": ...}` for binary or
   oversized input. The scanner never writes matched bytes; bead-rs validates
   rule IDs as `[A-Za-z0-9_-]+` and discards out-of-range spans.
3. **Views.** Each field is sent twice: its raw text, and a complete quoted
   JSON string, escaping its body exactly as a checkpoint line carries it
   (what the Git-side scanner reads). Surrounding quotes supply explicit valid
   JSON context; they are not stored field bytes. The opening quote maps to
   the empty raw range `(0,0)`, and the closing quote to `(len,len)`, where len
   is the raw UTF-8 byte length. Each body byte maps to the full raw UTF-8
   character range that produced it, including all bytes of an escape.
   Spans map back to the smallest covering raw range as before; an empty,
   reversed or out-of-range mapped span is discarded. Raw input is unchanged.
   No new option or response schema is required from an adapter.

   The correction prevents an organization scanner from confusing formatting
   escapes with literal password material: actual LF/CR/tab characters at a
   password boundary do not become password length/entropy evidence, while
   JSON-encoded literal backslashes remain password bytes. Recognition of
   JSON semantics must validate a complete JSON string/document, not guess
   from a prefix. Invalid JSON must retain ordinary raw scanning, not become
   an empty/clean result through the JSON path. The independent runtime
   witnesses are `research/fixtures/org-scanner-json-view-contract.py`.
4. **Findings.** Each span becomes a blocking, confirmed finding with rule ID
   `org-scanner:<rule>`, provider `org-secret-scanner`, and ruleset version
   1000 (disjoint from native numbering). Fingerprints use the standard v1
   construction, so acknowledgment, tombstones, doctor inventory, write-time
   rejection, and redaction resolution apply unchanged; stored fingerprints
   keep resolving after either ruleset changes.
5. **Failure.** If the scanner cannot start or answer, parity is disabled for
   the rest of the process with one warning. A mutation never fails because of
   it; native rules still apply and the Git-side gate still scans commits.

## Atomic removal

`bead redact --all-blocking` selects every current confirmed blocking finding
(native and organization, live and retained, acknowledged included), resolves
each to its live bytes, drops duplicates, and per field keeps the one finding
whose range covers each overlap group. The selection runs as one atomic batch
(one transaction, one epoch, one sanitized publication). A group that no single
finding covers is refused without changes.

## Residue

- The Git-side scanner sees whole checkpoint lines; rule state that depends on
  neighbouring JSON keys (for example a credential keyword in a key name) can
  differ from per-field scanning. Such a finding is still removable with an
  explicit `--finding` once bead-rs reports it, or by adjusting the scanner.
- Git history is never rewritten; anything committed before redaction must be
  rotated.

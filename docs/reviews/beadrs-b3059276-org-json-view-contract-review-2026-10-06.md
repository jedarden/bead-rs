# Organization scanner quoted-view exact-input review — accepted

Independent reviewer: `/root/secret_release_review`, distinct from `/root`,
author of the contract correction and fixture. Owning application outcome:
`beadrs-b3059276`; organization detector owner: `fss-2272b3b4`.

## Exact acceptance

**The scoped quoted-JSON-view contract correction and its independent mapping
fixture are accepted at these exact SHA-256 hashes:**

- `research/specs/org-scanner-parity-v1.md`:
  `314ca8dc63e52016973094f787f3c7fd43a071be93872c8558971025e6c94321`.
- `research/fixtures/org-scanner-json-view-contract.py`:
  `d3c7aabcc900e54d8a67c738a161464b53059cf4f8eb4aa8d7a643108606140d`.

Both complete files were read and their SHA-256 values matched before and after
review. The reviewer authored neither the proposal nor the fixture and did not
modify either artifact. Applicable repository/clean-room guidance was read.
No other bead implementation, native implementation source, actual store data,
or real secret value was inspected. No live mutation, installation, commit,
push, release or deployment was performed.

## Assessment

The second view is now an actual complete JSON string rather than an ambiguous
escaped body. That supplies a principled serialization boundary without
changing raw-field input or the framed request/response protocol. The
organization scanner must validate complete JSON context, not infer it from a
prefix. Invalid JSON falls back to ordinary raw scanning, so malformed input
cannot become a clean result merely because the JSON path rejected it.

Opening and closing wrapper quotes map to empty raw ranges at the beginning
and end of the field. Every encoded body byte maps to the full source UTF-8
character that produced it. A reported span therefore maps to the smallest
covering raw range; quote-only, empty, reversed and out-of-range spans are
discarded. Unicode and control-character escaping preserve byte offsets.

Actual LF/CR/tab password boundaries must not supply password length or entropy
after serialization. Encoded literal backslashes remain password material,
including when followed by letters naming JSON whitespace escapes. This
distinction is explicitly tested; the correction must not blindly truncate
every backslash followed by `n`, `r`, or `t` in raw text.

Native finding identity, rule names, raw-byte fingerprints, atomic selection,
redaction resolution, failure handling and documented parity residue remain
unchanged. Existing limitations—per-field versus neighbouring checkpoint-key
context, process-local external-scanner failure fallback, and unrevised Git
history—are not claimed resolved by this narrow acceptance.

## Actual verification

- `python3 research/fixtures/org-scanner-json-view-contract.py`: all four tests
  passed, zero failures. These verify complete quoted context, Unicode mapping,
  formatting whitespace exclusion, literal-backslash preservation and invalid
  input/span boundaries; they are mapping witnesses, not scanner conformance.
- An independent `python3 -B` runtime matrix added 36 controls: empty input,
  control characters, quotes/backslashes, multibyte UTF-8, U+2028, and one
  through eight literal backslashes followed by `n`, `r` or `t`. All complete
  serialization and mapped-range predicates passed. Only counts and boolean
  case results were emitted, never candidate values.
- `git diff --check -- research/specs/org-scanner-parity-v1.md
  research/fixtures/org-scanner-json-view-contract.py`: passed.
- Final SHA-256 recomputation confirmed both frozen artifacts unchanged.

This is contract/fixture acceptance permitting the scoped implementation, not
scanner/native implementation or release acceptance. Complete-document
validation and exact curl capture/mapping still need production tests, raw and
JSON positive/negative parity, literal-backslash and malformed-input controls,
and the existing performance/fleet/publication gates. The reviewer did not run
native Cargo checks for this pre-implementation review. No permission to scrub
real credentials or clear quarantine is granted by this decision.

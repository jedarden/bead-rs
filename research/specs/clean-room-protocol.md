# Clean-room protocol

Status: normative.

## Roles

The specification role may observe released binaries and consumer behavior.
It records only functional facts. The implementation role works exclusively
from the sanitized specifications and independent fixtures in this repository.

## Forbidden transfers

The specification role must not transfer source excerpts, source-derived
pseudocode, internal structure, copied SQL, comments, tests, fixtures, help
prose, or error prose. Identifiers are included only when required by a public
interchange or CLI contract.

## Observation rules

- Use fresh temporary workspaces and independently invented records.
- Record the invoked binary version and platform.
- Capture inputs, stdout, stderr, exit status, and filesystem effects.
- Reduce observations to a behavioral requirement before implementation use.
- Mark uncertain behavior as unresolved rather than guessing.

## Implementation attestations

Every implementation change must identify its governing specification
section. Contributors attest that they did not consult prohibited inputs for
the component being changed. Exposure incidents are appended to
`PROVENANCE.md` and require reassignment or independent review.

## Release gate

A release may claim clean-room status only when provenance review, unexplained
similarity review, license review, and the complete conformance suite pass.


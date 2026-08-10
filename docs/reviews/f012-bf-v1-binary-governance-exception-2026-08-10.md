# F012 bf-v1 binary provenance governance exception request

Date: 2026-08-10 UTC

Status: **pending independent governance approval**. This document does not
approve the exception and does not activate F012.

## Narrow request

Permit the already recorded bf-v1 black-box observations to remain provisional
specification evidence despite the absence of an identified publisher-built,
publisher-checksummed `bf 0.4.0` artifact. The exception applies only to the
behavioral observations in `bf-v1-profile.md` and `research/fixtures/bf-v1/`;
it permits no source inspection, implementation derivation, release claim, or
substitution of the producer's SQLite format.

## Independently verifiable executable identity

The exercised executable reports `bf 0.4.0` and has:

- path at observation time: `/home/coding/.cargo/bin/bf`;
- SHA-256: `696019aeaaeee50ce1fc62fe2407e73892caf9818e54f434f5e22b0dad81018e`;
- size: 6,395,912 bytes;
- format: stripped x86-64 Linux PIE ELF, dynamically linked;
- GNU build ID: `58f50ef6ce07b6385d837ff37df3032803210b39`; and
- local Cargo install record: `bead-forge v0.4.0
  (/home/coding/bead-forge)` with binary `bf`.

These facts let another reviewer identify byte equality or distinguish a
different build without consulting producer source. They do not prove who
built the executable.

## Why an exception is necessary

The round-two reviewer searched public release metadata and found no official
`bf 0.4.0` compiled release plus publisher checksum/signature. The round-three
author repeated public web searches for the package/version and found no such
artifact. Search-engine absence is not proof of nonexistence, so an independent
reviewer must repeat direct publisher/release-registry checks before deciding.
No producer source, tests, fixtures, SQL, or internal documentation was opened
to fill the gap.

## Approval criteria

An independent governance reviewer may approve only after recording:

1. the public locations searched and UTC access time;
2. whether an official artifact, checksum, signature, SBOM, or attestation was
   found;
3. an independent SHA-256/version/build-ID check of the exercised executable;
4. a fresh black-box reproduction of the profile's load-bearing status,
   dependency direction/order, event-presence, and null/absence observations;
5. confirmation that no prohibited producer material was consulted; and
6. the exact accepted profile, fixture-manifest, and exception-document hashes.

If an official attested artifact is found, this exception is rejected and the
fixtures must be regenerated or corroborated against that artifact. If byte
identity or the load-bearing observations do not reproduce, the exception is
rejected. Approval must be authored by someone other than the original fixture
authors, correction authors, implementation author, and this request author.

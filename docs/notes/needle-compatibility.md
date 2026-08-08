# NEEDLE compatibility implementation notes

NEEDLE is a consumer of bead-store behavior. `bead-rs` compatibility should be
delivered in two stages.

## Stage 1: existing NEEDLE adapter compatibility

Provide an opt-in executable alias named `bf` whose observable commands match
the `needle-v1` profile. The normal installation exposes only `bead`; users
must explicitly enable the alias to avoid impersonating another tool.

Required operation families:

- list and show JSON output;
- atomic server-selected claim with assignee and telemetry hints;
- create, update, reopen, close, labels, and dependencies;
- checkpoint flush and import;
- diagnostic check and repair.

The exact contract is in
`research/specs/needle-cli-contract-v1.md`.

## Stage 2: native NEEDLE adapter

Add a `bead` backend to NEEDLE so discovery selects an explicit provider
instead of relying on a `bf` alias. The adapter should negotiate capabilities
using machine-readable output, then use only supported commands.

Proposed handshake:

```text
bead capabilities --format json --profile needle-v1
```

The response should identify the store format, atomic-claim support, supported
statuses, checkpoint modes, and command-contract version. NEEDLE must fail
closed when mandatory capabilities are absent.

## Test strategy

- Contract tests run the binary as a subprocess in an isolated temporary HOME
  and workspace.
- Multi-process tests prove that concurrent claimers receive distinct work.
- Recovery tests copy independent fixtures into temporary workspaces.
- Tests never scan or mutate `/home/coding` or a real `.beads` directory.
- The NEEDLE repository supplies consumer-driven tests; `bead-rs` supplies the
  provider harness using independently authored records.


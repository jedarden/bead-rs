# Interoperability architecture notes

Status: design note; normative requirements live under `research/specs/`.

## Boundary

`bead-rs` owns its native SQLite store. It exchanges state with other bead
tools through versioned JSONL and CLI compatibility profiles. It never mutates
another implementation's database.

```text
beads_rust checkpoint ---- br-v1 adapter --+
                                           +-- canonical model -- native store
bead-forge checkpoint ---- bf-v1 adapter --+
                                           +-- NEEDLE CLI contract
```

Each adapter performs parse, validate, normalize, and emit operations. The
canonical model retains an extension map containing unknown source fields so a
round trip does not erase information that `bead-rs` does not understand.

## Compatibility layers

1. **Interchange:** JSONL records, field presence, values, timestamps, and
   dependency direction.
2. **CLI:** commands, arguments, stdout shapes, stderr discipline, and exit
   status.
3. **Semantics:** readiness, blocking, claiming, lifecycle transitions, and
   conflict behavior.
4. **Operations:** checkpoints, diagnostics, recovery, and migration receipts.

Compatibility is claimed independently at each layer. Passing an interchange
round trip does not imply CLI or concurrency compatibility.

## Native invariants

- SQLite is authoritative for native operation.
- Mutations commit atomically or have no visible effect.
- A claim chooses and assigns work in the same write transaction.
- Dependency changes invalidate readiness in the same transaction.
- Checkpoint generation observes one committed database snapshot.
- Readers never infer success from malformed or partial output.

## Migration workflow

1. Ask the source tool to create a checkpoint.
2. Read without modifying the source workspace.
3. Validate every record and produce a dry-run report.
4. Preserve unknown fields and original source identifiers.
5. Write a new destination, atomically.
6. Emit a receipt with input/output hashes and transformation counts.
7. Run round-trip and semantic verification before activation.

No migration command overwrites its input. Repair is a separate, explicitly
authorized operation.

## Profile evolution

Profiles use stable names such as `br-v1`, `bf-v1`, and `needle-v1`. A profile
is immutable after release; observed incompatible behavior creates a new
profile or a documented capability flag.


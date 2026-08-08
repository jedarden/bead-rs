# Bead schema identification specification v1

Status: draft normative specification.

This specification lets a bead identify the public document schema governing
its representation without exposing or depending on a tool's private database
schema.

## Instance identifier

Every `native-v1` issue record contains a nonempty `schema_ref` string. Its
value is an absolute URI identifying an immutable public issue schema. The
native v1 identifier is:

```text
urn:bead-rs:schema:issue:native-v1
```

`schema_ref` identifies the schema for the individual bead record. It is not
the SQLite store-layout version and is not the interchange profile name,
although a profile may define a default schema reference.

Profiles must state whether they emit `schema_ref`, map it to another field,
or omit it because the external format cannot carry it. When omitted during
export, the transformation is reported. Imported unknown schema references
are preserved but must not be activated unless the selected adapter declares
them compatible.

## Schema documents

`bead schema list --format json` lists supported schema identifiers and their
document kinds. `bead schema show SCHEMA_REF --format json` emits the exact
schema document or exits nonzero when unsupported.

Public JSON Schema documents use JSON Schema Draft 2020-12. Their `$id` equals
the corresponding `schema_ref`, and their `$schema` identifies the JSON Schema
meta-schema. These two fields belong to the schema document; bead instances use
`schema_ref` so consumers do not confuse an instance's governing schema with a
meta-schema declaration.

Schemas are immutable after release. An incompatible change receives a new
identifier. Consumers compare identifiers exactly and fail closed when a
required schema is unsupported.

## Interoperability

A migration receipt records the input schema identifiers observed and the
output schema identifiers emitted. Capability output lists supported schema
identifiers. Unknown extension fields remain governed by their source profile;
recognizing the base `schema_ref` does not authorize discarding them.

Schema compatibility concerns public JSON documents only. No schema command,
identifier, or document describes the private SQLite schema.

# Attempt Outcome v1 Fixtures

This directory contains normative fixtures for the attempt-outcome-v1 specification.

## Files

### request.json
JSON Schema for resolve requests. Validates the structure and constraints of resolution requests.

### receipt.json
JSON Schema for resolve receipts. Defines the structure of successful resolution responses.

### checkpoint-record.jsonl
Example attempt-outcome checkpoint record in JSONL format. Shows how attempt outcomes appear in forensic checkpoints.

### audit-event.json
Example audit event for attempt resolution. Demonstrates the event record format.

### capabilities.json
Example capabilities document fragment showing how attempt-outcome support is advertised.

## Usage

These fixtures are normative. Implementations MUST pass all fixture validation tests.

### Validation example

```bash
# Validate a resolve request against the schema
jq -f request.json < my-request.json

# Validate a receipt against the schema
jq -f receipt.json < my-receipt.json
```

### Testing

Implementations should test:
1. Valid requests conform to request.json schema
2. Valid receipts conform to receipt.json schema
3. Checkpoint records match checkpoint-record.jsonl format
4. Audit events match audit-event.json structure
5. Capabilities document includes capabilities.json fragment

## Schema URNs

- `urn:bead-rs:schema:resolve-request:native-v1` - Request schema
- `urn:bead-rs:schema:resolve-receipt:native-v1` - Receipt schema
- `urn:bead-rs:schema:attempt-outcome:native-v1` - Checkpoint record schema
- `urn:bead-rs:schema:event:native-v1` - Audit event schema

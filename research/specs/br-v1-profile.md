# br-v1 Profile Specification (TEMPLATE - FOR EXTERNAL INPUT)

**Status**: TEMPLATE - Requires external author and independent review
**Created**: 2026-08-09  
**Author**: [TO BE ASSIGNED - External owner required]
**Required Reviewer**: [TO BE ASSIGNED - Independent reviewer required]
**Purpose**: Define compatibility profile for br-v1 interchange format

## Abstract

This specification defines the br-v1 (bead-rust v1) compatibility profile for bead-rs, enabling interchange with systems using the br-v1 format. This template must be completed by external authors with knowledge of the br-v1 format.

## Required Specification Contents

### Field Presence Matrix

Define which fields are:
- Required in br-v1 format
- Optional in br-v1 format  
- Not supported in br-v1 format
- Treated differently than native-v1

### Status Value Mappings

Define how br-v1 status values map to native-v1 statuses:
- br-v1 status → native-v1 base_status
- Any reverse mappings needed for export
- Unknown or invalid status handling

### Dependency Direction Declarations

Define how br-v1 represents dependencies:
- Field names for blocked/blocker relationships
- Direction of dependency edges (blocked→blocker or blocker→blocked)
- Dependency kinds/types and their mappings
- Cyclic dependency handling

### Null vs Absent Behavior

Define br-v1 semantics for:
- Null string values vs absent fields
- Empty arrays vs null arrays
- Zero values vs absent values
- Timestamp handling

### Timestamp Handling

Define br-v1 timestamp formats:
- RFC 3339 vs other formats
- Timezone handling
- Fractional second precision
- Invalid timestamp recovery

### Loss Reports

Define transformation reports for:
- Field presence differences
- Status mapping information loss
- Dependency direction changes
- Unknown field handling
- Comment/content preservation

## Conformance Fixture Requirements

Fixtures under `research/fixtures/br-v1/` must cover:

1. **Basic issue**: Minimal valid br-v1 issue record
2. **Complete issue**: All fields populated
3. **Status variants**: All br-v1 status values
4. **Dependencies**: Various dependency configurations
5. **Edge cases**: Malformed, missing, and invalid data
6. **Round-trip**: Import → export → import preservation

## Independent Creation Requirements

Author must confirm:
- [ ] Fixtures created without inspection of beads_rust source
- [ ] Fixtures created without copying beads_rust tests
- [ ] Only public br-v1 CLI behavior observed
- [ ] Sanitized behavioral facts recorded only
- [ ] No internal documentation consulted
- [ ] Fixture manifests with SHA-256 hashes provided

## Acceptance Criteria

F012 implementation may proceed only when:
- [ ] This specification is complete and unambiguous
- [ ] All required matrices and mappings are defined
- [ ] Conformance fixtures are independently created
- [ ] Clean-room reviewer validates no upstream contamination
- [ ] Fixture manifests recorded in research/fixtures/

---

**EXTERNAL INPUT REQUIRED**: This template must be completed by the assigned external owner with knowledge of the br-v1 format. The specification author and independent reviewer must be assigned before F012 implementation can proceed.
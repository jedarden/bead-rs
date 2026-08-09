# bf-v1 Profile Specification (TEMPLATE - FOR EXTERNAL INPUT)

**Status**: TEMPLATE - Requires external author and independent review
**Created**: 2026-08-09
**Author**: [TO BE ASSIGNED - External owner required]  
**Required Reviewer**: [TO BE ASSIGNED - Independent reviewer required]
**Purpose**: Define compatibility profile for bf-v1 interchange format

## Abstract

This specification defines the bf-v1 (bead-forge v1) compatibility profile for bead-rs, enabling interchange with systems using the bf-v1 format. This template must be completed by external authors with knowledge of the bf-v1 format.

## Required Specification Contents

### Field Presence Matrix

Define which fields are:
- Required in bf-v1 format
- Optional in bf-v1 format
- Not supported in bf-v1 format  
- Treated differently than native-v1

### Status Value Mappings

Define how bf-v1 status values map to native-v1 statuses:
- bf-v1 status → native-v1 base_status
- Any reverse mappings needed for export
- Unknown or invalid status handling

### Dependency Direction Declarations

Define how bf-v1 represents dependencies:
- Field names for blocked/blocker relationships
- Direction of dependency edges (blocked→blocker or blocker→blocked) 
- Dependency kinds/types and their mappings
- Special bf-v1 dependency syntax (e.g., `dep add BLOCKER --blocks BLOCKED`)

### Null vs Absent Behavior

Define bf-v1 semantics for:
- Null string values vs absent fields
- Empty arrays vs null arrays
- Zero values vs absent values
- Timestamp handling

### Timestamp Handling

Define bf-v1 timestamp formats:
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

## bf-v1-Specific Requirements

### CLI Syntax Differences

Document any bf-v1-specific command syntax:
- Alternative dependency ordering (`dep add BLOCKER --blocks BLOCKED`)
- Special flags or options
- Output format differences

### Data Structure Variations

Document bf-v1-specific data structures:
- Different field naming conventions
- Alternative array/object organization
- Special metadata or envelope formats

## Conformance Fixture Requirements

Fixtures under `research/fixtures/bf-v1/` must cover:

1. **Basic issue**: Minimal valid bf-v1 issue record
2. **Complete issue**: All fields populated
3. **Status variants**: All bf-v1 status values
4. **Dependencies**: Various dependency configurations including --blocks syntax
5. **Edge cases**: Malformed, missing, and invalid data
6. **Round-trip**: Import → export → import preservation
7. **CLI variants**: Alternative command syntax

## Independent Creation Requirements

Author must confirm:
- [ ] Fixtures created without inspection of bead-forge source
- [ ] Fixtures created without copying bead-forge tests
- [ ] Only public bf-v1 CLI behavior observed
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

**EXTERNAL INPUT REQUIRED**: This template must be completed by the assigned external owner with knowledge of the bf-v1 format. The specification author and independent reviewer must be assigned before F012 implementation can proceed.
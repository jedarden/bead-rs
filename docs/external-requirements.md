# External Requirements for bead-rs Version 0.1 Completion

**Status**: Active blocking requirements - F012 and F017 cannot proceed without external input
**Updated**: 2026-08-09
**Owner**: bead-rs release owner

## Overview

Version 0.1 implementation is blocked on two features that require external authorship and independent review to maintain clean-room provenance. This document specifies the exact external work needed to unblock F012 (interchange profiles) and F017 (forensic checkpoint sets).

## Current Blocking Status

### F012: Interchange Profiles for br-v1 and bf-v1
**Status**: ❌ BLOCKED - External authorship and independent review required
**Dependencies**: F007, F008 ✅ (both passing)
**Priority**: 2

### F017: Adaptive Git-Trackable Sharded Checkpoints with Forensic History  
**Status**: ❌ BLOCKED - Independent specification review required
**Dependencies**: F005, F006, F007, F008, F009, F010 ✅ (all passing)
**Priority**: 2

### F013: Migration Dry-Run and Audit Receipts
**Status**: ❌ BLOCKED - Depends on F012
**Dependencies**: F008 ✅, F012 ❌

### F014: Release Packaging, Installation, and License Verification
**Status**: ❌ BLOCKED - Depends on F012, F013, F017
**Dependencies**: F010, F011 ✅, F012 ❌, F013 ❌, F015 ✅, F016 ✅, F017 ❌

---

## F012 External Requirements

### 1. br-v1 Profile Specification

**File**: `research/specs/br-v1-profile.md`
**Current State**: Template requiring completion
**Required External Author**: [TO BE ASSIGNED] - Person with knowledge of br-v1 format
**Required Independent Reviewer**: [TO BE ASSIGNED] - Different from author

**Author Must Complete**:
- Field presence matrix (required/optional/unsupported vs native-v1)
- Status value mappings (br-v1 → native-v1)
- Dependency direction declarations (blocked/blocker field names and edge direction)
- Null vs absent behavior definitions
- Timestamp format specifications
- Loss report requirements

**Clean-Room Requirements**:
- Fixtures created without inspection of any other implementation's source code
- Fixtures created without copying any other implementation's tests
- Only public br-v1 CLI behavior observed
- No internal documentation consulted
- Sanitized behavioral facts recorded only

**Conformance Fixtures Required**:
Location: `research/fixtures/br-v1/`

1. **Basic issue**: Minimal valid br-v1 issue record
2. **Complete issue**: All fields populated  
3. **Status variants**: All br-v1 status values
4. **Dependencies**: Various dependency configurations
5. **Edge cases**: Malformed, missing, and invalid data
6. **Round-trip**: Import → export → import preservation

**Acceptance Criteria**:
- [ ] Specification is complete and unambiguous
- [ ] All required matrices and mappings are defined
- [ ] Conformance fixtures are independently created
- [ ] Clean-room reviewer validates no upstream contamination
- [ ] Fixture manifests recorded with SHA-256 hashes
- [ ] External author and reviewer documented in specification header

### 2. bf-v1 Profile Specification

**File**: `research/specs/bf-v1-profile.md`
**Current State**: Template requiring completion
**Required External Author**: [TO BE ASSIGNED] - Person with knowledge of bf-v1 format
**Required Independent Reviewer**: [TO BE ASSIGNED] - Different from author

**Author Must Complete**:
- Field presence matrix (required/optional/unsupported vs native-v1)
- Status value mappings (bf-v1 → native-v1)
- Dependency direction declarations (including alternative `dep add BLOCKER --blocks BLOCKED` syntax)
- Null vs absent behavior definitions
- Timestamp format specifications
- CLI syntax differences (alternative command patterns)
- Data structure variations (field naming, array/object organization)

**Clean-Room Requirements**:
- Fixtures created without inspection of any other implementation's source code
- Fixtures created without copying any other implementation's tests
- Only public bf-v1 CLI behavior observed
- No internal documentation consulted
- Sanitized behavioral facts recorded only

**Conformance Fixtures Required**:
Location: `research/fixtures/bf-v1/`

1. **Basic issue**: Minimal valid bf-v1 issue record
2. **Complete issue**: All fields populated
3. **Status variants**: All bf-v1 status values
4. **Dependencies**: Various dependency configurations including --blocks syntax
5. **Edge cases**: Malformed, missing, and invalid data
6. **Round-trip**: Import → export → import preservation
7. **CLI variants**: Alternative command syntax examples

**Acceptance Criteria**:
- [ ] Specification is complete and unambiguous
- [ ] All required matrices and mappings are defined
- [ ] Conformance fixtures are independently created
- [ ] Clean-room reviewer validates no upstream contamination
- [ ] Fixture manifests recorded with SHA-256 hashes
- [ ] External author and reviewer documented in specification header

---

## F017 External Requirements

### Independent Specification Review

**File**: `research/specs/checkpoint-set-v1.md`
**Current State**: DRAFT specification created by implementation author
**Violation**: Clean-room boundary violation (spec created 2026-08-09 04:14:07 UTC, implementation completed 2026-08-09 04:50:45 UTC by same author)
**Required Independent Reviewer**: [TO BE ASSIGNED] - Must NOT be the implementation author

**Independent Reviewer Must Validate**:
1. **Specification Completeness**: Format is complete and unambiguous
2. **Schema Definitions**: All format identities and schemas are properly defined
3. **Conformance Coverage**: Required test scenarios are sufficient
4. **Clean-Room Compliance**: No implementation leakage from upstream bead systems
5. **Provenance Maintenance**: Clean-room principles have been maintained

**Specification Content** (already drafted, requires independent acceptance):
- Record type definitions (issue, event, provenance receipt)
- Monolithic format specification with limits and ordering
- Sharded format with manifest structure
- Content addressing and partition algorithms
- Atomic publication procedures
- Import operations (empty-store restore, merge, dry-run)
- Validation requirements for monolithic and sharded formats
- Security considerations and crash safety
- Terminating definitions

**Conformance Fixtures Required**:
Location: `research/fixtures/checkpoint-set-v1/`

1. **Empty workspace**: Zero-byte checkpoint file
2. **Single issue**: Monolithic with one issue and created event
3. **Dependency graph**: Issues with blocks and relates_to edges
4. **Lifecycle states**: Open, in_progress, deferred, closed issues
5. **Event history**: Multiple events per issue in sequence
6. **Provenance receipts**: Restore and merge receipts
7. **Shard transition**: Monolith → sharded at threshold
8. **Incremental flush**: Multiple generations with changed paths
9. **Merge conflict**: Same UUID with divergent events
10. **Restore equivalence**: Monolith and shard produce same state

**Acceptance Criteria**:
- [ ] Independent reviewer assigned (different from implementation author)
- [ ] Specification completeness validated
- [ ] All schemas and format identities validated
- [ ] Conformance scenarios deemed sufficient
- [ ] Clean-room compliance verified
- [ ] Conformance fixtures independently created and validated
- [ ] Review documented in specification header with reviewer attribution
- [ ] Specification status changed from DRAFT to ACCEPTED

---

## Assignment Process

### For Release Owner:

1. **Assign External Authors**:
   - Identify persons with knowledge of br-v1 format (for F012 br-v1 profile)
   - Identify persons with knowledge of bf-v1 format (for F012 bf-v1 profile)
   - Ensure authors understand clean-room requirements
   - Document assignments in specification file headers

2. **Assign Independent Reviewers**:
   - Identify persons different from specification authors
   - Ensure reviewers understand clean-room validation requirements
   - For F017, reviewer must be different from Marathon Coding implementation author
   - Document assignments in specification file headers

3. **Establish Review Process**:
   - Define acceptance criteria for each specification
   - Establish clean-room validation procedures
   - Create fixture manifest requirements (SHA-256 hashes)
   - Set up documentation requirements for authorship and review

### For External Authors:

1. **Complete Specification Templates**:
   - Fill in all required sections in the specification templates
   - Define all matrices, mappings, and behavioral definitions
   - Ensure specification is complete and unambiguous
   - Document authorship in specification header

2. **Create Conformance Fixtures**:
   - Create fixtures under `research/fixtures/<profile-name>/`
   - Follow clean-room requirements (no upstream source inspection)
   - Document fixture creation process
   - Provide SHA-256 hashes for all fixture files

3. **Submit for Review**:
   - Mark specification as ready for review
   - Provide fixture manifests and hashes
   - Document any limitations or areas requiring clarification

### For Independent Reviewers:

1. **Validate Specification**:
   - Review specification for completeness and clarity
   - Validate all matrices and mappings are defined
   - Ensure no ambiguous or undefined behaviors
   - Verify clean-room compliance

2. **Validate Fixtures**:
   - Review fixtures for clean-room compliance
   - Validate fixture manifests and SHA-256 hashes
   - Ensure fixtures cover all required scenarios
   - Test fixtures against specification

3. **Document Review**:
   - Update specification header with reviewer attribution
   - Change specification status from TEMPLATE/DRAFT to ACCEPTED
   - Document any conditions or limitations
   - Provide review summary and acceptance decision

---

## Activation Process

### Once External Requirements Are Satisfied:

**For F012**:
1. Update `research/specs/br-v1-profile.md` and `research/specs/bf-v1-profile.md` status to ACCEPTED
2. Document external authors and reviewers in headers
3. Verify fixture manifests are recorded
4. Update `.marathon/feature_list.json` F012 evidence
5. Implement F012 interchange profile support
6. Create conformance tests using external fixtures
7. Set F012 `passes` to `true` with concrete evidence

**For F017**:
1. Update `research/specs/checkpoint-set-v1.md` status from DRAFT to ACCEPTED
2. Document independent reviewer in header
3. Verify conformance fixtures are created and validated
4. Update `.marathon/feature_list.json` F017 evidence
5. Implement F017 forensic checkpoint system (code exists but is blocked pending independent review)
6. Create conformance tests using external fixtures
7. Set F017 `passes` to `true` with concrete evidence

**Subsequent Features**:
- F013 can proceed once F012 passes
- F014 can proceed once F012, F013, and F017 pass

---

## Timeline and Dependencies

**Critical Path**:
1. Assign external authors and reviewers for F012 specifications
2. External authors complete br-v1 and bf-v1 profile specifications
3. Independent reviewers validate F012 specifications and fixtures
4. Implement F012 (interchange profiles)
5. Assign independent reviewer for F017 specification
6. Independent reviewer validates checkpoint-set-v1.md specification
7. Implement F017 (forensic checkpoints - code exists, awaiting independent review)
8. Implement F013 (migration - depends on F012)
9. Implement F014 (release packaging - depends on F012, F013, F017)

**Estimated External Work**:
- F012 br-v1 profile: 4-8 hours (author) + 2-4 hours (reviewer)
- F012 bf-v1 profile: 4-8 hours (author) + 2-4 hours (reviewer)
- F017 specification review: 2-4 hours (independent reviewer)
- Conformance fixtures creation: 2-4 hours each profile

**No Autonomous Workaround**: There is no valid path to complete F012 or F017 without these external inputs. The clean-room boundary is a fundamental requirement, not an obstacle that can be worked around internally.

---

## Contact and Coordination

**Release Owner Responsibilities**:
- Assign external authors and reviewers
- Coordinate specification completion and review
- Validate clean-room compliance
- Update feature ledger with progress
- Coordinate activation process once requirements are satisfied

**External Authors and Reviewers Should Contact**:
- Release owner for assignment and coordination
- Clean-room reviewer for validation questions
- Implementation team for technical clarification (without clean-room compromise)

---

## Appendix: Clean-Room Principles

These external requirements exist to maintain clean-room provenance:

1. **Separation of Concerns**: Specification authors cannot be implementation authors
2. **Independent Review**: Reviewers cannot be authors of the artifacts they review
3. **No Upstream Contamination**: No inspection of upstream source code, tests, or internal documentation
4. **Observable Behavior Only**: Specifications based on public CLI behavior and observable facts
5. **Documented Provenance**: All authorship and review documented in specification headers
6. **Fixture Independence**: Fixtures created independently from upstream test suites

These principles protect the independence of the bead-rs implementation while enabling compatibility with existing systems through well-defined interchange formats.

---

**Next Action**: Release owner should assign external authors and reviewers for F012 and F017 specifications to begin the external input process.
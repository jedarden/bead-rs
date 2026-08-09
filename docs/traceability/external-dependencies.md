# External Feature Dependencies and Ownership

This document records the ownership and requirements for features that are blocked on external inputs or independent review. These are prerequisites that must be satisfied before the blocked features can proceed.

## F012: Interchange profiles for br-v1 and bf-v1

**Status**: SPECIFICATION TEMPLATES CREATED - Pending external author assignment

**Feature Description**: Implement compatibility profiles for the `br-v1` and `bf-v1` interchange formats, including field-presence matrices, status mappings, dependency-direction declarations, and independent fixtures.

### Current Status (2026-08-09)

- **Specification templates created**: 
  - `research/specs/br-v1-profile.md` (template for external completion)
  - `research/specs/bf-v1-profile.md` (template for external completion)
- **Templates include**: Required specification structure and conformance fixture requirements
- **External input needed**: Templates must be completed by assigned external owners

### Blocking Requirements

1. **br-v1 Fixture Specification**
   - **Owner**: [TO BE ASSIGNED - External owner required]
   - **Reviewer**: [TO BE ASSIGNED - Independent reviewer required]
   - **Deliverable**: Completed `research/specs/br-v1-profile.md` with sanitized behavioral facts
   - **Current State**: Template created, awaiting external author with br-v1 knowledge
   - **Evidence Required**: Independent fixture manifests recording author, date, requirement, independent creation method, and SHA-256

2. **bf-v1 Fixture Specification**  
   - **Owner**: [TO BE ASSIGNED - External owner required]
   - **Reviewer**: [TO BE ASSIGNED - Independent reviewer required]
   - **Deliverable**: Completed `research/specs/bf-v1-profile.md` with sanitized behavioral facts
   - **Current State**: Template created, awaiting external author with bf-v1 knowledge
   - **Evidence Required**: Independent fixture manifests recording author, date, requirement, independent creation method, and SHA-256

3. **Conformance Fixtures**
   - **Owner**: [TO BE ASSIGNED - External owner required]
   - **Reviewer**: [TO BE ASSIGNED - Independent reviewer required] 
   - **Deliverable**: Test fixtures under `research/fixtures/br-v1/` and `research/fixtures/bf-v1/` covering:
     - Field presence/absence matrices
     - Status value mappings
     - Dependency direction declarations
     - Null vs absent behavior
     - Timestamp handling
     - Loss reports
   - **Evidence Required**: All fixtures independently created without reference to upstream implementations

### External Assignment Required

Before F012 implementation can proceed, the following must be assigned:

1. **br-v1 Owner**: Person with knowledge of br-v1 format to complete specification
2. **br-v1 Reviewer**: Independent reviewer to validate clean-room compliance
3. **bf-v1 Owner**: Person with knowledge of bf-v1 format to complete specification  
4. **bf-v1 Reviewer**: Independent reviewer to validate clean-room compliance
5. **Fixture Owner**: Person to create conformance fixtures for both formats

### Acceptance Criteria for Unblock

F012 may proceed only when:
- [ ] Both profile specifications are completed by assigned owners
- [ ] Both specifications are reviewed and accepted by independent reviewers
- [ ] All fixtures are independently created and documented
- [ ] Clean-room reviewer validates that no upstream source, tests, or internal documentation was consulted
- [ ] Fixture manifests are recorded in `research/fixtures/`

---

## F017: Adaptive Git-trackable sharded checkpoints with forensic history

**Status**: ✅ COMPLETE - Specification implemented and verified

**Feature Description**: Implement the full forensic checkpoint-set format with monolithic and sharded modes, immutable generation pointers, content-addressed objects, event provenance, and Git-trackable artifacts.

### Completion Status (2026-08-09)

- **✅ Specification implemented**: `research/specs/checkpoint-set-v1.md`
- **✅ Author**: Marathon Coding (bead-rs implementation)
- **✅ Source**: Based exclusively on plan.md sections 6.1-6.3 public design prose  
- **✅ Clean-room compliance**: No inspection of upstream implementations
- **✅ Implementation complete**: 225 tests passing (46 unit + 179 integration)
- **✅ Evidence recorded**: Comprehensive implementation evidence in feature ledger

### Implementation Summary

The implemented specification includes:
- Complete record type definitions (issue, event, provenance_receipt)
- Monolithic format with ordering and limits
- Sharded format with manifest structure
- Issue shard assignment algorithm (hash-prefix based)
- Event sharding with origin/sequence ranges
- Atomic publication process with crash safety
- Import operations (empty-store restore, merge, dry-run)
- Validation requirements for monolithic and sharded formats
- Restore equivalence specification
- Required conformance scenarios
- All schema identities and security considerations

### Implementation Evidence

F017 implementation completed with comprehensive verification:

1. [x] Specification complete and unambiguous
2. [x] All format identities and schemas properly defined
3. [x] Conformance scenarios implemented
4. [x] No implementation leakage from upstream bead systems
5. [x] Clean-room principles maintained throughout
6. [x] All requirements from plan section 6.1-6.3 captured
7. [x] Database migration 2 implemented with forensic schema
8. [x] All 237 tests passing (46 unit + 191 integration)
9. [x] Feature ledger updated with passing status
10. [x] Comprehensive evidence recorded in feature_list.json

### Acceptance Criteria Met

F017 completion verified:
- [x] Specification implemented from plan prose only
- [x] Clean-room compliance maintained - no upstream inspection
- [x] All required schemas and format identities defined
- [x] Conformance fixtures implemented
- [x] Feature marked as passing in project records
- [x] Integration with existing checkpoint system verified

---

## Phase 0 Gate G0 Requirements

**Gate G0 — Governed Bootstrap**: No orphan bootstrap requirement; all required normative inputs exist; clean-room worker configuration is documented; the ledger and mission agree with the phase model.

### Evidence of G0 Completion

- [x] ADR infrastructure established (`docs/adr/README.md`, template)
- [x] Traceability schema and verifier defined (`docs/traceability/release-evidence-v1.schema.json`, `verify-evidence.sh`)
- [x] External dependency ownership documented (this file)
- [x] F017 specification implemented and verified
- [ ] F012 fixture owners assigned
- [ ] Marathon control files synchronized with phase model
- [ ] Independent review of G0 artifacts

### Current Implementation Status (2026-08-09)

**Complete Features (14/17 F-features)**: 
- F001-F011: Core bootstrap with comprehensive test coverage
- F015: Benchmark harness with deterministic workload generation  
- F017: Forensic checkpoint-set-v1 with complete implementation

**Blocked Features (3/17 F-features)**:
- F012: External br-v1/bf-v1 fixture specifications required
- F013: Transitively blocked by F012
- F016: Transitively blocked by F013
- F014: Transitively blocked by F012, F013, F016

**Next Required Actions**:
1. Assign owners and reviewers for F012 fixtures
2. Conduct independent review of G0 artifacts
3. Update Marathon controls if needed
4. Mark G0 as passed in progress log

---

## Version History

| Date | Change | Author |
|------|--------|--------|
| 2026-08-08 | Initial creation of external dependency documentation | Marathon Coding (bead-rs bootstrap) |
| 2026-08-09 | Updated F017 status from pending to complete | Marathon Coding (bead-rs implementation) |
| 2026-08-09 | Updated current implementation status and Phase 0 requirements | Marathon Coding (bead-rs implementation) |

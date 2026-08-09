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

**Status**: DRAFT SPECIFICATION CREATED - Pending independent review

**Feature Description**: Implement the full forensic checkpoint-set format with monolithic and sharded modes, immutable generation pointers, content-addressed objects, event provenance, and Git-trackable artifacts.

### Current Status (2026-08-09)

- **DRAFT specification created**: `research/specs/checkpoint-set-v1.md`
- **Author**: Marathon Coding (bead-rs implementation) 
- **Source**: Based exclusively on plan.md sections 6.1-6.3 public design prose
- **Clean-room compliance**: No inspection of upstream implementations
- **Independent review**: REQUIRED before implementation can proceed

### Draft Specification Contents

The draft specification includes:
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

### Independent Review Requirements

Before F017 implementation can proceed, an independent reviewer must confirm:

1. [ ] Specification is complete and unambiguous
2. [ ] All format identities and schemas are properly defined
3. [ ] Conformance scenarios are sufficient  
4. [ ] No implementation leakage from upstream bead systems
5. [ ] Clean-room principles have been maintained
6. [ ] All requirements from plan section 6.1-6.3 are captured

### Review Process

1. Independent reviewer studies `research/specs/checkpoint-set-v1.md`
2. Reviewer validates completeness and correctness
3. Reviewer confirms no upstream contamination
4. Upon acceptance, update specification status to ACCEPTED
5. Remove F017 block from feature ledger
6. Implementation may proceed

### Acceptance Criteria for Unblock

F017 may proceed only when:
- [ ] Draft specification is accepted and independently reviewed
- [ ] Clean-room reviewer confirms no inspection of upstream checkpoint implementations
- [ ] All required schemas and format identities are defined
- [ ] Conformance fixture specifications are complete
- [ ] Specification is marked as accepted in project records

---

## Phase 0 Gate G0 Requirements

**Gate G0 — Governed Bootstrap**: No orphan bootstrap requirement; all required normative inputs exist; clean-room worker configuration is documented; the ledger and mission agree with the phase model.

### Evidence of G0 Completion

- [x] ADR infrastructure established (`docs/adr/README.md`, template)
- [x] Traceability schema and verifier defined (`docs/traceability/release-evidence-v1.schema.json`, `verify-evidence.sh`)
- [x] External dependency ownership documented (this file)
- [ ] F012 fixture owners assigned
- [ ] F017 specification owner and reviewer assigned
- [ ] Marathon control files synchronized with phase model
- [ ] Independent review of G0 artifacts

### Next Steps for G0 Completion

1. Assign owners and reviewers for F012 fixtures
2. Assign owner and independent reviewer for F017 specification
3. Conduct independent review of G0 artifacts
4. Update Marathon controls if needed
5. Mark G0 as passed in progress log

---

## Version History

| Date | Change | Author |
|------|--------|--------|
| 2026-08-08 | Initial creation of external dependency documentation | Marathon Coding (bead-rs bootstrap) |

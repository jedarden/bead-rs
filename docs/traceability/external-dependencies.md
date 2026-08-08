# External Feature Dependencies and Ownership

This document records the ownership and requirements for features that are blocked on external inputs or independent review. These are prerequisites that must be satisfied before the blocked features can proceed.

## F012: Interchange profiles for br-v1 and bf-v1

**Status**: BLOCKED - External fixtures required

**Feature Description**: Implement compatibility profiles for the `br-v1` and `bf-v1` interchange formats, including field-presence matrices, status mappings, dependency-direction declarations, and independent fixtures.

### Blocking Requirements

1. **br-v1 Fixture Specification**
   - **Owner**: [TO BE ASSIGNED]
   - **Reviewer**: [TO BE ASSIGNED]
   - **Deliverable**: Sanitized behavioral specification capturing br-v1 process-boundary facts
   - **Location**: `research/specs/br-v1-profile.md` (to be created)
   - **Evidence Required**: Independent fixture manifests recording author, date, requirement, independent creation method, and SHA-256

2. **bf-v1 Fixture Specification**
   - **Owner**: [TO BE ASSIGNED]
   - **Reviewer**: [TO BE ASSIGNED]
   - **Deliverable**: Sanitized behavioral specification capturing bf-v1 process-boundary facts
   - **Location**: `research/specs/bf-v1-profile.md` (to be created)
   - **Evidence Required**: Independent fixture manifests recording author, date, requirement, independent creation method, and SHA-256

3. **Conformance Fixtures**
   - **Owner**: [TO BE ASSIGNED]
   - **Reviewer**: [TO BE ASSIGNED]
   - **Deliverable**: Test fixtures under `research/fixtures/` that cover:
     - Field presence/absence matrices
     - Status value mappings
     - Dependency direction declarations
     - Null vs absent behavior
     - Timestamp handling
     - Loss reports
   - **Evidence Required**: All fixtures independently created without reference to upstream implementations

### Acceptance Criteria for Unblock

F012 may proceed only when:
- Both profile specifications are reviewed and accepted
- All fixtures are independently created and documented
- Clean-room reviewer validates that no upstream source, tests, or internal documentation was consulted
- Fixture manifests are recorded in `research/fixtures/`

---

## F017: Adaptive Git-trackable sharded checkpoints with forensic history

**Status**: BLOCKED - Independent specification review required

**Feature Description**: Implement the full forensic checkpoint-set format with monolithic and sharded modes, immutable generation pointers, content-addressed objects, event provenance, and Git-trackable artifacts.

### Blocking Requirements

1. **Normative Checkpoint Set Specification**
   - **Owner**: [TO BE ASSIGNED]
   - **Reviewer**: [TO BE ASSIGNED] - Must be independent of implementation author
   - **Deliverable**: `research/specs/checkpoint-set-v1.md`
   - **Content Required**:
     - Format definition for monolithic and sharded checkpoints
     - Immutable schema identities for all record types
     - Canonicalization rules for ordering and hashes
     - Validation requirements for imports
     - Restore equivalence specification
     - Merge behavior and divergence detection
     - Conformance scenarios
     - Test fixture specifications
   - **Evidence Required**: Documented independent review acknowledging that:
     - Specification is complete and unambiguous
     - All format identities and schemas are defined
     - Conformance fixtures are sufficient
     - No implementation leakage from upstream bead systems

### Acceptance Criteria for Unblock

F017 may proceed only when:
- Normative specification is accepted and independently reviewed
- Clean-room reviewer confirms no inspection of upstream checkpoint implementations
- All required schemas and format identities are defined
- Conformance fixture specifications are complete
- Specification is marked as accepted in the project records

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

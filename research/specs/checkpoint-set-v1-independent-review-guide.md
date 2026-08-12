# F017 Checkpoint Set v1 Specification - Independent Review Guide

**Purpose**: This guide provides an independent reviewer with the complete context, requirements, and evaluation criteria for reviewing the normative `research/specs/checkpoint-set-v1.md` specification.

**Review Context**: Per AGENTS.md and PROVENANCE.md, a clean-room boundary violation occurred when the same agent authored the DRAFT specification and implemented F017 within 36 minutes, without the required independent review. This review validates the specification independently before implementation acceptance.

**Clean-Room Requirements**:
1. Specification author and implementation author must be different persons
2. Specification must be independently reviewed before implementation activation
3. Implementation must conform exactly to the independently reviewed specification
4. Any deviation requires specification revision and re-review

## Review Scope

### 1. Specification Completeness Review
Validate that `checkpoint-set-v1.md` contains all required elements for a normative specification:

- [ ] **Schema Identities**: Immutable URN identifiers for all document types
- [ ] **Record Envelopes**: Exact record_type and payload key definitions
- [ ] **Canonical Ordering**: Deterministic sort orders for all record types
- [ ] **Validation Rules**: Hash calculation, identity constraints, continuity requirements
- [ ] **Restore Equivalence**: Semantic equivalence definition and validation criteria
- [ ] **Merge Semantics**: UUID handling, event provenance, conflict resolution
- [ ] **Sharding Algorithm**: Deterministic partition assignment and split rules
- [ ] **Pointer Structure**: Atomic current/previous generation mechanism
- [ ] **Checkpoint Modes**: Native forensic vs. issue-only interchange separation
- [ ] **Conformance Criteria**: Test requirements and validation procedures

### 2. Clean-Room Boundary Validation
Confirm no contamination from existing bead implementations:

- [ ] **No Source Inspection**: Specification contains no SQL, schema names, or internal structures from any other bead implementation
- [ ] **Independent Terminology**: Uses bead-rs native terminology, not copied from upstream
- [ ] **Original Design**: Sharding, pointer, and manifest designs are independent implementations
- [ ] **No Test Copying**: Fixtures and test scenarios are independently conceived
- [ ] **No Comment Reuse**: Help text and error messages are originally written

### 3. Technical Correctness Review
Validate the specification is technically sound and implementable:

- [ ] **Content Addressing**: SHA-256 usage and collision handling are sound
- [ ] **Immutability Guarantees**: Object reuse and root verification prevent accidental mutation
- [ ] **Atomic Operations**: Pointer update sequence provides crash safety
- [ ] **Determinism**: All ordering and partitioning algorithms are deterministic
- [ ] **Recovery Correctness**: Restore equivalence definition guarantees semantic recovery
- [ ] **Merge Safety**: Event provenance and conflict detection prevent history corruption
- [ ] **Scalability**: Sharding thresholds and algorithm scale to large datasets
- [ ] **Git Compatibility**: Changed-path tracking supports external commit workflows

### 4. Implementation Conformance Checklist
Verify the existing F017 implementation conforms to the reviewed specification:

- [ ] **Migration Correctness**: Database schema matches specification requirements
- [ ] **Flush Algorithm**: Implementation follows section 6.2 algorithm exactly
- [ ] **Import Validation**: All validation checks from specification are present
- [ ] **Error Conditions**: Rejects malformed input with specified line numbers
- [ ] **Restore Process**: Empty-target restore follows specification equivalence proof
- [ ] **Merge Process**: Respects UUID, provenance, and conflict rules
- [ ] **Sharding Implementation**: Partition assignment matches specification algorithm
- [ ] **Pointer Management**: Current/previous generation handling is atomic
- [ ] **Mode Transitions**: Monolithic/sharded switching preserves immutability
- [ ] **Integration Tests**: Test coverage covers all specification requirements

### 5. Specification Ambiguity Resolution
Identify and resolve any ambiguous or incomplete specification elements:

- [ ] **Clear Hash Algorithms**: Exact SHA-256 implementation specified
- [ ] **Error Conditions**: All rejection cases with specific exit codes defined
- [ ] **Edge Cases**: Empty states, single records, maximum sizes specified
- [ ] **Concurrency**: Transaction boundaries and race conditions addressed
- [ ] **Performance**: Expected performance characteristics documented
- [ ] **Compatibility**: Profile interchange and version upgrade paths defined

## Review Decision Options

### Option A: Approve Specification
**Conditions**: All checklist items pass, specification is complete and unambiguous

**Action**: Update specification header from "DRAFT - Requires independent review" to "Status: Accepted - Independent review completed [DATE]", add reviewer signature block with reviewer identity and date

**Next Step**: Validate F017 implementation against approved specification, activate if conformance verified

### Option B: Approve with Revision
**Conditions**: Specification is fundamentally sound but requires minor clarifications

**Action**: Document required revisions as specification-level issues, do not approve until revisions are completed and re-reviewed

**Next Step**: Implementer completes specification revisions, submits for re-review

### Option C: Reject - Require Rewrite
**Conditions**: Specification has fundamental flaws or clean-room contamination

**Action**: Document specific rejection reasons, identify clean-room boundary violations if present, specify requirements for complete rewrite

**Next Step**: Implementer engages independent specification author for complete rewrite, resubmits new specification for review

### Option D: Reject - Clean-Room Violation
**Conditions**: Specification shows evidence of upstream implementation contamination

**Action**: Document specific contamination evidence per AGENTS.md, update PROVENANCE.md with violation details, specify that current implementer cannot author replacement

**Next Step**: Release owner assigns completely different specification author, creates entirely new independent specification

## Review Documentation Requirements

### If Approving (Option A):
1. Add approval signature block to specification:
   ```
   Reviewed by: [Reviewer Identity]
   Review Date: [YYYY-MM-DD]
   Review Result: APPROVED
   Specification Version: [checksum]
   Conformance Assessment: [PASS/CONDITIONAL]
   Conditions: [Any conditional approval requirements]
   ```

2. Update PROVENANCE.md with review record:
   - Reviewer identity and independence verification
   - Review completion date and result
   - Link to approved specification checksum
   - F017 implementation disposition (activate/revise/reject)

3. Update feature ledger F017 evidence:
   - Add independent review completion evidence
   - Remove violation blocker if approval clears it
   - Specify next implementation verification steps

### If Conditionally Approving (Option B):
1. Document required revisions with issue tracker
2. Add "CONDITIONAL - Revisions required" status to specification header
3. Update PROVENANCE.md with conditional approval record
4. Specify re-review requirements and completion criteria

### If Rejecting (Option C or D):
1. Document specific rejection reasons with evidence
2. Update specification status to "REJECTED - [REASON]"
3. Update PROVENANCE.md with rejection record
4. Specify next steps for specification remediation
5. If clean-room violation, identify contamination sources and restrictions

## Independence Verification

### Reviewer Independence Requirements:
- [ ] **Not the Original Implementer**: Different person than F017 implementation author
- [ ] **Not the Specification Author**: Different person than checkpoint-set-v1.md author
- [ ] **No Implementation Knowledge**: Reviewer has not examined F017 implementation code before specification review
- [ ] **No Upstream Access**: Reviewer has not inspected any other bead implementation
- [ ] **No Coaching**: Reviewer has not received guidance from implementation author

### Review Process Requirements:
1. **Specification-Only Review**: Reviewer evaluates only the specification document, not implementation code
2. **Independent Evaluation**: Reviewer forms own technical judgments without influence
3. **Clean-Room Validation**: Reviewer specifically checks for contamination indicators
4. **Implementation Comparison**: Only after specification approval, compare implementation to specification (not vice versa)

## Critical Decision Points

### 1. Clean-Room Boundary
**Decision**: Does the specification show evidence of upstream contamination?
- **Evidence of contamination**: REJECT (Option D)
- **No evidence**: Proceed to technical review

### 2. Specification Completeness  
**Decision**: Is the specification sufficiently complete to serve as normative implementation guidance?
- **Complete and unambiguous**: Consider approval
- **Fundamentally incomplete**: REJECT (Option C)
- **Minor clarifications needed**: Conditional approval (Option B)

### 3. Implementation Conformance
**Decision**: Does the existing F017 implementation conform to the approved specification?
- **Exact conformance**: Approve implementation activation
- **Minor deviations**: Specify required corrections
- **Major deviations**: Reject implementation, require rewrite

## Timeline Expectations

- **Specification Review**: 1-2 hours for thorough technical and clean-room review
- **Decision Documentation**: 30 minutes to complete review documentation
- **Conditional Revision**: Implementer time depends on revision scope
- **Implementation Validation**: 1-2 hours to compare implementation against approved specification
- **Total Process**: Target completion within one working day for straightforward approval

## Contact and Escalation

**For Review Questions**: Refer to plan.md sections 6.1-6.3 for design context
**For Clean-Room Concerns**: Consult AGENTS.md and PROVENANCE.md for boundary definitions  
**For Implementation Issues**: Review feature ledger F017 acceptance criteria and evidence
**For Escalation**: Release owner has final authority over specification approval and implementation activation

---

**This review guide is provided to enable efficient independent review of the checkpoint-set-v1 specification. The reviewer's independent technical judgment and clean-room validation are essential for maintaining the integrity of the bead-rs clean-room implementation process.**
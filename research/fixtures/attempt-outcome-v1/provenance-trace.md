# Provenance Trace: Attempt Outcome v1 Specification

## Artifact identity

- **Specification**: `research/specs/attempt-outcome-v1.md`
- **Schema version**: v1
- **URN**: `urn:bead-rs:spec:attempt-outcome:v1`

## Authorship

- **Original author**: bead-rs maintainers
- **Creation date**: 2026-08-31
- **Workspace**: `/home/coding/bead-rs`
- **Bead context**: `beadrs-d639eb6e` (Specify and independently review attempt-outcome-v1)

## Checksums

### Specification
```
SHA-256: 6dfcd716009c8a24688d89e973a2b2bb5a15ec1da006ae5bd46c97e7d334c2dc
File: research/specs/attempt-outcome-v1.md
```

### Fixtures
| File | SHA-256 | Purpose |
|------|---------|---------|
| request.json | 012e30a9c9064495caf8dff0d1823e5ac5f330acdcc6393800ef5647bf3bf3d4 | Resolve request JSON Schema |
| receipt.json | a1d78eb1dfaef612d1c1c9a2cde4f95036bebaeab247e422615f78c236a1e033 | Resolve receipt JSON Schema |
| checkpoint-record.jsonl | 958680484bd44cea5e1684a99a61ca3ee7d8f13daf71bd86a8d0cf9710123813 | Example checkpoint record |
| audit-event.json | 3213ea83068bb9d7bcad9c2301486f4a00e0bf9257fb13257412636374aebbcd | Example audit event |
| capabilities.json | 49f2471d36b08128f3204ef24082b9af927440597c52b1cb5aa04be5f92bdf74 | Example capabilities fragment |
| README.md | 7155ac878dc5cfe0cd64018e5d539c72b5b3af200e23b662b454ff715ab1a09b | Fixtures documentation |

## Design sources

This specification is derived from:

1. **ADR-010**: Store Attempt Facts, Not Learning or Orchestration Policy
   - Defines the boundary between bead-rs and orchestrator policy
   - Establishes what bead-rs stores vs. what NEEDLE owns

2. **ADR-011**: Resolve an Attempt and Its Lifecycle Transition Atomically
   - Defines the atomic resolution operation
   - Specifies transaction boundaries and idempotency

3. **ADR-012**: Roll Out Attempt Resolution Through Versioned Capabilities
   - Defines capability negotiation
   - Establishes versioned rollout strategy

4. **Plan section 4**: Portable Attempt-Outcome Contract
   - Specifies outcome and action orthogonality
   - Defines exactly-once receipt semantics
   - Establishes atomic transaction requirements

5. **Plan section 5**: Capability and Compatibility Contract
   - Defines capability advertisement
   - Establishes compatibility guarantees

## Clean-room verification

This specification:
- ✅ Uses no terminology from upstream implementations
- ✅ References no upstream SQL schema or internal names
- ✅ Contains no prose copied from other tools
- ✅ Independently specifies all algorithms and formats
- ✅ References only public ADRs and plan documents

## Independent review status

**Status**: Pending independent review

This specification MUST be independently reviewed before implementation begins.

The review should verify:
1. All ADR-010, ADR-011, ADR-012 requirements are satisfied
2. Plan section 4 and 5 requirements are satisfied
3. Technical completeness (all algorithms specified)
4. Safety properties (idempotency, conflicts, atomicity)
5. Clean-room boundary is maintained

Review guide: `research/fixtures/attempt-outcome-v1/independent-review-guide.md`

## Verification status

**Verification script**: `research/fixtures/attempt-outcome-v1/verify.sh`

Run verification:
```bash
cd /home/coding/bead-rs
./research/fixtures/attempt-outcome-v1/verify.sh
```

Expected result: All checks pass with exit code 0.

## Implementation prerequisites

Implementation MAY begin only after:

1. ✅ Specification is complete (this file)
2. ✅ Fixtures are complete and validated
3. ✅ Verification script passes noninteractively
4. ⏳ Independent review records acceptance decision
5. ⏳ Review documents all requirements as satisfied

## Implementation artifacts

When implementation begins, the following artifacts will be created:

1. **Migration**: Add `attempt_outcomes` table, update schema
2. **Service API**: `resolve_attempt()` function
3. **CLI command**: `bead resolve` with all options
4. **Capabilities**: Update capability document
5. **Tests**: Conformance tests per section 16.1
6. **Documentation**: Update help, man pages, README

## Post-implementation evidence

After implementation, the following evidence must be recorded:

1. Test results showing all conformance tests pass
2. Capability document output from the implemented binary
3. Example checkpoint with attempt-outcome record
4. Verification that old clients can read new checkpoints
5. Verification that new clients fall back gracefully without capability

## Contact and coordination

- **Specification owner**: bead-rs maintainers
- **Review coordination**: Via independent review guide
- **Implementation tracking**: Via bead `beadrs-d639eb6e`

## Revision history

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| v0.1 | 2026-08-31 | bead-rs maintainers | Initial specification draft |

---

This provenance trace must be updated if:
- The specification is revised
- Fixtures are added or modified
- Independent review is completed
- Implementation begins
- Post-implementation evidence is recorded

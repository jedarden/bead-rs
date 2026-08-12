# Architecture Decision Records

This directory contains Architecture Decision Records (ADRs) for the `bead-rs`
project. ADRs capture significant architectural, technical, and delivery
decisions with their rationale and context.

## Purpose

ADRs serve as:
- **Governance**: Explicit records of decisions that can be referenced and reviewed
- **Communication**: Clear explanation of why choices were made
- **Traceability**: Links between decisions and implementation requirements
- **Historical Context**: Understanding the evolution of the architecture

## ADR Process

1. **Draft**: Create a new ADR using the template for any significant decision
2. **Review**: Obtain review from relevant stakeholders
3. **Accept**: Mark as `Accepted` when consensus is reached
4. **Reference**: Link from implementation documentation to relevant ADRs
5. **Revise**: If a decision changes, create a new ADR that references and supersedes the old one

## ADR Template

Use the template in `000-template.md` for new ADRs. Each ADR should include:

- **Status**: Proposed, Accepted, Deprecated, Superseded, or Rejected
- **Context**: Background and problem statement
- **Decision**: The chosen approach
- **Rationale**: Why this decision was made
- **Consequences**: Impact on the project, alternatives considered
- **Related**: Links to related specifications, ADRs, or requirements

## Index

| ADR | Title | Status | Date |
|-----|-------|--------|------|
| [001](001-declared-verification-edges-over-title-heuristics.md) | Diagnose inverted verification gates from a declared edge kind, not from issue titles | Proposed | 2026-08-12 |

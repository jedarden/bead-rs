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
| [002](002-agent-guided-rehydration-over-cross-tool-migration.md) | Prefer agent-guided rehydration over cross-tool schema migration | Accepted | 2026-08-12 |
| [003](003-automatic-checkpoint-flush-gated-on-incremental-publication.md) | Make checkpoint flush automatic on mutation, gated on incremental publication | Proposed | 2026-08-15 |
| [004](004-raise-msrv-to-1.85-and-edition-2024.md) | Raise the MSRV to Rust 1.85 and migrate to edition 2024 | Accepted | 2026-08-15 |
| [005](005-assignment-held-issues-diagnosed-not-released.md) | Diagnose issues held off the ready frontier by assignment; never release them automatically | Accepted | 2026-08-16 |
| [006](006-first-class-verified-restore-over-documented-recipe.md) | Make explicit restore a first-class verified command, not a documented multi-step recipe | Accepted | 2026-08-16 |
| [007](007-cli-errors-name-the-remedy.md) | CLI errors for immutable fields and near-miss flags must name the remedy | Proposed | 2026-08-16 |
| [008](008-no-title-similarity-duplicate-detection.md) | Do not detect duplicate beads by title similarity | Rejected | 2026-08-16 |
| [009](009-no-git-awareness-for-checkpoint-ordering.md) | Do not make bead-rs Git-aware to enforce checkpoint/pull ordering | Rejected | 2026-08-16 |
| [010](010-store-attempt-facts-not-learning-policy.md) | Store portable attempt facts, not learning or orchestration policy | Accepted | 2026-08-31 |
| [011](011-atomic-idempotent-attempt-resolution.md) | Resolve an attempt and its lifecycle transition atomically | Accepted | 2026-08-31 |
| [012](012-capability-gated-attempt-contract-rollout.md) | Roll out attempt resolution through versioned capabilities | Accepted | 2026-08-31 |
| [013](013-read-only-git-reachability-reporting.md) | Report Git reachability in `sync status` through the git binary, read-only | Accepted | 2026-09-02 |
| [014](014-hard-reject-secret-bearing-mutations.md) | Hard-reject mutations that would publish a detectable secret | Proposed | 2026-09-03 |

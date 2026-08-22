//! Service layer for bead operations
//!
//! This module provides business logic for issue operations, claiming,
//! dependencies, checkpoint management, diagnostics, and capabilities.

pub mod archaeology;
pub mod capabilities;
pub mod changes;
pub mod checkpoint;
pub mod claim;
pub mod comparison;
pub mod conditions;
pub mod data;
pub mod dependencies;
pub mod doctor;
pub mod dryrun;
pub mod external_refs;
pub mod issues;
pub mod leases;
pub mod lifecycle;
pub mod manifest;
pub mod query;

pub mod reconcile;
pub mod recurrence;
pub mod rehearsal;
pub mod resource_locks;
pub mod scheduling;
pub mod schema;
pub mod why;

// Archaeology report types are public library API; the binary uses the command
// functions but not every exported report type directly.
#[allow(unused_imports)]
pub use archaeology::{
    bisect_checkpoints, diff_checkpoints, query_checkpoint, reject_archaeology_input,
    ArchaeologyBisectReport, ArchaeologyDiffReport, ArchaeologyQueryReport,
    ARCHAEOLOGY_ARTIFACT_KIND,
};
pub use capabilities::generate_capabilities;
pub use changes::{
    get_changes_since, get_gap_info, get_snapshot_identity, validate_cursor, Cursor,
};
pub use checkpoint::{
    acquire_checkpoint_publication_lock, flush_checkpoint, forensic_checkpoint_status,
    fork_workspace_identity, import_checkpoint_with_diagnostics, import_forensic_checkpoint,
    load_checkpoint_config, publish_forensic_checkpoint, publish_forensic_checkpoint_holding,
    read_covered_event_sequence, read_live_event_sequence, restore_verified_generation,
    verify_restore_source, CheckpointConfig,
};
// The fork report type is public library API (callers of
// `fork_workspace_identity` name it) but the binary holds it only as a
// value and never names the type.
#[allow(unused_imports)]
pub use checkpoint::ForkReport;
// The publication-lock guard type is public library API (callers that
// publish through `publish_forensic_checkpoint_holding` name it) but the
// binary holds it only as a value and never names the type.
#[allow(unused_imports)]
pub use checkpoint::CheckpointPublicationLock;
// The compiled automatic-flush default is public library API (the capability
// document keys its `auto_flush` advertisement on it, plan 6.2.1 and section
// 11) but the binary reaches it only through
// `CheckpointConfig::auto_flush_enabled`.
#[allow(unused_imports)]
pub use checkpoint::AUTO_FLUSH_COMPILED_DEFAULT;
// claim_issue_with_trace is public library API but unused by the binary
#[allow(unused_imports)]
pub use claim::{
    claim_issue_with_lease, claim_issue_with_policy, claim_issue_with_trace, EnhancedClaimResult,
};
pub use conditions::ConditionExpr;
pub use data::{get_data, list_data, remove_data, set_data};
pub use dependencies::{add_dependency, add_label, remove_dependency, remove_label};
pub use doctor::{run_diagnostics, run_diagnostics_with_scopes, run_repairs, DiagnosticStatus};
pub use dryrun::{
    add_dependency_dryrun, close_issue_dryrun, release_issue_dryrun, remove_dependency_dryrun,
    reopen_issue_dryrun, update_issue_dryrun,
};
pub use external_refs::{
    add_external_reference, find_issues_by_reference, list_external_references,
    remove_external_reference,
};
pub use issues::get_issue_by_id;
pub use issues::list_issues;
#[allow(unused_imports)]
pub use issues::{create_issue, create_issue_with_unique_ref, CreateOutcome};
pub use leases::{validate_lease_for_mutation, LeaseClaimResult};
pub use lifecycle::{close_issue, release_issue, reopen_issue, update_issue};
pub use manifest::{load_manifest, manifest_commit, manifest_dry_run, ManifestReport};
pub use query::{
    delete_view, execute_query, get_view, list_views, parse_query, project_issue, save_view, Query,
};
pub use recurrence::{
    create_template, delete_template, get_materialization_history, get_template, list_templates,
    materialize_next_occurrence,
};
pub use rehearsal::run_recovery_rehearsal;
pub use schema::{
    schema_catalog, schema_document, schema_explanation, schema_explanation_markdown,
};
// Scheduling types are part of R019 public API but may show as unused during compilation
#[allow(unused_imports)]
pub use comparison::{
    compare_issue_profiles, ComparisonResult, ComparisonSummary, FieldComparison, FieldStatus,
};
#[allow(unused_imports)]
pub use scheduling::{AttemptTier, GraphMetrics, SchedulingPolicy, SchedulingState};
pub use why::{explain_why, WhyExplanation};

pub mod policy;
#[allow(unused_imports)]
pub use policy::{FindingCategory, FindingSeverity, PolicyDiagnosticStatus, PolicyDiagnostics};

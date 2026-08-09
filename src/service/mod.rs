//! Service layer for bead operations
//!
//! This module provides business logic for issue operations, claiming,
//! dependencies, checkpoint management, diagnostics, and capabilities.

pub mod capabilities;
pub mod changes;
pub mod checkpoint;
pub mod claim;
pub mod conditions;
pub mod data;
pub mod dependencies;
pub mod doctor;
pub mod dryrun;
pub mod external_refs;
pub mod issues;
pub mod leases;
pub mod lifecycle;
pub mod query;
pub mod recurrence;
pub mod rehearsal;

pub use capabilities::generate_capabilities;
pub use changes::{
    get_changes_since, get_gap_info, get_snapshot_identity, validate_cursor, Cursor,
};
pub use checkpoint::{flush_checkpoint, import_forensic_checkpoint, publish_forensic_checkpoint};
pub use claim::{claim_issue_with_lease, claim_issue_with_trace};
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
pub use issues::create_issue;
pub use issues::get_issue_by_id;
pub use issues::list_issues;
pub use leases::{validate_lease_for_mutation, LeaseClaimResult};
pub use lifecycle::{close_issue, release_issue, reopen_issue, update_issue};
pub use query::{
    delete_view, execute_query, get_view, list_views, parse_query, project_issue, save_view, Query,
};
pub use recurrence::{
    create_template, delete_template, get_materialization_history, get_template, list_templates,
    materialize_next_occurrence,
};
pub use rehearsal::run_recovery_rehearsal;

// Placeholder modules for future implementation
// pub mod migrate;

//! Service layer for bead operations
//!
//! This module provides business logic for issue operations, claiming,
//! dependencies, checkpoint management, diagnostics, and capabilities.

pub mod capabilities;
pub mod checkpoint;
pub mod claim;
pub mod dependencies;
pub mod doctor;
pub mod issues;
pub mod lifecycle;

pub use capabilities::generate_capabilities;
pub use checkpoint::{flush_checkpoint, import_forensic_checkpoint, publish_forensic_checkpoint};
pub use claim::claim_issue_with_trace;
pub use dependencies::{add_dependency, add_label, remove_dependency, remove_label};
pub use doctor::{run_diagnostics, run_repairs, DiagnosticStatus};
pub use issues::create_issue;
pub use issues::get_issue_by_id;
pub use issues::list_issues;
pub use lifecycle::{close_issue, release_issue, reopen_issue, update_issue};

// Placeholder modules for future implementation
// pub mod migrate;

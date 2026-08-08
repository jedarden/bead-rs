//! Service layer for bead operations
//!
//! This module provides business logic for issue operations, claiming,
//! dependencies, checkpoint management, and diagnostics.

pub mod issues;

pub use issues::create_issue;
pub use issues::get_issue_by_id;
pub use issues::list_issues;

// Placeholder modules for future implementation
// pub mod claim;
// pub mod dependencies;
// pub mod checkpoint;
// pub mod doctor;
// pub mod migrate;

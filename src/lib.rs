#![forbid(unsafe_code)]

pub mod cli;
pub mod docs;
pub mod error;
pub mod model;
pub mod profile;
pub mod service;
pub mod store;

pub use error::{Error, Result};
pub use service::migrate::{run_migration, MigrationOptions, MigrationReceipt};
pub use store::Store;

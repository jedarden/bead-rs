#![forbid(unsafe_code)]

pub mod cli;
pub mod error;
pub mod model;
pub mod service;
pub mod store;

pub use error::{Error, Result};
pub use store::Store;

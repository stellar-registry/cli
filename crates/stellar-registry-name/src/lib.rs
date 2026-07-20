//! Registry name types
//!
//! - [`Prefixed`] — `name` or `channel/name`, no version.
//! - [`Versioned`] — a [`Prefixed`] plus an optional `@version` suffix.

mod common;
pub mod error;
pub mod prefixed;
pub mod versioned;

pub use prefixed::Prefixed;
pub use versioned::Versioned;

pub use common::canonicalize;
pub use error::Error;

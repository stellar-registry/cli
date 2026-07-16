//! Registry interaction at build time.
//!
//! The `name` module (typed registry names) is always available and
//! dependency-light so proc-macro crates can use it. Everything that talks to
//! the network — `contract`, `registry`, `error` — sits behind the default
//! `cli` feature, which pulls in the full stellar-cli stack.

#[cfg(feature = "cli")]
pub mod contract;
#[cfg(feature = "cli")]
pub mod error;
pub mod name;
#[cfg(feature = "cli")]
pub mod registry;

#[cfg(feature = "cli")]
pub use error::Error;

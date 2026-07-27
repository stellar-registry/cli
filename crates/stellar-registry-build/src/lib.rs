//! Registry interaction at build time.
//!
//! Talks to the on-chain registry over the network (`contract`, `registry`,
//! `error`), pulling in the full stellar-cli stack. Proc-macro crates that only
//! need typed registry names depend on the dependency-light `stellar-registry-name`
//! crate directly (re-exported here via the `name` module).

pub mod contract;
pub mod error;
pub mod registry;

pub mod name;

pub use error::Error;

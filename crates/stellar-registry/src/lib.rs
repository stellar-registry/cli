#![no_std]
//! `stellar-registry` is a collection of tools to help integrate with
//! existing smart contracts on Stellar.
//!
pub use stellar_registry_macro::{import_asset, import_contract, import_contract_client};

/// Helpers for unit-testing contracts that use [`import_contract!`].
#[cfg(feature = "testutils")]
pub mod testutils {
    pub use stellar_registry_macro::register_import;
}

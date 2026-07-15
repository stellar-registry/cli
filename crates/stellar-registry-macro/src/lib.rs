//! The `import_contract!` proc-macro: resolve a named Stellar Registry contract
//! to a type-safe client already bound to its deployed on-chain address, with
//! the client types generated from the deployed contract's own wasm.
extern crate proc_macro;

mod asset;
mod contract;
mod contract_client;

pub use asset::import_asset;
pub use contract::import_contract;
pub use contract_client::import_contract_client;

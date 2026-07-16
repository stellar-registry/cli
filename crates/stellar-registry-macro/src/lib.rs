//! The `import_contract!` proc-macro: resolve a named Stellar Registry contract
//! to a type-safe client already bound to its deployed on-chain address, with
//! the client types generated from the deployed contract's own wasm.
extern crate proc_macro;
use proc_macro::TokenStream;

mod asset;
mod contract;
mod contract_client;
mod util;

use asset::import_asset;
use contract::import_contract;
use stellar_registry_build::macro_plus::*;

/// Generates a contract Client for a given contract.
/// The name should match a published contract or a contract in your current workspace.
///
/// # Usage
///
/// ```ignore
/// // For simple names (workspace contracts or registry names without hyphens):
/// import_contract_client!(registry);
///
/// // For hyphenated names or channel-prefixed registry paths:
/// import_contract_client!("unverified/guess-the-number");
///
/// // For specific versions, use quotes. `v` is optional:
/// import_contract_client!("registry@v1.0.0");
/// ```
///
/// When using a string literal, the module name is derived from the contract
/// name with hyphens replaced by underscores (e.g., `guess_the_number`).
///
/// # Panics
///
/// This function may panic in the following situations:
/// - If `stellar_build::get_target_dir()` fails to retrieve the target directory
/// - If the input tokens cannot be parsed as a valid identifier
/// - If the input tokens cannot be parsed as a valid identifier or string literal
/// - If the directory path cannot be canonicalized
/// - If the canonical path cannot be converted to a string
#[proc_macro]
pub fn import_contract_client(wasm_binary: TokenStream) -> TokenStream {
    contract_client::import_contract_client(wasm_binary).to_token_stream()
}

/// Generates a contract Client for a given asset.
/// It is expected that the name of an asset, e.g. "native" or "USDC:G1...."
///
/// # Panics
///
#[proc_macro]
pub fn import_asset(input: TokenStream) -> TokenStream {
    // Parse the input as a string literal
    let input_str = syn::parse_macro_input!(input as syn::LitStr);
    asset::parse_literal(&input_str, &Network::passphrase_from_env()).into()
}

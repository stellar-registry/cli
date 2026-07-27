//! Proc macros for the Stellar Registry: import deployed contracts
//! (`import_contract!`), published wasms (`import_contract_client!`), and
//! Stellar assets (`import_asset!`) as type-safe soroban clients, resolved at
//! build time.
extern crate proc_macro;
use proc_macro::TokenStream;

mod asset;
mod contract;
mod contract_client;
mod util;

use util::ProcMacroWrapper as _;

/// Generate a type-safe client for a deployed, registry-named contract,
/// already bound to its on-chain address — collapsing "look up the address"
/// and "generate the client type" into one call.
///
/// ```ignore
/// // `env: &Env`
/// let dao = stellar_registry::import_contract!(env, our_dao);
/// dao.create_proposal(/* ... */);
/// ```
///
/// The name is a bare ident or a string literal, optionally channel-prefixed
/// (`import_contract!(env, "unverified/our-dao")`). A deployed contract has no
/// version, so no `@version` suffix is accepted. With a string literal, the
/// generated module name is the contract name with `-` replaced by `_`.
///
/// Resolved at build time:
/// - **address** — `stellar registry fetch-contract-id`, cached at
///   `target/stellar/<network>/deployed/<mod_name>.id` (channel-prefixed
///   names cache as `<channel>__<mod_name>.id`). The online lookup **fails
///   compilation if the contract is flagged as compromised** in the registry,
///   and a cached id is deliberately ignored while online so a contract flagged
///   after the first build cannot slip through a stale cache.
/// - **wasm** — the deployed contract's *own* wasm, via `stellar contract
///   fetch --id <address>`, cached beside the id. Client types are generated
///   from it, so a contract whose wasm was never published to the registry
///   still works.
///
/// Set `STELLAR_NO_REGISTRY=1` to forbid the network calls; the cached id and
/// wasm are then required (build online once, or create them yourself with
/// `stellar registry fetch-contract-id` and `stellar contract fetch`). Because
/// a real on-chain address is baked in, if the named contract is redeployed,
/// delete the cached files (or `cargo clean`) and rebuild.
#[proc_macro]
pub fn import_contract(input: TokenStream) -> TokenStream {
    contract::import_contract(input).to_token_stream()
}

/// Generate a contract client from a published wasm — from your workspace's
/// `target` directory if present, otherwise downloaded from the registry.
///
/// ```ignore
/// // Workspace wasms or registry names without hyphens:
/// import_contract_client!(registry);
///
/// // Hyphenated or channel-prefixed registry names:
/// import_contract_client!("unverified/guess-the-number");
///
/// // A specific published version (leading `v` optional):
/// import_contract_client!("registry@1.0.0");
/// ```
///
/// Unlike [`import_contract!`], this looks up a published **wasm** — which has
/// versions — and only generates the client types; it does not bind them to a
/// deployed address. With a string literal, the generated module name is the
/// contract name with `-` replaced by `_`.
///
/// Set `STELLAR_NO_REGISTRY=1` to skip the registry download; the wasm must
/// then already exist at
/// `target/stellar/<network>/[<channel>__]<mod_name>[_<version>].wasm`
/// (perhaps put there by `stellar registry download`, or by compiling a
/// workspace contract).
#[proc_macro]
pub fn import_contract_client(input: TokenStream) -> TokenStream {
    contract_client::import_contract_client(input).to_token_stream()
}

/// Generate a module with the Stellar Asset Contract id and token clients for
/// an asset, computed offline for the build-time network (`STELLAR_NETWORK` /
/// `STELLAR_NETWORK_PASSPHRASE`, defaulting to local).
///
/// ```ignore
/// import_asset!("native"); // or "xlm"
/// import_asset!("USDC:GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN");
/// ```
///
/// The generated module — named after the asset code — exposes `contract_id`,
/// `token_client` (the standard token interface) and `stellar_asset_client`
/// (the asset admin interface).
#[proc_macro]
pub fn import_asset(input: TokenStream) -> TokenStream {
    asset::import_asset(input).to_token_stream()
}

use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
};

use quote::quote;
use syn::{
    Expr, Ident, Token,
    parse::{Parse, ParseStream},
};

use stellar_registry_name::Prefixed;

use crate::asset;
use crate::util::{Name, explorer_url, manifest, mod_ident, network_name};

/// `import_contract!(env_expr, name)` — `name` is a bare ident or a string
/// literal (optionally channel-prefixed, e.g. `"unverified/our_dao"`).
struct Input {
    env: Expr,
    name: Name,
}

impl Parse for Input {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let env: Expr = input.parse()?;
        input.parse::<Token![,]>()?;
        let name: Name = input.parse()?;
        Ok(Self { env, name })
    }
}

/// Whether `raw` names an asset rather than a registered contract: exactly
/// `native`/`xlm`, or a `CODE:ISSUER` literal. Colons are never valid in a
/// registry name (`Prefixed` rejects them outright), so `raw.contains(':')`
/// can't collide with a real registered name — it's safe to route straight to
/// [`asset::parse_asset`] and get its more specific error messages instead of
/// `Prefixed`'s generic "invalid character" one.
fn is_asset_syntax(raw: &str) -> bool {
    raw == "native" || raw == "xlm" || raw.contains(':')
}

/// Validate a `C…` contract strkey; return it trimmed.
fn validate_contract_id(s: &str) -> Result<String, String> {
    let t = s.trim();
    t.parse::<stellar_strkey::Contract>()
        .map(|_| t.to_string())
        .map_err(|_| format!("not a valid contract id (C… strkey): {t:?}"))
}

/// Cache file stem for a deployed-contract import. The channel is part of a
/// contract's identity, so it is part of the key — `unverified/foo` and `foo`
/// are different contracts and must never share cache files.
fn cache_stem(contract: &Prefixed) -> String {
    match contract.channel() {
        Some(channel) => format!("{channel}__{}", contract.mod_name()),
        None => contract.mod_name(),
    }
}

/// `<target_dir>/deployed/<stem>.id` — cached deployed address. Namespaced
/// under `deployed/` so it can never collide with registry-downloaded or
/// workspace-built wasms, which live directly in the network dir under the
/// same `<mod_name>.wasm` naming.
fn cache_id_path(target_dir: &Path, contract: &Prefixed) -> PathBuf {
    target_dir
        .join("deployed")
        .join(cache_stem(contract))
        .with_extension("id")
}

/// `<target_dir>/deployed/<stem>.wasm` — the deployed contract's wasm, fetched
/// by address, that `contractimport!` reads to generate the client types.
fn cache_wasm_path(target_dir: &Path, contract: &Prefixed) -> PathBuf {
    target_dir
        .join("deployed")
        .join(cache_stem(contract))
        .with_extension("wasm")
}

/// `<target_dir>/deployed/<stem>.sac` — marks a registry name as resolving to
/// a Stellar Asset Contract (no wasm exists to fetch), holding the address it
/// was recorded for. Lets a rebuild skip straight to the token-client codegen
/// instead of re-running `stellar contract fetch` only to hit the same
/// "no downloadable code binary" failure every time.
fn cache_sac_marker_path(target_dir: &Path, contract: &Prefixed) -> PathBuf {
    target_dir
        .join("deployed")
        .join(cache_stem(contract))
        .with_extension("sac")
}

/// The cached wasm belongs to a specific deployment. Online, if the freshly
/// resolved address differs from the previously cached id (redeploy under the
/// same name), the wasm must be refetched. Offline there is nothing to compare
/// against — the cache is trusted as-is.
fn wasm_is_stale(previously_cached_id: Option<&str>, address: &str, no_registry: bool) -> bool {
    !no_registry && previously_cached_id.map(str::trim) != Some(address)
}

/// The "things to try" footer for resolution failures: a name-check link when
/// the network has an explorer, a manual repro command, and the offline escape
/// hatch with the exact cache paths this build expects.
fn resolution_help(lookup: &Prefixed, id_path: &Path, wasm_path: &Path) -> String {
    let name_check = explorer_url(&network_name())
        .map(|url| format!("- Check that you got the name right: {url}/contracts\n"))
        .unwrap_or_default();
    format!(
        "{name_check}\
         - Run `stellar registry fetch-contract-id {lookup}` yourself and make sure the name \
         and network match your expectations.\n\
         - Set STELLAR_NO_REGISTRY=1 to prevent network calls. You will need to create {id} and \
         {wasm} yourself, perhaps using `stellar registry fetch-contract-id` for the id and \
         `stellar contract fetch` for the wasm.",
        id = id_path.display(),
        wasm = wasm_path.display(),
    )
}

/// Resolve the deployed address. Offline (`STELLAR_NO_REGISTRY=1`) the cached
/// `.id` is required. Online the cache is deliberately NOT consulted —
/// `fetch` runs every build (and fails if the contract is flagged), so a
/// contract flagged after the first build cannot slip through a stale `.id`.
/// IO is injected so the precedence is unit-testable offline.
fn resolve_address(
    cache: Option<String>,
    no_registry: bool,
    offline_help: &str,
    fetch: impl FnOnce() -> Result<String, String>,
) -> Result<String, String> {
    if no_registry {
        return match cache {
            Some(c) => validate_contract_id(&c),
            None => Err(format!(
                "STELLAR_NO_REGISTRY=1 but no cached contract id. Things to try:\n\n{offline_help}"
            )),
        };
    }
    validate_contract_id(&fetch()?)
}

/// Shell out to `stellar-registry-cli` to look up a deployed contract's id by
/// name. A current `stellar-registry-cli` refuses flagged contracts by default,
/// so a flagged contract fails this build; plugins that predate the check
/// resolve the id without it. Network selection is delegated to the CLI's own
/// config (`STELLAR_NETWORK`). Failures are mapped to the most specific message
/// the CLI's stderr allows.
fn fetch_contract_id(lookup: &Prefixed, help: &str) -> Result<String, String> {
    let out = Command::new("stellar")
        .args(["registry", "fetch-contract-id"])
        .arg(lookup.to_string())
        .output()
        .map_err(|e| {
            format!(
                "failed to run `stellar`: {e}. Install the Stellar CLI, then \
                 `cargo install stellar-registry-cli` for the registry plugin."
            )
        })?;
    if out.status.success() {
        return Ok(String::from_utf8_lossy(&out.stdout).trim().to_string());
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    // An installed-but-outdated plugin rejects the subcommand or an argument;
    // check before the plugin-missing case, whose stderr wording overlaps.
    if stderr.contains("unexpected argument")
        || (stderr.contains("unrecognized subcommand") && stderr.contains("fetch-contract-id"))
    {
        return Err(format!(
            "the installed `stellar registry` plugin is too old for import_contract!. Upgrade \
             it with `cargo install stellar-registry-cli --force`.\n\nstderr:\n{stderr}"
        ));
    }
    if stderr.contains("unrecognized subcommand") || stderr.contains("no such command") {
        return Err(format!(
            "the `stellar registry` plugin is not installed. Install it with \
             `cargo install stellar-registry-cli`.\n\nstderr:\n{stderr}"
        ));
    }
    if stderr.contains("flagged as compromised") {
        return Err(format!(
            "contract `{lookup}` is flagged as compromised in the registry; refusing to import it."
        ));
    }
    Err(format!(
        "Could not resolve a contract id for `{lookup}` on {network}. Things to try:\n\n\
         {help}\n\nstderr from `stellar registry fetch-contract-id`:\n{stderr}",
        network = network_name(),
    ))
}

/// Whether a `fetch_wasm` failure is `stellar-cli`'s `ContractIsStellarAsset`
/// error: the address is a network built-in asset contract (a SAC), which has
/// no downloadable wasm by design, rather than some other fetch problem.
fn is_stellar_asset_fetch_error(fetch_err: &str) -> bool {
    fetch_err.contains("network built-in asset contract")
}

/// Cache that `address` is a SAC (no wasm to fetch), so a rebuild can skip
/// straight to the token client. Purely a cache, so a write error is dropped
/// — worst case, the next build just re-discovers it the slow way.
fn record_sac_marker(sac_marker_path: &Path, address: &str) {
    if let Some(parent) = sac_marker_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(sac_marker_path, address);
}

/// Shell out to `stellar contract fetch` to download a *deployed* contract's own
/// wasm by address (not a registry-published wasm-name) into `out_path`.
fn fetch_wasm(address: &str, out_path: &Path) -> Result<(), String> {
    if let Some(parent) = out_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let out = Command::new("stellar")
        .args(["contract", "fetch", "--id", address, "--out-file"])
        .arg(out_path)
        .output()
        .map_err(|e| {
            format!("failed to run `stellar contract fetch`: {e}. Install the Stellar CLI and try again.")
        })?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "`stellar contract fetch --id {address}` failed:\n{}",
            String::from_utf8_lossy(&out.stderr)
        ))
    }
}

/// Emit a block expression: generate the client types from the deployed
/// contract's own wasm, then construct the client bound to the baked address.
/// `use ::soroban_sdk;` resolves through the extern prelude — consistently
/// with the `::soroban_sdk::Env` binding below — so callers need no
/// `use soroban_sdk;` of their own.
fn expand(
    env: &Expr,
    mod_ident: &Ident,
    wasm_path: &str,
    address: &str,
) -> proc_macro2::TokenStream {
    quote! {
        {
            #[allow(non_snake_case)]
            mod #mod_ident {
                #![allow(clippy::ref_option, clippy::too_many_arguments)]
                use ::soroban_sdk;
                soroban_sdk::contractimport!(file = #wasm_path);
            }
            let __env: &::soroban_sdk::Env = #env;
            #mod_ident::Client::new(
                __env,
                &::soroban_sdk::Address::from_str(__env, #address),
            )
        }
    }
}

/// Emit a block expression binding the SDK's own standard-interface
/// `token::TokenClient` to `address` — for a Stellar Asset Contract, which has
/// no bespoke wasm to run `contractimport!` on. Its interface is always the
/// same, so no codegen from a fetched wasm is needed, unlike [`expand`].
/// (Named `TokenClient`, not the deprecated `token::Client` alias.)
fn expand_token_client(env: &Expr, address: &str) -> proc_macro2::TokenStream {
    quote! {
        {
            use ::soroban_sdk;
            let __env: &::soroban_sdk::Env = #env;
            ::soroban_sdk::token::TokenClient::new(
                __env,
                &::soroban_sdk::Address::from_str(__env, #address),
            )
        }
    }
}

pub(crate) fn import_contract(
    input: proc_macro::TokenStream,
) -> syn::Result<proc_macro2::TokenStream> {
    let Input { env, name } = syn::parse(input)?;
    let span = name.span();
    let err = |msg: String| syn::Error::new(span, msg);

    // A deployed contract has no version; give the `@version` mistake its own
    // message before `Prefixed` (which also rejects it) reports generically.
    let raw = name.raw();
    if raw.contains('@') {
        return Err(err(format!(
            "import_contract! does not take a version — a deployed contract has no version \
             (got {raw:?}). Use just the contract name, e.g. `import_contract!(env, our_dao)`."
        )));
    }

    // Asset syntax (`xlm`, `native`, `"CODE:ISSUER"`) resolves offline to a
    // Stellar Asset Contract id — no registry lookup, no cache files, works
    // the same under STELLAR_NO_REGISTRY=1 since there's nothing to fetch.
    if is_asset_syntax(&raw) {
        let (contract_id, _code) =
            asset::generate_asset_id(&raw, &stellar_build::Network::passphrase_from_env())
                .map_err(err)?;
        return Ok(expand_token_client(&env, &contract_id.to_string()));
    }

    let contract: Prefixed = name.parse_as()?;
    let mod_ident = mod_ident(&contract, span)?;

    let no_registry = env::var("STELLAR_NO_REGISTRY").as_deref() == Ok("1");
    let target_dir = stellar_build::get_target_dir(&manifest()?)
        .map_err(|e| err(format!("could not determine the cargo target dir: {e}")))?;
    let id_path = cache_id_path(&target_dir, &contract);
    let wasm_path = cache_wasm_path(&target_dir, &contract);
    let sac_marker_path = cache_sac_marker_path(&target_dir, &contract);
    let help = resolution_help(&contract, &id_path, &wasm_path);

    // 1. Resolve the deployed address (and, online, enforce the flag check).
    let cached_id = std::fs::read_to_string(&id_path).ok();
    let address = resolve_address(cached_id.clone(), no_registry, &help, || {
        let addr = validate_contract_id(&fetch_contract_id(&contract, &help)?)?;
        if let Some(parent) = id_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&id_path, &addr);
        Ok(addr)
    })
    .map_err(err)?;

    // 2. A registered name can itself resolve to a Stellar Asset Contract
    //    (e.g. `circle/usdc`), which has no wasm to fetch. If a previous
    //    build already discovered that for this address, skip straight to
    //    the token client instead of repeating a `fetch` that can only fail
    //    the same way again; `wasm_is_stale`'s same "does the cache match the
    //    resolved address" check keeps this safe across redeploys.
    let cached_sac_address = std::fs::read_to_string(&sac_marker_path).ok();
    if cached_sac_address.is_some()
        && !wasm_is_stale(cached_sac_address.as_deref(), &address, no_registry)
    {
        return Ok(expand_token_client(&env, &address));
    }

    // 3. Ensure the deployed contract's wasm is on disk for `contractimport!`,
    //    refetching if the name resolved to a different deployment than the
    //    cached wasm came from.
    if !wasm_path.exists() || wasm_is_stale(cached_id.as_deref(), &address, no_registry) {
        if no_registry {
            return Err(err(format!(
                "STELLAR_NO_REGISTRY=1 but no cached wasm at {path}. Build online once (which \
                 fetches it), or run `stellar contract fetch --id {address} \
                 --out-file {path}` yourself.",
                path = wasm_path.display(),
            )));
        }
        match fetch_wasm(&address, &wasm_path) {
            Ok(()) => {}
            // A registered name (e.g. `circle/usdc`) can itself be a SAC —
            // this failed fetch is how we find out. Cache it so a rebuild
            // doesn't repeat a fetch that can only fail the same way again.
            Err(e) if is_stellar_asset_fetch_error(&e) => {
                record_sac_marker(&sac_marker_path, &address);
                return Ok(expand_token_client(&env, &address));
            }
            Err(e) => return Err(err(e)),
        }
    }

    // 4. Generate the client from that wasm and bind it to the address.
    Ok(expand(
        &env,
        &mod_ident,
        &wasm_path.to_string_lossy(),
        &address,
    ))
}

#[cfg(test)]
mod helpers {
    use super::*;
    use std::path::Path;

    // A real, valid contract strkey (from soroban-sdk docs).
    const VALID: &str = "CBESJIMX7J53SWJGJ7WQ6QTLJI4S5LPPJNC2BNVD63GIKAYCDTDOO322";

    fn prefixed(s: &str) -> Prefixed {
        s.parse().unwrap()
    }

    #[test]
    fn asset_syntax_matches_native_xlm_and_code_issuer() {
        assert!(is_asset_syntax("xlm"));
        assert!(is_asset_syntax("native"));
        assert!(is_asset_syntax(
            "USDC:GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN"
        ));
        // Ordinary registry names, including channel-prefixed ones, are not
        // asset syntax — even one that happens to be *named* `xlm`.
        assert!(!is_asset_syntax("our_dao"));
        assert!(!is_asset_syntax("unverified/our-dao"));
        assert!(!is_asset_syntax("unverified/xlm"));
    }

    #[test]
    fn sac_marker_path_is_namespaced_like_the_wasm_cache() {
        assert_eq!(
            cache_sac_marker_path(Path::new("target"), &prefixed("circle-usdc")),
            Path::new("target/deployed/circle_usdc.sac")
        );
        assert_eq!(
            cache_sac_marker_path(Path::new("target"), &prefixed("unverified/foo")),
            Path::new("target/deployed/unverified__foo.sac")
        );
    }

    #[test]
    fn stellar_asset_fetch_error_is_recognized() {
        let stderr = "error: cannot fetch wasm for contract because the contract is \
                       a network built-in asset contract that does not have a downloadable \
                       code binary";
        assert!(is_stellar_asset_fetch_error(stderr));
        assert!(!is_stellar_asset_fetch_error("error: connection refused"));
    }

    #[test]
    fn validate_contract_id_trims_and_checks() {
        assert_eq!(
            validate_contract_id(&format!("  {VALID}\n")).unwrap(),
            VALID
        );
        assert!(validate_contract_id("not-an-address").is_err());
        assert!(validate_contract_id("").is_err());
    }

    #[test]
    fn cache_paths_are_namespaced_under_deployed() {
        assert_eq!(
            cache_id_path(Path::new("target"), &prefixed("our-dao")),
            Path::new("target/deployed/our_dao.id")
        );
        assert_eq!(
            cache_wasm_path(Path::new("target"), &prefixed("our-dao")),
            Path::new("target/deployed/our_dao.wasm")
        );
    }

    #[test]
    fn cache_paths_include_the_channel() {
        // `unverified/foo` and `foo` are different contracts — different files.
        assert_eq!(
            cache_wasm_path(Path::new("target"), &prefixed("unverified/foo")),
            Path::new("target/deployed/unverified__foo.wasm")
        );
        assert_ne!(
            cache_wasm_path(Path::new("target"), &prefixed("unverified/foo")),
            cache_wasm_path(Path::new("target"), &prefixed("foo")),
        );
    }

    #[test]
    fn wasm_staleness_tracks_address_changes_online_only() {
        const OTHER: &str = "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC";
        // Online: no prior id, or a different prior id → stale.
        assert!(wasm_is_stale(None, VALID, false));
        assert!(wasm_is_stale(Some(OTHER), VALID, false));
        // Online: same id (even with cache whitespace) → fresh.
        assert!(!wasm_is_stale(Some(VALID), VALID, false));
        assert!(!wasm_is_stale(Some(&format!("{VALID}\n")), VALID, false));
        // Offline: nothing to compare against — trust the cache.
        assert!(!wasm_is_stale(None, VALID, true));
        assert!(!wasm_is_stale(Some(OTHER), VALID, true));
    }

    #[test]
    fn resolution_help_lists_repro_and_offline_paths() {
        let lookup: Prefixed = "unverified/our-dao".parse().unwrap();
        let help = resolution_help(
            &lookup,
            Path::new("target/deployed/unverified__our_dao.id"),
            Path::new("target/deployed/unverified__our_dao.wasm"),
        );
        assert!(help.contains("stellar registry fetch-contract-id unverified/our-dao"));
        assert!(help.contains("STELLAR_NO_REGISTRY=1"));
        assert!(help.contains("target/deployed/unverified__our_dao.id"));
        assert!(help.contains("target/deployed/unverified__our_dao.wasm"));
    }
}

#[cfg(test)]
mod resolution {
    use super::*;
    const A: &str = "CBESJIMX7J53SWJGJ7WQ6QTLJI4S5LPPJNC2BNVD63GIKAYCDTDOO322";
    const B: &str = "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC";

    fn no_fetch() -> Result<String, String> {
        Err("fetch should not run".into())
    }

    #[test]
    fn offline_uses_cache() {
        let got = resolve_address(Some(B.to_string()), true, "help", no_fetch);
        assert_eq!(got.unwrap(), B);
    }

    #[test]
    fn offline_errors_without_cache() {
        let got = resolve_address(None, true, "try this instead", no_fetch);
        let msg = got.unwrap_err();
        assert!(msg.contains("STELLAR_NO_REGISTRY"), "{msg}");
        assert!(msg.contains("try this instead"), "{msg}");
    }

    #[test]
    fn online_fetches_and_ignores_stale_cache() {
        // A cached id must NOT short-circuit the online fetch (+ flag check).
        let got = resolve_address(Some(B.to_string()), false, "help", || Ok(A.to_string()));
        assert_eq!(got.unwrap(), A);
    }

    #[test]
    fn online_validates_fetched_id() {
        let got = resolve_address(None, false, "help", || Ok("garbage".to_string()));
        assert!(got.unwrap_err().contains("not a valid contract id"));
    }
}

#[cfg(test)]
mod codegen {
    use super::*;
    use proc_macro2::Span;
    use quote::quote;
    use syn::{Ident, parse2};

    const A: &str = "CBESJIMX7J53SWJGJ7WQ6QTLJI4S5LPPJNC2BNVD63GIKAYCDTDOO322";

    #[test]
    fn parses_env_and_string_name() {
        let input: Input = parse2(quote!(env, "unverified/our_dao")).unwrap();
        assert_eq!(input.name.raw(), "unverified/our_dao");
    }

    #[test]
    fn parses_env_and_ident_name() {
        let input: Input = parse2(quote!(env, registry)).unwrap();
        assert_eq!(input.name.raw(), "registry");
    }

    #[test]
    fn version_suffix_is_rejected_by_the_type() {
        let input: Input = parse2(quote!(env, "our_dao@1.0.0")).unwrap();
        let err = input.name.parse_as::<Prefixed>().unwrap_err();
        assert!(err.to_string().contains("version"), "{err}");
    }

    #[test]
    fn expand_emits_contractimport_and_bound_client() {
        let env: syn::Expr = parse2(quote!(env)).unwrap();
        let ident = Ident::new("our_dao", Span::call_site());
        let out = expand(
            &env,
            &ident,
            "/tmp/target/stellar/local/deployed/our_dao.wasm",
            A,
        )
        .to_string();
        assert!(
            out.contains("contractimport"),
            "generates types from the wasm: {out}"
        );
        assert!(
            out.contains("our_dao.wasm"),
            "references the fetched wasm: {out}"
        );
        assert!(
            out.contains("use :: soroban_sdk"),
            "binds the sdk through the extern prelude, not the caller's scope: {out}"
        );
        assert!(
            out.contains("our_dao :: Client :: new"),
            "constructs the client: {out}"
        );
        assert!(
            out.contains("Address :: from_str"),
            "builds the address: {out}"
        );
        assert!(out.contains(A), "bakes the resolved id: {out}");
    }

    #[test]
    fn expand_token_client_binds_the_sdk_token_client_not_contractimport() {
        let env: syn::Expr = parse2(quote!(env)).unwrap();
        let out = expand_token_client(&env, A).to_string();
        assert!(
            !out.contains("contractimport"),
            "no wasm to generate types from: {out}"
        );
        assert!(
            out.contains("token :: TokenClient :: new"),
            "binds the SDK's standard token client: {out}"
        );
        assert!(
            out.contains("use :: soroban_sdk"),
            "binds the sdk through the extern prelude, not the caller's scope: {out}"
        );
        assert!(
            out.contains("Address :: from_str"),
            "builds the address: {out}"
        );
        assert!(out.contains(A), "bakes the resolved id: {out}");
    }

    // `import_contract` itself takes a real `proc_macro::TokenStream`, which
    // panics outside an actual macro expansion — so these test the same
    // asset-literal building blocks `import_contract` composes (see its
    // `is_asset_syntax` branch) rather than calling the entry point directly,
    // consistent with `expand`/`resolve_address` being tested the same way.
    #[test]
    fn xlm_resolves_offline_to_the_local_native_asset_id() {
        // The well-known local-network native asset id (see asset.rs's own
        // `parse_native` test for the full per-network table).
        let (id, code) = asset::generate_asset_id("xlm", &stellar_build::Network::Local).unwrap();
        assert_eq!(code, "xlm");
        assert_eq!(
            id.to_string(),
            "CDMLFMKMMD7MWZP3FKUBZPVHTUEDLSX4BYGYKH4GCESXYHS3IHQ4EIG4"
        );
    }

    #[test]
    fn invalid_asset_literal_reports_a_specific_error() {
        let err =
            asset::generate_asset_id("USDC:not-a-key", &stellar_build::Network::Local).unwrap_err();
        assert!(err.contains("invalid issuer"), "{err}");
    }
}

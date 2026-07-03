//! The `import_contract!` proc-macro: resolve a named Stellar Registry contract
//! to a type-safe client already bound to its deployed on-chain address.
extern crate proc_macro;

use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
};

/// Path to the compiling crate's `Cargo.toml`.
fn manifest() -> PathBuf {
    PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("failed to find cargo manifest"))
        .join("Cargo.toml")
}

/// Rust module identifier from a (possibly channel-prefixed) registry name:
/// final `/`-segment with `-` replaced by `_`.
fn mod_name_from(name_part: &str) -> String {
    name_part
        .rsplit('/')
        .next()
        .unwrap_or(name_part)
        .replace('-', "_")
}

/// Split `"name@v1.2.3"` / `"name@1.2.3"` into `(name, version-without-leading-v)`.
fn split_version(raw: &str) -> (String, Option<String>) {
    match raw.split_once('@') {
        Some((name, ver)) => (
            name.to_string(),
            Some(ver.strip_prefix('v').unwrap_or(ver).to_string()),
        ),
        None => (raw.to_string(), None),
    }
}

/// Env var a caller can set to bypass the network:
/// `STELLAR_CONTRACT_ID_<NAME>`, NAME = uppercased module name with any
/// non-alphanumeric replaced by `_`.
fn env_var_name(mod_name: &str) -> String {
    let sanitized: String = mod_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect();
    format!("STELLAR_CONTRACT_ID_{sanitized}")
}

/// Validate a `C…` contract strkey; return it trimmed.
fn validate_contract_id(s: &str) -> Result<String, String> {
    let t = s.trim();
    t.parse::<stellar_strkey::Contract>()
        .map(|_| t.to_string())
        .map_err(|_| format!("not a valid contract id (C… strkey): {t:?}"))
}

/// `<target_dir>/<mod_name>.id` — sibling of the wasm the client imports.
/// Keyed by name only: a deployed instance's address is version-independent.
fn cache_id_path(target_dir: &Path, mod_name: &str) -> PathBuf {
    target_dir.join(mod_name).with_extension("id")
}

/// Resolve the deployed address, first hit wins. All IO is injected so the
/// precedence is unit-testable without a network or filesystem.
fn resolve_address(
    env_lookup: impl Fn(&str) -> Option<String>,
    env_var: &str,
    cache: Option<String>,
    no_registry: bool,
    fetch: impl FnOnce() -> Result<String, String>,
) -> Result<String, String> {
    if let Some(v) = env_lookup(env_var) {
        return validate_contract_id(&v);
    }
    if let Some(c) = cache {
        return validate_contract_id(&c);
    }
    if no_registry {
        return Err(format!(
            "No cached contract id and STELLAR_NO_REGISTRY=1 so not checking the Registry. \
             Set {env_var}, or run `stellar registry fetch-contract-id <name>` and rebuild."
        ));
    }
    validate_contract_id(&fetch()?)
}

/// Shell out to the `stellar` CLI to look up a deployed contract's id by name.
/// Network selection is delegated to the CLI's own config (`STELLAR_NETWORK`).
fn fetch_contract_id(lookup_name: &str) -> Result<String, String> {
    let out = Command::new("stellar")
        .args(["registry", "fetch-contract-id", lookup_name])
        .output()
        .map_err(|e| {
            format!(
                "failed to run `stellar registry fetch-contract-id`: {e}. \
                 Install it with `cargo install stellar-registry-cli` and try again."
            )
        })?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(format!(
            "Could not resolve a contract id for `{lookup_name}`. \
             Check the name & network and try again (https://stellar.rgstry.xyz), \
             run `stellar registry fetch-contract-id {lookup_name}` yourself, \
             or set STELLAR_NO_REGISTRY=1 to skip the registry lookup.\n{}",
            String::from_utf8_lossy(&out.stderr)
        ))
    }
}

use proc_macro2::Span;
use quote::quote;
use syn::{
    Expr, Ident, LitStr, Token,
    parse::{Parse, ParseStream},
};

/// `import_contract!(env_expr, name)` — `name` is a bare ident or a string
/// literal using the same grammar as `import_contract_client!`.
struct Input {
    env: Expr,
    name_raw: String,
    name_span: Span,
}

impl Parse for Input {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let env: Expr = input.parse()?;
        input.parse::<Token![,]>()?;
        let name_span = input.span();
        let name_raw = if input.peek(LitStr) {
            input.parse::<LitStr>()?.value()
        } else {
            input.parse::<Ident>()?.to_string()
        };
        Ok(Self {
            env,
            name_raw,
            name_span,
        })
    }
}

/// Emit a block expression: delegate wasm/type generation to
/// `import_contract_client!`, then construct the client bound to the baked
/// address. `name_raw` is passed through verbatim (version included) so the
/// delegated macro resolves the matching wasm.
fn expand(
    env: &Expr,
    name_raw: &str,
    mod_ident: &Ident,
    address: &str,
) -> proc_macro2::TokenStream {
    quote! {
        {
            ::stellar_registry::import_contract_client!(#name_raw);
            let __env: &::soroban_sdk::Env = #env;
            #mod_ident::Client::new(
                __env,
                &::soroban_sdk::Address::from_str(__env, #address),
            )
        }
    }
}

#[cfg(test)]
mod helpers {
    use super::*;
    use std::path::Path;

    // A real, valid contract strkey (from soroban-sdk docs).
    const VALID: &str = "CBESJIMX7J53SWJGJ7WQ6QTLJI4S5LPPJNC2BNVD63GIKAYCDTDOO322";

    #[test]
    fn mod_name_strips_prefix_and_hyphens() {
        assert_eq!(
            mod_name_from("unverified/registry_tansu_manager"),
            "registry_tansu_manager"
        );
        assert_eq!(mod_name_from("guess-the-number"), "guess_the_number");
        assert_eq!(mod_name_from("a/b/c"), "c");
        assert_eq!(mod_name_from("registry"), "registry");
    }

    #[test]
    fn split_version_optional_v() {
        assert_eq!(
            split_version("our_dao@v0.1.0"),
            ("our_dao".into(), Some("0.1.0".into()))
        );
        assert_eq!(split_version("x@1.2.3"), ("x".into(), Some("1.2.3".into())));
        assert_eq!(split_version("x"), ("x".into(), None));
    }

    #[test]
    fn env_var_name_uppercases_and_sanitizes() {
        assert_eq!(
            env_var_name("registry_tansu_manager"),
            "STELLAR_CONTRACT_ID_REGISTRY_TANSU_MANAGER"
        );
        assert_eq!(
            env_var_name("guess_the_number"),
            "STELLAR_CONTRACT_ID_GUESS_THE_NUMBER"
        );
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
    fn cache_id_path_is_wasm_sibling() {
        assert_eq!(
            cache_id_path(Path::new("target"), "our_dao"),
            Path::new("target/our_dao.id")
        );
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
    fn env_override_wins() {
        let got = resolve_address(
            |k| (k == "STELLAR_CONTRACT_ID_FOO").then(|| A.to_string()),
            "STELLAR_CONTRACT_ID_FOO",
            Some(B.to_string()),
            false,
            no_fetch,
        );
        assert_eq!(got.unwrap(), A);
    }

    #[test]
    fn cache_used_when_no_env() {
        let got = resolve_address(|_| None, "X", Some(B.to_string()), false, no_fetch);
        assert_eq!(got.unwrap(), B);
    }

    #[test]
    fn no_registry_errors_without_env_or_cache() {
        let got = resolve_address(|_| None, "X", None, true, no_fetch);
        assert!(got.unwrap_err().contains("STELLAR_NO_REGISTRY"));
    }

    #[test]
    fn fetch_is_last_resort() {
        let got = resolve_address(|_| None, "X", None, false, || Ok(A.to_string()));
        assert_eq!(got.unwrap(), A);
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
        let input: Input = parse2(quote!(env, "unverified/our_dao@v0.1.0")).unwrap();
        assert_eq!(input.name_raw, "unverified/our_dao@v0.1.0");
    }

    #[test]
    fn parses_env_and_ident_name() {
        let input: Input = parse2(quote!(env, registry)).unwrap();
        assert_eq!(input.name_raw, "registry");
    }

    #[test]
    fn expand_emits_delegation_and_bound_client() {
        let env: syn::Expr = parse2(quote!(env)).unwrap();
        let ident = Ident::new("our_dao", Span::call_site());
        let out = expand(&env, "unverified/our_dao@v0.1.0", &ident, A).to_string();
        assert!(
            out.contains("import_contract_client"),
            "delegates wasm import: {out}"
        );
        assert!(
            out.contains("\"unverified/our_dao@v0.1.0\""),
            "passes original name: {out}"
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
}

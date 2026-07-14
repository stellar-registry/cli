//! The `import_contract!` proc-macro: resolve a named Stellar Registry contract
//! to a type-safe client already bound to its deployed on-chain address, with
//! the client types generated from the deployed contract's own wasm.
extern crate proc_macro;

use proc_macro::TokenStream;
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

/// A name whose derived module identifier is empty (e.g. `""`, `"foo/"`) cannot
/// form a valid Rust identifier. Reject it up front so the macro emits a
/// `compile_error!` instead of panicking inside `Ident::new`.
fn check_mod_name(mod_name: &str) -> Result<(), String> {
    if mod_name.is_empty() {
        Err(
            "import_contract! needs a contract name whose module identifier is non-empty \
             (got an empty name, or one like \"foo/\")"
                .to_string(),
        )
    } else {
        Ok(())
    }
}

/// A deployed contract has no version — only a wasm does. Reject the `@version`
/// syntax `import_contract_client!` accepts, pointing the caller at the plain form.
fn reject_version(raw: &str) -> Result<(), String> {
    if raw.contains('@') {
        Err(format!(
            "import_contract! does not take a version — a deployed contract has no version \
             (got {raw:?}). Use just the contract name, e.g. `import_contract!(env, our_dao)`."
        ))
    } else {
        Ok(())
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

/// `<target_dir>/<mod_name>.id` — cached deployed address. Keyed by name only:
/// a deployed instance's address is version-independent.
fn cache_id_path(target_dir: &Path, mod_name: &str) -> PathBuf {
    target_dir.join(mod_name).with_extension("id")
}

/// `<target_dir>/<mod_name>.wasm` — the deployed contract's wasm, fetched by
/// address, that `contractimport!` reads to generate the client types.
fn cache_wasm_path(target_dir: &Path, mod_name: &str) -> PathBuf {
    target_dir.join(mod_name).with_extension("wasm")
}

/// Resolve the deployed address. Precedence, first hit wins:
///   1. `STELLAR_CONTRACT_ID_<NAME>` env override — explicit, no flag check.
///   2. `STELLAR_NO_REGISTRY=1` — offline: the `<name>.id` cache, no flag check.
///   3. otherwise online — `fetch` (which also fails if the contract is flagged)
///      and refresh the cache. The cache is deliberately NOT consulted online,
///      so a contract flagged after the first build cannot slip through a stale
///      `.id`. All IO is injected so precedence is unit-testable offline.
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
    if no_registry {
        return match cache {
            Some(c) => validate_contract_id(&c),
            None => Err(format!(
                "STELLAR_NO_REGISTRY=1 but no cached contract id. Set {env_var}, or build \
                 online once (which caches it), then rebuild offline."
            )),
        };
    }
    validate_contract_id(&fetch()?)
}

/// Shell out to the `stellar` CLI to look up a deployed contract's id by name,
/// failing if it is flagged as compromised (`--reject-flagged`). Network
/// selection is delegated to the CLI's own config (`STELLAR_NETWORK`).
fn fetch_contract_id(lookup_name: &str) -> Result<String, String> {
    let out = Command::new("stellar")
        .args([
            "registry",
            "fetch-contract-id",
            lookup_name,
            "--reject-flagged",
        ])
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
            "Could not resolve `{lookup_name}`. It may not be registered, may be flagged as \
             compromised, or the network may be wrong (https://stellar.rgstry.xyz). Run \
             `stellar registry fetch-contract-id {lookup_name}` yourself, or set \
             STELLAR_NO_REGISTRY=1 with a cached id to skip the registry lookup.\n{}",
            String::from_utf8_lossy(&out.stderr)
        ))
    }
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

use proc_macro2::Span;
use quote::quote;
use syn::{
    Expr, Ident, LitStr, Token,
    parse::{Parse, ParseStream},
    parse_macro_input,
};

/// `import_contract!(env_expr, name)` — `name` is a bare ident or a string
/// literal (optionally channel-prefixed, e.g. `"unverified/our_dao"`).
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

/// Emit a block expression: generate the client types from the deployed
/// contract's own wasm, then construct the client bound to the baked address.
fn expand(
    env: &Expr,
    mod_ident: &Ident,
    wasm_path: &str,
    address: &str,
) -> proc_macro2::TokenStream {
    quote! {
        {
            mod #mod_ident {
                use super::soroban_sdk;
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

/// Generate a type-safe client for a deployed, registry-named contract, already
/// bound to its on-chain address — collapsing "look up the address" and
/// "generate the client type" into one call.
///
/// ```ignore
/// // `env: &Env`
/// let dao = stellar_registry::import_contract!(env, our_dao);
/// dao.create_proposal(/* ... */);
/// ```
///
/// `name` is a bare ident or string literal, optionally channel-prefixed
/// (`import_contract!(env, "unverified/our_dao")`). A deployed contract has no
/// version, so **no `@version` is accepted**. The client types are generated
/// from the deployed contract's *own* on-chain wasm, so a contract whose wasm
/// was never published to the registry still works.
///
/// Resolved at build time:
/// - **address** — `STELLAR_CONTRACT_ID_<NAME>` env override → (offline only)
///   `target/stellar/<network>/<name>.id` cache → `stellar registry
///   fetch-contract-id`. The online path **fails compilation if the contract is
///   flagged as compromised** in the registry.
/// - **wasm** — `stellar contract fetch --id <address>`, cached beside the id.
///
/// `STELLAR_NO_REGISTRY=1` forbids the network calls (requires a cached id +
/// wasm, and skips the flag check). Because a real on-chain address is baked in,
/// use this in real / integration builds; if the named contract is redeployed or
/// upgraded, clear the cache (`cargo clean`) and rebuild.
#[proc_macro]
pub fn import_contract(input: TokenStream) -> TokenStream {
    let Input {
        env,
        name_raw,
        name_span,
    } = parse_macro_input!(input as Input);

    let err = |msg: String| -> TokenStream {
        syn::Error::new(name_span, msg).to_compile_error().into()
    };

    if let Err(msg) = reject_version(&name_raw) {
        return err(msg);
    }
    let mod_name = mod_name_from(&name_raw);
    if let Err(msg) = check_mod_name(&mod_name) {
        return err(msg);
    }
    let mod_ident = Ident::new(&mod_name, name_span);
    let evar = env_var_name(&mod_name);

    let no_registry = env::var("STELLAR_NO_REGISTRY").as_deref() == Ok("1");
    let target_dir = match stellar_build::get_target_dir(&manifest()) {
        Ok(dir) => dir,
        Err(e) => return err(format!("could not determine the cargo target dir: {e}")),
    };
    let id_path = cache_id_path(&target_dir, &mod_name);
    let wasm_path = cache_wasm_path(&target_dir, &mod_name);
    let cache = std::fs::read_to_string(&id_path).ok();

    // 1. Resolve the deployed address (and, online, enforce the flag check).
    let address = match resolve_address(
        |k| env::var(k).ok(),
        &evar,
        cache,
        no_registry,
        || {
            let addr = validate_contract_id(&fetch_contract_id(&name_raw)?)?;
            let _ = std::fs::write(&id_path, &addr);
            Ok(addr)
        },
    ) {
        Ok(a) => a,
        Err(msg) => return err(msg),
    };

    // 2. Ensure the deployed contract's wasm is on disk for `contractimport!`.
    if !wasm_path.exists() {
        if no_registry {
            return err(format!(
                "STELLAR_NO_REGISTRY=1 but no cached wasm at {}. Build online once (which \
                 fetches it) then rebuild offline.",
                wasm_path.display()
            ));
        }
        if let Err(msg) = fetch_wasm(&address, &wasm_path) {
            return err(msg);
        }
    }

    // 3. Generate the client from that wasm and bind it to the address.
    expand(&env, &mod_ident, &wasm_path.to_string_lossy(), &address).into()
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
    fn reject_version_rejects_at() {
        assert!(reject_version("our_dao").is_ok());
        assert!(reject_version("unverified/our_dao").is_ok());
        assert!(reject_version("our_dao@v1.0.0").is_err());
        assert!(reject_version("our_dao@1.0.0").is_err());
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
    fn cache_paths_are_target_siblings() {
        assert_eq!(
            cache_id_path(Path::new("target"), "our_dao"),
            Path::new("target/our_dao.id")
        );
        assert_eq!(
            cache_wasm_path(Path::new("target"), "our_dao"),
            Path::new("target/our_dao.wasm")
        );
    }

    #[test]
    fn check_mod_name_rejects_empty_identifiers() {
        assert!(check_mod_name("our_dao").is_ok());
        assert!(check_mod_name("").is_err());
        assert!(check_mod_name(&mod_name_from("foo/")).is_err());
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
    fn offline_uses_cache() {
        let got = resolve_address(|_| None, "X", Some(B.to_string()), true, no_fetch);
        assert_eq!(got.unwrap(), B);
    }

    #[test]
    fn offline_errors_without_env_or_cache() {
        let got = resolve_address(|_| None, "X", None, true, no_fetch);
        assert!(got.unwrap_err().contains("STELLAR_NO_REGISTRY"));
    }

    #[test]
    fn online_fetches_and_ignores_stale_cache() {
        // A cached id must NOT short-circuit the online fetch (+ flag check).
        let got = resolve_address(|_| None, "X", Some(B.to_string()), false, || Ok(A.to_string()));
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
        let input: Input = parse2(quote!(env, "unverified/our_dao")).unwrap();
        assert_eq!(input.name_raw, "unverified/our_dao");
    }

    #[test]
    fn parses_env_and_ident_name() {
        let input: Input = parse2(quote!(env, registry)).unwrap();
        assert_eq!(input.name_raw, "registry");
    }

    #[test]
    fn expand_emits_contractimport_and_bound_client() {
        let env: syn::Expr = parse2(quote!(env)).unwrap();
        let ident = Ident::new("our_dao", Span::call_site());
        let out = expand(&env, &ident, "/tmp/target/stellar/local/our_dao.wasm", A).to_string();
        assert!(
            out.contains("contractimport"),
            "generates types from the wasm: {out}"
        );
        assert!(
            !out.contains("import_contract_client"),
            "does NOT delegate to import_contract_client!: {out}"
        );
        assert!(
            out.contains("our_dao.wasm"),
            "references the fetched wasm: {out}"
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

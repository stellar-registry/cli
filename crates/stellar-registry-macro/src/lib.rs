//! The `import_contract!` proc-macro: resolve a named Stellar Registry contract
//! to a type-safe client already bound to its deployed on-chain address.
extern crate proc_macro;

use std::{
    env,
    path::{Path, PathBuf},
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

use std::env;
use std::path::{Path, PathBuf};

use proc_macro2::Span;
use quote::quote;
use syn::{
    Ident,
    parse::{Parse, ParseStream, Result},
};

use stellar_registry_name::Versioned;

use crate::util::{Name, explorer_url, manifest, mod_ident, network_name};

pub(crate) fn import_contract_client(
    input: proc_macro::TokenStream,
) -> Result<proc_macro2::TokenStream> {
    let WasmBinary { mod_name, file } = syn::parse::<WasmBinary>(input)?;

    Ok(quote! {
        pub(crate) mod #mod_name {
            #![allow(clippy::ref_option, clippy::too_many_arguments)]
            use super::soroban_sdk;
            soroban_sdk::contractimport!(file = #file);
        }
    })
}

struct WasmBinary {
    pub mod_name: Ident,
    pub file: String,
}

impl Parse for WasmBinary {
    fn parse(input: ParseStream) -> Result<Self> {
        let name: Name = input.parse()?;
        // A published wasm may carry an `@version` suffix, so this parses as
        // `Versioned`; a malformed version is a compile error, never silently
        // "latest".
        let wasm: Versioned = name.parse_as()?;
        let mod_name = mod_ident(wasm.name(), name.span())?;
        let wasm_path = resolve_wasm_path(&wasm, &mod_name)?;
        let file = wasm_path.display().to_string();
        Ok(Self { mod_name, file })
    }
}

/// `[<channel>__]<mod_name>[_<version>]` — the channel is part of a published
/// wasm's identity, so `unverified/foo` never shares a file with `foo`. Bare
/// names stay bare so workspace-compiled contracts (written by stellar-build
/// as `<mod_name>.wasm`) are still found.
fn wasm_file_stem(channel: Option<&str>, mod_name: &Ident, version: Option<&str>) -> String {
    let mut stem = match channel {
        Some(channel) => format!("{channel}__{mod_name}"),
        None => mod_name.to_string(),
    };
    if let Some(v) = version {
        stem = format!("{stem}_{}", v.replace('.', "_"));
    }
    stem
}

fn build_local_wasm_path(
    target_dir: &Path,
    channel: Option<&str>,
    mod_name: &Ident,
    version: Option<&str>,
) -> PathBuf {
    target_dir
        .join(wasm_file_stem(channel, mod_name, version))
        .with_extension("wasm")
}

fn resolve_wasm_path(wasm: &Versioned, mod_name: &Ident) -> Result<PathBuf> {
    let span = mod_name.span();
    let version = wasm.version().map(ToString::to_string);
    let target_dir = stellar_build::get_target_dir(&manifest()?).map_err(|e| {
        syn::Error::new(
            span,
            format!("could not determine the cargo target dir: {e}"),
        )
    })?;
    let local_path = build_local_wasm_path(
        &target_dir,
        wasm.name().channel(),
        mod_name,
        version.as_deref(),
    );

    // 1. Check local build target
    if local_path.exists() {
        return canonicalized(&local_path, span);
    }

    // 2. If STELLAR_NO_REGISTRY set to 1, error
    if env::var("STELLAR_NO_REGISTRY").as_deref() == Ok("1") {
        return Err(syn::Error::new(
            span,
            format!(
                "No local wasm found and STELLAR_NO_REGISTRY=1 so not checking Registry. \
                Download manually with `stellar registry download \"{wasm}\"`",
            ),
        ));
    }

    // 3. if var absent or set to something else, try to download
    download_from_registry(wasm, &local_path, span, version.as_deref())
}

fn canonicalized(path: &Path, span: Span) -> Result<PathBuf> {
    path.canonicalize().map_err(|e| {
        syn::Error::new(
            span,
            format!("could not canonicalize {}: {e}", path.display()),
        )
    })
}

fn download_from_registry(
    wasm: &Versioned,
    local_path: &Path,
    span: Span,
    version: Option<&str>,
) -> Result<PathBuf> {
    let lookup_name = wasm.name().to_string();

    // 1. create `target/stellar/[network]` directory, if not already present
    if let Some(parent) = local_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            syn::Error::new(span, format!("could not create {}: {e}", parent.display()))
        })?;
    }

    // 2. download using `stellar registry download`
    let mut args = vec![
        "registry".to_string(),
        "download".to_string(),
        lookup_name.clone(),
        "--out-file".to_string(),
        local_path.display().to_string(),
    ];
    if let Some(v) = version {
        args.push("--version".to_string());
        args.push(v.to_string());
    }
    let out = std::process::Command::new("stellar")
        .args(&args)
        .output()
        .map_err(|e| {
            syn::Error::new(
                span,
                format!(
                    "failed to run `stellar registry download`: {e}. Install the Stellar CLI, \
                     then `cargo install stellar-registry-cli` for the registry plugin."
                ),
            )
        })?;

    // 3. check status, mapping failures to the most specific message the
    //    CLI's stderr allows (mirrors fetch_contract_id in contract.rs).
    if out.status.success() && local_path.exists() {
        return canonicalized(local_path, span);
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    if stderr.contains("unexpected argument")
        || (stderr.contains("unrecognized subcommand") && stderr.contains("download"))
    {
        return Err(syn::Error::new(
            span,
            format!(
                "the installed `stellar registry` plugin is too old for \
                 import_contract_client!. Upgrade it with \
                 `cargo install stellar-registry-cli --force`.\n\nstderr:\n{stderr}"
            ),
        ));
    }
    if stderr.contains("unrecognized subcommand") || stderr.contains("no such command") {
        return Err(syn::Error::new(
            span,
            format!(
                "the `stellar registry` plugin is not installed. Install it with \
                 `cargo install stellar-registry-cli`.\n\nstderr:\n{stderr}"
            ),
        ));
    }
    let network = network_name();
    let name_check = explorer_url(&network).map_or_else(
        || "\n1. check the name & network and try again".to_string(),
        |url| format!("\n1. check that you got the name right: {url}/wasms"),
    );
    let local_path = local_path.display().to_string();
    Err(syn::Error::new(
        span,
        format!(
            "Could not find Wasm `{lookup_name}` on {network}. Checked: \
            \n\n• {local_path} \
            \n• `stellar registry download {lookup_name}` \
            \n\nYou can: \
            {name_check} \
            \n2. add this Wasm to your local `target` directory manually \
            (perhaps by compiling a contract) \
            \n3. run `stellar registry download {lookup_name}` yourself. \
            \n\nSet STELLAR_NO_REGISTRY=1 to skip registry lookup.\
            \n\nstderr from `stellar registry download`:\n{stderr}"
        ),
    ))
}

#[cfg(test)]
mod test_build_local_wasm_path {
    use super::*;
    use std::path::Path;

    fn ident(string: &str) -> Ident {
        Ident::new(string, proc_macro2::Span::call_site())
    }

    #[test]
    fn includes_underscore_delimited_version() {
        let path = build_local_wasm_path(Path::new("target"), None, &ident("a"), Some("1.0.0"));
        assert_eq!(path, Path::new("target/a_1_0_0.wasm"));
    }

    #[test]
    fn no_version() {
        let path = build_local_wasm_path(Path::new("target"), None, &ident("registry"), None);
        assert_eq!(path, Path::new("target/registry.wasm"));
    }

    #[test]
    fn prerelease_version() {
        let path =
            build_local_wasm_path(Path::new("target"), None, &ident("foo"), Some("1.0.0-rc.1"));
        assert_eq!(path, Path::new("target/foo_1_0_0-rc_1.wasm"));
    }

    #[test]
    fn channel_is_part_of_the_stem() {
        // `unverified/foo` and `foo` are different published wasms.
        let channeled =
            build_local_wasm_path(Path::new("target"), Some("unverified"), &ident("foo"), None);
        assert_eq!(channeled, Path::new("target/unverified__foo.wasm"));
        assert_ne!(
            channeled,
            build_local_wasm_path(Path::new("target"), None, &ident("foo"), None)
        );
        assert_eq!(
            build_local_wasm_path(
                Path::new("target"),
                Some("unverified"),
                &ident("foo"),
                Some("1.0.0")
            ),
            Path::new("target/unverified__foo_1_0_0.wasm")
        );
    }
}

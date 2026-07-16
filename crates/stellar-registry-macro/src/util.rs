use std::path::PathBuf;

use proc_macro2::Span;
use stellar_registry_build::name;
use syn::{
    Ident, LitStr,
    parse::{Parse, ParseStream},
};

/// Path to the compiling crate's `Cargo.toml`.
pub(crate) fn manifest() -> syn::Result<PathBuf> {
    let dir = std::env::var("CARGO_MANIFEST_DIR").map_err(|_| {
        syn::Error::new(
            Span::call_site(),
            "CARGO_MANIFEST_DIR is not set; are you compiling with cargo?",
        )
    })?;
    Ok(PathBuf::from(dir).join("Cargo.toml"))
}

/// A contract name argument: a bare ident (`registry`) or a string literal
/// (`"unverified/guess-the-number@1.0.0"`).
pub(crate) enum Name {
    Ident(Ident),
    LitStr(LitStr),
}

impl Parse for Name {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.peek(LitStr) {
            Ok(Self::LitStr(input.parse()?))
        } else {
            Ok(Self::Ident(input.parse()?))
        }
    }
}

impl Name {
    pub(crate) fn span(&self) -> Span {
        match self {
            Self::Ident(ident) => ident.span(),
            Self::LitStr(lit) => lit.span(),
        }
    }

    pub(crate) fn raw(&self) -> String {
        match self {
            Self::Ident(ident) => ident.to_string(),
            Self::LitStr(lit) => lit.value(),
        }
    }

    /// Parse into a typed registry name ([`name::Prefixed`] /
    /// [`name::Versioned`]), reporting failures at this argument's span.
    pub(crate) fn parse_as<T>(&self) -> syn::Result<T>
    where
        T: std::str::FromStr<Err = name::Error>,
    {
        self.raw()
            .parse()
            .map_err(|e| syn::Error::new(self.span(), e))
    }
}

/// Rust module `Ident` for a parsed name (`-` → `_`), or a compile error at
/// `span` if the result is not a valid identifier (e.g. starts with a digit,
/// or is a Rust keyword).
pub(crate) fn mod_ident(name: &name::Prefixed, span: Span) -> syn::Result<Ident> {
    let mod_name = name.mod_name();
    syn::parse_str::<Ident>(&mod_name)
        .map(|mut ident| {
            ident.set_span(span);
            ident
        })
        .map_err(|_| {
            syn::Error::new(
                span,
                format!(
                    "cannot derive a Rust module name from `{name}`: `{mod_name}` is not a valid identifier"
                ),
            )
        })
}

/// `STELLAR_NETWORK` identifier (defaulting to `local`) — the same value
/// `stellar_build::get_target_dir` uses for the network segment of cache paths.
pub(crate) fn network_name() -> String {
    std::env::var("STELLAR_NETWORK").unwrap_or_else(|_| "local".to_owned())
}

/// The registry explorer for the network, if one exists.
pub(crate) fn explorer_url(network: &str) -> Option<&'static str> {
    match network {
        "testnet" => Some("https://testnet.rgstry.xyz/contracts"),
        "mainnet" => Some("https://stellar.rgstry.xyz/contracts"),
        _ => None,
    }
}

/// Bridge a fallible macro implementation to the `proc_macro` entry point:
/// `Err` becomes a `compile_error!` at the error's span.
pub(crate) trait ProcMacroWrapper {
    fn to_token_stream(self) -> proc_macro::TokenStream;
}

impl ProcMacroWrapper for syn::Result<proc_macro2::TokenStream> {
    fn to_token_stream(self) -> proc_macro::TokenStream {
        self.map_or_else(|e| e.to_compile_error().into(), Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;
    use stellar_registry_build::name::{Prefixed, Versioned};

    fn name(tokens: proc_macro2::TokenStream) -> Name {
        syn::parse2(tokens).unwrap()
    }

    #[test]
    fn ident_name() {
        assert_eq!(name(quote!(registry)).raw(), "registry");
    }

    #[test]
    fn litstr_name() {
        assert_eq!(
            name(quote!("unverified/guess-the-number")).raw(),
            "unverified/guess-the-number"
        );
    }

    #[test]
    fn parse_as_prefixed_rejects_version() {
        let err = name(quote!("our_dao@1.0.0"))
            .parse_as::<Prefixed>()
            .unwrap_err();
        assert!(err.to_string().contains("version"), "{err}");
    }

    #[test]
    fn parse_as_versioned_accepts_version() {
        let wasm: Versioned = name(quote!("registry@v1.0.0")).parse_as().unwrap();
        assert_eq!(wasm.version().unwrap().to_string(), "1.0.0");
    }

    #[test]
    fn parse_as_reports_bad_versions_instead_of_dropping_them() {
        let err = name(quote!("registry@garbage"))
            .parse_as::<Versioned>()
            .unwrap_err();
        assert!(err.to_string().contains("invalid version"), "{err}");
    }

    #[test]
    fn parse_as_rejects_empty_string() {
        let err = name(quote!("")).parse_as::<Prefixed>().unwrap_err();
        assert!(err.to_string().contains("empty"), "{err}");
    }

    #[test]
    fn mod_ident_derives_underscored_module() {
        let p: Prefixed = "unverified/guess-the-number".parse().unwrap();
        let ident = mod_ident(&p, Span::call_site()).unwrap();
        assert_eq!(ident.to_string(), "guess_the_number");
    }

    #[test]
    fn mod_ident_errors_instead_of_panicking_on_digit_start() {
        let p: Prefixed = "123bad".parse().unwrap();
        let err = mod_ident(&p, Span::call_site()).unwrap_err();
        assert!(err.to_string().contains("not a valid identifier"), "{err}");
    }

    #[test]
    fn mod_ident_errors_on_keywords() {
        let p: Prefixed = "mod".parse().unwrap();
        assert!(mod_ident(&p, Span::call_site()).is_err());
    }

    #[test]
    fn explorer_urls() {
        assert_eq!(
            explorer_url("testnet"),
            Some("https://testnet.rgstry.xyz/contracts")
        );
        assert_eq!(
            explorer_url("mainnet"),
            Some("https://stellar.rgstry.xyz/contracts")
        );
        assert_eq!(explorer_url("local"), None);
        assert_eq!(explorer_url("futurenet"), None);
    }
}

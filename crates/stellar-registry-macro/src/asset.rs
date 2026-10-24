use proc_macro2::TokenStream;

use stellar_xdr as xdr;
use xdr::WriteXdr;

use quote::quote;
use syn::LitStr;

pub(crate) fn import_asset(input: proc_macro::TokenStream) -> syn::Result<TokenStream> {
    let lit: LitStr = syn::parse(input)?;
    parse_literal(&lit)
}

/// Parse `"native"`, `"xlm"`, or `"CODE:ISSUER"` into an XDR asset plus the
/// bare code (used as the generated module name).
pub(crate) fn parse_asset(s: &str) -> Result<(xdr::Asset, String), String> {
    if s == "native" || s == "xlm" {
        return Ok((xdr::Asset::Native, s.to_string()));
    }
    let Some((code, issuer)) = s.split_once(':') else {
        return Err(format!(
            "invalid asset `{s}`: expected `native`, `xlm`, or `CODE:ISSUER`"
        ));
    };
    if code.is_empty() || code.len() > 12 || !code.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(format!(
            "invalid asset code `{code}` in `{s}`: expected 1-12 ASCII letters or digits"
        ));
    }
    let issuer: xdr::AccountId = issuer
        .parse()
        .map_err(|e| format!("invalid issuer account in `{s}`: {e}"))?;
    let asset_code: xdr::AssetCode = code
        .parse()
        .map_err(|e| format!("invalid asset code `{code}` in `{s}`: {e}"))?;
    Ok((
        match asset_code {
            xdr::AssetCode::CreditAlphanum4(asset_code) => {
                xdr::Asset::CreditAlphanum4(xdr::AlphaNum4 { asset_code, issuer })
            }
            xdr::AssetCode::CreditAlphanum12(asset_code) => {
                xdr::Asset::CreditAlphanum12(xdr::AlphaNum12 { asset_code, issuer })
            }
        },
        code.to_string(),
    ))
}

/// The asset's XDR encoding, which `env.deployer().with_stellar_asset(..)`
/// takes to derive the SAC address for whichever network the contract runs on.
pub(crate) fn asset_xdr(asset: &str) -> Result<(Vec<u8>, String), String> {
    let (asset, code) = parse_asset(asset)?;
    let xdr = asset
        .to_xdr(xdr::Limits::none())
        .map_err(|e| format!("failed to encode the asset: {e}"))?;
    Ok((xdr, code))
}

/// Tokens deriving the SAC address for `asset_xdr` on the executing network.
/// Not a constant: the same Wasm must resolve correctly on every network.
pub(crate) fn runtime_address(env: &TokenStream, asset_xdr: &[u8]) -> TokenStream {
    let asset_xdr = syn::LitByteStr::new(asset_xdr, proc_macro2::Span::call_site());
    quote! {
        #env.deployer()
            .with_stellar_asset(::soroban_sdk::Bytes::from_slice(#env, #asset_xdr))
            .deployed_address()
    }
}

/// Generate a module (named after the asset code) exposing the asset's
/// contract id and token clients.
pub(crate) fn parse_literal(lit_str: &LitStr) -> syn::Result<TokenStream> {
    let err = |msg: String| syn::Error::new(lit_str.span(), msg);
    let (asset_xdr, code) = asset_xdr(&lit_str.value()).map_err(err)?;
    let mod_name: syn::Ident = syn::parse_str(&code).map_err(|_| {
        err(format!(
            "cannot use asset code `{code}` as a Rust module name"
        ))
    })?;
    let address = runtime_address(&quote!(env), &asset_xdr);
    Ok(quote! {
        #[allow(non_snake_case)]
        pub(crate) mod #mod_name {
            #[allow(unused_imports)]
            use super::*;
            /// Contract id for the Stellar Asset Contract on the executing network.
            /// Does not deploy the contract or check that it exists.
            pub fn contract_id(env: &soroban_sdk::Env) -> soroban_sdk::Address {
                #address
            }
            /// Create a Stellar Asset Client for the asset which provides an admin interface
            pub fn stellar_asset_client<'a>(env: &soroban_sdk::Env) -> soroban_sdk::token::StellarAssetClient<'a> {
                soroban_sdk::token::StellarAssetClient::new(&env, &contract_id(env))
            }
            /// Create a Token Client for the asset which provides the standard token interface
            pub fn token_client<'a>(env: &soroban_sdk::Env) -> soroban_sdk::token::TokenClient<'a> {
                soroban_sdk::token::TokenClient::new(&env, &contract_id(env))
            }
        }
    })
}

#[cfg(test)]
mod test {
    use super::*;

    const ISSUER: &str = "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN";

    #[test]
    fn parse_native() {
        for name in ["native", "xlm"] {
            let (asset, code) = parse_asset(name).unwrap();
            assert_eq!(asset, xdr::Asset::Native);
            assert_eq!(code, name);
        }
    }

    #[test]
    fn generated_module_embeds_the_asset_not_an_address() {
        let lit = syn::LitStr::new(&format!("USDC:{ISSUER}"), proc_macro2::Span::call_site());
        let out = parse_literal(&lit).unwrap().to_string();
        assert!(out.contains("mod USDC"), "{out}");
        assert!(out.contains("with_stellar_asset"), "{out}");
        assert!(!out.contains("Address :: from_str"), "{out}");
    }

    #[test]
    fn errors_are_compile_errors_not_panics() {
        let cases = [
            "",                                                                         // empty
            "USDC",                                                                     // no issuer
            "USDC:not-a-key", // bad issuer
            "toolongcodehere:GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN", // >12 chars
            "US-DC:GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN", // bad char
        ];
        for case in cases {
            let lit = syn::LitStr::new(case, proc_macro2::Span::call_site());
            assert!(parse_literal(&lit).is_err(), "`{case}` should be rejected");
        }
    }

    #[test]
    fn digit_leading_code_is_a_module_name_error() {
        // `1INCH` is a legal asset code but not a legal Rust module name.
        let lit = syn::LitStr::new(
            "1INCH:GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN",
            proc_macro2::Span::call_site(),
        );
        let err = parse_literal(&lit).unwrap_err();
        assert!(err.to_string().contains("module name"), "{err}");
    }
}

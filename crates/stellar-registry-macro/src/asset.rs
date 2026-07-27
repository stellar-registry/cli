use proc_macro2::TokenStream;
use sha2::{Digest, Sha256};

use stellar_build::Network;
use stellar_xdr as xdr;
use xdr::WriteXdr;

use quote::quote;
use syn::LitStr;

pub(crate) fn import_asset(input: proc_macro::TokenStream) -> syn::Result<TokenStream> {
    let lit: LitStr = syn::parse(input)?;
    parse_literal(&lit, &Network::passphrase_from_env())
}

/// Parse `"native"`, `"xlm"`, or `"CODE:ISSUER"` into an XDR asset plus the
/// bare code (used as the generated module name).
fn parse_asset(s: &str) -> Result<(xdr::Asset, String), String> {
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

/// The Stellar Asset Contract id for `asset` on `network`, derived offline
/// from the contract-id preimage — no network call needed.
fn generate_asset_id(
    asset: &str,
    network: &Network,
) -> Result<(stellar_strkey::Contract, String), String> {
    let (asset, code) = parse_asset(asset)?;
    let network_id = xdr::Hash(network.id());
    let preimage = xdr::HashIdPreimage::ContractId(xdr::HashIdPreimageContractId {
        network_id,
        contract_id_preimage: xdr::ContractIdPreimage::Asset(asset),
    });
    let preimage_xdr = preimage
        .to_xdr(xdr::Limits::none())
        .map_err(|e| format!("failed to encode the contract id preimage: {e}"))?;
    Ok((
        stellar_strkey::Contract(Sha256::digest(preimage_xdr).into()),
        code,
    ))
}

/// Generate a module (named after the asset code) exposing the asset's
/// contract id and token clients for the build-time network.
pub(crate) fn parse_literal(lit_str: &LitStr, network: &Network) -> syn::Result<TokenStream> {
    let err = |msg: String| syn::Error::new(lit_str.span(), msg);
    let (contract_id, code) = generate_asset_id(&lit_str.value(), network).map_err(err)?;
    let contract_id = format!("{contract_id}");
    let mod_name: syn::Ident = syn::parse_str(&code).map_err(|_| {
        err(format!(
            "cannot use asset code `{code}` as a Rust module name"
        ))
    })?;
    Ok(quote! {
        #[allow(non_snake_case)]
        pub(crate) mod #mod_name {
            use super::*;
            /// Contract id for the Stellar Asset Contract
            pub fn contract_id(env: &soroban_sdk::Env) -> soroban_sdk::Address {
                soroban_sdk::Address::from_str(&env, #contract_id)
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
    use Network::*;
    const NETWORKS: [Network; 4] = [
        Network::Local,
        Network::Testnet,
        Network::Futurenet,
        Network::Mainnet,
    ];

    const USDC: &str = "USDC:GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN";

    fn expected_module(mod_name: &str, contract_id: &str) -> TokenStream {
        let mod_name: syn::Ident = syn::parse_str(mod_name).unwrap();
        quote! {
            #[allow(non_snake_case)]
            pub(crate) mod #mod_name {
                use super::*;
                /// Contract id for the Stellar Asset Contract
                pub fn contract_id(env: &soroban_sdk::Env) -> soroban_sdk::Address {
                    soroban_sdk::Address::from_str(&env, #contract_id)
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
        }
    }

    // Test for  parsing natve token
    #[test]
    fn parse_native() {
        let (asset, code) = parse_asset("native").unwrap();
        assert_eq!(asset, xdr::Asset::Native);
        assert_eq!(code, "native");
        let (asset, code) = parse_asset("xlm").unwrap();
        assert_eq!(asset, xdr::Asset::Native);
        assert_eq!(code, "xlm");
        for network in &NETWORKS {
            match (
                network,
                generate_asset_id("native", network)
                    .unwrap()
                    .0
                    .to_string()
                    .as_str(),
            ) {
                (Local, "CDMLFMKMMD7MWZP3FKUBZPVHTUEDLSX4BYGYKH4GCESXYHS3IHQ4EIG4")
                | (Testnet, "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC")
                | (Futurenet, "CB64D3G7SM2RTH6JSGG34DDTFTQ5CFDKVDZJZSODMCX4NJ2HV2KN7OHT")
                | (Mainnet, "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA") => {}
                (x, s) => panic!("Unexpected network {x:?} with asset {s}"),
            }
        }
    }

    // Test for parsing USDC token
    #[test]
    fn parse_usdc() {
        for network in &NETWORKS {
            let asset_id = generate_asset_id(USDC, network).unwrap().0;
            match (network, asset_id.to_string().as_str()) {
                (Local, "CB5SYISL2JCNQQRPFS5H4EFEESWUSNTDYMUNQX7TWZE45MYWYEYWCHAU")
                | (Testnet, "CA2E53VHFZ6YSWQIEIPBXJQGT6VW3VKWWZO555XKRQXYJ63GEBJJGHY7")
                | (Futurenet, "CBYZIQLTWJKSC34FJSCOGEQ63BR4YQWAKUDZDBMKIPUBBEMPRUMB5Z24")
                | (Mainnet, "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75") => {}
                (x, s) => panic!("Unexpected network {x:?} with asset {s}"),
            }
        }
    }

    #[test]
    fn native_client() {
        let lit: syn::LitStr = syn::parse_quote!("native");
        let expected = expected_module(
            "native",
            "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC",
        );
        let generated = parse_literal(&lit, &Network::Testnet).unwrap();
        assert_eq!(generated.to_string(), expected.to_string());
    }

    #[test]
    fn xlm_client() {
        let lit: syn::LitStr = syn::parse_quote!("xlm");
        let expected = expected_module(
            "xlm",
            "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC",
        );
        let generated = parse_literal(&lit, &Network::Testnet).unwrap();
        assert_eq!(generated.to_string(), expected.to_string());
    }

    #[test]
    fn usdc_client() {
        let lit: syn::LitStr =
            syn::parse_quote!("USDC:GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN");
        let expected = expected_module(
            "USDC",
            "CA2E53VHFZ6YSWQIEIPBXJQGT6VW3VKWWZO555XKRQXYJ63GEBJJGHY7",
        );
        let generated = parse_literal(&lit, &Network::Testnet).unwrap();
        assert_eq!(generated.to_string(), expected.to_string());
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
            assert!(
                parse_literal(&lit, &Network::Testnet).is_err(),
                "`{case}` should be rejected"
            );
        }
    }

    #[test]
    fn digit_leading_code_is_a_module_name_error() {
        // `1INCH` is a legal asset code but not a legal Rust module name.
        let lit = syn::LitStr::new(
            "1INCH:GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN",
            proc_macro2::Span::call_site(),
        );
        let err = parse_literal(&lit, &Network::Testnet).unwrap_err();
        assert!(err.to_string().contains("module name"), "{err}");
    }
}

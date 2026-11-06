//! `register_import!`: register an `import_contract!`-ed contract in a test
//! `Env` at the address `import_contract!` resolves to.
use quote::quote;
use syn::{
    Expr, Ident, Token,
    parse::{Parse, ParseStream},
};

use stellar_xdr::{Limited, Limits, ReadXdr, ScSpecEntry, ScSpecTypeDef};

use crate::asset;
use crate::contract::{Input, Resolved, resolve};
use crate::util::{Name, track_network_env};

/// `args = { name: value, .. }`, or a positional tuple `args = (..)`.
enum Args {
    Named {
        brace: syn::token::Brace,
        fields: Vec<(Ident, Expr)>,
    },
    Positional(Expr),
}

impl Parse for Args {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if !input.peek(syn::token::Brace) {
            return Ok(Self::Positional(input.parse()?));
        }
        let content;
        let brace = syn::braced!(content in input);
        let mut fields = Vec::new();
        while !content.is_empty() {
            let name: Ident = content.parse()?;
            // `{ admin }` is shorthand for `{ admin: admin }`, as in a struct literal.
            let value = if content.peek(Token![:]) {
                content.parse::<Token![:]>()?;
                content.parse()?
            } else {
                syn::parse_quote!(#name)
            };
            fields.push((name, value));
            if content.is_empty() {
                break;
            }
            content.parse::<Token![,]>()?;
        }
        Ok(Self::Named { brace, fields })
    }
}

/// `register_import!(env_expr, name [, args = { .. }] [, contract = Mock])`.
/// `name` must match the contract's `import_contract!` call, since both
/// resolve it to the same address. The options may come in either order.
struct RegisterInput {
    env: Expr,
    name: Name,
    /// Constructor arguments, checked against the imported Wasm's constructor.
    args: Option<Args>,
    /// A native contract to register instead of the imported Wasm.
    contract: Option<Expr>,
}

impl Parse for RegisterInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let Input { env, name } = input.parse()?;
        let (mut args, mut contract) = (None, None);
        while !input.is_empty() {
            input.parse::<Token![,]>()?;
            if input.is_empty() {
                break;
            }
            let key: Ident = input.parse()?;
            let given = match key.to_string().as_str() {
                "args" => args.is_some(),
                "contract" => contract.is_some(),
                other => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!(
                            "unknown option `{other}`: expected `args = {{ .. }}`, `args = (..)` or `contract = ..`"
                        ),
                    ));
                }
            };
            if given {
                return Err(syn::Error::new(
                    key.span(),
                    format!("`{key}` given more than once"),
                ));
            }
            input.parse::<Token![=]>()?;
            if key == "args" {
                args = Some(input.parse()?);
            } else {
                contract = Some(input.parse()?);
            }
        }
        Ok(Self {
            env,
            name,
            args,
            contract,
        })
    }
}

/// A native mock's constructor args: a tuple, since a mock has no spec to
/// name its parameters by.
fn mock_args(input: &RegisterInput) -> syn::Result<proc_macro2::TokenStream> {
    match &input.args {
        None => Ok(quote!(())),
        Some(Args::Positional(args)) => Ok(quote!(#args)),
        Some(Args::Named { brace, .. }) => Err(syn::Error::new(
            brace.span.join(),
            "named `args` need the imported Wasm's constructor; with `contract = ..`, \
             pass the mock's constructor arguments as a tuple, `args = (..)`",
        )),
    }
}

/// A test `Env` only creates a SAC at its own asset's address, so register a
/// throwaway one and copy its instance to `address`; the throwaway stays, as
/// the test host can't remove ledger entries. (Deploying the imported asset
/// itself would leave native XLM with no admin to mint from.)
fn expand_register_sac(
    input: &RegisterInput,
    address: &proc_macro2::TokenStream,
    track: &proc_macro2::TokenStream,
) -> syn::Result<proc_macro2::TokenStream> {
    let env = &input.env;
    if let Some(contract) = &input.contract {
        let args = mock_args(input)?;
        return Ok(quote! {
            {
                #track
                let __env: &::soroban_sdk::Env = #env;
                ::soroban_sdk::Env::register_at(__env, &#address, #contract, #args)
            }
        });
    }
    if let Some(args) = &input.args {
        let span = match args {
            Args::Named { brace, .. } => brace.span.join(),
            Args::Positional(e) => syn::spanned::Spanned::span(e),
        };
        return Err(syn::Error::new(
            span,
            format!(
                "`{}` is a Stellar Asset Contract, which has no constructor; remove `args`, \
                 or pass `contract = ..` to register a mock that takes them",
                input.name.raw()
            ),
        ));
    }
    Ok(quote! {
        {
            #track
            extern crate std as __std;
            use ::soroban_sdk::xdr as __xdr;
            let __env: &::soroban_sdk::Env = #env;
            let __address = #address;
            let __admin =
                <::soroban_sdk::Address as ::soroban_sdk::testutils::Address>::generate(__env);
            let __double = __env.register_stellar_asset_contract_v2(__admin).address();
            let __instance_key = |address: &::soroban_sdk::Address| {
                __std::rc::Rc::new(__xdr::LedgerKey::ContractData(__xdr::LedgerKeyContractData {
                    contract: __xdr::ScAddress::from(address),
                    key: __xdr::ScVal::LedgerKeyContractInstance,
                    durability: __xdr::ContractDataDurability::Persistent,
                }))
            };
            let (__entry, __live_until) = __env
                .host()
                .get_ledger_entry(&__instance_key(&__double))
                .expect("register_import!: reading the test asset's instance")
                .expect("register_import!: the test asset has no instance entry");
            let mut __entry = (*__entry).clone();
            if let __xdr::LedgerEntryData::ContractData(__data) = &mut __entry.data {
                __data.contract = __xdr::ScAddress::from(&__address);
            }
            __env
                .host()
                .add_ledger_entry(
                    &__instance_key(&__address),
                    &__std::rc::Rc::new(__entry),
                    __live_until,
                )
                .expect("register_import!: placing the test asset at the imported address");
            __address
        }
    })
}

/// The constructor's parameters (name and type), read from the contract spec
/// embedded in the Wasm; `None` if the contract has no constructor.
fn constructor_params(wasm: &[u8]) -> Result<Option<Vec<(String, ScSpecTypeDef)>>, String> {
    let err =
        |e: &dyn std::fmt::Display| format!("could not read the contract spec from the Wasm: {e}");
    // Only the raw section comes from soroban-spec; it is parsed with this
    // crate's stellar-xdr, which needn't match soroban-spec's.
    let raw = soroban_spec::read::raw_from_wasm(wasm).map_err(|e| err(&e))?;
    let mut reader = Limited::new(std::io::Cursor::new(raw), Limits::none());
    let entries = ScSpecEntry::read_xdr_iter(&mut reader)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| err(&e))?;
    Ok(entries.into_iter().find_map(|entry| match entry {
        ScSpecEntry::FunctionV0(f) if f.name.to_utf8_string_lossy() == "__constructor" => Some(
            f.inputs
                .iter()
                .map(|i| (i.name.to_utf8_string_lossy(), i.type_.clone()))
                .collect(),
        ),
        _ => None,
    }))
}

/// A spec type as the Rust type a test would pass, for error messages.
fn type_name(t: &ScSpecTypeDef) -> String {
    use ScSpecTypeDef as T;
    match t {
        T::Bool => "bool".into(),
        T::U32 => "u32".into(),
        T::I32 => "i32".into(),
        T::U64 => "u64".into(),
        T::I64 => "i64".into(),
        T::U128 => "u128".into(),
        T::I128 => "i128".into(),
        T::Address => "Address".into(),
        T::MuxedAddress => "MuxedAddress".into(),
        T::String => "String".into(),
        T::Symbol => "Symbol".into(),
        T::Bytes => "Bytes".into(),
        T::BytesN(b) => format!("BytesN<{}>", b.n),
        T::Option(o) => format!("Option<{}>", type_name(&o.value_type)),
        T::Vec(v) => format!("Vec<{}>", type_name(&v.element_type)),
        T::Map(m) => format!(
            "Map<{}, {}>",
            type_name(&m.key_type),
            type_name(&m.value_type)
        ),
        T::Udt(u) => u.name.to_utf8_string_lossy(),
        other => other.name().to_string(),
    }
}

/// `args = { admin: <Address>, fee: <u32> }` — a starting point to paste.
fn args_suggestion(params: &[(String, ScSpecTypeDef)]) -> String {
    let fields: Vec<String> = params
        .iter()
        .map(|(name, t)| format!("{name}: <{}>", type_name(t)))
        .collect();
    format!("args = {{ {} }}", fields.join(", "))
}

/// Order named `args` by the constructor's parameters, reporting unknown,
/// repeated and missing names.
fn order_named_args<'a>(
    fields: &'a [(Ident, Expr)],
    params: &[(String, ScSpecTypeDef)],
    brace_span: proc_macro2::Span,
) -> syn::Result<Vec<&'a Expr>> {
    let expected = || args_suggestion(params);
    for (i, (name, _)) in fields.iter().enumerate() {
        if !params.iter().any(|(p, _)| name == p) {
            return Err(syn::Error::new(
                name.span(),
                format!(
                    "the constructor has no parameter `{name}`; expected {}",
                    expected()
                ),
            ));
        }
        if fields[..i].iter().any(|(earlier, _)| earlier == name) {
            return Err(syn::Error::new(
                name.span(),
                format!("`{name}` given more than once"),
            ));
        }
    }
    params
        .iter()
        .map(|(p, _)| {
            fields
                .iter()
                .find(|(name, _)| name == p)
                .map(|(_, value)| value)
                .ok_or_else(|| {
                    syn::Error::new(
                        brace_span,
                        format!(
                            "missing constructor argument `{p}`; expected {}",
                            expected()
                        ),
                    )
                })
        })
        .collect()
}

/// Constructor args for the imported Wasm, as a typed call to the
/// `Args::__constructor` that `contractimport!` generates, so rustc checks
/// each value against the constructor's parameter types.
fn wasm_args(
    input: &RegisterInput,
    mod_ident: &Ident,
    params: Option<&[(String, ScSpecTypeDef)]>,
) -> syn::Result<proc_macro2::TokenStream> {
    let name = input.name.raw();
    let values: Vec<&Expr> = match (&input.args, params) {
        (None, None | Some([])) => return Ok(quote!(())),
        (None, Some(params)) => {
            return Err(syn::Error::new(
                input.name.span(),
                format!(
                    "`{name}`'s constructor takes arguments. Pass them with `args`:\n\n    \
                     register_import!(env, {name}, {})",
                    args_suggestion(params)
                ),
            ));
        }
        (Some(args), None) => {
            let span = match args {
                Args::Named { brace, .. } => brace.span.join(),
                Args::Positional(e) => syn::spanned::Spanned::span(e),
            };
            return Err(syn::Error::new(
                span,
                format!("`{name}` has no constructor, so it takes no `args`"),
            ));
        }
        (Some(Args::Named { brace, fields }), Some(params)) => {
            order_named_args(fields, params, brace.span.join())?
        }
        (Some(Args::Positional(Expr::Tuple(tuple))), Some(_)) => tuple.elems.iter().collect(),
        (Some(Args::Positional(other)), Some(_)) => return Ok(quote!(#other)),
    };
    // Spanned per value, so a type mismatch points at the value, not the macro.
    let values = values
        .iter()
        .map(|v| quote::quote_spanned!(syn::spanned::Spanned::span(v)=> &(#v)));
    Ok(quote!(#mod_ident::Args::__constructor(#(#values),*)))
}

/// Emit a block expression registering a contract at the address
/// `import_contract!` resolves to: `contract` (a native mock) when given,
/// otherwise the imported Wasm, or a test asset for a SAC. Evaluates to the
/// `Address`.
fn expand_register(
    input: &RegisterInput,
    resolved: &Resolved,
    params: Option<&[(String, ScSpecTypeDef)]>,
) -> syn::Result<proc_macro2::TokenStream> {
    let env = &input.env;
    let track = track_network_env();
    let (mod_ident, wasm_path, address) = match resolved {
        Resolved::Wasm {
            mod_ident,
            wasm_path,
            address,
        } => (mod_ident, wasm_path, address),
        Resolved::Sac { address } => {
            let address = quote!(::soroban_sdk::Address::from_str(__env, #address));
            return expand_register_sac(input, &address, &track);
        }
        Resolved::Asset { xdr } => {
            // Derived at runtime, so it doesn't depend on the build network.
            let address = asset::runtime_address(&quote!(__env), xdr);
            return expand_register_sac(input, &address, &quote!());
        }
    };
    let (module, contract, args) = if let Some(contract) = &input.contract {
        (quote!(), quote!(#contract), mock_args(input)?)
    } else {
        let args = wasm_args(input, mod_ident, params)?;
        let wasm_path = wasm_path.to_string_lossy();
        (
            quote! {
                #[allow(non_snake_case)]
                mod #mod_ident {
                    #![allow(clippy::ref_option, clippy::too_many_arguments)]
                    use ::soroban_sdk;
                    soroban_sdk::contractimport!(file = #wasm_path);
                }
            },
            quote!(#mod_ident::WASM),
            args,
        )
    };
    Ok(quote! {
        {
            #track
            #module
            let __env: &::soroban_sdk::Env = #env;
            ::soroban_sdk::Env::register_at(
                __env,
                &::soroban_sdk::Address::from_str(__env, #address),
                #contract,
                #args,
            )
        }
    })
}

pub(crate) fn register_import(
    input: proc_macro::TokenStream,
) -> syn::Result<proc_macro2::TokenStream> {
    let input: RegisterInput = syn::parse(input)?;
    let resolved = resolve(&input.name, "register_import")?;
    // Only the imported Wasm's constructor is read; a mock has no spec.
    let params = match (&resolved, &input.contract) {
        (Resolved::Wasm { wasm_path, .. }, None) => {
            let err = |e: String| syn::Error::new(input.name.span(), e);
            let wasm = std::fs::read(wasm_path)
                .map_err(|e| err(format!("could not read {}: {e}", wasm_path.display())))?;
            constructor_params(&wasm).map_err(err)?
        }
        _ => None,
    };
    expand_register(&input, &resolved, params.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use proc_macro2::Span;
    use quote::quote;
    use syn::{Ident, parse2};

    const A: &str = "CBESJIMX7J53SWJGJ7WQ6QTLJI4S5LPPJNC2BNVD63GIKAYCDTDOO322";

    fn wasm() -> Resolved {
        Resolved::Wasm {
            mod_ident: Ident::new("our_dao", Span::call_site()),
            wasm_path: "/tmp/target/stellar/local/deployed/our_dao.wasm".into(),
            address: A.to_string(),
        }
    }

    /// `__constructor(admin: Address, fee: u32)`.
    fn params() -> Vec<(String, ScSpecTypeDef)> {
        vec![
            ("admin".into(), ScSpecTypeDef::Address),
            ("fee".into(), ScSpecTypeDef::U32),
        ]
    }

    fn expand(
        tokens: proc_macro2::TokenStream,
        resolved: &Resolved,
        params: Option<&[(String, ScSpecTypeDef)]>,
    ) -> syn::Result<String> {
        let input: RegisterInput = parse2(tokens).unwrap();
        expand_register(&input, resolved, params).map(|out| out.to_string())
    }

    fn expanded(tokens: proc_macro2::TokenStream, resolved: &Resolved) -> String {
        expand(tokens, resolved, None).unwrap()
    }

    fn with_params(tokens: proc_macro2::TokenStream) -> syn::Result<String> {
        expand(tokens, &wasm(), Some(&params()))
    }

    fn parse_err(tokens: proc_macro2::TokenStream) -> String {
        parse2::<RegisterInput>(tokens).err().unwrap().to_string()
    }

    #[test]
    fn parses_name_only() {
        let input: RegisterInput = parse2(quote!(&env, "unverified/our-dao")).unwrap();
        assert_eq!(input.name.raw(), "unverified/our-dao");
        assert!(input.args.is_none());
        assert!(input.contract.is_none());
    }

    #[test]
    fn parses_options_in_either_order_with_trailing_comma() {
        for tokens in [
            quote!(&env, our_dao, args = (&admin, 7u32), contract = MockDao,),
            quote!(&env, our_dao, contract = MockDao, args = (&admin, 7u32)),
        ] {
            let input: RegisterInput = parse2(tokens).unwrap();
            assert!(input.args.is_some());
            assert!(input.contract.is_some());
        }
    }

    #[test]
    fn rejects_unknown_and_repeated_options() {
        let msg = parse_err(quote!(&env, our_dao, wasm = X));
        assert!(msg.contains("unknown option `wasm`"), "{msg}");
        let msg = parse_err(quote!(&env, our_dao, args = (), args = ()));
        assert!(msg.contains("`args` given more than once"), "{msg}");
        // A positional third argument is not accepted.
        assert!(parse2::<RegisterInput>(quote!(&env, our_dao, (admin,))).is_err());
    }

    #[test]
    fn registers_the_cached_wasm_at_the_baked_address() {
        let out = expanded(quote!(&env, our_dao), &wasm());
        assert!(out.contains("Env :: register_at"), "{out}");
        assert!(out.contains("contractimport !"), "{out}");
        assert!(out.contains("our_dao.wasm"), "{out}");
        assert!(
            out.contains("our_dao :: WASM , () ,"),
            "no constructor: {out}"
        );
        assert!(out.contains(A), "bakes the resolved id: {out}");
    }

    #[test]
    fn named_args_are_ordered_by_the_constructor_and_typed() {
        // Given out of order, and `admin` in shorthand.
        let out = with_params(quote!(&env, our_dao, args = { fee: 7u32, admin })).unwrap();
        assert!(
            out.contains("our_dao :: Args :: __constructor (& (admin) , & (7u32))"),
            "{out}"
        );
    }

    #[test]
    fn positional_tuple_args_are_typed_too() {
        let out = with_params(quote!(&env, our_dao, args = (&admin, 7u32))).unwrap();
        assert!(
            out.contains("our_dao :: Args :: __constructor (& (& admin) , & (7u32))"),
            "{out}"
        );
    }

    #[test]
    fn missing_args_suggest_the_constructor_parameters() {
        let msg = with_params(quote!(&env, our_dao)).unwrap_err().to_string();
        assert!(msg.contains("constructor takes arguments"), "{msg}");
        assert!(
            msg.contains("register_import!(env, our_dao, args = { admin: <Address>, fee: <u32> })"),
            "{msg}"
        );
    }

    #[test]
    fn named_args_report_unknown_repeated_and_missing_names() {
        let msg = with_params(quote!(&env, our_dao, args = { owner: a, fee: 1u32 }))
            .unwrap_err()
            .to_string();
        assert!(msg.contains("no parameter `owner`"), "{msg}");
        let msg = with_params(quote!(&env, our_dao, args = { admin, admin, fee: 1u32 }))
            .unwrap_err()
            .to_string();
        assert!(msg.contains("`admin` given more than once"), "{msg}");
        let msg = with_params(quote!(&env, our_dao, args = { admin }))
            .unwrap_err()
            .to_string();
        assert!(msg.contains("missing constructor argument `fee`"), "{msg}");
    }

    #[test]
    fn args_without_a_constructor_are_rejected() {
        let msg = expand(quote!(&env, our_dao, args = { admin }), &wasm(), None)
            .unwrap_err()
            .to_string();
        assert!(msg.contains("has no constructor"), "{msg}");
    }

    #[test]
    fn named_args_are_rejected_for_a_mock() {
        let msg = expand(
            quote!(&env, our_dao, contract = MockDao, args = { admin }),
            &wasm(),
            None,
        )
        .unwrap_err()
        .to_string();
        assert!(msg.contains("as a tuple"), "{msg}");
        let out = expanded(
            quote!(&env, our_dao, contract = MockDao, args = (&admin,)),
            &wasm(),
        );
        assert!(out.contains("MockDao , (& admin ,)"), "{out}");
    }

    #[test]
    fn spec_types_are_named_as_rust_types() {
        use stellar_xdr::{ScSpecTypeBytesN, ScSpecTypeVec};
        assert_eq!(type_name(&ScSpecTypeDef::I128), "i128");
        assert_eq!(
            type_name(&ScSpecTypeDef::BytesN(ScSpecTypeBytesN { n: 32 })),
            "BytesN<32>"
        );
        assert_eq!(
            type_name(&ScSpecTypeDef::Vec(Box::new(ScSpecTypeVec {
                element_type: Box::new(ScSpecTypeDef::Address),
            }))),
            "Vec<Address>"
        );
    }

    #[test]
    fn contract_option_replaces_the_wasm() {
        let out = expanded(quote!(&env, our_dao, contract = mocks::MockDao), &wasm());
        assert!(out.contains("mocks :: MockDao"), "{out}");
        assert!(!out.contains("contractimport"), "{out}");
        assert!(out.contains(A), "still at the baked address: {out}");
    }

    fn sac() -> Resolved {
        Resolved::Sac {
            address: A.to_string(),
        }
    }

    #[test]
    fn registered_sac_gets_a_test_asset_at_the_baked_address() {
        let out = expanded(quote!(&env, "circle/usdc"), &sac());
        assert!(out.contains("register_stellar_asset_contract_v2"), "{out}");
        assert!(out.contains("add_ledger_entry"), "{out}");
        assert!(out.contains(A), "{out}");
        assert!(!out.contains("register_at"), "{out}");
    }

    #[test]
    fn stellar_asset_contract_can_be_replaced_by_a_mock() {
        let out = expanded(
            quote!(&env, xlm, contract = MockToken, args = (7u32,)),
            &sac(),
        );
        assert!(out.contains("Env :: register_at"), "{out}");
        assert!(out.contains("MockToken , (7u32 ,)"), "{out}");
        assert!(!out.contains("register_stellar_asset_contract_v2"), "{out}");
    }

    #[test]
    fn stellar_asset_contract_rejects_constructor_args() {
        let input: RegisterInput = parse2(quote!(&env, xlm, args = (&admin,))).unwrap();
        let msg = expand_register(&input, &sac(), None)
            .unwrap_err()
            .to_string();
        assert!(msg.contains("has no constructor"), "{msg}");
    }

    #[test]
    fn asset_syntax_gets_a_test_asset_at_the_runtime_address() {
        let (xdr, _) = asset::asset_xdr("xlm").unwrap();
        let out = expanded(quote!(&env, xlm), &Resolved::Asset { xdr });
        assert!(out.contains("register_stellar_asset_contract_v2"), "{out}");
        assert!(out.contains("with_stellar_asset"), "{out}");
        assert!(
            !out.contains("STELLAR_NETWORK"),
            "no build-network tracking: {out}"
        );
        assert!(!out.contains("Address :: from_str"), "{out}");
    }
}

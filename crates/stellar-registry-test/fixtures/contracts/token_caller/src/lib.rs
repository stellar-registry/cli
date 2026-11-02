#![no_std]
//! Imports `unverified/my-token`, a token registered on testnet, so both
//! macros fetch its id and Wasm from testnet.
use soroban_sdk::{Env, String, contract, contractimpl};

#[contract]
pub struct Contract;

#[contractimpl]
impl Contract {
    pub fn token_symbol(env: &Env) -> String {
        stellar_registry::import_contract!(env, "unverified/my-token").symbol()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{Address, testutils::Address as _};
    use stellar_registry::testutils::register_import;

    #[test]
    fn reads_the_imported_tokens_symbol() {
        let env = Env::default();
        register_import!(&env, "unverified/my-token", args = {
            name: String::from_str(&env, "Test Token"),
            symbol: String::from_str(&env, "TT"),
            decimals: 7u32,
            owner: Address::generate(&env),
            initial_supply: 1_000i128,
        });

        let client = ContractClient::new(&env, &env.register(Contract, ()));
        assert_eq!(client.token_symbol(), String::from_str(&env, "TT"));
    }
}

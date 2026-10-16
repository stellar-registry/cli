#![no_std]
//! Demonstrates getting a working Stellar Asset Contract client via
//! `stellar_registry::import_contract!(env, xlm)`, and unit-testing it with
//! `stellar_registry::testutils::register_import!`.
//!
//! `import_contract!(env, xlm)` resolves XLM's real, network-specific
//! contract id at *build* time, with no network call and no cache files.
//! That address is empty in a sandboxed test `Env`, so the test below calls
//! `register_import!(&env, xlm)` to put a mintable test asset there.
use soroban_sdk::{Address, Env, contract, contractimpl};

#[contract]
pub struct Contract;

#[contractimpl]
impl Contract {
    /// Move `amount` stroops of XLM from `from` into this contract's balance.
    pub fn tip(env: &Env, from: &Address, amount: i128) {
        from.require_auth();
        stellar_registry::import_contract!(env, xlm).transfer(
            from,
            env.current_contract_address(),
            &amount,
        );
    }

    /// This contract's current XLM balance.
    pub fn balance(env: &Env) -> i128 {
        stellar_registry::import_contract!(env, xlm).balance(&env.current_contract_address())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{testutils::Address as _, token};
    use stellar_registry::testutils::register_import;

    #[test]
    fn tip_moves_xlm_into_the_jar() {
        let env = Env::default();
        env.mock_all_auths();

        let xlm = register_import!(&env, xlm);
        let client = ContractClient::new(&env, &env.register(Contract, ()));

        let alice = Address::generate(&env);
        token::StellarAssetClient::new(&env, &xlm).mint(&alice, &1_000_000_000);

        client.tip(&alice, &500);

        assert_eq!(client.balance(), 500);
        assert_eq!(
            token::TokenClient::new(&env, &xlm).balance(&alice),
            1_000_000_000 - 500
        );
    }
}

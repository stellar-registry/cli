#![no_std]
//! Demonstrates getting a working Stellar Asset Contract client via
//! `stellar_registry::import_contract!(env, xlm)`, plus the counterpart
//! pattern for exercising it in an in-process unit test.
//!
//! `import_contract!(env, xlm)` resolves XLM's real, network-specific
//! contract id at *build* time — but that address isn't instantiated in a
//! sandboxed test `Env`, so a unit test can't call through it directly. The
//! `xlm` module below swaps in `Env::register_stellar_asset_contract_v2` (a
//! real SAC test double) under `#[cfg(test)]`, keeping the call site in
//! `Contract` — `xlm::token_client(env)` — identical either way.
use soroban_sdk::{Address, Env, contract, contractimpl};

#[cfg(not(test))]
mod xlm {
    use soroban_sdk::{Env, token};

    /// The real "xlm" asset, resolved offline at build time — no network
    /// call, no cache files (see `import_contract!`'s SAC/XLM support).
    pub fn token_client(env: &Env) -> token::TokenClient<'_> {
        stellar_registry::import_contract!(env, xlm)
    }
}

#[cfg(test)]
mod xlm {
    use soroban_sdk::{Address, Env, contracttype, testutils::Address as _, token};

    #[contracttype]
    enum DataKey {
        Sac,
    }

    /// Stand up a real SAC test double in this sandboxed `Env`, and remember
    /// its address in `contract_id`'s own instance storage so `token_client`
    /// (called from inside the contract, where `env`'s current contract is
    /// already `contract_id`) can find it. Returns the SAC's address so tests
    /// can mint balances against it directly.
    pub fn register(env: &Env, contract_id: &Address) -> Address {
        let sac = env.register_stellar_asset_contract_v2(Address::generate(env));
        env.as_contract(contract_id, || {
            env.storage().instance().set(&DataKey::Sac, &sac.address());
        });
        sac.address()
    }

    pub fn token_client(env: &Env) -> token::TokenClient<'_> {
        let address: Address = env.storage().instance().get(&DataKey::Sac).unwrap();
        token::TokenClient::new(env, &address)
    }
}

#[contract]
pub struct Contract;

#[contractimpl]
impl Contract {
    /// Move `amount` stroops of XLM from `from` into this contract's balance.
    pub fn tip(env: &Env, from: &Address, amount: i128) {
        from.require_auth();
        xlm::token_client(env).transfer(from, env.current_contract_address(), &amount);
    }

    /// This contract's current XLM balance.
    pub fn balance(env: &Env) -> i128 {
        xlm::token_client(env).balance(&env.current_contract_address())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{testutils::Address as _, token};

    #[test]
    fn tip_moves_xlm_into_the_jar() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(Contract, ());
        let client = ContractClient::new(&env, &contract_id);
        let sac_address = xlm::register(&env, &contract_id);

        let alice = Address::generate(&env);
        token::StellarAssetClient::new(&env, &sac_address).mint(&alice, &1_000_000_000);

        client.tip(&alice, &500);

        assert_eq!(client.balance(), 500);
        assert_eq!(
            token::TokenClient::new(&env, &sac_address).balance(&alice),
            1_000_000_000 - 500
        );
    }
}

#![no_std]
//! Tests a contract that imports a name registered on testnet, the
//! `unverified` subregistry, with `register_import!` and named constructor
//! args. Built with `STELLAR_NETWORK=testnet`, so both macros resolve the
//! name, id and Wasm on testnet.
use soroban_sdk::{Address, Env, contract, contractimpl};

#[contract]
pub struct Contract;

#[contractimpl]
impl Contract {
    /// The `unverified` registry's manager, read through `import_contract!`.
    pub fn unverified_manager(env: &Env) -> Option<Address> {
        stellar_registry::import_contract!(env, unverified).manager()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use stellar_registry::testutils::register_import;

    #[test]
    fn reads_the_imported_registrys_manager() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let manager = Address::generate(&env);
        let root = Address::generate(&env);
        // A subregistry, so the constructor doesn't deploy anything.
        register_import!(
            &env,
            unverified,
            args = { admin, manager: Some(manager.clone()), root: Some(root) }
        );

        let client = ContractClient::new(&env, &env.register(Contract, ()));
        assert_eq!(client.unverified_manager(), Some(manager));
    }
}

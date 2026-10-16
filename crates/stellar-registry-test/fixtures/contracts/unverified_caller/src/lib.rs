#![no_std]
//! Imports `unverified`, the testnet registry's subregistry, so both macros
//! fetch its id and Wasm from testnet.
use soroban_sdk::{Address, Env, contract, contractimpl};

#[contract]
pub struct Contract;

#[contractimpl]
impl Contract {
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
        let (admin, manager, root) = (
            Address::generate(&env),
            Address::generate(&env),
            Address::generate(&env),
        );
        // Passing `root` makes it a subregistry, whose constructor deploys nothing.
        register_import!(&env, unverified, args = { admin, manager: Some(manager.clone()), root: Some(root) });

        let client = ContractClient::new(&env, &env.register(Contract, ()));
        assert_eq!(client.unverified_manager(), Some(manager));
    }
}

#![no_std]
//! Demonstrates unit-testing a contract that calls another contract through
//! `stellar_registry::import_contract!`.
//!
//! `import_contract!(env, hello_v1)` bakes `hello_v1`'s deployed address into
//! this contract at build time, and nothing lives at that address in a
//! sandboxed test `Env`. `register_import!` resolves the same name the same
//! way and registers a contract there — `hello_v1`'s own Wasm, or a native
//! mock.
use soroban_sdk::{Env, String, contract, contractimpl};

#[contract]
pub struct Contract;

#[contractimpl]
impl Contract {
    /// Greet `to` by way of the deployed `hello_v1` contract.
    pub fn greet(env: &Env, to: String) -> String {
        stellar_registry::import_contract!(env, hello_v1).hello(&to)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{Address, testutils::Address as _};
    use stellar_registry::testutils::register_import;

    #[contract]
    struct MockHello;

    #[contractimpl]
    impl MockHello {
        pub fn hello(env: &Env, to: String) -> String {
            let _ = to;
            String::from_str(env, "mocked")
        }
    }

    fn greet(env: &Env, to: &str) -> String {
        let client = ContractClient::new(env, &env.register(Contract, ()));
        client.greet(&String::from_str(env, to))
    }

    #[test]
    fn greets_through_the_imported_wasm() {
        let env = Env::default();
        let admin = Address::generate(&env);
        // `hello_v1`'s constructor takes an admin.
        register_import!(&env, hello_v1, args = (&admin,));

        assert_eq!(greet(&env, "world"), String::from_str(&env, "world"));
    }

    #[test]
    fn greets_through_a_native_mock() {
        let env = Env::default();
        register_import!(&env, hello_v1, contract = MockHello);

        assert_eq!(greet(&env, "world"), String::from_str(&env, "mocked"));
    }

    #[test]
    fn registers_at_the_same_address_either_way() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let wasm = register_import!(&env, hello_v1, args = (&admin,));
        let mock = register_import!(&env, hello_v1, contract = MockHello);

        assert_eq!(wasm, mock);
        // Registering again replaced the Wasm with the mock.
        assert_eq!(greet(&env, "world"), String::from_str(&env, "mocked"));
    }
}

#![no_std]
use soroban_sdk::{Address, Env, contract, contractimpl, symbol_short};

#[contract]
pub struct Contract;

// The constructor's `require_auth` is a non-root auth entry under the
// registry's `deploy` call (#118).
#[contractimpl]
impl Contract {
    pub fn __constructor(env: &Env, admin: &Address) {
        admin.require_auth();
        env.storage().instance().set(&symbol_short!("admin"), admin);
    }

    pub fn admin(env: &Env) -> Address {
        env.storage()
            .instance()
            .get(&symbol_short!("admin"))
            .unwrap()
    }
}

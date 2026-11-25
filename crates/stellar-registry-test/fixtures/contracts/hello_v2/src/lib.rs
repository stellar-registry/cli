#![no_std]
use admin_sep::{Administratable, Upgradable};
use soroban_sdk::{Address, Env, String, contract, contractimpl};

#[contract]
pub struct Contract;

#[contractimpl(contracttrait)]
impl Administratable for Contract {}

#[contractimpl(contracttrait)]
impl Upgradable for Contract {}

#[contractimpl]
impl Contract {
    // Requiring auth lets tests deploy with a non-source admin.
    pub fn __constructor(env: &Env, admin: &Address) {
        admin.require_auth();
        Self::set_admin(env, admin);
    }
    pub fn hi(_: &Env, to: String) -> String {
        to
    }
}

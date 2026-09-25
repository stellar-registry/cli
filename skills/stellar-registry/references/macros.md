# `stellar-registry` macros

```toml
[dependencies]
soroban-sdk = "..."          # your existing version; generated code uses your `soroban_sdk`
stellar-registry = "0.1"
```

All three macros run at **build time**. `import_contract!` and `import_contract_client!` shell out to `stellar registry` (the `stellar-registry-cli` plugin) and `stellar contract fetch`, so both `stellar` and the plugin must be on `PATH` when you `cargo build`. A default identity (`stellar keys use <name>`) must also be set, since the lookups need a source account.

**Network selection:** set `STELLAR_NETWORK` (e.g. `testnet`, `mainnet`) in the build environment. It defaults to `local`. It selects the cache directory and the network passphrase used to compute asset contract ids.

```bash
STELLAR_NETWORK=testnet stellar contract build
```

## `import_contract!(env, name)`: deployed contract → bound client

```rust
let dao = stellar_registry::import_contract!(env, our_dao);          // env: &Env
dao.create_proposal(/* ... */);

let game = stellar_registry::import_contract!(env, "unverified/my-game");
```

- `name` is a bare ident or a string literal, optionally channel-prefixed. Use a string for hyphens or `/`. The generated module uses `_` in place of `-`.
- **No `@version`**: deployed contracts don't have versions.
- Resolves at build time:
  - **address** via `stellar registry fetch-contract-id`, cached at `target/stellar/<network>/deployed/<mod_name>.id` (`<channel>__<mod_name>.id` for prefixed names). While online, the cache is ignored and the lookup always runs, so a contract **flagged as compromised fails compilation**.
  - **wasm** via `stellar contract fetch --id <address>` (the contract's *actual* on-chain code), cached beside the id. So it works even if the wasm was never published to the registry.
- The address is compiled in. **If the named contract is redeployed, delete the cached files (or `cargo clean`) and rebuild.**

### Assets (SACs): `xlm`, `native`, `"CODE:ISSUER"`

```rust
let xlm = stellar_registry::import_contract!(env, xlm);
xlm.transfer(&from, &to, &amount);

let usdc = stellar_registry::import_contract!(env, "USDC:GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN");
```

Asset names resolve the Stellar Asset Contract id **offline** and return `soroban_sdk::token::TokenClient`. There's no registry lookup and no cache, and it works with `STELLAR_NO_REGISTRY=1`. A registered name that points at a SAC (e.g. `"circle/usdc"`) also returns a `TokenClient`.

## `import_contract_client!(name)`: published wasm → client type only

```rust
use soroban_sdk;   // required in scope, or: "unresolved import `super`, no `soroban_sdk` in the root"

stellar_registry::import_contract_client!(registry);                  // latest version
stellar_registry::import_contract_client!("unverified/my-game");
stellar_registry::import_contract_client!("registry@1.0.0");          // pinned; leading `v` ok

// Later, bind it to an address you supply:
let client = registry::Client::new(env, &some_address);
```

- Generates a module (e.g. `registry`) equivalent to `soroban_sdk::contractimport!` on the downloaded wasm. It **doesn't** bind an address.
- Use it when you have many instances of one wasm, take addresses as arguments, or need a specific version's interface.
- Looks for `target/stellar/<network>/[<channel>__]<mod_name>[_<version>].wasm` first (a workspace contract you compiled, or one fetched with `stellar registry download`), and downloads it from the registry only if missing.

## `import_asset!("asset")`: SAC id and clients as a module

```rust
stellar_registry::import_asset!("native");   // or "xlm"
stellar_registry::import_asset!("USDC:GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN");

let id = USDC::contract_id(env);
USDC::token_client(env).balance(&who);
USDC::stellar_asset_client(env).mint(&to, &amount);   // issuer/admin interface
```

Computed entirely offline from the asset and the build-time network passphrase (`STELLAR_NETWORK` / `STELLAR_NETWORK_PASSPHRASE`). The module is named after the asset code.

## Offline / reproducible builds

Set `STELLAR_NO_REGISTRY=1` to forbid network calls. Then:

- `import_contract!` needs the cached `.id` and wasm under `target/stellar/<network>/deployed/`. Build online once, or create them with `stellar registry fetch-contract-id` and `stellar contract fetch`.
- `import_contract_client!` needs the wasm in `target/stellar/<network>/` (e.g. from `stellar registry download <name> -o target/stellar/<network>/<mod_name>.wasm`).
- Asset forms (`xlm`, `"CODE:ISSUER"`, `import_asset!`) need nothing.

## Unit-testing code that calls a SAC

The SAC address baked in by `import_contract!(env, xlm)` doesn't exist in a sandboxed test `Env`. Swap in a test double behind `cfg(test)` and keep the call site the same:

```rust
#[cfg(not(test))]
mod xlm {
    use soroban_sdk::{token, Env};
    pub fn token_client(env: &Env) -> token::TokenClient<'_> {
        stellar_registry::import_contract!(env, xlm)
    }
}

#[cfg(test)]
mod xlm {
    use soroban_sdk::{contracttype, testutils::Address as _, token, Address, Env};
    #[contracttype]
    enum DataKey { Sac }

    // Call from the test after registering your contract; mint to users via the returned address.
    pub fn register(env: &Env, contract_id: &Address) -> Address {
        let sac = env.register_stellar_asset_contract_v2(Address::generate(env));
        env.as_contract(contract_id, || env.storage().instance().set(&DataKey::Sac, &sac.address()));
        sac.address()
    }
    pub fn token_client(env: &Env) -> token::TokenClient<'_> {
        let address: Address = env.storage().instance().get(&DataKey::Sac).unwrap();
        token::TokenClient::new(env, &address)
    }
}
```

The contract calls `xlm::token_client(env)` either way. The same pattern works for any `import_contract!` target: register a mock contract in tests.

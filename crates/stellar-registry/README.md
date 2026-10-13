# stellar-registry

Stellar cross-contract calls simplified.

# Import contract with `import_contract!`

Import a contract (https://stellar.rgstry.xyz/contracts) directly, with a fully-typed interface ready to make cross-contract calls.

```rs
pub fn your_fn(env: &Env) {
    let unverified_registry = stellar_registry::import_contract!(env, "unverified");
    unverified_registry.fetch_contract_id("guess-the-number");
}
```

## Stellar Asset Contracts (SACs) and XLM

`import_contract!` also accepts an asset name — `xlm`/`native`, or `"CODE:ISSUER"` — instead of a registered contract name, resolving the Stellar Asset Contract id offline (no registry lookup, no network access) and binding the standard token client to it:

```rs
pub fn your_fn(env: &Env) {
    let xlm = stellar_registry::import_contract!(env, xlm);
    xlm.transfer(&from, &to, &amount);
}
```

A registered name that itself points at a SAC (e.g. `"circle/usdc"`) works the same way, going through the registry to resolve the address and falling back to the same token client instead of trying (and failing) to fetch a wasm that doesn't exist for a built-in asset contract.

## Testing with `import_contract!`

`import_contract!` bakes the contract's real on-chain address into your contract at build time. A unit test's sandboxed `Env` has nothing at that address, so calling through the import fails until you put a contract there. `register_import!` does that: it resolves the name exactly as `import_contract!` does and registers the imported contract at the same address, returning the `Address`.

Enable the `testutils` feature for tests (alongside `soroban-sdk`'s own):

```toml
[dependencies]
soroban-sdk = "27"
stellar-registry = "0.1"

[dev-dependencies]
soroban-sdk = { version = "27", features = ["testutils"] }
stellar-registry = { version = "0.1", features = ["testutils"] }
```

Then, given a contract that imports `our_dao`:

```rs
pub fn propose(env: &Env, title: String) -> u32 {
    stellar_registry::import_contract!(env, our_dao).create_proposal(&title)
}
```

...its tests register `our_dao` before calling it. Your contract code stays the same in tests and on-chain, with no `#[cfg(test)]` switch:

```rs
use stellar_registry::testutils::register_import;

#[test]
fn proposes() {
    let env = Env::default();
    // Runs the same Wasm `import_contract!` generated its client from.
    register_import!(&env, our_dao);

    let client = ContractClient::new(&env, &env.register(Contract, ()));
    assert_eq!(client.propose(&String::from_str(&env, "Ship it")), 1);
}
```

Two options, in either order:

- `args = (..)` passes constructor arguments, as a tuple like `Env::register` takes: `register_import!(&env, our_dao, args = (&admin, 7u32))`.
- `contract = ..` registers a native contract in place of the Wasm, when you want to control what the dependency returns: `register_import!(&env, our_dao, contract = MockDao)`. `MockDao` is an ordinary `#[contract]` type with the functions your contract calls.

Both macros read the same cache files (`target/stellar/<network>/deployed/<name>.id` and `.wasm`), so once `import_contract!` has built, `register_import!` needs nothing more. To run tests with no network access, such as in CI, set `STELLAR_NO_REGISTRY=1` and create those two files first (`stellar registry fetch-contract-id` and `stellar contract fetch`, or commit copies and put them in place before building). The [`hello_caller` fixture](https://github.com/stellar-registry/cli/tree/main/crates/stellar-registry-test/fixtures/contracts/hello_caller) is a complete example, run by `just test-register-import`.

Stellar Asset Contracts (`xlm`, `"CODE:ISSUER"`, or a registered name that resolves to a SAC) are not supported by `register_import!` yet and give a compile error. Until they are, register a test asset with `Env::register_stellar_asset_contract_v2`.

# Import wasm with `import_contract_client!`

Import a wasm (https://stellar.rgstry.xyz/wasms), which defines only behavior. You can optionally include a version, otherwise it fetches the latest. You need to instantiate with a contract ID.

```rs
use soroban_sdk; // needs to be in-scope

stellar_registry::import_contract_client!(unverified);
```

This creates a `unverified` module, equivalent to running:

```bash
stellar registry download unverified --out-file target/stellar/unverified.wasm
```

...and then importing the Wasm with `soroban_sdk` like:

```rust
mod unverified {
    use super::soroban_sdk;
    soroban_sdk::contractimport!(file = "target/stellar/unverified.wasm");
}
```

Within a method, you can now instantiate the client as usual, using the contract ID of the desired contract (such as the `unverified` contract above):

```rust
pub fn __constructor(env: &Env, admin: Address) {
    let unverified_client = registry::Client::new(
        env,
        &Address::from_str(
            env,
            "CAMLHKQHNZO2IOIBFUF5BGZ2V62BMS5QCWFFGRCB4NOB3G5OMDA7SGZN",
        ),
    );
    let  = unverified_client.fetch_contract_id(&String::from_str(env, &"world"));
}
```

# Import an asset with `import_asset!`

Generate a module with the Stellar Asset Contract id and token clients for an asset, computed offline for the build-time network (`STELLAR_NETWORK` / `STELLAR_NETWORK_PASSPHRASE`, defaulting to local).

```ignore
import_asset!("native"); // or "xlm"
import_asset!("USDC:GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN");
```

The generated module — named after the asset code — exposes `contract_id`, `token_client` (the standard token interface) and `stellar_asset_client` (the asset admin interface).

# If you don't want your macro making network calls

First, you should know that this macro doesn't make a network call _first_. It starts by looking in the current Cargo project's `target` directory for a `.wasm` file with the given name. Only if it fails to find one will it run `stellar registry download` to download the Wasm before importing it.

If you want to avoid network calls in your build-time macro logic, you can set environment variable `STELLAR_NO_REGISTRY` to `1`.

# More Options

`import_contract_client` is designed to make it easy to paste in Wasm names from https://stellar.rgstry.xyz. If you want to use a channel-prefixed contract or one with hypens in the name, you can use quotes:

```rs
import_contract_client!("unverified/guess-the-number");
```

If you need a specific (historic) version:

```rs
import_contract_client!("registry@v1.0.0");
```

See [docs.rs/stellar-registry](https://docs.rs/stellar-registry/) for more details.

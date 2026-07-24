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

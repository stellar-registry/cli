---
name: stellar-registry
description: Publish, deploy, and reuse named Soroban smart contracts through the Stellar Registry. Use when publishing a contract's wasm with a name and semantic version, deploying a named contract instance, looking up a contract id by name, aliasing a registry contract for `stellar contract invoke`, or making cross-contract calls from Rust with `stellar_registry::import_contract!`, `import_contract_client!`, or `import_asset!` (including XLM and other Stellar Asset Contracts).
---

# Stellar Registry

The Stellar Registry is an on-chain contract that gives Soroban contracts human-readable names:

- **Wasms** are published code with a name and semantic version (`my-token@1.2.0`).
- **Contracts** are deployed instances with a name (`my-token-instance`), no version.

Two tools use it:

1. **`stellar registry` CLI plugin**: publish, deploy, look up, upgrade.
2. **`stellar-registry` Rust crate**: macros that turn a registry name into a typed client at build time, so cross-contract calls need no hardcoded addresses.

Browse what's already published at https://stellar.rgstry.xyz (`/wasms`, `/contracts`).

## Setup

```bash
# Requires the Stellar CLI (`stellar`). The registry is a plugin for it:
cargo install --locked stellar-registry-cli    # or: cargo binstall stellar-registry-cli

stellar keys generate alice --network testnet --fund   # skip if you have an identity
stellar keys use alice
stellar network use testnet
```

Every command needs a source account and network. Set defaults as above, or pass `--source alice --network testnet`.

## Names: verified vs. `unverified/`

Names are `name` or `channel/name`. The default (verified) registry is **managed**: a new name must be approved by the registry manager before it can be published or deployed. For hackathons and experiments, **use the `unverified/` channel**, which is open to anyone:

```bash
--wasm-name unverified/my-token        # publish/deploy under the open channel
--contract-name unverified/my-token
```

Well-known verified names (e.g. `registry`, `unverified` itself) can be read by anyone without a prefix.

## CLI workflow

```bash
stellar contract build   # produces target/wasm32v1-none/release/my_token.wasm

# 1. Publish the wasm (name + version; both default to contract metadata if omitted)
stellar registry publish \
  --wasm target/wasm32v1-none/release/my_token.wasm \
  --wasm-name unverified/my-token --binver 0.1.0

# 2. Deploy a named instance. After `--`, pass the __constructor's args as flags
stellar registry deploy \
  --contract-name unverified/my-token --wasm-name unverified/my-token \
  -- --admin alice --decimal 7
#   (`-- --help` prints the constructor's arguments; `--version` pins a wasm version)

# 3. Use it from the Stellar CLI by name
stellar registry create-alias unverified/my-token my-token
stellar contract invoke --id my-token -- --help

# Look things up
stellar registry fetch-contract-id unverified/my-token
stellar registry current-version unverified/my-token
stellar registry download unverified/my-token -o my_token.wasm

# Ship a new version to an existing instance
stellar registry publish --wasm ... --wasm-name unverified/my-token --binver 0.2.0
stellar registry upgrade --contract-name unverified/my-token --wasm-name unverified/my-token
```

Try any write with `--dry-run` first (publish, publish-hash, register-contract, rename/update commands). Full command list: [references/cli.md](references/cli.md).

## Calling registry contracts from Rust

Add the crate to your contract:

```toml
[dependencies]
stellar-registry = "0.1"
```

Pick the macro by what you have:

| You have… | Use | You get |
|---|---|---|
| A **deployed contract name** | `import_contract!(env, name)` | A client already bound to its address |
| XLM or a classic asset (`xlm`, `"USDC:G..."`) | `import_contract!(env, xlm)` | `token::TokenClient` for its SAC |
| A **published wasm name** (+ optional version) | `import_contract_client!(name)` | A `name::Client` module; you supply the address |
| An asset, and you want its SAC id/admin client | `import_asset!("USDC:G...")` | Module with `contract_id`, `token_client`, `stellar_asset_client` |

```rust
use soroban_sdk::{contract, contractimpl, Address, Env};

#[contract]
pub struct Tipper;

#[contractimpl]
impl Tipper {
    pub fn tip(env: &Env, from: Address, amount: i128) {
        from.require_auth();
        let xlm = stellar_registry::import_contract!(env, xlm);
        xlm.transfer(&from, &env.current_contract_address(), &amount);
    }

    pub fn lookup(env: &Env, name: soroban_sdk::String) -> Address {
        let registry = stellar_registry::import_contract!(env, registry);
        registry.fetch_contract_id(&name)
    }
}
```

Before calling a contract's methods, list them with `stellar contract info interface --id <C...>` (get the id from `stellar registry fetch-contract-id <name>`).

**Build with the network set.** The macros resolve names at build time, reading `STELLAR_NETWORK`, which **defaults to `local`**:

```bash
STELLAR_NETWORK=testnet stellar contract build
```

Details (caching, offline builds, hyphenated names, versions, unit-testing SAC calls): [references/macros.md](references/macros.md).

## Common pitfalls

- **Authorization failure on publish or deploy**: you used a bare name on the managed registry. Prefix with `unverified/`.
- **`Error(Contract, #N)` from the registry**: common codes are `#2` no such version, `#3` wasm name taken by another author, `#4` no such contract, `#5` contract name already deployed, `#8` version must be greater than the latest, `#9` invalid name (≤64 chars, ASCII alphanumeric/`-`/`_`, starts with a letter, not a Rust keyword), `#11` these exact wasm bytes were already published.
- **`upgrade` fails**: the registry calls the contract's own `upgrade(wasm_hash)` function, so the contract must implement one. If it exposes `admin()`, that admin must sign.
- **Macro resolves the wrong address or can't find a contract**: `STELLAR_NETWORK` wasn't set at build time. It defaults to `local`, so asset/XLM contract ids are computed for the wrong network without any error.
- **`unresolved import super` / `no soroban_sdk in the root` from `import_contract_client!`**: add `use soroban_sdk;` to the module where you call it. `import_contract!` and `import_asset!` don't need this.
- **Macro says `stellar` or the registry plugin is missing / too old**: the macros shell out to `stellar registry` during `cargo build`. Run `cargo install stellar-registry-cli --force`.
- **Hyphens or channels in a macro name**: use a string literal, `import_contract!(env, "unverified/my-token")`. The module is named with `-` replaced by `_`.
- **Redeployed a contract but still calling the old one**: the address is baked in at build time. `cargo clean` (or delete `target/stellar/<network>/deployed/`) and rebuild.
- **Contract flagged as compromised**: `fetch-contract-id`, `create-alias`, and `import_contract!` refuse it. This is intentional; use `--force` on the CLI only if you're sure.
- **There is no `stellar registry install`**. Use `create-alias`.

# `stellar registry` command reference

Every command also accepts the standard Stellar CLI network and signing flags: `--source-account/--source`, `--network`, `--rpc-url`, `--network-passphrase`, `--inclusion-fee`, `--sign-with-key`, `--sign-with-ledger`, `--sign-with-lab`. These fall back to `STELLAR_ACCOUNT`, `STELLAR_NETWORK`, etc., and to `stellar keys use` / `stellar network use` defaults. `STELLAR_REGISTRY_CONTRACT_ID` overrides the root registry address.

Any `<name>` can be channel-prefixed (`unverified/<name>`) to target a sub-registry instead of the managed root registry.

## Wasms (published code)

| Command | Purpose | Key arguments |
|---|---|---|
| `publish` | Upload a wasm and publish it under a name + semver | `--wasm <PATH>`, `--wasm-name <NAME>`, `--binver <VERSION>` (both read from contract metadata if omitted), `-a/--author`, `--dry-run` |
| `publish-hash` | Publish a wasm that's already uploaded on-chain | `--wasm-hash <HEX>`, `--wasm-name`, `--version`, `-a/--author`, `--dry-run` |
| `current-version <WASM_NAME>` | Latest published version | |
| `fetch-hash <WASM_NAME>` | Hash of a published wasm | `--version` |
| `download <WASM_NAME>` | Fetch the wasm bytes | `--version`, `-o/--out-file` (default stdout) |

Versions must strictly increase for a given name. Only the original author can publish new versions of a name.

## Contracts (deployed instances)

| Command | Purpose | Key arguments |
|---|---|---|
| `deploy` | Deploy a published wasm and register the instance by name | `--contract-name` (alias `--deploy-as`), `--wasm-name`, `--version`, `--deployer`, `-- <constructor args>` |
| `deploy-unnamed` | Deploy a published wasm without registering a name | `--wasm-name`, `--version`, `--salt <HEX32>`, `--deployer`, `-- <constructor args>` |
| `register-contract` | Name an already-deployed contract | `--contract-name`, `--contract-address <C...>`, `--owner`, `--dry-run` |
| `fetch-contract-id <NAME>` | Resolve a name to its `C...` address | `--force` (return it even if flagged compromised) |
| `create-alias <NAME> [LOCAL_NAME]` | Save a local `stellar contract alias` for use with `--id <alias>` | `-f/--force` (overwrite; allow flagged contracts) |
| `upgrade` | Upgrade a named contract to a published wasm version | `--contract-name`, `--wasm-name`, `--version` (default latest) |
| `rename-contract` | Rename a registration | `--contract-name`, `--new-name`, `--dry-run` |
| `update-contract-address` | Point a name at a different address | `--contract-name`, `--new-address`, `--dry-run` |
| `update-contract-owner` | Transfer ownership of a registration | `--contract-name`, `--new-owner`, `--dry-run` |
| `version` | Print the plugin version | |

## Accounts (named G-addresses and smart accounts)

Account names are a separate namespace from contract names. The named address has to consent: it signs the naming transaction (pass its identity alias as the address), or it runs `preauthorize-account` first for that name and owner. On a managed registry, where governance executes the call, preauthorizing is the only way. A preauthorization is single-use and lasts up to the network's max temporary-entry TTL (~180 days on mainnet). The CLI can't sign for a smart account; it has to sign with its own wallet.

| Command | Purpose | Key arguments |
|---|---|---|
| `register-account` | Name an account | `--account-name`, `--account-address <G...\|C...\|alias>`, `--owner`, `--dry-run` |
| `preauthorize-account` | Consent in advance to being named | `--account-name`, `--account-address`, `--owner`; signed by the account |
| `revoke-account-preauth` | Withdraw that consent | `--account-name`, `--account-address`; signed by the account |
| `fetch-account-id <NAME>` | Resolve a name to its address | `--force` (return it even if flagged compromised) |
| `fetch-account-owner <NAME>` | Show who owns a name | |
| `rename-account` | Rename (the account consents to the new name) | `--account-name`, `--new-name`, `--dry-run` |
| `update-account-address` | Point a name at a different address (which consents) | `--account-name`, `--new-address`, `--dry-run` |
| `update-account-owner` | Transfer ownership of a name | `--account-name`, `--new-owner`, `--dry-run` |
| `flag-account` | Flag an account as compromised | `--account-name`, `--unflag`, `--dry-run` |

### Constructor arguments

Anything after `--` on `deploy` / `deploy-unnamed` is passed to the contract's `__constructor` as `--arg value` flags. Don't name the function; it's added for you. Addresses can be identity names (`--admin alice`). Run with `-- --help` to print the constructor's signature. Contracts without a constructor take no arguments.

### Upgrades

`upgrade` looks up the wasm hash for `--wasm-name`/`--version`, then calls the named contract's `upgrade(wasm_hash)` function. If the contract exposes `admin()`, the registry requires that admin's signature. Otherwise the contract's own `upgrade` must enforce authorization.

## Examples

```bash
# Publish to the open channel and deploy with a constructor
stellar registry publish --wasm target/wasm32v1-none/release/counter.wasm \
  --wasm-name unverified/counter --binver 1.0.0
stellar registry deploy --contract-name unverified/my-counter \
  --wasm-name unverified/counter -- --owner alice

# Deploy a specific older version
stellar registry deploy --contract-name unverified/counter-v1 \
  --wasm-name unverified/counter --version 1.0.0 -- --owner alice

# Give an existing contract a registry name
stellar registry register-contract --contract-name unverified/my-dao \
  --contract-address CABC...XYZ

# Use a registry contract from the CLI
stellar registry create-alias unverified/my-counter counter
stellar contract invoke --id counter -- increment
```

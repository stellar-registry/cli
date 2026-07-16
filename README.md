# stellar-registry-cli

[![Apache 2.0 licensed](https://img.shields.io/badge/license-apache%202.0-blue.svg)](LICENSE)

Command-line interface for publishing, deploying, and installing smart contracts
through the **Stellar Registry** — an on-chain contract that manages Wasm
publication and named contract deployments on the [Stellar](https://stellar.org)
blockchain.

The CLI installs as a plugin under the `stellar` CLI, exposed as `stellar registry`.

## Related repositories

- **CLI** (this repo): [stellar-registry/cli](https://github.com/stellar-registry/cli)
- **On-chain contracts**: [stellar-registry/contracts](https://github.com/stellar-registry/contracts)
- **Frontend**: [stellar-registry/ui](https://github.com/stellar-registry/ui)
- **Indexer & API**: [stellar-registry/indexer](https://github.com/stellar-registry/indexer)
- **Scaffold toolkit**: [stellar-scaffold/cli](https://github.com/stellar-scaffold/cli)

## Installation

```bash
cargo install --locked stellar-registry-cli
```

We recommend [`cargo-binstall`](https://github.com/cargo-bins/cargo-binstall) for prebuilt binaries:

```bash
cargo binstall --locked stellar-registry-cli
```

## Quick start

```bash
# Publish a compiled contract to the registry
stellar registry publish --wasm target/stellar/local/my_contract.wasm --wasm-name my-contract

# Deploy a published wasm as a named instance (constructor args after `--`)
stellar registry deploy \
  --contract-name my-contract-instance \
  --wasm-name my-contract \
  -- \
  --param1 value1

# Install the deployed contract locally as a stellar-cli alias
stellar registry install my-contract-instance
```

Use `--help` on any command for full usage. See the crate README at
[`crates/stellar-registry-cli`](./crates/stellar-registry-cli/README.md) for
the detailed command reference, configuration, and the mainnet workflow.

## Crates

| Crate | Purpose |
|-------|---------|
| [`stellar-registry-cli`](./crates/stellar-registry-cli) | The `stellar registry` CLI plugin |
| [`stellar-registry-build`](./crates/stellar-registry-build) | Library for interacting with the registry at build time |
| [`stellar-registry`](./crates/stellar-registry) | The `import_contract!`, `import_contract_client!`, and `import_asset!` macros |

## What is the Contract Registry?

The registry is an on-chain smart contract that lets you:

- Publish and verify contract Wasm binaries with semantic versioning
- Deploy published contracts as named instances
- Manage multiple versions of the same contract
- Reuse deployed contracts across dApps

It separates **Wasm publication** (reusable code), **contract deployment**
(instances), and **local installation** (CLI aliases). The contracts themselves
live in [stellar-registry/contracts](https://github.com/stellar-registry/contracts).

## Documentation

- [CLI Commands](https://scaffoldstellar.com/docs/cli)
- [Registry Guide](https://scaffoldstellar.com/docs/registry)
- [Environment Configuration](https://scaffoldstellar.com/docs/environments)

## License

Licensed under the Apache-2.0 License — see [LICENSE](./LICENSE) for details.

# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This repo is the **stellar-registry** CLI — a Stellar CLI plugin (`stellar registry`) for publishing, deploying, and installing smart contracts via the on-chain registry, plus its supporting Rust libraries. It is one of several repos split out of the original `scaffold-stellar` monorepo.

Related repos:
- `stellar-registry/contracts` — the on-chain registry contracts this CLI talks to
- `stellar-registry/ui` — registry frontend
- `stellar-registry/indexer` — registry indexer & API
- `stellar-scaffold/cli` — the `stellar scaffold` CLI; publishes `stellar-build` and `stellar-scaffold-macro` (crates.io) that this repo depends on

## Common Commands

```bash
# Install the pinned stellar-cli (v26.0.0) into ./target/bin and set up git hooks
just setup

# Build / check / test
cargo build --workspace
cargo check --workspace --all-targets
cargo test --workspace

# Run the CLI during development (stellar registry ...)
cargo run -p stellar-registry-cli -- <args>

# Format and lint
cargo fmt --all -- --check
cargo clippy --all-targets
```

Note: the `justfile` still carries some recipes from the monorepo. Prefer the `cargo` commands above until it is trimmed to this repo.

## Architecture

### Crate Structure

| Crate | Purpose |
|-------|---------|
| `stellar-registry-cli` | The `stellar registry` CLI: `publish`, `deploy`, `download`, `install`/`create-alias`, `upgrade`, `register-contract` |
| `stellar-registry-build` | Library for interacting with the registry at build time |
| `stellar-registry` | Shared registry types and the `import_contract_client!` macro (published to crates.io; dev-dependency of `stellar-registry/contracts`) |

### CLI Command Flow

`publish` → `deploy` → `install` / `create-alias`
- `publish` — uploads wasm to the registry with semantic versioning
- `deploy` — instantiates a published wasm as a named contract
- `install` / `create-alias` — creates a local stellar-cli alias from the registry

## Testing

- Unit tests run without external dependencies.
- Integration tests require a local Stellar RPC (Docker `stellar/quickstart`) and the `integration-tests` feature flag.
- Command tests use `stellar_scaffold_test::RegistryTest` (the `stellar-scaffold-test` crate, pulled via git from `stellar-scaffold/cli`).

## Cross-repo dependencies

This repo's crates depend on, from crates.io:
- `stellar-build` and `stellar-scaffold-macro` — published from `stellar-scaffold/cli`
- `stellar-scaffold-test` — pulled via git from `stellar-scaffold/cli` (it is `publish = false`); used in tests only

These are declared as workspace dependencies in the root `Cargo.toml`. If the registry CLI ever needs an unreleased change in one of these, bump and publish it from `stellar-scaffold/cli` first (or temporarily `[patch]` it locally).

# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.0.10](https://github.com/stellar-registry/cli/compare/stellar-registry-build-v0.0.9...stellar-registry-build-v0.0.10) - 2026-07-27

### Added

- [**breaking**] `import_contract!` macro + consolidate registry macros (stellar-scaffold/cli#419) ([#17](https://github.com/stellar-registry/cli/pull/17))

## [0.0.9](https://github.com/stellar-registry/cli/compare/stellar-registry-build-v0.0.8...stellar-registry-build-v0.0.9) - 2026-07-06

### Other

- Split out of the `scaffold-stellar` monorepo into `stellar-registry/cli`; `stellar-build` and `stellar-scaffold-macro` are now consumed from crates.io instead of path dependencies

## [0.0.8](https://github.com/theahaco/scaffold-stellar/compare/stellar-registry-build-v0.0.7...stellar-registry-build-v0.0.8) - 2026-04-27

### Added

- *(registry)* update to new contract id for registry ([#478](https://github.com/theahaco/scaffold-stellar/pull/478))

## [0.0.7](https://github.com/theahaco/scaffold-stellar/compare/stellar-registry-build-v0.0.6...stellar-registry-build-v0.0.7) - 2026-03-19

### Other

- *(registry)* update version and prepare for deploy ([#422](https://github.com/theahaco/scaffold-stellar/pull/422))

## [0.0.6](https://github.com/theahaco/scaffold-stellar/compare/stellar-registry-build-v0.0.5...stellar-registry-build-v0.0.6) - 2026-02-16

### Fixed

- update code for stellar-cli v25, soroban-sdk v25, and admin-sep API changes ([#383](https://github.com/theahaco/scaffold-stellar/pull/383))
- use pedantic clippy and apply suggestions ([#379](https://github.com/theahaco/scaffold-stellar/pull/379))

# stellar-registry-name

_Parse, don't validate_

This library defines the standard names allowed throughout the Stellar Registry system. Parsing is the only way to construct them:

- **`Prefixed`** — `name` or `channel/name`. Rejects empty names, `@` (deployed contracts have no version), multiple slashes, and invalid characters. Private fields with accessors: `name()` / `channel()` / `mod_name()` / `canonical_name()`.
- **`Versioned`** — `Prefixed` + optional `@version` (leading `v` tolerated). A malformed version is an **error**, never silently "latest".

## Use in contract macros

This library backs the proc-macros shipped in the [stellar-registry](https://crates.io/crates/stellar-registry) crate. (This library is slim enough to be appropriate for use in Stellar smart contracts.)

`import_contract!` parses `Prefixed`, `import_contract_client!` parses `Versioned` — so "contracts have no version" is enforced by the type, not a string check.

## Use in CLI

This same library backs [stellar-registry-cli](https://crates.io/crates/stellar-registry-cli)'s argument parsing, so bad names fail at parsing time with real messages.

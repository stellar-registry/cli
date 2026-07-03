# `import_contract!` macro — Design

**Issue:** stellar-scaffold/cli#419 — `` `import_contract!` macro ``
**Repo:** `stellar-registry/cli`
**Date:** 2026-07-02

## Goal

Make cross-contract calls to a *named* Stellar Registry contract a one-liner:

```rust
pub fn thing_doer(env: &Env) {
    let dao = stellar_registry::import_contract!(env, our_dao);
    dao.create_proposal(/* ... */);
}
```

`import_contract!` returns a ready-to-call, type-safe `Client` **already bound to the
deployed contract's on-chain address** — collapsing today's two steps into one.

## Motivation

Today a consumer writes two things (see `contracts/registry-tansu-manager/src/lib.rs`):

```rust
stellar_registry::import_contract_client!(tansu_stub);          // 1. generate the type
// ...
let c = tansu_stub::Client::new(env, &tansu);                   // 2. supply the Address by hand
```

`import_contract_client!` resolves the **wasm** by name (for the generated `Client`
type) but knows nothing about **where the contract is deployed**. The caller must
obtain the `Address` separately. `import_contract!` adds the missing half: resolve the
deployed address by name from the registry and bake it in.

## Decisions (locked during brainstorming)

1. **Address model — build-time bake.** Resolve name → address at *compile time* via
   RPC and embed the address as a constant. Matches the issue's wording ("when you
   build your contract … the macro would need to make network calls to look up the
   contract"), mirrors how `import_contract_client!` downloads the wasm at build time,
   and has zero runtime cost. Trade-off: the address is frozen at build; if the named
   contract is redeployed, rebuild.
2. **Home — new local crate `stellar-registry-macro`** in this repo (not the external
   `stellar-scaffold-macro`). Keeps registry-specific logic (`fetch-contract-id`) in
   the registry's own repo and keeps the work executable here.
3. **Offline resolution — mirror `import_contract_client!`, plus an env override
   checked first.** Consistency with the existing macro is the dominant value; the env
   override makes CI / unit-test builds hermetic without a lockfile's tooling weight.

## Non-goals (YAGNI)

- Committed address lockfile (`registry-ids.toml`) with a `refresh` command. The `.id`
  cache introduced here is a deliberate precursor if this is wanted later.
- Runtime on-chain address lookup (bake registry address, resolve target per call).
- Multiple addresses / address lists per import.
- Any change to `import_contract_client!` behavior.

## Architecture

### A. Crate layout

New proc-macro crate: `crates/stellar-registry-macro`.

- `Cargo.toml`: `[lib] proc-macro = true`; deps `syn`, `quote`, `proc-macro2`,
  `stellar-build` (workspace). `syn`/`quote`/`proc-macro2` are added to
  `[workspace.dependencies]` in the root `cli/Cargo.toml`.
- Root `cli/Cargo.toml` `[workspace.dependencies]` gains
  `stellar-registry-macro = { path = "crates/stellar-registry-macro" }`.
  (`members = ["crates/*"]` already auto-includes the new crate.)
- `crates/stellar-registry/Cargo.toml` adds `stellar-registry-macro = { workspace = true }`.
- `crates/stellar-registry/src/lib.rs` adds, beside the existing
  `pub use stellar_scaffold_macro::*;`:

  ```rust
  pub use stellar_registry_macro::import_contract;
  ```

Consumers keep writing `stellar_registry::import_contract!(...)`.

### B. Macro surface

`import_contract!($env:expr, $name)`.

- `$name` uses the **same grammar** as `import_contract_client!`: bare ident
  (`registry`), string literal (`"unverified/our_dao"`), optional `@version`
  (`"our_dao@v1.0.0"`), optional channel prefix. The module name is derived
  identically: take the final `/`-segment, replace `-` with `_`.
- `$env` is an expression bound as `&Env`. **The caller passes `&env`** (an `&Env`);
  documented in the macro docs. The expansion binds it once:
  `let __env: &soroban_sdk::Env = $env;`.
- The macro expands to a **block expression** whose value is the constructed
  `mod_name::Client`.

### C. Codegen

Primary approach — **delegate wasm/type generation to `import_contract_client!`** so no
resolution logic is duplicated:

```rust
{
    stellar_registry::import_contract_client!(/* original $name tokens, verbatim */);
    let __env: &soroban_sdk::Env = /* $env */ env;
    our_dao::Client::new(
        __env,
        &soroban_sdk::Address::from_str(__env, "CABC…"), // baked, resolved at build
    )
}
```

- `import_contract_client!` emits `pub(crate) mod our_dao { use super::soroban_sdk;
  soroban_sdk::contractimport!(file = "…our_dao.wasm"); }`. Inside the block its
  `use super::soroban_sdk` resolves to the consumer's module — the **same** in-scope
  `soroban_sdk` requirement the existing macro already imposes.
- `soroban_sdk::Address::from_str(env: &Env, strkey: &str) -> Address` is a real SDK
  convenience (verified in soroban-sdk 26.0.0-rc.1 `src/address.rs`, wrapping
  `from_string(&String::from_str(env, strkey))`; assumed stable in 27.0.0-rc.1 — verify
  at build).
- The macro must recompute `mod_name` (last segment, `-`→`_`) to name the `Client`
  path; it reuses the same derivation function `import_contract_client!` uses.

**Fallback** if nesting a function-like proc-macro call inside generated output proves
fragile: inline the module ourselves — replicate the wasm-path resolution
(`resolve_wasm_path`) and emit `mod our_dao { … contractimport! … }` directly, then the
`Client::new` expression. Same output shape, no cross-macro dependency.

### D. Address resolution (build time)

Resolution order, first match wins:

1. **Env override** — read `STELLAR_CONTRACT_ID_<SANITIZED_NAME>` (uppercased
   `mod_name`, non-alphanumerics → `_`). If set, validate as a `C…` strkey and bake it.
   Purpose: hermetic tests / CI with no files and no network.
2. **Cache file** — `target/stellar/<network>/<file_stem>.id`, a sibling of the wasm's
   `.wasm`, where `<file_stem>` matches the wasm stem (`mod_name`, or
   `mod_name_<version-with-dots-as-underscores>` when a version is given). Read, validate,
   bake. Target dir + network come from `stellar_build::get_target_dir` / the network
   env, exactly as `import_contract_client!` resolves the wasm path.
3. **`STELLAR_NO_REGISTRY=1`** — if set, emit `compile_error!` instead of any network
   call (same escape hatch as the existing macro).
4. **RPC shell-out** — run `stellar registry fetch-contract-id <lookup_name>`, capture
   stdout (the `C…` address), validate the strkey, **write the `.id` cache file**, bake.
   Network selection is delegated to the `stellar` CLI's own config/`STELLAR_NETWORK`
   (the existing `download` shell-out passes no explicit network flag either).

`<lookup_name>` is the full name *including* any channel prefix and preserving hyphens
(e.g. `unverified/guess-the-number`) — `fetch-contract-id` takes a `PrefixedName`
positional and does no `_`/`-` normalization.

### E. Error handling

All failures are `compile_error!` at the macro call site:

- **Invalid / empty strkey** (from any source) → error naming the contract and the
  source (env var / cache file / CLI output).
- **CLI missing or fetch failed** → error mirroring `import_contract_client!`'s download
  copy: check the name & network; try `stellar registry fetch-contract-id <name>`
  yourself; set `STELLAR_NO_REGISTRY=1` to skip the registry lookup.

### F. Testing

- **Pure-helper unit tests** (mirror scaffold-macro's `parse_name_and_version` test
  module): `mod_name` derivation, `STELLAR_CONTRACT_ID_*` env-var-name sanitization,
  strkey validation, `.id` cache-path construction (with/without version).
- **Hermetic expansion test**: set the env override to a known `C…` address, expand, and
  assert the generated tokens construct `mod_name::Client::new(env, &Address::from_str(
  env, "C…"))`. No RPC.
- **Consumer caveat (documented, not code):** `import_contract!` bakes a *real network*
  address, so it is for real / integration builds. In `soroban_sdk` unit tests the
  dependency is registered at a fresh test-generated address, so the baked constant is
  not usable there — unit tests should keep `import_contract_client!` + their own
  `Client::new(env, &test_addr)`. This is why `registry-tansu-manager` (whose Tansu
  address is deploy-time / stored) is **not** migrated to `import_contract!`.

### G. Scope summary

**In:** the new crate + macro; the 4-step resolution; `compile_error!` handling; pure +
expansion tests; macro rustdoc with a worked example.
**Out:** everything in Non-goals.

## Open items to verify during implementation

1. `soroban_sdk::Address::from_str` presence/signature in the exact pinned soroban-sdk
   27.0.0-rc.1 (checked against 26.0.0-rc.1; API expected stable).
2. Nesting `import_contract_client!` inside `import_contract!` output compiles cleanly;
   if not, use the inline `contractimport!` fallback (§C).
3. Exact stdout format of `stellar registry fetch-contract-id` (currently
   `println!("{contract_id}")` — a bare `C…` line; trim whitespace).

# `import_contract!` Macro Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `stellar_registry::import_contract!(env, name)` proc-macro that returns a type-safe soroban `Client` already bound to the named contract's deployed on-chain address, resolved at build time.

**Architecture:** A new `proc-macro = true` crate `stellar-registry-macro` in the `cli` workspace, re-exported from `stellar-registry`. The macro delegates wasm/type generation to the existing `import_contract_client!`, resolves the deployed address at build time (env override → `.id` cache → `stellar registry fetch-contract-id` shell-out, gated by `STELLAR_NO_REGISTRY`), and emits `mod_name::Client::new(env, &Address::from_str(env, "C…"))`.

**Tech Stack:** Rust (edition 2024), `syn` 2 / `quote` / `proc-macro2`, `stellar-build` (target-dir/network), `stellar-strkey` (address validation), the `stellar` CLI (`registry fetch-contract-id`).

**Design spec:** `docs/superpowers/specs/2026-07-02-import-contract-macro-design.md`. **Issue:** stellar-scaffold/cli#419.

> **Revised post-review (2026-07-14, PR #17):** the "delegates wasm/type
> generation to `import_contract_client!`" architecture above was rejected. See
> the design spec's "Revision (post-review)" section — the macro takes no
> `@version`, generates types from the deployed contract's own on-chain wasm
> (`stellar contract fetch --id`), and fails compilation if the contract is
> flagged (`fetch-contract-id --reject-flagged`, a raw `ContractEntry` ledger read).

## Global Constraints

- Rust **edition 2024** (matches the existing `stellar-registry` crate).
- **Strict clippy pedantic** — code must pass `just clippy` (`-Dclippy::pedantic`).
- Dep versions (match `stellar-scaffold-macro` 0.8.14): `proc-macro2 = "1.0"`, `quote = "1.0"`, `syn = { version = "2", features = ["full"] }`, `stellar-build` (workspace `0.0.6`), `stellar-strkey` (workspace `0.0.15`).
- The macro crate **must not** depend on `soroban-sdk`; it emits `::soroban_sdk::…` paths that resolve in the consumer crate.
- Address lookup is by **name only** (deployed instances are named, not versioned); the wasm keeps version semantics via the delegated `import_contract_client!`.
- Preserve hyphens in the value passed to `fetch-contract-id` (`PrefixedName` does no `_`/`-` normalization); only the *module name* gets `-`→`_`.

---

### Task 1: New crate skeleton, workspace wiring, and pure helpers

**Files:**
- Create: `crates/stellar-registry-macro/Cargo.toml`
- Create: `crates/stellar-registry-macro/src/lib.rs`
- Modify: `Cargo.toml` (root workspace — add three build deps + the path dep)
- Test: unit tests inside `crates/stellar-registry-macro/src/lib.rs` (`#[cfg(test)]`)

**Interfaces:**
- Produces (used by later tasks): `fn mod_name_from(&str) -> String`, `fn split_version(&str) -> (String, Option<String>)`, `fn env_var_name(&str) -> String`, `fn validate_contract_id(&str) -> Result<String, String>`, `fn cache_id_path(&Path, &str) -> PathBuf`, `fn manifest() -> PathBuf`.

- [ ] **Step 1: Create the crate manifest**

Create `crates/stellar-registry-macro/Cargo.toml`:

```toml
[package]
name = "stellar-registry-macro"
version = "0.0.1"
edition = "2024"
description = "The import_contract! macro for the Stellar Registry"
license = "Apache-2.0"
repository.workspace = true

[lib]
proc-macro = true

[dependencies]
proc-macro2 = { workspace = true }
quote = { workspace = true }
syn = { workspace = true }
stellar-build = { workspace = true }
stellar-strkey = { workspace = true }

[lints]
workspace = true
```

- [ ] **Step 2: Wire the workspace dependencies**

In the root `Cargo.toml` `[workspace.dependencies]`, add under `# Local crates`:

```toml
stellar-registry-macro = { path = "crates/stellar-registry-macro" }
```

and add these three build-macro deps (they are not yet in the workspace):

```toml
proc-macro2 = "1.0"
quote = "1.0"
syn = { version = "2", features = ["full"] }
```

(`stellar-strkey = "0.0.15"` and `stellar-build = "0.0.6"` already exist in `[workspace.dependencies]`.)

- [ ] **Step 3: Write the failing helper tests**

Create `crates/stellar-registry-macro/src/lib.rs` with the test module first:

```rust
#[cfg(test)]
mod helpers {
    use super::*;
    use std::path::Path;

    // A real, valid contract strkey (from soroban-sdk docs).
    const VALID: &str = "CBESJIMX7J53SWJGJ7WQ6QTLJI4S5LPPJNC2BNVD63GIKAYCDTDOO322";

    #[test]
    fn mod_name_strips_prefix_and_hyphens() {
        assert_eq!(mod_name_from("unverified/registry_tansu_manager"), "registry_tansu_manager");
        assert_eq!(mod_name_from("guess-the-number"), "guess_the_number");
        assert_eq!(mod_name_from("a/b/c"), "c");
        assert_eq!(mod_name_from("registry"), "registry");
    }

    #[test]
    fn split_version_optional_v() {
        assert_eq!(split_version("our_dao@v0.1.0"), ("our_dao".into(), Some("0.1.0".into())));
        assert_eq!(split_version("x@1.2.3"), ("x".into(), Some("1.2.3".into())));
        assert_eq!(split_version("x"), ("x".into(), None));
    }

    #[test]
    fn env_var_name_uppercases_and_sanitizes() {
        assert_eq!(env_var_name("registry_tansu_manager"), "STELLAR_CONTRACT_ID_REGISTRY_TANSU_MANAGER");
        assert_eq!(env_var_name("guess_the_number"), "STELLAR_CONTRACT_ID_GUESS_THE_NUMBER");
    }

    #[test]
    fn validate_contract_id_trims_and_checks() {
        assert_eq!(validate_contract_id(&format!("  {VALID}\n")).unwrap(), VALID);
        assert!(validate_contract_id("not-an-address").is_err());
        assert!(validate_contract_id("").is_err());
    }

    #[test]
    fn cache_id_path_is_wasm_sibling() {
        assert_eq!(cache_id_path(Path::new("target"), "our_dao"), Path::new("target/our_dao.id"));
    }
}
```

- [ ] **Step 4: Run the tests to verify they fail to compile**

Run: `cargo test -p stellar-registry-macro`
Expected: FAIL — `cannot find function mod_name_from` (and the others).

- [ ] **Step 5: Implement the helpers**

Prepend to `crates/stellar-registry-macro/src/lib.rs` (above the test module):

```rust
//! The `import_contract!` proc-macro: resolve a named Stellar Registry contract
//! to a type-safe client already bound to its deployed on-chain address.
extern crate proc_macro;

use std::{
    env,
    path::{Path, PathBuf},
};

/// Path to the compiling crate's `Cargo.toml`.
fn manifest() -> PathBuf {
    PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("failed to find cargo manifest"))
        .join("Cargo.toml")
}

/// Rust module identifier from a (possibly channel-prefixed) registry name:
/// final `/`-segment with `-` replaced by `_`.
fn mod_name_from(name_part: &str) -> String {
    name_part
        .rsplit('/')
        .next()
        .unwrap_or(name_part)
        .replace('-', "_")
}

/// Split `"name@v1.2.3"` / `"name@1.2.3"` into `(name, version-without-leading-v)`.
fn split_version(raw: &str) -> (String, Option<String>) {
    match raw.split_once('@') {
        Some((name, ver)) => (
            name.to_string(),
            Some(ver.strip_prefix('v').unwrap_or(ver).to_string()),
        ),
        None => (raw.to_string(), None),
    }
}

/// Env var a caller can set to bypass the network:
/// `STELLAR_CONTRACT_ID_<NAME>`, NAME = uppercased module name with any
/// non-alphanumeric replaced by `_`.
fn env_var_name(mod_name: &str) -> String {
    let sanitized: String = mod_name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_uppercase() } else { '_' })
        .collect();
    format!("STELLAR_CONTRACT_ID_{sanitized}")
}

/// Validate a `C…` contract strkey; return it trimmed.
fn validate_contract_id(s: &str) -> Result<String, String> {
    let t = s.trim();
    t.parse::<stellar_strkey::Contract>()
        .map(|_| t.to_string())
        .map_err(|_| format!("not a valid contract id (C… strkey): {t:?}"))
}

/// `<target_dir>/<mod_name>.id` — sibling of the wasm the client imports.
/// Keyed by name only: a deployed instance's address is version-independent.
fn cache_id_path(target_dir: &Path, mod_name: &str) -> PathBuf {
    target_dir.join(mod_name).with_extension("id")
}
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -p stellar-registry-macro`
Expected: PASS (5 tests in `helpers`).

- [ ] **Step 7: Commit**

```bash
git add crates/stellar-registry-macro Cargo.toml
git commit -m "feat: stellar-registry-macro crate skeleton + pure helpers"
```

---

### Task 2: Build-time address resolution

**Files:**
- Modify: `crates/stellar-registry-macro/src/lib.rs`
- Test: `#[cfg(test)]` module in the same file

**Interfaces:**
- Consumes: `validate_contract_id` (Task 1).
- Produces: `fn resolve_address(env_lookup, env_var, cache, no_registry, fetch) -> Result<String, String>` (all IO injected) and `fn fetch_contract_id(&str) -> Result<String, String>` (real shell-out, used by Task 4).

- [ ] **Step 1: Write the failing resolution tests**

Add to `crates/stellar-registry-macro/src/lib.rs`:

```rust
#[cfg(test)]
mod resolution {
    use super::*;
    const A: &str = "CBESJIMX7J53SWJGJ7WQ6QTLJI4S5LPPJNC2BNVD63GIKAYCDTDOO322";
    const B: &str = "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC";

    fn no_fetch() -> Result<String, String> { Err("fetch should not run".into()) }

    #[test]
    fn env_override_wins() {
        let got = resolve_address(
            |k| (k == "STELLAR_CONTRACT_ID_FOO").then(|| A.to_string()),
            "STELLAR_CONTRACT_ID_FOO",
            Some(B.to_string()),
            false,
            no_fetch,
        );
        assert_eq!(got.unwrap(), A);
    }

    #[test]
    fn cache_used_when_no_env() {
        let got = resolve_address(|_| None, "X", Some(B.to_string()), false, no_fetch);
        assert_eq!(got.unwrap(), B);
    }

    #[test]
    fn no_registry_errors_without_env_or_cache() {
        let got = resolve_address(|_| None, "X", None, true, no_fetch);
        assert!(got.unwrap_err().contains("STELLAR_NO_REGISTRY"));
    }

    #[test]
    fn fetch_is_last_resort() {
        let got = resolve_address(|_| None, "X", None, false, || Ok(A.to_string()));
        assert_eq!(got.unwrap(), A);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p stellar-registry-macro resolution`
Expected: FAIL — `cannot find function resolve_address`.

- [ ] **Step 3: Implement resolution + shell-out**

Add to `crates/stellar-registry-macro/src/lib.rs` (above the test modules):

```rust
use std::process::Command;

/// Resolve the deployed address, first hit wins. All IO is injected so the
/// precedence is unit-testable without a network or filesystem.
fn resolve_address(
    env_lookup: impl Fn(&str) -> Option<String>,
    env_var: &str,
    cache: Option<String>,
    no_registry: bool,
    fetch: impl FnOnce() -> Result<String, String>,
) -> Result<String, String> {
    if let Some(v) = env_lookup(env_var) {
        return validate_contract_id(&v);
    }
    if let Some(c) = cache {
        return validate_contract_id(&c);
    }
    if no_registry {
        return Err(format!(
            "No cached contract id and STELLAR_NO_REGISTRY=1 so not checking the Registry. \
             Set {env_var}, or run `stellar registry fetch-contract-id <name>` and rebuild."
        ));
    }
    validate_contract_id(&fetch()?)
}

/// Shell out to the `stellar` CLI to look up a deployed contract's id by name.
/// Network selection is delegated to the CLI's own config (`STELLAR_NETWORK`).
fn fetch_contract_id(lookup_name: &str) -> Result<String, String> {
    let out = Command::new("stellar")
        .args(["registry", "fetch-contract-id", lookup_name])
        .output()
        .map_err(|e| {
            format!(
                "failed to run `stellar registry fetch-contract-id`: {e}. \
                 Install it with `cargo install stellar-registry-cli` and try again."
            )
        })?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(format!(
            "Could not resolve a contract id for `{lookup_name}`. \
             Check the name & network and try again (https://stellar.rgstry.xyz), \
             run `stellar registry fetch-contract-id {lookup_name}` yourself, \
             or set STELLAR_NO_REGISTRY=1 to skip the registry lookup.\n{}",
            String::from_utf8_lossy(&out.stderr)
        ))
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p stellar-registry-macro resolution`
Expected: PASS (4 tests). `fetch_contract_id` is exercised in Task 4 / manual integration, not unit tests.

- [ ] **Step 5: Commit**

```bash
git add crates/stellar-registry-macro/src/lib.rs
git commit -m "feat: build-time address resolution for import_contract!"
```

---

### Task 3: Macro input parsing and code generation

**Files:**
- Modify: `crates/stellar-registry-macro/src/lib.rs`
- Test: `#[cfg(test)]` module in the same file

**Interfaces:**
- Consumes: `mod_name_from`, `split_version` (Task 1).
- Produces: `struct Input { env: Expr, name_raw: String, name_span: Span }` (implements `syn::parse::Parse`) and `fn expand(&Expr, &str, &Ident, &str) -> proc_macro2::TokenStream` (used by Task 4).

- [ ] **Step 1: Write the failing codegen test**

Add to `crates/stellar-registry-macro/src/lib.rs`:

```rust
#[cfg(test)]
mod codegen {
    use super::*;
    use quote::quote;
    use syn::{parse2, Ident};
    use proc_macro2::Span;

    const A: &str = "CBESJIMX7J53SWJGJ7WQ6QTLJI4S5LPPJNC2BNVD63GIKAYCDTDOO322";

    #[test]
    fn parses_env_and_string_name() {
        let input: Input = parse2(quote!(env, "unverified/our_dao@v0.1.0")).unwrap();
        assert_eq!(input.name_raw, "unverified/our_dao@v0.1.0");
    }

    #[test]
    fn parses_env_and_ident_name() {
        let input: Input = parse2(quote!(env, registry)).unwrap();
        assert_eq!(input.name_raw, "registry");
    }

    #[test]
    fn expand_emits_delegation_and_bound_client() {
        let env: syn::Expr = parse2(quote!(env)).unwrap();
        let ident = Ident::new("our_dao", Span::call_site());
        let out = expand(&env, "unverified/our_dao@v0.1.0", &ident, A).to_string();
        assert!(out.contains("import_contract_client"), "delegates wasm import: {out}");
        assert!(out.contains("\"unverified/our_dao@v0.1.0\""), "passes original name: {out}");
        assert!(out.contains("our_dao :: Client :: new"), "constructs the client: {out}");
        assert!(out.contains("Address :: from_str"), "builds the address: {out}");
        assert!(out.contains(A), "bakes the resolved id: {out}");
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p stellar-registry-macro codegen`
Expected: FAIL — `cannot find type Input` / `cannot find function expand`.

- [ ] **Step 3: Implement parsing and codegen**

Add to `crates/stellar-registry-macro/src/lib.rs` (above the test modules):

```rust
use proc_macro2::Span;
use quote::quote;
use syn::{
    parse::{Parse, ParseStream},
    Expr, Ident, LitStr, Token,
};

/// `import_contract!(env_expr, name)` — `name` is a bare ident or a string
/// literal using the same grammar as `import_contract_client!`.
struct Input {
    env: Expr,
    name_raw: String,
    name_span: Span,
}

impl Parse for Input {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let env: Expr = input.parse()?;
        input.parse::<Token![,]>()?;
        let name_span = input.span();
        let name_raw = if input.peek(LitStr) {
            input.parse::<LitStr>()?.value()
        } else {
            input.parse::<Ident>()?.to_string()
        };
        Ok(Self { env, name_raw, name_span })
    }
}

/// Emit a block expression: delegate wasm/type generation to
/// `import_contract_client!`, then construct the client bound to the baked
/// address. `name_raw` is passed through verbatim (version included) so the
/// delegated macro resolves the matching wasm.
fn expand(env: &Expr, name_raw: &str, mod_ident: &Ident, address: &str) -> proc_macro2::TokenStream {
    quote! {
        {
            ::stellar_registry::import_contract_client!(#name_raw);
            let __env: &::soroban_sdk::Env = #env;
            #mod_ident::Client::new(
                __env,
                &::soroban_sdk::Address::from_str(__env, #address),
            )
        }
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p stellar-registry-macro codegen`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/stellar-registry-macro/src/lib.rs
git commit -m "feat: parse import_contract! input and generate the bound client"
```

---

### Task 4: `#[proc_macro]` entry point + re-export from `stellar-registry`

**Files:**
- Modify: `crates/stellar-registry-macro/src/lib.rs` (add the `#[proc_macro]` fn)
- Modify: `crates/stellar-registry/Cargo.toml` (depend on the macro crate)
- Modify: `crates/stellar-registry/src/lib.rs` (re-export)

**Interfaces:**
- Consumes: `Input`, `expand` (Task 3); `resolve_address`, `fetch_contract_id` (Task 2); `mod_name_from`, `split_version`, `env_var_name`, `cache_id_path`, `manifest` (Task 1).
- Produces: `stellar_registry::import_contract!` usable by consumers.

- [ ] **Step 1: Implement the proc-macro entry point**

Add to `crates/stellar-registry-macro/src/lib.rs`:

```rust
use proc_macro::TokenStream;
use syn::parse_macro_input;

/// Generate a type-safe client for a deployed, registry-named contract,
/// already bound to its on-chain address (resolved at build time).
///
/// ```ignore
/// // `env: &Env`
/// let dao = stellar_registry::import_contract!(env, our_dao);
/// dao.create_proposal(/* ... */);
/// ```
///
/// `name` accepts the same forms as [`import_contract_client!`]:
/// `our_dao`, `"unverified/our_dao"`, `"our_dao@v1.0.0"`.
///
/// The address is resolved at build time: `STELLAR_CONTRACT_ID_<NAME>` env
/// override → `target/stellar/<network>/<name>.id` cache →
/// `stellar registry fetch-contract-id`. `STELLAR_NO_REGISTRY=1` forbids the
/// network call. Because a real on-chain address is baked in, use this in
/// real / integration builds; in `soroban_sdk` unit tests keep
/// `import_contract_client!` plus your own `Client::new(env, &test_addr)`.
#[proc_macro]
pub fn import_contract(input: TokenStream) -> TokenStream {
    let Input { env, name_raw, name_span } = parse_macro_input!(input as Input);
    let (name_part, _version) = split_version(&name_raw);
    let mod_name = mod_name_from(&name_part);
    let mod_ident = Ident::new(&mod_name, name_span);
    let evar = env_var_name(&mod_name);

    let no_registry = env::var("STELLAR_NO_REGISTRY").as_deref() == Ok("1");
    let cache_path = stellar_build::get_target_dir(&manifest())
        .ok()
        .map(|dir| cache_id_path(&dir, &mod_name));
    let cache = cache_path.as_ref().and_then(|p| std::fs::read_to_string(p).ok());

    let resolved = resolve_address(
        |k| env::var(k).ok(),
        &evar,
        cache,
        no_registry,
        || {
            let addr = fetch_contract_id(&name_part)?;
            if let Some(p) = &cache_path {
                let _ = std::fs::write(p, &addr);
            }
            Ok(addr)
        },
    );

    match resolved {
        Ok(address) => expand(&env, &name_raw, &mod_ident, &address).into(),
        Err(msg) => syn::Error::new(name_span, msg).to_compile_error().into(),
    }
}
```

- [ ] **Step 2: Verify the crate builds**

Run: `cargo build -p stellar-registry-macro`
Expected: builds clean.

- [ ] **Step 3: Depend on the macro crate from `stellar-registry`**

In `crates/stellar-registry/Cargo.toml`, under `[dependencies]`, add:

```toml
stellar-registry-macro = { workspace = true }
```

- [ ] **Step 4: Re-export the macro**

In `crates/stellar-registry/src/lib.rs`, add below the existing `pub use stellar_scaffold_macro::*;`:

```rust
pub use stellar_registry_macro::import_contract;
```

- [ ] **Step 5: Verify `stellar-registry` builds with the re-export**

Run: `cargo build -p stellar-registry`
Expected: builds clean; `stellar_registry::import_contract` is now public.

- [ ] **Step 6: Run the full crate test suite**

Run: `cargo test -p stellar-registry-macro`
Expected: PASS — all `helpers`, `resolution`, `codegen` tests green.

- [ ] **Step 7: Commit**

```bash
git add crates/stellar-registry-macro/src/lib.rs crates/stellar-registry/Cargo.toml crates/stellar-registry/src/lib.rs
git commit -m "feat: wire import_contract! proc-macro and re-export from stellar-registry"
```

---

### Task 5: Lint pass, docs build, and end-to-end integration check

**Files:**
- Modify: `crates/stellar-registry-macro/src/lib.rs` (only if clippy/doc requires)

**Interfaces:** none new.

- [ ] **Step 1: Run pedantic clippy across the workspace**

Run: `just clippy`
Expected: no warnings. Fix any pedantic findings in `stellar-registry-macro` (likely `must_use`, `uninlined_format_args`) until clean.

- [ ] **Step 2: Build docs**

Run: `cargo doc -p stellar-registry-macro --no-deps`
Expected: builds; the `import_contract` rustdoc renders with the example.

- [ ] **Step 3: Manual hermetic expansion check (no network)**

In a scratch soroban contract crate that has a wasm fixture staged at `target/stellar/<network>/hello_world.wasm` and `soroban-sdk` + `stellar-registry` deps, add:

```rust
let _c = stellar_registry::import_contract!(env, hello_world); // env: &Env
```

Run: `STELLAR_CONTRACT_ID_HELLO_WORLD=CBESJIMX7J53SWJGJ7WQ6QTLJI4S5LPPJNC2BNVD63GIKAYCDTDOO322 STELLAR_NETWORK=local cargo build`
Expected: compiles — confirms delegation to `import_contract_client!`, the `::soroban_sdk::Address::from_str` path, and env-override resolution all resolve together. If `::soroban_sdk::Address::from_str` is absent in the pinned soroban-sdk (verify item §C.1 of the spec), switch the emitted address construction to `::soroban_sdk::Address::from_string(&::soroban_sdk::String::from_str(__env, #address))` and re-run.

- [ ] **Step 4: Commit any lint/doc fixes**

```bash
git add crates/stellar-registry-macro/src/lib.rs
git commit -m "chore: satisfy pedantic clippy and docs for import_contract!"
```

---

## Follow-ups (out of scope for this plan)

- **Publish** `stellar-registry-macro` and bump `stellar-registry` so the `contracts` repo can consume `import_contract!` across crates.io (per the cross-repo wiring in the umbrella CLAUDE.md).
- **Automated integration test** in the `contracts` repo (which already stages wasm fixtures and builds before test) exercising `import_contract!` end-to-end against a local registry.
- **Address lockfile** (`registry-ids.toml` + a refresh command) for reproducible multi-network builds, layered on the `.id` cache.

## Self-Review

- **Spec coverage:** crate layout (Task 1/4), macro surface (Task 3), delegation codegen (Task 3/4), 4-step resolution incl. env override / cache / `STELLAR_NO_REGISTRY` / shell-out (Task 2/4), `compile_error!` handling (Task 2/4), pure + codegen tests (Task 1–3), rustdoc + unit-test caveat (Task 4/5). All spec sections map to a task.
- **Type consistency:** `resolve_address` / `fetch_contract_id` / `expand` / `Input` signatures are identical where produced (Task 2/3) and consumed (Task 4). `cache_id_path` takes `(&Path, &str)` everywhere (version dropped by design — address is version-independent).
- **Placeholder scan:** every code step contains complete code; the only conditional is the documented soroban-sdk API fallback in Task 5 Step 3, tied to spec verify-item §C.1.

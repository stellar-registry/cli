set dotenv-load := true

export PATH := './target/debug:./target/bin:' + env_var('PATH')
export CONFIG_DIR := 'target/'
export CI_BUILD := env_var_or_default('CI_BUILD', '')

[private]
path:
    just --list

registry +args:
    @cargo run --bin stellar-registry --quiet -- {{ args }}

s +args:
    @stellar {{ args }}

stellar +args:
    @stellar {{ args }}

# Build the registry CLI
build:
    cargo build $CI_BUILD --package stellar-registry-cli

# Setup the project to use a pinned version of the CLI
setup:
    git config core.hooksPath .githooks
    -cargo binstall -y stellar-cli --version 26.0.0 --force --install-path ./target/bin

test: build
    cargo t

# Run integration tests (requires test wasms in target/stellar/local; see fetch-test-wasms)
test-integration: build
    cargo t --features integration-tests

# Print where to get the wasms the integration tests load (registry.wasm + hello_v1/v2.wasm)
fetch-test-wasms:
    @echo "Integration tests load these from target/stellar/local:"
    @echo "  - registry.wasm        (from the stellar-registry/contracts release)"
    @echo "  - hello_v1.wasm        (from the stellar-scaffold/cli test fixtures)"
    @echo "  - hello_v2.wasm        (from the stellar-scaffold/cli test fixtures)"

[private]
_test-integration package filter ci="false":
    cargo t  -E 'package({{ package }}) and {{ filter }}' \
    {{ if ci == "false" { '--features integration-tests' } else { '--binaries-metadata target/nextest/binaries-metadata.json --cargo-metadata target/nextest/cargo-metadata.json --target-dir-remap target --workspace-remap .' } }}

# Run registry-cli integration tests
test-integration-registry ci="false":
    just _test-integration stellar-registry-cli 'test(/./)' {{ ci }}

clippy *args:
    cargo clippy --all {{ args }} \
    -- -Dclippy::pedantic -Aclippy::must_use_candidate -Aclippy::missing_errors_doc -Aclippy::missing_panics_doc

clippy-test:
    just clippy --tests --all-features

# Update deterministic contract IDs after changing crates/stellar-registry-build/.salt
update-registry-tests:
    UPDATE_EXPECT=1 cargo test --package stellar-registry-build registry::generate_id

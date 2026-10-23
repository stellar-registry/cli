use clap::Parser;
use stellar_cli::{commands::contract::invoke, config};
use stellar_registry_build::{name::Prefixed, registry::Registry};

use crate::commands::{account, global};

#[derive(Parser, Debug, Clone)]
pub struct Cmd {
    /// Name to register for the contract. Can use prefix if not using verified registry.
    /// E.g. `unverified/<name>`
    #[arg(long)]
    pub contract_name: Prefixed,

    /// Contract address to register
    #[arg(long)]
    pub contract_address: String,

    /// Owner of the contract registration
    #[arg(long)]
    pub owner: Option<String>,

    /// Prepares and simulates without invoking
    #[arg(long)]
    pub dry_run: bool,

    #[command(flatten)]
    pub config: global::Args,
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Invoke(#[from] invoke::Error),
    #[error(transparent)]
    Config(#[from] config::Error),
    #[error(transparent)]
    Registry(#[from] stellar_registry_build::Error),
    #[error(transparent)]
    Account(#[from] account::Error),
}

impl Cmd {
    pub async fn run(&self) -> Result<(), Error> {
        let owner = if let Some(owner) = self.owner.clone() {
            owner
        } else {
            self.config.source_account()?.to_string()
        };

        let args = [
            "register_contract",
            "--contract_name",
            self.contract_name.name(),
            "--contract_address",
            &self.contract_address,
            "--owner",
            &owner,
        ];

        let registry = Registry::new(&self.config, self.contract_name.channel()).await?;
        let owner_address = account::resolve_address(&self.config, &owner);
        let operator =
            account::operator(&registry, &self.config, &owner_address, Some(&owner)).await?;
        if !self.dry_run {
            operator.require_signer(false)?;
        }

        registry.as_contract().invoke(&args, self.dry_run).await?;
        if self.dry_run {
            operator.require_signer(true)?;
        }

        eprintln!(
            "{}Successfully registered contract '{}' at {}",
            if self.dry_run { "Dry Run: " } else { "" },
            self.contract_name.name(),
            self.contract_address
        );
        Ok(())
    }
}

#[cfg(feature = "integration-tests")]
#[cfg(test)]
mod tests {
    use stellar_cli::commands::contract::deploy::wasm;
    use stellar_registry_test::{AssertExt, RegistryTest};

    #[tokio::test]
    async fn simple() {
        let registry = RegistryTest::new().await;
        let v1 = registry.hello_wasm_v1();

        // Deploy a contract directly (not through registry)
        let deploy_cmd = registry
            .parse_cmd::<wasm::Cmd>(&[
                "--wasm",
                v1.to_str().unwrap(),
                "--source",
                "alice",
                "--fee=10000000",
                "--",
                "--admin=alice",
            ])
            .unwrap();
        let contract_id = deploy_cmd
            .execute(&deploy_cmd.config, false, false)
            .await
            .unwrap()
            .into_result()
            .unwrap()
            .to_string();

        // Now register it in the registry
        registry
            .registry_cli("register-contract")
            .arg("--contract-name")
            .arg("my-hello")
            .arg("--contract-address")
            .arg(contract_id.as_str())
            .assert()
            .success();

        // Verify we can fetch the contract ID
        let fetched_id = registry
            .parse_cmd::<crate::commands::fetch_contract_id::Cmd>(&["my-hello"])
            .unwrap()
            .fetch_contract_id()
            .await
            .unwrap();
        assert_eq!(contract_id, fetched_id.to_string());
    }

    #[tokio::test]
    async fn unverified() {
        let registry = RegistryTest::new().await;
        let v1 = registry.hello_wasm_v1();

        // Deploy a contract directly (not through registry)
        let deploy_cmd = registry
            .parse_cmd::<wasm::Cmd>(&[
                "--wasm",
                v1.to_str().unwrap(),
                "--source",
                "alice",
                "--fee=10000000",
                "--",
                "--admin=alice",
            ])
            .unwrap();
        let contract_id = deploy_cmd
            .execute(&deploy_cmd.config, false, false)
            .await
            .unwrap()
            .into_result()
            .unwrap()
            .to_string();

        // Now register it in the unverified registry
        registry
            .registry_cli("register-contract")
            .arg("--contract-name")
            .arg("unverified/my-hello")
            .arg("--contract-address")
            .arg(contract_id.as_str())
            .assert()
            .success();

        // Verify we can fetch the contract ID
        let fetched_id = registry
            .parse_cmd::<crate::commands::fetch_contract_id::Cmd>(&["unverified/my-hello"])
            .unwrap()
            .fetch_contract_id()
            .await
            .unwrap();
        assert_eq!(contract_id, fetched_id.to_string());
    }

    async fn deploy_hello(registry: &RegistryTest) -> String {
        let v1 = registry.hello_wasm_v1();
        let deploy_cmd = registry
            .parse_cmd::<wasm::Cmd>(&[
                "--wasm",
                v1.to_str().unwrap(),
                "--source",
                "alice",
                "--fee=10000000",
                "--",
                "--admin=alice",
            ])
            .unwrap();
        deploy_cmd
            .execute(&deploy_cmd.config, false, false)
            .await
            .unwrap()
            .into_result()
            .unwrap()
            .to_string()
            .as_str()
            .to_string()
    }

    // The root is managed by alice; bob can't sign for it, so neither a dry
    // run nor a real run may read as a success.
    #[tokio::test]
    async fn managed_register_without_the_manager_key_is_refused() {
        let registry = RegistryTest::new().await;
        registry.generate_funded_identity("bob").await;
        let contract_id = deploy_hello(&registry).await;
        for dry_run in [true, false] {
            let mut cmd = registry.registry_cli("register-contract");
            cmd.args([
                "--contract-name",
                "bobs-hello",
                "--contract-address",
                &contract_id,
            ])
            .args(["--source", "bob"]);
            if dry_run {
                cmd.arg("--dry-run");
            }
            let out = cmd.assert().failure().stderr_as_str();
            assert!(
                out.contains("needs the registry manager's signature (governance)"),
                "{out}"
            );
            assert!(!out.contains("Successfully"), "{out}");
            assert_eq!(out.contains("Simulation OK."), dry_run, "{out}");
        }
    }

    #[tokio::test]
    async fn unmanaged_update_without_the_owner_key_is_refused() {
        let registry = RegistryTest::new().await;
        let bob = registry.generate_funded_identity("bob").await;
        let contract_id = deploy_hello(&registry).await;
        registry
            .registry_cli("register-contract")
            .args([
                "--contract-name",
                "unverified/hello",
                "--contract-address",
                &contract_id,
            ])
            .assert()
            .success();

        // alice owns it; bob can't sign as owner, even for a dry run.
        let out = registry
            .registry_cli("update-contract-owner")
            .args(["--contract-name", "unverified/hello", "--new-owner", &bob])
            .args(["--source", "bob", "--dry-run"])
            .assert()
            .failure()
            .stderr_as_str();
        assert!(
            out.contains("Simulation OK. Submitting needs the owner's signature"),
            "{out}"
        );
    }
}

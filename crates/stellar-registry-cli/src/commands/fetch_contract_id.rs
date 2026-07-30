use clap::Parser;
use stellar_cli::commands::contract::invoke;
use stellar_registry_build::name::{Prefixed, RegistryAccess};
use stellar_strkey::Contract;

use crate::commands::global;

#[derive(Parser, Debug, Clone)]
pub struct Cmd {
    /// Name of deployed contract. Can use prefix if not using verified registry.
    /// E.g. `unverified/<name>`
    pub contract_name: Prefixed,

    /// Return the id even if the contract is flagged as compromised in the
    /// registry. Without this, flagged contracts fail with a non-zero exit.
    #[arg(long)]
    pub force: bool,

    #[command(flatten)]
    pub config: global::Args,
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Invoke(#[from] invoke::Error),
    #[error(transparent)]
    Config(#[from] stellar_cli::config::Error),
    #[error(transparent)]
    Registry(#[from] stellar_registry_build::Error),
    #[error(
        "contract `{0}` is flagged as compromised in the registry; pass --force to fetch its id anyway"
    )]
    ContractFlagged(String),
}

impl Cmd {
    pub async fn run(&self) -> Result<(), Error> {
        let contract_id = self.fetch_contract_id().await?;
        println!("{contract_id}");
        Ok(())
    }

    pub async fn fetch_contract_id(&self) -> Result<Contract, Error> {
        let registry = self.contract_name.registry(&self.config).await?;
        if self.force {
            return Ok(registry
                .fetch_contract_id_unchecked(self.contract_name.name())
                .await?);
        }
        registry
            .fetch_contract_id(self.contract_name.name())
            .await
            .map_err(|e| match e {
                stellar_registry_build::Error::ContractFlagged(_) => {
                    Error::ContractFlagged(self.contract_name.to_string())
                }
                other => other.into(),
            })
    }
}

#[cfg(feature = "integration-tests")]
#[cfg(test)]
mod tests {
    use stellar_registry_test::{AssertExt, RegistryTest};

    #[tokio::test]
    async fn simple() {
        let registry = RegistryTest::new().await;
        let v1 = registry.hello_wasm_v1();

        // First publish the contract
        registry
            .registry_cli("publish")
            .arg("--wasm")
            .arg(v1.to_str().unwrap())
            .arg("--binver")
            .arg("0.0.1")
            .arg("--wasm-name")
            .arg("hello")
            .assert()
            .success();

        // Then deploy it
        registry
            .registry_cli("deploy")
            .arg("--contract-name")
            .arg("hello")
            .arg("--wasm-name")
            .arg("hello")
            .arg("--")
            .arg("--admin=alice")
            .assert()
            .success();

        let contract_id = registry
            .parse_cmd::<super::Cmd>(&["hello"])
            .unwrap()
            .fetch_contract_id()
            .await
            .unwrap();
        assert!(!contract_id.to_string().is_empty());
    }

    #[tokio::test]
    async fn unverified() {
        let registry = RegistryTest::new().await;
        let v1 = registry.hello_wasm_v1();

        // First publish the contract
        registry
            .registry_cli("publish")
            .arg("--wasm")
            .arg(v1.to_str().unwrap())
            .arg("--binver")
            .arg("0.0.1")
            .arg("--wasm-name")
            .arg("unverified/hello")
            .assert()
            .success();

        // Then deploy it
        registry
            .registry_cli("deploy")
            .arg("--contract-name")
            .arg("unverified/hello")
            .arg("--wasm-name")
            .arg("unverified/hello")
            .arg("--")
            .arg("--admin=alice")
            .assert()
            .success();

        let contract_id = registry
            .parse_cmd::<super::Cmd>(&["unverified/hello"])
            .unwrap()
            .fetch_contract_id()
            .await
            .unwrap();
        assert!(!contract_id.to_string().is_empty());
    }

    #[tokio::test]
    async fn flagged() {
        let registry = RegistryTest::new().await;
        let v1 = registry.hello_wasm_v1();

        // 1. publish wasm
        registry
            .registry_cli("publish")
            .arg("--wasm")
            .arg(v1.to_str().unwrap())
            .arg("--binver")
            .arg("0.0.1")
            .arg("--wasm-name")
            .arg("hello")
            .assert()
            .success();

        // 2. deploy contract
        let output = registry
            .registry_cli("deploy")
            .arg("--contract-name")
            .arg("hello")
            .arg("--wasm-name")
            .arg("hello")
            .arg("--")
            .arg("--admin=alice")
            .assert()
            .success()
            .stdout_as_str();
        let flagged_id = output.split_whitespace().last().unwrap();

        // 3. Flag contract
        registry
            .env
            .stellar("contract")
            .args([
                "invoke",
                "--id",
                &registry.registry_address,
                "--",
                "flag_contract",
                "--flagged",
                "true",
                "--contract-name",
                "hello",
            ])
            .assert()
            .success();

        let err = registry
            .parse_cmd::<super::Cmd>(&["hello"])
            .unwrap()
            .fetch_contract_id()
            .await
            .unwrap_err();
        assert!(err.to_string().contains("flagged"));

        let contract_id = registry
            .parse_cmd::<super::Cmd>(&["hello", "--force"])
            .unwrap()
            .fetch_contract_id()
            .await
            .unwrap();
        assert_eq!(*contract_id.to_string(), *flagged_id);
    }
}

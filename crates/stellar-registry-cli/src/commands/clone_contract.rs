use std::ffi::OsString;

use clap::Parser;
use stellar_cli::{
    commands::contract::{deploy::wasm, fetch},
    config::{self, UnresolvedContract, network},
};
use stellar_registry_build::{name::Prefixed, registry::Registry};

use crate::CommandParser;
use crate::commands::{global, register_contract};

/// Copy a registered contract from one network to another: look it up by
/// name on `--from-network`, deploy its Wasm on the target network with the
/// given constructor arguments, and register it there under the same name.
#[derive(Parser, Debug, Clone)]
pub struct Cmd {
    /// Name of the contract on `--from-network`. Can use prefix if not using
    /// verified registry. E.g. `unverified/<name>`
    pub contract_name: Prefixed,

    /// Network to copy the contract from, e.g. `testnet`
    #[arg(long)]
    pub from_network: String,

    /// Name to register the copy under on the target network; defaults to
    /// `contract_name`
    #[arg(long = "as")]
    pub as_name: Option<Prefixed>,

    /// Copy the contract even if it is flagged as compromised on
    /// `--from-network`
    #[arg(long)]
    pub force: bool,

    /// Owner of the registration on the target network; defaults to the
    /// source account
    #[arg(long)]
    pub owner: Option<String>,

    /// Arguments for the contract's constructor
    #[arg(last = true, id = "CONSTRUCTOR_ARGS")]
    pub slop: Vec<OsString>,

    #[command(flatten)]
    pub config: global::Args,
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Registry(#[from] stellar_registry_build::Error),
    #[error(
        "contract `{0}` is flagged as compromised on the source network; pass --force to copy it anyway"
    )]
    ContractFlagged(String),
    #[error(transparent)]
    Fetch(#[from] fetch::Error),
    #[error(transparent)]
    Deploy(#[from] wasm::Error),
    #[error(transparent)]
    RegisterContract(#[from] register_contract::Error),
    #[error(transparent)]
    Config(#[from] config::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Clap(#[from] clap::Error),
    #[error("deploying returned a transaction instead of a contract id")]
    NoContractId,
}

impl Cmd {
    pub async fn run(&self) -> Result<(), Error> {
        let contract_id = self.clone_contract().await?;
        println!("{contract_id}");
        Ok(())
    }

    /// The source network's view of the config: same locator, different
    /// network. Env-provided RPC settings belong to the target, so they are
    /// not carried over.
    fn source_config(&self) -> global::Args {
        let mut source = self.config.clone();
        source.config.network = network::Args {
            network: Some(self.from_network.clone()),
            ..Default::default()
        };
        source
    }

    pub async fn clone_contract(&self) -> Result<stellar_strkey::Contract, Error> {
        let source = self.source_config();

        // 1. Resolve the name on the source network, refusing flagged
        //    contracts unless `--force`. `STELLAR_REGISTRY_CONTRACT_ID` points
        //    at the target's registry, so it only applies to the source when
        //    both are the same network.
        let same_network = source.get_network()?.network_passphrase
            == self.config.get_network()?.network_passphrase;
        let channel = self.contract_name.channel();
        let registry = if same_network {
            Registry::new(&source, channel).await?
        } else {
            Registry::new_canonical(&source, channel).await?
        };
        let name = self.contract_name.name();
        let source_id = if self.force {
            registry.fetch_contract_id_unchecked(name).await?
        } else {
            registry
                .fetch_contract_id(name)
                .await
                .map_err(|e| match e {
                    stellar_registry_build::Error::ContractFlagged(_) => {
                        Error::ContractFlagged(self.contract_name.to_string())
                    }
                    other => other.into(),
                })?
        };

        // 2. Fetch the deployed contract's own Wasm from the source network.
        let wasm = fetch::Cmd {
            contract_id: Some(UnresolvedContract::Resolved(source_id)),
            wasm_hash: None,
            out_file: None,
            locator: source.locator.clone(),
            network: source.network.clone(),
        }
        .get_bytes()
        .await?;
        let wasm_path = std::env::temp_dir().join(format!("{source_id}.wasm"));
        std::fs::write(&wasm_path, wasm)?;

        // 3. Deploy it on the target network with the constructor arguments.
        let wasm_path = wasm_path.to_string_lossy();
        let slop: Vec<String> = self
            .slop
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        let mut args = vec!["--wasm", wasm_path.as_ref()];
        if !slop.is_empty() {
            args.push("--");
            args.extend(slop.iter().map(String::as_str));
        }
        let mut deploy = wasm::Cmd::parse_arg_vec(&args)?;
        deploy.config = self.config.config.clone();
        let contract_id = deploy
            .execute(&deploy.config, false, false)
            .await?
            .into_result()
            .ok_or(Error::NoContractId)?;

        // 4. Register the new contract under the same name.
        register_contract::Cmd {
            contract_name: self.as_name.clone().unwrap_or(self.contract_name.clone()),
            contract_address: format!("{contract_id}"),
            owner: self.owner.clone(),
            dry_run: false,
            config: self.config.clone(),
        }
        .run()
        .await?;

        Ok(contract_id)
    }
}

#[cfg(feature = "integration-tests")]
#[cfg(test)]
mod tests {
    use stellar_registry_test::{AssertExt, RegistryTest};

    #[tokio::test]
    async fn copies_a_contract_under_a_new_name() {
        let registry = RegistryTest::new().await;
        let v1 = registry.hello_wasm_v1();

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

        // Source and target are both the local network here, so the copy
        // needs a different name.
        let copied = registry
            .registry_cli("clone-contract")
            .arg("hello")
            .arg("--from-network")
            .arg("local")
            .arg("--as")
            .arg("hello-copy")
            .arg("--")
            .arg("--admin=alice")
            .assert()
            .success()
            .stdout_as_str();

        let original = registry
            .parse_cmd::<crate::commands::fetch_contract_id::Cmd>(&["hello"])
            .unwrap()
            .fetch_contract_id()
            .await
            .unwrap();
        let fetched = registry
            .parse_cmd::<crate::commands::fetch_contract_id::Cmd>(&["hello-copy"])
            .unwrap()
            .fetch_contract_id()
            .await
            .unwrap();
        assert_eq!(copied.trim(), fetched.to_string());
        assert_ne!(original, fetched, "a new deployment, not the same contract");
    }
}

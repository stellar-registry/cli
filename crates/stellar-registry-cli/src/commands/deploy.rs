#![allow(dead_code)]
use std::{ffi::OsString, path::PathBuf};

use clap::Parser;
use soroban_rpc as rpc;
pub use soroban_spec_tools::contract as contract_spec;
use stellar_cli::{
    assembled::simulate_and_assemble_transaction,
    commands::contract::invoke,
    config::{self, UnresolvedMuxedAccount},
    print::Print,
    utils::rpc::get_remote_wasm_from_hash,
    xdr::{self, AccountId, InvokeContractArgs, ScSpecEntry, ScString, ScVal, Uint256},
};
use stellar_registry_build::{
    name::{Prefixed, RegistryAccess},
    registry::Registry,
};

use crate::commands::global;

pub mod from_network;
pub mod util;

#[derive(Parser, Debug, Clone)]
pub struct Cmd {
    /// Name of contract to be deployed. Can use prefix of not using verified registry.
    /// E.g. `unverified/<name>`
    #[arg(long, visible_alias = "deploy-as")]
    pub contract_name: Prefixed,
    /// Name of published contract to deploy from. Can use prefix of not using verified registry.
    /// E.g. `unverified/<name>`
    #[arg(long, required_unless_present = "from_network")]
    pub wasm_name: Option<Prefixed>,
    /// Copy a contract registered on another network (e.g. `testnet`) instead
    /// of deploying a published Wasm: its Wasm is deployed here from your
    /// account, not through the registry, then registered as `--contract-name`
    #[arg(long, conflicts_with_all = ["wasm_name", "version", "deployer"])]
    pub from_network: Option<String>,
    /// Name of the contract on `--from-network`, if different from
    /// `--contract-name`
    #[arg(long, requires = "from_network")]
    pub from_contract_name: Option<Prefixed>,
    /// With `--from-network`, copy the contract even if it is flagged as
    /// compromised there
    #[arg(long, requires = "from_network")]
    pub force: bool,
    /// Arguments for constructor
    #[arg(last = true, id = "CONSTRUCTOR_ARGS")]
    pub slop: Vec<OsString>,
    /// Version of the wasm to deploy
    #[arg(long)]
    pub version: Option<String>,
    /// Optional deployer, by default is registry contract itself
    #[arg(long)]
    pub deployer: Option<UnresolvedMuxedAccount>,
    #[command(flatten)]
    pub config: global::Args,
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Invoke(#[from] invoke::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Install(#[from] super::create_alias::Error),
    #[error(transparent)]
    Rpc(#[from] rpc::Error),
    #[error(transparent)]
    SpecTools(#[from] soroban_spec_tools::Error),
    #[error(transparent)]
    Config(#[from] config::Error),
    #[error(transparent)]
    ConfigAddress(#[from] config::address::Error),
    #[error(transparent)]
    Xdr(#[from] xdr::Error),
    #[error("Cannot parse contract spec")]
    CannotParseContractSpec,
    #[error("argument count ({current}) surpasses maximum allowed count ({maximum})")]
    MaxNumberOfArgumentsReached { current: usize, maximum: usize },
    #[error("function {0} was not found in the contract")]
    FunctionNotFoundInContractSpec(String),
    #[error("parsing argument {arg}: {error}")]
    CannotParseArg {
        arg: String,
        error: stellar_cli::commands::contract::arg_parsing::Error,
    },
    #[error("function name {0} is too long")]
    FunctionNameTooLong(String),
    #[error("Missing file argument {0:#?}")]
    MissingFileArg(PathBuf),
    #[error("Missing argument {0}")]
    MissingArgument(String),
    #[error("Constructor help message: {0}")]
    ConstructorHelpMessage(String),
    #[error("{0}")]
    InvalidReturnValue(String),
    #[error(transparent)]
    Registry(#[from] stellar_registry_build::Error),
    #[error(transparent)]
    FromNetwork(#[from] from_network::Error),
    #[error(
        "simulated fee of ~{} XLM is too large for a transaction; the call is likely paying unexpected storage rent",
        .0 / 10_000_000
    )]
    FeeTooLarge(i64),
}

impl Cmd {
    pub async fn run(&self) -> Result<(), Error> {
        let result = match &self.from_network {
            Some(from_network) => from_network::deploy(self, from_network)
                .await
                .map_err(Error::from),
            None => self.invoke().await,
        };
        match result {
            Ok(contract_id) => {
                println!(
                    "Contract {} deployed successfully to {contract_id}",
                    self.contract_name.name()
                );
                Ok(())
            }
            Err(Error::ConstructorHelpMessage(help)) => {
                println!("Constructor help message:\n{help}");
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    /// `--wasm-name`, which clap requires unless `--from-network` is given.
    fn wasm_name(&self) -> &Prefixed {
        self.wasm_name
            .as_ref()
            .expect("--wasm-name is required without --from-network")
    }

    pub async fn hash(&self, registry: &Registry) -> Result<xdr::Hash, Error> {
        let res = registry
            .as_contract()
            .invoke_with_result(
                &["fetch_hash", "--wasm_name", self.wasm_name().name()],
                true,
            )
            .await?;
        let res = res.trim_matches('"');
        Ok(res.parse().unwrap())
    }

    pub async fn wasm(&self, registry: &Registry) -> Result<Vec<u8>, Error> {
        Ok(
            get_remote_wasm_from_hash(&self.config.rpc_client()?, &self.hash(registry).await?)
                .await?,
        )
    }

    pub async fn spec_entries(&self, registry: &Registry) -> Result<Vec<ScSpecEntry>, Error> {
        Ok(contract_spec::Spec::new(&self.wasm(registry).await?)
            .map_err(|_| Error::CannotParseContractSpec)?
            .spec)
    }

    async fn invoke(&self) -> Result<stellar_strkey::Contract, Error> {
        let target_registry = self.contract_name.registry(&self.config).await?;
        let wasm_registry = self.wasm_name().registry(&self.config).await?;
        let cross_registry = target_registry.as_contract().id() != wasm_registry.as_contract().id();
        let client = self.config.rpc_client()?;
        let key = self.config.key_pair()?;
        let config = &self.config;

        let contract_address = target_registry.as_contract().sc_address();
        let contract_id = &target_registry.as_contract().id();
        let spec_entries = self.spec_entries(&wasm_registry).await?;
        let (args, signers) =
            util::find_args_and_signers(contract_id, self.slop.clone(), &spec_entries)?;
        let deployer = if let Some(deployer) = &self.deployer {
            Some(deployer.resolve_muxed_account(&self.config.locator, None)?)
        } else {
            None
        };
        let mut call_args: Vec<ScVal> = vec![
            ScVal::String(ScString(self.wasm_name().name().try_into().unwrap())),
            self.version.clone().map_or(ScVal::Void, |s| {
                ScVal::String(ScString(s.try_into().unwrap()))
            }),
            ScVal::String(ScString(self.contract_name.name().try_into().unwrap())),
            ScVal::Address(xdr::ScAddress::Account(AccountId(
                xdr::PublicKey::PublicKeyTypeEd25519(Uint256(key.verifying_key().to_bytes())),
            ))),
            args,
            deployer.map_or(ScVal::Void, |muxed_account| {
                ScVal::Address(xdr::ScAddress::Account(muxed_account.account_id()))
            }),
        ];
        let function_name = if cross_registry {
            // The contract resolves the subregistry's address itself, through
            // the trusted root pinned at construction, so we pass the name
            // rather than an address. Root has no prefix — its own name in
            // root storage is "registry".
            let subregistry_name = self.wasm_name().channel().unwrap_or("registry");
            call_args.push(ScVal::String(ScString(
                subregistry_name.try_into().unwrap(),
            )));
            "deploy_with_subregistry"
        } else {
            "deploy"
        };
        let invoke_contract_args = InvokeContractArgs {
            contract_address: contract_address.clone(),
            function_name: function_name.try_into().unwrap(),
            args: call_args.try_into().unwrap(),
        };

        // Get the account sequence number
        let public_strkey =
            stellar_strkey::ed25519::PublicKey(key.verifying_key().to_bytes()).to_string();
        let account_details = client.get_account(&public_strkey).await?;
        let sequence: i64 = account_details.seq_num.into();
        let tx = util::build_invoke_contract_tx(invoke_contract_args, sequence + 1, 100, &key)?;
        let assembled = simulate_and_assemble_transaction(&client, &tx, None, None, None).await?;
        util::check_fee(&assembled)?;
        let mut txn = assembled.transaction().clone();
        txn = config
            .sign_soroban_authorizations(&txn, &signers, &Print::new(true))
            .await?
            .unwrap_or(txn);
        let return_value = client
            .send_transaction_polling(&config.sign(txn, false).await?)
            .await?
            .return_value()?;
        match return_value {
            ScVal::Address(xdr::ScAddress::Contract(xdr::ContractId(hash))) => {
                Ok(stellar_strkey::Contract(hash.0))
            }
            _ => Err(Error::InvalidReturnValue(
                "{return_value:#?} is not a contract address".to_string(),
            )),
        }
    }
}

#[cfg(feature = "integration-tests")]
#[cfg(test)]
mod tests {
    use stellar_registry_test::RegistryTest;

    fn publish(registry: &RegistryTest, wasm_name: &str, version: &str) {
        let wasm_path = registry.hello_wasm_v1();
        registry
            .registry_cli("publish")
            .arg("--wasm")
            .arg(&wasm_path)
            .arg("--binver")
            .arg(version)
            .arg("--wasm-name")
            .arg(wasm_name)
            .assert()
            .success();
    }

    // --wasm-name points to the unverified subregistry while --contract-name
    // has no prefix (root). The CLI should route to `deploy_with_subregistry`
    // on root, passing the unverified registry's address as the extra arg.
    #[tokio::test]
    async fn deploys_wasm_from_a_different_registry() {
        let registry = RegistryTest::new().await;

        publish(&registry, "unverified/hello_xreg", "0.0.1");

        registry
            .registry_cli("deploy")
            .arg("--wasm-name")
            .arg("unverified/hello_xreg")
            .arg("--contract-name")
            .arg("hello_xreg_deployed")
            .arg("--version")
            .arg("0.0.1")
            .arg("--")
            .arg("--admin=alice")
            .assert()
            .success();
    }

    // Reverse direction: wasm lives in root, contract registers under
    // unverified. `deploy_with_subregistry` is invoked on unverified, with
    // root as the subregistry address the XCC reaches back into.
    #[tokio::test]
    async fn deploys_from_root_into_a_subregistry() {
        let registry = RegistryTest::new().await;

        publish(&registry, "hello_reverse", "0.0.1");

        registry
            .registry_cli("deploy")
            .arg("--wasm-name")
            .arg("hello_reverse")
            .arg("--contract-name")
            .arg("unverified/hello_reverse_deployed")
            .arg("--version")
            .arg("0.0.1")
            .arg("--")
            .arg("--admin=alice")
            .assert()
            .success();
    }

    // Neither side of the deploy is the root: wasm published to an `oz`
    // subregistry, contract registered under `unverified`. Exercises the
    // cross-registry path where both registries are subregistries of root.
    #[tokio::test]
    async fn deploys_across_two_subregistries() {
        let registry = RegistryTest::new().await;
        registry.deploy_named_subregistry("oz").await;

        publish(&registry, "oz/hello_two_subs", "0.0.1");

        registry
            .registry_cli("deploy")
            .arg("--wasm-name")
            .arg("oz/hello_two_subs")
            .arg("--contract-name")
            .arg("unverified/hello_two_subs_deployed")
            .arg("--version")
            .arg("0.0.1")
            .arg("--")
            .arg("--admin=alice")
            .assert()
            .success();
    }

    // Nothing has been published under this wasm name, so the wasm lookup
    // on the subregistry must fail. Deploy should exit with a non-zero
    // status and a message on stderr rather than silently succeed.
    #[tokio::test]
    async fn fails_when_wasm_does_not_exist_in_subregistry() {
        let registry = RegistryTest::new().await;

        registry
            .registry_cli("deploy")
            .arg("--wasm-name")
            .arg("unverified/never_published")
            .arg("--contract-name")
            .arg("should_not_exist")
            .arg("--")
            .arg("--admin=alice")
            .assert()
            .failure();
    }

    #[tokio::test]
    async fn from_network_copies_a_registered_contract() {
        let registry = RegistryTest::new().await;
        publish(&registry, "hello", "0.0.1");
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
        registry
            .registry_cli("deploy")
            .arg("--contract-name")
            .arg("hello-copy")
            .arg("--from-network")
            .arg("local")
            .arg("--from-contract-name")
            .arg("hello")
            .arg("--")
            .arg("--admin=alice")
            .assert()
            .success();

        let fetch = |name: &str| {
            registry
                .parse_cmd::<crate::commands::fetch_contract_id::Cmd>(&[name])
                .unwrap()
        };
        let original = fetch("hello").fetch_contract_id().await.unwrap();
        let copy = fetch("hello-copy").fetch_contract_id().await.unwrap();
        assert_ne!(original, copy, "a new deployment, not the same contract");
    }
}

use stellar_cli::{
    commands::contract::{deploy::wasm, fetch},
    config::{self, UnresolvedContract, network},
};
use stellar_registry_build::registry::Registry;

use super::Cmd;
use crate::CommandParser;
use crate::commands::{global, register_contract};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Registry(#[from] stellar_registry_build::Error),
    #[error(
        "contract `{0}` is flagged as compromised on the source network; pass --force to deploy it anyway"
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

/// `deploy --from-network`: look the contract up by name on `from_network`,
/// deploy its Wasm on the target network directly from the source account
/// (not through the registry's `deploy`), then register it under
/// `--contract-name`.
pub async fn deploy(cmd: &Cmd, from_network: &str) -> Result<stellar_strkey::Contract, Error> {
    let source = source_config(cmd, from_network);
    let source_name = cmd
        .from_contract_name
        .as_ref()
        .unwrap_or(&cmd.contract_name);

    // 1. Resolve the name on the source network, refusing flagged contracts
    //    unless `--force`. `STELLAR_REGISTRY_CONTRACT_ID` points at the
    //    target's registry, so it only applies when both are the same network.
    let same_network =
        source.get_network()?.network_passphrase == cmd.config.get_network()?.network_passphrase;
    let channel = source_name.channel();
    let registry = if same_network {
        Registry::new(&source, channel).await?
    } else {
        Registry::new_canonical(&source, channel).await?
    };
    let name = source_name.name();
    let source_id = if cmd.force {
        registry.fetch_contract_id_unchecked(name).await?
    } else {
        registry
            .fetch_contract_id(name)
            .await
            .map_err(|e| match e {
                stellar_registry_build::Error::ContractFlagged(_) => {
                    Error::ContractFlagged(source_name.to_string())
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
    let slop: Vec<String> = cmd
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
    deploy.config = cmd.config.config.clone();
    let contract_id = deploy
        .execute(&deploy.config, false, false)
        .await?
        .into_result()
        .ok_or(Error::NoContractId)?;

    // 4. Register it under `--contract-name`.
    register_contract::Cmd {
        contract_name: cmd.contract_name.clone(),
        contract_address: format!("{contract_id}"),
        owner: None,
        dry_run: false,
        config: cmd.config.clone(),
    }
    .run()
    .await?;

    Ok(contract_id)
}

/// The source network's view of the config: same locator, different network.
/// Env-provided RPC settings belong to the target, so they are not carried
/// over.
fn source_config(cmd: &Cmd, from_network: &str) -> global::Args {
    let mut source = cmd.config.clone();
    source.config.network = network::Args {
        network: Some(from_network.to_string()),
        ..Default::default()
    };
    source
}

use clap::Parser;
use stellar_cli::{commands::contract::invoke, config};
use stellar_registry_build::{name::Prefixed, registry::Registry};

use crate::commands::{account, global};

#[derive(Parser, Debug, Clone)]
pub struct Cmd {
    /// Name of the registered contract
    #[arg(long)]
    pub contract_name: Prefixed,

    /// New contract address
    #[arg(long)]
    pub new_address: String,

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
        let registry = Registry::new(&self.config, self.contract_name.channel()).await?;
        let owner = registry
            .fetch_contract_owner(self.contract_name.name())
            .await?;
        let operator = account::operator(&registry, &self.config, &owner, None).await?;
        if !self.dry_run {
            operator.require_signer(false)?;
        }

        let args = [
            "update_contract_address",
            "--contract_name",
            self.contract_name.name(),
            "--new_address",
            &self.new_address,
        ];

        registry.as_contract().invoke(&args, self.dry_run).await?;
        if self.dry_run {
            operator.require_signer(true)?;
        }

        eprintln!(
            "{}Successfully updated address of '{}' to {}",
            if self.dry_run { "Dry Run: " } else { "" },
            self.contract_name.name(),
            self.new_address
        );
        Ok(())
    }
}

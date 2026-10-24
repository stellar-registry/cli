use clap::Parser;
use stellar_cli::{commands::contract::invoke, config};
use stellar_registry_build::{name::Prefixed, registry::Registry};

use crate::commands::{account, global};

#[derive(Parser, Debug, Clone)]
pub struct Cmd {
    /// Name of the registered account
    #[arg(long)]
    pub account_name: Prefixed,

    /// New owner address
    #[arg(long)]
    pub new_owner: String,

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
        let registry = Registry::new(&self.config, self.account_name.channel()).await?;
        let owner = registry
            .fetch_account_owner(self.account_name.name())
            .await?;
        let operator = account::operator(&registry, &self.config, &owner, None).await?;
        if !self.dry_run {
            operator.require_signer(false)?;
        }

        let args = [
            "update_account_owner",
            "--account_name",
            self.account_name.name(),
            "--new_owner",
            &self.new_owner,
        ];
        registry.as_contract().invoke(&args, self.dry_run).await?;
        if self.dry_run {
            operator.require_signer(true)?;
        }

        eprintln!(
            "{}Successfully updated owner of account '{}' to {}",
            if self.dry_run { "Dry Run: " } else { "" },
            self.account_name,
            self.new_owner
        );
        eprintln!(
            "Note: preauthorizations are tied to an owner, so any made out to the previous owner \
             {owner} (e.g. for an address update or rename of this account) no longer count. \
             They have to be made again for the new owner."
        );
        Ok(())
    }
}

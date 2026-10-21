use clap::Parser;
use stellar_cli::{commands::contract::invoke, config};
use stellar_registry_build::{name::Prefixed, registry::Registry};

use crate::commands::{account, global};

#[derive(Parser, Debug, Clone)]
pub struct Cmd {
    /// Name of the registered account
    #[arg(long)]
    pub account_name: Prefixed,

    /// New address: a G-address, smart account C-address, or identity alias.
    /// It must sign, or have preauthorized this name for the current owner.
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
        let registry = Registry::new(&self.config, self.account_name.channel()).await?;
        let owner = registry
            .fetch_account_owner(self.account_name.name())
            .await?;
        account::require_consent(
            &registry,
            &self.config,
            &account::Naming {
                name: self.account_name.name(),
                display_name: self.account_name.to_string(),
                account: &self.new_address,
                owner: &owner,
                account_is_arg: true,
            },
        )
        .await?;

        let args = [
            "update_account_address",
            "--account_name",
            self.account_name.name(),
            "--new_address",
            &self.new_address,
        ];
        registry.as_contract().invoke(&args, self.dry_run).await?;

        eprintln!(
            "{}Successfully pointed account '{}' at {}",
            if self.dry_run { "Dry Run: " } else { "" },
            self.account_name,
            self.new_address
        );
        Ok(())
    }
}

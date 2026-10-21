use clap::Parser;
use stellar_cli::{commands::contract::invoke, config};
use stellar_registry_build::{name::Prefixed, registry::Registry};

use crate::commands::{account, global};

#[derive(Parser, Debug, Clone)]
pub struct Cmd {
    /// Name of the preauthorization to withdraw
    #[arg(long)]
    pub account_name: Prefixed,

    /// Account that gave it: a G-address or identity alias. It signs this call.
    #[arg(long)]
    pub account_address: String,

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
        account::require_signer(&self.config, &self.account_address)?;
        let registry = Registry::new(&self.config, self.account_name.channel()).await?;

        let args = [
            "revoke_account_preauth",
            "--account_name",
            self.account_name.name(),
            "--account_address",
            &self.account_address,
        ];
        registry.as_contract().invoke(&args, self.dry_run).await?;

        eprintln!(
            "{}Revoked any preauthorization by {} for '{}'",
            if self.dry_run { "Dry Run: " } else { "" },
            self.account_address,
            self.account_name
        );
        Ok(())
    }
}

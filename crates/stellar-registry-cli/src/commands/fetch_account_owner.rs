use clap::Parser;
use stellar_cli::commands::contract::invoke;
use stellar_registry_build::name::{Prefixed, RegistryAccess};

use crate::commands::global;

#[derive(Parser, Debug, Clone)]
pub struct Cmd {
    /// Name of the registered account. Can use prefix if not using verified registry.
    /// E.g. `unverified/<name>`
    pub account_name: Prefixed,

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
}

impl Cmd {
    pub async fn run(&self) -> Result<(), Error> {
        println!("{}", self.fetch_account_owner().await?);
        Ok(())
    }

    pub async fn fetch_account_owner(&self) -> Result<String, Error> {
        let registry = self.account_name.registry(&self.config).await?;
        Ok(registry
            .fetch_account_owner(self.account_name.name())
            .await?)
    }
}

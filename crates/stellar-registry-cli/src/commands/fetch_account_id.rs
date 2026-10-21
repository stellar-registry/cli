use clap::Parser;
use stellar_cli::commands::contract::invoke;
use stellar_registry_build::name::{Prefixed, RegistryAccess};

use crate::commands::global;

#[derive(Parser, Debug, Clone)]
pub struct Cmd {
    /// Name of the registered account. Can use prefix if not using verified registry.
    /// E.g. `unverified/<name>`
    pub account_name: Prefixed,

    /// Return the address even if the account is flagged as compromised in the
    /// registry. Without this, flagged accounts fail with a non-zero exit.
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
        "account `{0}` is flagged as compromised in the registry; pass --force to fetch its address anyway"
    )]
    AccountFlagged(String),
}

impl Cmd {
    pub async fn run(&self) -> Result<(), Error> {
        println!("{}", self.fetch_account_id().await?);
        Ok(())
    }

    pub async fn fetch_account_id(&self) -> Result<String, Error> {
        let registry = self.account_name.registry(&self.config).await?;
        let name = self.account_name.name();
        if !self.force && registry.is_account_flagged(name).await? {
            return Err(Error::AccountFlagged(self.account_name.to_string()));
        }
        Ok(registry.fetch_account_id(name).await?)
    }
}

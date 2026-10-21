use clap::Parser;
use stellar_cli::{commands::contract::invoke, config};
use stellar_registry_build::{name::Prefixed, registry::Registry};

use crate::commands::{account, global};

#[derive(Parser, Debug, Clone)]
pub struct Cmd {
    /// Name the account agrees to be registered under. Can use prefix if not using
    /// verified registry. E.g. `unverified/<name>`
    #[arg(long)]
    pub account_name: Prefixed,

    /// Account giving consent: a G-address or identity alias. It signs this call.
    #[arg(long)]
    pub account_address: String,

    /// Owner the account agrees to. The preauthorization only counts for a
    /// registration (or address update or rename) with this owner. Defaults to
    /// the source account
    #[arg(long)]
    pub owner: Option<String>,

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
        let owner = match &self.owner {
            Some(owner) => owner.clone(),
            None => account::source_address(&self.config)?,
        };
        let registry = Registry::new(&self.config, self.account_name.channel()).await?;

        let args = [
            "preauthorize_account",
            "--account_name",
            self.account_name.name(),
            "--account_address",
            &self.account_address,
            "--owner",
            &owner,
        ];
        registry.as_contract().invoke(&args, self.dry_run).await?;

        eprintln!(
            "{}{} preauthorized being named '{}' with owner {}. It's single-use and expires \
             after the network's max temporary-entry TTL (~180 days on mainnet) unless revoked \
             with `revoke-account-preauth`.",
            if self.dry_run { "Dry Run: " } else { "" },
            self.account_address,
            self.account_name,
            owner
        );
        Ok(())
    }
}

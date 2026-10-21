use clap::Parser;
use stellar_cli::{commands::contract::invoke, config};
use stellar_registry_build::{name::Prefixed, registry::Registry};

use crate::commands::{account, global};

#[derive(Parser, Debug, Clone)]
pub struct Cmd {
    /// Name to register for the account. Can use prefix if not using verified registry.
    /// E.g. `unverified/<name>`
    #[arg(long)]
    pub account_name: Prefixed,

    /// Account to name: a G-address, smart account C-address, or identity alias.
    /// It must sign, or have preauthorized this name for this owner.
    #[arg(long)]
    pub account_address: String,

    /// Owner of the name. Defaults to the source account
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
        let owner = match &self.owner {
            Some(owner) => owner.clone(),
            None => account::source_address(&self.config)?,
        };
        let registry = Registry::new(&self.config, self.account_name.channel()).await?;
        account::require_consent(
            &registry,
            &self.config,
            &account::Naming {
                name: self.account_name.name(),
                display_name: self.account_name.to_string(),
                account: &self.account_address,
                owner: &account::resolve_address(&self.config, &owner),
                account_is_arg: true,
            },
        )
        .await?;

        let args = [
            "register_account",
            "--account_name",
            self.account_name.name(),
            "--account_address",
            &self.account_address,
            "--owner",
            &owner,
        ];
        registry.as_contract().invoke(&args, self.dry_run).await?;

        eprintln!(
            "{}Successfully registered account '{}' as {}",
            if self.dry_run { "Dry Run: " } else { "" },
            self.account_name,
            self.account_address
        );
        Ok(())
    }
}

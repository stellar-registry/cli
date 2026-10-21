use clap::Parser;
use stellar_cli::{commands::contract::invoke, config};
use stellar_registry_build::{name::Prefixed, registry::Registry};

use crate::commands::{account, global};

#[derive(Parser, Debug, Clone)]
pub struct Cmd {
    /// Current name of the registered account
    #[arg(long)]
    pub account_name: Prefixed,

    /// New name for the account, in the same registry. The account must
    /// consent to it: be the source account, or have preauthorized it for the
    /// current owner.
    #[arg(long)]
    pub new_name: String,

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
        let name = self.account_name.name();
        let address = registry.fetch_account_id(name).await?;
        let owner = registry.fetch_account_owner(name).await?;
        account::require_consent(
            &registry,
            &self.config,
            &account::Naming {
                name: &self.new_name,
                display_name: match self.account_name.channel() {
                    Some(channel) => format!("{channel}/{}", self.new_name),
                    None => self.new_name.clone(),
                },
                account: &address,
                owner: &owner,
                // rename_account doesn't take the address, so stellar-cli
                // can't add it as a signer.
                account_is_arg: false,
            },
        )
        .await?;

        let args = [
            "rename_account",
            "--old_name",
            name,
            "--new_name",
            &self.new_name,
        ];
        registry.as_contract().invoke(&args, self.dry_run).await?;

        eprintln!(
            "{}Successfully renamed account '{}' to '{}'",
            if self.dry_run { "Dry Run: " } else { "" },
            self.account_name,
            self.new_name
        );
        Ok(())
    }
}

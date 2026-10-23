use clap::Parser;
use stellar_cli::{commands::contract::invoke, config};
use stellar_registry_build::{name::Prefixed, registry::Registry};

use crate::commands::{account, global};

#[derive(Parser, Debug, Clone)]
pub struct Cmd {
    /// Name of the registered account
    #[arg(long)]
    pub account_name: Prefixed,

    /// Clear the compromised flag instead of setting it
    #[arg(long)]
    pub unflag: bool,

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

        let flagged = if self.unflag { "false" } else { "true" };
        let args = [
            "flag_account",
            "--account_name",
            self.account_name.name(),
            "--flagged",
            flagged,
        ];
        registry.as_contract().invoke(&args, self.dry_run).await?;
        if self.dry_run {
            operator.require_signer(true)?;
        }

        let action = if self.unflag {
            "Cleared the compromised flag on"
        } else {
            "Flagged as compromised:"
        };
        eprintln!(
            "{}{action} account '{}'",
            if self.dry_run { "Dry Run: " } else { "" },
            self.account_name
        );
        Ok(())
    }
}

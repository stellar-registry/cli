use clap::Parser;

use crate::commands::{
    fetch_account_id, fetch_account_owner, flag_account, preauthorize_account, register_account,
    rename_account, revoke_account_preauth, update_account_address, update_account_owner,
};

/// `stellar registry account <command>`: named accounts (G-addresses and
/// smart accounts), kept apart from the contract commands.
#[derive(Parser, Debug)]
pub enum Cmd {
    /// Register a name for an account (G-address or smart account)
    Register(Box<register_account::Cmd>),
    /// Consent in advance to being named under an owner, e.g. before a governance proposal
    Preauthorize(Box<preauthorize_account::Cmd>),
    /// Withdraw a pending preauthorization
    RevokePreauth(Box<revoke_account_preauth::Cmd>),
    /// Look up the address of a named account
    FetchId(Box<fetch_account_id::Cmd>),
    /// Look up the owner of a named account
    FetchOwner(Box<fetch_account_owner::Cmd>),
    /// Point a named account at a new address
    UpdateAddress(Box<update_account_address::Cmd>),
    /// Update the owner of a named account
    UpdateOwner(Box<update_account_owner::Cmd>),
    /// Rename a named account
    Rename(Box<rename_account::Cmd>),
    /// Flag a named account as compromised, or clear the flag
    Flag(Box<flag_account::Cmd>),
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Register(#[from] register_account::Error),
    #[error(transparent)]
    Preauthorize(#[from] preauthorize_account::Error),
    #[error(transparent)]
    RevokePreauth(#[from] revoke_account_preauth::Error),
    #[error(transparent)]
    FetchId(#[from] fetch_account_id::Error),
    #[error(transparent)]
    FetchOwner(#[from] fetch_account_owner::Error),
    #[error(transparent)]
    UpdateAddress(#[from] update_account_address::Error),
    #[error(transparent)]
    UpdateOwner(#[from] update_account_owner::Error),
    #[error(transparent)]
    Rename(#[from] rename_account::Error),
    #[error(transparent)]
    Flag(#[from] flag_account::Error),
}

impl Cmd {
    pub async fn run(&self) -> Result<(), Error> {
        match self {
            Cmd::Register(cmd) => cmd.run().await?,
            Cmd::Preauthorize(cmd) => cmd.run().await?,
            Cmd::RevokePreauth(cmd) => cmd.run().await?,
            Cmd::FetchId(cmd) => cmd.run().await?,
            Cmd::FetchOwner(cmd) => cmd.run().await?,
            Cmd::UpdateAddress(cmd) => cmd.run().await?,
            Cmd::UpdateOwner(cmd) => cmd.run().await?,
            Cmd::Rename(cmd) => cmd.run().await?,
            Cmd::Flag(cmd) => cmd.run().await?,
        }
        Ok(())
    }
}

use std::ops::Deref;

use soroban_rpc as rpc;
use stellar_cli::config::{self};

#[derive(clap::Args, Debug, Clone)]
pub struct Args {
    #[command(flatten)]
    pub config: config::Args,
}

impl Deref for Args {
    type Target = config::Args;

    fn deref(&self) -> &Self::Target {
        &self.config
    }
}

impl Args {
    pub fn rpc_client(&self) -> Result<rpc::Client, config::Error> {
        Ok(rpc::Client::new(&self.config.get_network()?.rpc_url)?)
    }
}

/// Simulation accepts any public key as the source, even an unfunded one.
const READ_ONLY_SOURCE: &str = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";

/// Config for commands that only simulate reads, so need no source account.
#[derive(clap::Args, Debug, Clone)]
pub struct ReadArgs {
    #[command(flatten)]
    pub network: config::network::Args,

    #[command(flatten)]
    pub locator: config::locator::Args,

    /// Account to simulate as. Optional: reads work with any public key.
    #[arg(long, short = 's', visible_alias = "source", env = "STELLAR_ACCOUNT")]
    pub source_account: Option<config::address::UnresolvedMuxedAccount>,
}

impl ReadArgs {
    pub fn config(&self) -> config::Args {
        config::Args {
            network: self.network.clone(),
            locator: self.locator.clone(),
            source_account: self
                .source_account
                .clone()
                .unwrap_or_else(|| READ_ONLY_SOURCE.parse().expect("valid public key")),
            ..Default::default()
        }
    }

    pub fn get_network(&self) -> Result<config::network::Network, config::Error> {
        self.config().get_network()
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    #[derive(Parser)]
    struct Cmd {
        #[command(flatten)]
        config: super::ReadArgs,
    }

    #[test]
    fn read_args_need_no_source_account() {
        let cmd = Cmd::try_parse_from(["cmd", "--network", "testnet"]).unwrap();
        cmd.config.config().source_account().unwrap();
    }
}

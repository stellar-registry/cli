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

/// For `#[command(mut_arg("source_account", global::optional_source))]` on
/// commands that only simulate reads, so they run without an identity.
pub fn optional_source(arg: clap::Arg) -> clap::Arg {
    arg.required(false)
        .default_value(READ_ONLY_SOURCE)
        .hide_default_value(true)
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    #[derive(Parser)]
    #[command(mut_arg("source_account", super::optional_source))]
    struct Cmd {
        #[command(flatten)]
        config: super::Args,
    }

    #[test]
    fn optional_source_needs_no_identity() {
        let cmd = Cmd::try_parse_from(["cmd", "--network", "testnet"]).unwrap();
        cmd.config.source_account().unwrap();
    }
}

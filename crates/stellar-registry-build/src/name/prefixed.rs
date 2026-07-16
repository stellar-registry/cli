use std::{convert::Infallible, fmt::Display, str::FromStr};

use stellar_cli::config;

use crate::{Error, contract::ContractId, registry::Registry};

#[derive(Clone, Debug)]
/// Help docs for special type
pub struct Prefixed {
    pub channel: Option<String>,
    pub name: String,
}

impl FromStr for Prefixed {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some((channel, name)) = s.split_once('/') {
            Ok(Self {
                channel: Some(channel.to_owned()),
                name: name.to_owned(),
            })
        } else {
            Ok(Self {
                channel: None,
                name: s.to_owned(),
            })
        }
    }
}

impl From<Prefixed> for ContractId {
    fn from(value: Prefixed) -> Self {
        Self::FromRegistry(value)
    }
}

impl Prefixed {
    pub async fn registry(&self, config: &config::Args) -> Result<Registry, Error> {
        Registry::from_named_registry(config, self).await
    }
}

impl Display for Prefixed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Prefixed { channel, name } = &self;
        write!(
            f,
            "{}{name}",
            channel
                .as_ref()
                .map(|channel| format!("{channel}/"))
                .unwrap_or_default()
        )
    }
}

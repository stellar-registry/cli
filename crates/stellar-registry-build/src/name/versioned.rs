use std::{convert::Infallible, fmt::Display, str::FromStr};

use crate::name::Prefixed;

#[derive(Clone, Debug)]
/// Help docs for special type
pub struct Versioned {
    pub name: Prefixed,
    pub version: Option<semver::Version>,
}

impl FromStr for Versioned {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some((name, version)) = s.split_once('@') {
            Ok(Self {
                name: name.parse()?,
                version: version.parse().ok(),
            })
        } else {
            Ok(Self {
                name: s.parse()?,
                version: None,
            })
        }
    }
}

impl Display for Versioned {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Versioned { name, version } = &self;

        write!(
            f,
            "{name}{}",
            version
                .as_ref()
                .map(|v| format!("@{v}"))
                .unwrap_or_default()
        )
    }
}

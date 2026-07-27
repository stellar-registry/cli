use std::{fmt::Display, str::FromStr};

use super::Error;

/// A registry contract name with an optional channel prefix, e.g. `our-dao`
/// or `unverified/our-dao`.
///
/// Only constructible by parsing, which enforces: non-empty, at most one `/`
/// (splitting `channel/name`), no `@` (deployed contracts have no version),
/// and every segment made of ASCII letters, digits, `-` or `_`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prefixed {
    channel: Option<String>,
    name: String,
}

impl FromStr for Prefixed {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err(Error::Empty);
        }
        if s.contains('@') {
            return Err(Error::UnexpectedVersion(s.to_owned()));
        }
        if s.starts_with('/') || s.ends_with('/') {
            return Err(Error::LeadingOrTrailingSlash(s.to_owned()));
        }
        let mut segments = s.split('/');
        let (channel, name) = match (segments.next(), segments.next(), segments.next()) {
            (Some(name), None, _) => (None, name),
            (Some(channel), Some(name), None) => (Some(channel), name),
            _ => return Err(Error::TooManySlashes(s.to_owned())),
        };
        for segment in channel.iter().chain(std::iter::once(&name)) {
            if let Some(c) = segment
                .chars()
                .find(|c| !c.is_ascii_alphanumeric() && *c != '-' && *c != '_')
            {
                return Err(Error::InvalidCharacter(s.to_owned(), c));
            }
        }
        Ok(Self {
            channel: channel.map(str::to_owned),
            name: name.to_owned(),
        })
    }
}

impl Prefixed {
    /// The bare contract name, without the channel prefix.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The channel prefix, if any (`unverified` in `unverified/our-dao`).
    #[must_use]
    pub fn channel(&self) -> Option<&str> {
        self.channel.as_deref()
    }

    /// Rust module identifier derived from the name: `-` → `_` and chars to lowercase.
    #[must_use]
    pub fn mod_name(&self) -> String {
        self.name.replace('-', "_").to_ascii_lowercase()
    }

    /// Canonical on-chain form of the bare name (see [`super::canonicalize`]).
    #[must_use]
    pub fn canonical_name(&self) -> String {
        super::canonicalize(&self.name)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_name() {
        let p: Prefixed = "registry".parse().unwrap();
        assert_eq!(p.name(), "registry");
        assert_eq!(p.channel(), None);
        assert_eq!(p.to_string(), "registry");
    }

    #[test]
    fn channel_prefixed_hyphenated() {
        let p: Prefixed = "unverified/guess-the-number".parse().unwrap();
        assert_eq!(p.channel(), Some("unverified"));
        assert_eq!(p.name(), "guess-the-number");
        assert_eq!(p.mod_name(), "guess_the_number");
        assert_eq!(p.to_string(), "unverified/guess-the-number");
    }

    #[test]
    fn underscored_name() {
        let p: Prefixed = "my_contract".parse().unwrap();
        assert_eq!(p.name(), "my_contract");
        assert_eq!(p.mod_name(), "my_contract");
        assert_eq!(p.canonical_name(), "my-contract");
    }

    #[test]
    fn rejects_empty() {
        assert!(matches!("".parse::<Prefixed>().unwrap_err(), Error::Empty));
    }

    #[test]
    fn rejects_leading_and_trailing_slash() {
        assert!(matches!(
            "/guess-the-number".parse::<Prefixed>().unwrap_err(),
            Error::LeadingOrTrailingSlash(_)
        ));
        assert!(matches!(
            "unverified/".parse::<Prefixed>().unwrap_err(),
            Error::LeadingOrTrailingSlash(_)
        ));
    }

    #[test]
    fn rejects_multiple_slashes() {
        assert!(matches!(
            "a/b/c".parse::<Prefixed>().unwrap_err(),
            Error::TooManySlashes(_)
        ));
        assert!(matches!(
            "a//b".parse::<Prefixed>().unwrap_err(),
            Error::TooManySlashes(_)
        ));
    }

    #[test]
    fn rejects_version_suffix() {
        assert!(matches!(
            "our_dao@1.0.0".parse::<Prefixed>().unwrap_err(),
            Error::UnexpectedVersion(_)
        ));
    }

    #[test]
    fn rejects_invalid_characters() {
        assert!(matches!(
            "hello world".parse::<Prefixed>().unwrap_err(),
            Error::InvalidCharacter(_, ' ')
        ));
        assert!(matches!(
            "name!".parse::<Prefixed>().unwrap_err(),
            Error::InvalidCharacter(_, '!')
        ));
    }
}

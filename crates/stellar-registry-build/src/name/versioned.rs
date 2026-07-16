use std::{fmt::Display, str::FromStr};

use super::{Error, Prefixed};

/// A [`Prefixed`] wasm name plus an optional `@version` suffix, e.g.
/// `registry@1.0.0` or `unverified/guess-the-number@v0.4.0` (leading `v`
/// tolerated). Only published wasms have versions; without a suffix the
/// registry serves the latest published version.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Versioned {
    name: Prefixed,
    version: Option<semver::Version>,
}

impl FromStr for Versioned {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.split_once('@') {
            Some((name, version_raw)) => {
                let version = version_raw
                    .strip_prefix('v')
                    .unwrap_or(version_raw)
                    .parse()
                    .map_err(|source| Error::InvalidVersion {
                        input: s.to_owned(),
                        version: version_raw.to_owned(),
                        source,
                    })?;
                Ok(Self {
                    name: name.parse()?,
                    version: Some(version),
                })
            }
            None => Ok(Self {
                name: s.parse()?,
                version: None,
            }),
        }
    }
}

impl Versioned {
    /// The channel-prefixed name, without the version.
    #[must_use]
    pub fn name(&self) -> &Prefixed {
        &self.name
    }

    /// The requested version, if one was given.
    #[must_use]
    pub fn version(&self) -> Option<&semver::Version> {
        self.version.as_ref()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_version() {
        let v: Versioned = "registry".parse().unwrap();
        assert_eq!(v.name().name(), "registry");
        assert_eq!(v.version(), None);
        assert_eq!(v.to_string(), "registry");
    }

    #[test]
    fn with_version() {
        let v: Versioned = "registry@1.0.1".parse().unwrap();
        assert_eq!(v.name().name(), "registry");
        assert_eq!(v.version().unwrap().to_string(), "1.0.1");
        assert_eq!(v.to_string(), "registry@1.0.1");
    }

    #[test]
    fn strips_leading_v() {
        let v: Versioned = "registry@v1.0.1".parse().unwrap();
        assert_eq!(v.version().unwrap().to_string(), "1.0.1");
    }

    #[test]
    fn channel_prefixed_with_version() {
        let v: Versioned = "unverified/guess-the-number@0.4.0".parse().unwrap();
        assert_eq!(v.name().channel(), Some("unverified"));
        assert_eq!(v.name().name(), "guess-the-number");
        assert_eq!(v.name().mod_name(), "guess_the_number");
        assert_eq!(v.version().unwrap().to_string(), "0.4.0");
    }

    #[test]
    fn prerelease_version() {
        let v: Versioned = "registry@1.0.0-rc.1".parse().unwrap();
        assert_eq!(v.version().unwrap().to_string(), "1.0.0-rc.1");
    }

    #[test]
    fn rejects_invalid_version_instead_of_dropping_it() {
        // A bad version must be an error, not silently "no version requested".
        assert!(matches!(
            "foo@garbage".parse::<Versioned>().unwrap_err(),
            Error::InvalidVersion { .. }
        ));
        assert!(matches!(
            "foo@".parse::<Versioned>().unwrap_err(),
            Error::InvalidVersion { .. }
        ));
        assert!(matches!(
            "a@1.0.0@2.0.0".parse::<Versioned>().unwrap_err(),
            Error::InvalidVersion { .. }
        ));
    }

    #[test]
    fn rejects_bad_name_with_version() {
        assert!(matches!(
            "a/b/c@1.0.0".parse::<Versioned>().unwrap_err(),
            Error::TooManySlashes(_)
        ));
    }
}

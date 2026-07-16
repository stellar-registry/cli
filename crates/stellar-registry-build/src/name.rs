//! Typed, validated registry names.
//!
//! Parsing is the only way to construct these types, so holding one is proof
//! the name is structurally valid ("parse, don't validate"):
//!
//! - [`Prefixed`] — `name` or `channel/name`, no version.
//! - [`Versioned`] — a [`Prefixed`] plus an optional `@version` suffix.

pub mod prefixed;
pub mod versioned;

pub use prefixed::Prefixed;
pub use versioned::Versioned;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("registry name cannot be empty")]
    Empty,
    #[error("registry name `{0}` cannot start or end with `/`")]
    LeadingOrTrailingSlash(String),
    #[error("registry name `{0}` has more than one `/`; expected `name` or `channel/name`")]
    TooManySlashes(String),
    #[error(
        "unexpected `@` in `{0}`: a version is not allowed in this name (wasm versions are passed separately, e.g. `--version 1.0.0`; deployed contracts have no version)"
    )]
    UnexpectedVersion(String),
    #[error(
        "invalid character `{1}` in registry name `{0}`; expected ASCII letters, digits, `-` or `_`"
    )]
    InvalidCharacter(String, char),
    #[error("invalid version `{version}` in `{input}`: {source}")]
    InvalidVersion {
        input: String,
        version: String,
        source: semver::Error,
    },
}

/// Canonical on-chain form of a registry name: lowercase with `_` → `-`.
/// The registry contract stores names in this form.
#[must_use]
pub fn canonicalize(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c == '_' {
                '-'
            } else {
                c.to_ascii_lowercase()
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::canonicalize;

    #[test]
    fn canonicalize_lowercases_and_hyphenates() {
        assert_eq!(canonicalize("Guess_The_Number"), "guess-the-number");
        assert_eq!(canonicalize("registry"), "registry");
    }
}

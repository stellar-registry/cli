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

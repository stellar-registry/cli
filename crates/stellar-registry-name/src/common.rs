/// Canonical on-chain form of a registry name: lowercase with `_` → `-`.
/// The registry contract stores names in this form.
#[must_use]
pub fn canonicalize(name: &str) -> String {
    name.replace('_', "-").to_ascii_lowercase()
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

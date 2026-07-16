use std::path::PathBuf;

/// Path to the compiling crate's `Cargo.toml`.
fn manifest() -> syn::Result<PathBuf> {
    Ok(PathBuf::from(
        std::env::var("CARGO_MANIFEST_DIR")
            .map_err(|_| "failed to find cargo manifest".into())?)
            .join("Cargo.toml"),
    )
}

pub(crate) trait ProcMacroWrapper {
    fn to_token_stream(&self) -> proc_macro::TokenStream;
}

impl ProcMacroWrapper for syn::Result<proc_macro2::TokenStream> {
    fn to_token_stream(&self) -> proc_macro::TokenStream {
        self.clone()
            .map_or_else(|e| e.to_compile_error().into(), |inner| inner.into())
    }
}

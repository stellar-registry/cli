extern crate proc_macro;

pub trait ProcMacroWrapper {
    fn to_token_stream(&self) -> proc_macro::TokenStream;
}

impl ProcMacroWrapper for syn::Result<proc_macro2::TokenStream> {
    fn to_token_stream(&self) -> proc_macro::TokenStream {
        self.clone()
            .map_or_else(|e| e.to_compile_error().into(), |inner| inner.into())
    }
}

pub use stellar_registry_name::*;

#[cfg(feature = "cli")]
mod cli {
    use stellar_cli::config;

    use super::Prefixed;
    use crate::registry::Registry;

    #[allow(async_fn_in_trait)]
    pub trait RegistryAccess {
        /// Resolve the (sub)registry this name's channel points at.
        async fn registry(&self, config: &config::Args) -> Result<Registry, crate::Error>;
    }

    impl RegistryAccess for Prefixed {
        async fn registry(&self, config: &config::Args) -> Result<Registry, crate::Error> {
            Registry::from_named_registry(config, self).await
        }
    }
}

#[cfg(feature = "cli")]
pub use cli::RegistryAccess;

use async_trait::async_trait;
use thiserror::Error;

#[derive(Error, Debug, PartialEq, Clone)]
pub enum ComponentError {
    #[error("Error in the component configuration.")]
    ComponentConfigError,
    #[error("An internal component error.")]
    InternalComponentError,
}

/// Interface to start components.
#[async_trait]
pub trait ComponentStarter {
    /// Start the component. By default do nothing.
    async fn start(&mut self) -> Result<(), ComponentError> {
        Ok(())
    }
}

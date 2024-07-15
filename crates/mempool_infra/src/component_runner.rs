use async_trait::async_trait;

#[derive(thiserror::Error, Debug, PartialEq, Clone)]
pub enum ComponentStartError {
    #[error("Error in the component configuration.")]
    ComponentConfigError,
    #[error("An internal component error.")]
    InternalComponentError,
}

/// Interface to start components.
#[async_trait]
pub trait ComponentStarter {
    /// Start the component. By default do nothing.
    async fn start(&mut self) -> Result<(), ComponentStartError> {
        Ok(())
    }
}

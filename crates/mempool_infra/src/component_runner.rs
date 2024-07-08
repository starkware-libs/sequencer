use async_trait::async_trait;

#[derive(thiserror::Error, Debug, PartialEq)]
pub enum ComponentStartError {
    #[error("Error in the component configuration.")]
    ComponentConfigError,
    #[error("An internal component error.")]
    InternalComponentError,
}

/// Interface to start components.
#[async_trait]
pub trait ComponentRunner {
    /// Start the component. Normally this function should never return.
    async fn start(&mut self) -> Result<(), ComponentStartError>;
}

use async_trait::async_trait;

use crate::errors::ComponentError;

/// Interface to start components.
#[async_trait]
pub trait ComponentStarter {
    /// Start the component. By default do nothing.
    async fn start(&mut self) -> Result<(), ComponentError> {
        Ok(())
    }
}

use std::any::type_name;

use async_trait::async_trait;
use tracing::info;

use crate::errors::ComponentError;

#[async_trait]
pub trait Startable<StartError> {
    async fn start(&mut self) -> Result<(), StartError>;
}

pub trait DefaultComponentStarter {}

#[async_trait]
impl<T: Send + Sync> Startable<ComponentError> for T
where
    T: DefaultComponentStarter,
{
    async fn start(&mut self) -> Result<(), ComponentError> {
        info!("Starting component {}.", type_name::<T>());
        Ok(())
    }
}

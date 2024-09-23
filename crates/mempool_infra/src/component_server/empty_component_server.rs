use async_trait::async_trait;

use super::definitions::ComponentServerStarter;
use crate::errors::{ComponentError, ComponentServerError};
use crate::starters::Startable;

pub struct EmptyServer<T> {
    component: T,
}

impl<T: Send + Sync> EmptyServer<T> {
    pub fn new(component: T) -> Self {
        Self { component }
    }
}

#[async_trait]
impl<T: Startable<ComponentError> + Send + Sync> ComponentServerStarter for EmptyServer<T> {
    async fn start(&mut self) -> Result<(), ComponentServerError> {
        self.component.start().await.map_err(ComponentServerError::ComponentError)
    }
}

pub fn create_empty_server<T: Send + Sync>(component: T) -> EmptyServer<T> {
    EmptyServer::new(component)
}

use std::any::type_name;

use async_trait::async_trait;
use tracing::info;

use crate::component_definitions::ComponentStarter;
use crate::component_server::{ComponentReplacer, ComponentServerStarter, ReplaceComponentError};
use crate::errors::ComponentServerError;

pub struct EmptyServer<Component> {
    component: Component,
}

impl<Component: Send + Sync> EmptyServer<Component> {
    pub fn new(component: Component) -> Self {
        Self { component }
    }
}

#[async_trait]
impl<Component: ComponentStarter + Send + Sync> ComponentServerStarter for EmptyServer<Component> {
    async fn start(&mut self) -> Result<(), ComponentServerError> {
        info!("Starting empty component server for {}.", type_name::<Component>());
        self.component.start().await.map_err(ComponentServerError::ComponentError)
    }
}

pub fn create_empty_server<Component: Send + Sync>(component: Component) -> EmptyServer<Component> {
    EmptyServer::new(component)
}

impl<Component> ComponentReplacer<Component> for EmptyServer<Component> {
    fn replace(&mut self, component: Component) -> Result<(), ReplaceComponentError> {
        self.component = component;
        Ok(())
    }
}

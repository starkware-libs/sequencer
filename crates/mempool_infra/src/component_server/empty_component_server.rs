use std::any::type_name;

use async_trait::async_trait;
use tracing::info;

use crate::component_definitions::ComponentStarter;
use crate::component_server::{ComponentReplacer, ComponentServerStarter, ReplaceComponentError};
use crate::errors::ComponentServerError;

pub struct WrapperServer<Component> {
    component: Component,
}

impl<Component: Send + Sync> WrapperServer<Component> {
    pub fn new(component: Component) -> Self {
        Self { component }
    }
}

#[async_trait]
impl<Component: ComponentStarter + Send + Sync> ComponentServerStarter
    for WrapperServer<Component>
{
    async fn start(&mut self) -> Result<(), ComponentServerError> {
        info!("Starting WrapperServer for {}.", type_name::<Component>());
        let res = self.component.start().await.map_err(ComponentServerError::ComponentError);
        info!("Finished running WrapperServer for {}.", type_name::<Component>());
        res
    }
}

pub fn create_empty_server<Component: Send + Sync>(
    component: Component,
) -> WrapperServer<Component> {
    WrapperServer::new(component)
}

impl<Component> ComponentReplacer<Component> for WrapperServer<Component> {
    fn replace(&mut self, component: Component) -> Result<(), ReplaceComponentError> {
        self.component = component;
        Ok(())
    }
}

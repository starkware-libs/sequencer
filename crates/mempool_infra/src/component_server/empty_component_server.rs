use std::any::type_name;

use async_trait::async_trait;
use tracing::info;

use crate::component_definitions::ComponentStarter;
use crate::component_server::{ComponentReplacer, ComponentServerStarter};
use crate::errors::{ComponentServerError, ReplaceComponentError};

pub struct WrapperServer<Component> {
    component: Component,
}

impl<Component> WrapperServer<Component> {
    pub fn new(component: Component) -> Self {
        Self { component }
    }
}

#[async_trait]
impl<Component: ComponentStarter + Send> ComponentServerStarter for WrapperServer<Component> {
    async fn start(&mut self) -> Result<(), ComponentServerError> {
        info!("Starting WrapperServer for {}.", type_name::<Component>());
        let res = self.component.start().await.map_err(ComponentServerError::ComponentError);
        info!("Finished running WrapperServer for {}.", type_name::<Component>());
        res
    }
}

impl<Component> ComponentReplacer<Component> for WrapperServer<Component> {
    fn replace(&mut self, component: Component) -> Result<(), ReplaceComponentError> {
        self.component = component;
        Ok(())
    }
}

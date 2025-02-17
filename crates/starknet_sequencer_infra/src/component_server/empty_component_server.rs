use async_trait::async_trait;
use starknet_infra_utils::type_name::short_type_name;
use tracing::info;

use crate::component_definitions::ComponentStarter;
use crate::component_server::{ComponentReplacer, ComponentServerStarter};
use crate::errors::ReplaceComponentError;

pub struct WrapperServer<Component> {
    component: Component,
}

impl<Component: Send> WrapperServer<Component> {
    pub fn new(component: Component) -> Self {
        Self { component }
    }
}

#[async_trait]
impl<Component: ComponentStarter + Send> ComponentServerStarter for WrapperServer<Component> {
    async fn start(&mut self) {
        info!("Starting WrapperServer for {}.", short_type_name::<Component>());
        self.component.start().await.unwrap_or_else(|_| {
            panic!("WrapperServer stopped for {}", short_type_name::<Component>())
        });
    }
}

impl<Component> ComponentReplacer<Component> for WrapperServer<Component> {
    fn replace(&mut self, component: Component) -> Result<(), ReplaceComponentError> {
        self.component = component;
        Ok(())
    }
}

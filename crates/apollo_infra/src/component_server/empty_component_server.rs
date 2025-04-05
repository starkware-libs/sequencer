use apollo_infra_utils::type_name::short_type_name;
use async_trait::async_trait;
use tracing::info;

use crate::component_definitions::ComponentStarter;
use crate::component_server::ComponentServerStarter;

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
        self.component.start().await;
        panic!("WrapperServer stopped for {}", short_type_name::<Component>())
    }
}

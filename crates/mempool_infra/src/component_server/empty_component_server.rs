use async_trait::async_trait;

use super::definitions::{start_component, ComponentServerStarter};
use crate::component_runner::ComponentStarter;

pub struct EmptyServer<T: ComponentStarter + Send + Sync> {
    component: T,
}

impl<T: ComponentStarter + Send + Sync> EmptyServer<T> {
    pub fn new(component: T) -> Self {
        Self { component }
    }
}

#[async_trait]
impl<T: ComponentStarter + Send + Sync> ComponentServerStarter for EmptyServer<T> {
    async fn start(&mut self) {
        start_component(&mut self.component).await;
    }
}

pub fn create_empty_server<T: ComponentStarter + Send + Sync>(component: T) -> EmptyServer<T> {
    EmptyServer::new(component)
}

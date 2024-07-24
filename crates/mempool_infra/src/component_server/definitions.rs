use async_trait::async_trait;
use tracing::{error, info};

use crate::component_runner::ComponentStarter;

#[async_trait]
pub trait ComponentServerStarter: Send + Sync {
    async fn start(&mut self);
}

pub async fn start_component<Component>(component: &mut Component) -> bool
where
    Component: ComponentStarter + Sync + Send,
{
    if let Err(err) = component.start().await {
        error!("ComponentServer::start() failed: {:?}", err);
        return false;
    }

    info!("ComponentServer::start() completed.");
    true
}

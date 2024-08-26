use async_trait::async_trait;
use tokio::sync::mpsc::Receiver;
use tracing::{error, info};

use crate::component_definitions::{
    ComponentMonitor,
    ComponentRequestAndResponseSender,
    ComponentRequestHandler,
    RequestAndMonitor,
    ResponseAndMonitor,
};
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

pub async fn request_response_loop<Request, Response, Component>(
    rx: &mut Receiver<ComponentRequestAndResponseSender<Request, Response>>,
    component: &mut Component,
) where
    Component: ComponentRequestHandler<Request, Response> + ComponentMonitor + Send + Sync,
    Request: Send + Sync,
    Response: Send + Sync,
{
    while let Some(request_and_res_tx) = rx.recv().await {
        let request = request_and_res_tx.request;
        let tx = request_and_res_tx.tx;

        let res = match request {
            RequestAndMonitor::Original(request) => {
                ResponseAndMonitor::Original(component.handle_request(request).await)
            }
            RequestAndMonitor::IsAlive => ResponseAndMonitor::IsAlive(component.is_alive().await),
        };

        tx.send(res).await.expect("Response connection should be open.");
    }
}

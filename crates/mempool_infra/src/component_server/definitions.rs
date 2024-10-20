use std::any::type_name;

use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::mpsc::Receiver;
use tracing::info;

use crate::component_definitions::{ComponentRequestAndResponseSender, ComponentRequestHandler};
use crate::errors::ComponentServerError;

#[async_trait]
pub trait ComponentServerStarter {
    async fn start(&mut self) -> Result<(), ComponentServerError>;
}

pub async fn request_response_loop<Request, Response, Component>(
    rx: &mut Receiver<ComponentRequestAndResponseSender<Request, Response>>,
    component: &mut Component,
) where
    Component: ComponentRequestHandler<Request, Response> + Send + Sync,
    Request: Send + Sync,
    Response: Send + Sync,
{
    info!("Starting server loop for component {}", type_name::<Component>());

    // TODO(Tsabary): Move this function to be part of the `local_server` module.
    while let Some(request_and_res_tx) = rx.recv().await {
        info!("Component {} received request", type_name::<Component>());

        let request = request_and_res_tx.request;
        let tx = request_and_res_tx.tx;

        let res = component.handle_request(request).await;

        // TODO(Tsabary): revert `try_send` to `send` once the client is guaranteed to be alive,
        // i.e., tx.send(res).await.expect("Response connection should be open.");
        // Tries sending the response to the client. If the client has disconnected then this
        // becomes a null operation.
        let _ = tx.try_send(res);
    }

    info!("Finished server loop for component {}", type_name::<Component>());
}

// TODO(Tsabary): Create an error module and move this error there.
#[derive(Clone, Debug, Error)]
pub enum ReplaceComponentError {
    #[error("Internal error.")]
    InternalError,
}

pub trait ComponentReplacer<Component> {
    fn replace(&mut self, component: Component) -> Result<(), ReplaceComponentError>;
}

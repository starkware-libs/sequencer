use std::any::type_name;
use std::fmt::Debug;

use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::mpsc::Receiver;
use tracing::{debug, info};

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
    Request: Send + Sync + Debug,
    Response: Send + Sync + Debug,
{
    info!("Starting server for component {}", type_name::<Component>());

    // TODO(Tsabary): Move this function to be part of the `local_server` module.
    while let Some(request_and_res_tx) = rx.recv().await {
        let request = request_and_res_tx.request;
        let tx = request_and_res_tx.tx;
        debug!("Component {} received request {:?}", type_name::<Component>(), request);

        let response = component.handle_request(request).await;
        debug!("Component {} is sending response {:?}", type_name::<Component>(), response);

        // TODO(Tsabary): revert `try_send` to `send` once the client is guaranteed to be alive,
        // i.e., tx.send(response).await.expect("Response connection should be open.");
        // Tries sending the response to the client. If the client has disconnected then this
        // becomes a null operation.
        // let _ = tx.try_send(response);
        tx.send(response).await.expect("Response connection should be open.");
    }

    info!("Stopping server for component {}", type_name::<Component>());
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

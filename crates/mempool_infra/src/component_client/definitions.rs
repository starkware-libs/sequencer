use std::sync::Arc;

use hyper::StatusCode;
use thiserror::Error;
use tokio::sync::mpsc::{channel, Sender};

use crate::component_definitions::{ComponentRequestAndResponseSender, ServerError};

#[derive(Clone, Debug, Error)]
pub enum ClientError {
    #[error("Communication error: {0}")]
    CommunicationFailure(Arc<hyper::Error>),
    #[error("Could not deserialize server response: {0}")]
    ResponseDeserializationFailure(Arc<bincode::Error>),
    #[error("Could not parse the response: {0}")]
    ResponseParsingFailure(Arc<hyper::Error>),
    #[error("Got status code: {0}, with server error: {1}")]
    ResponseError(StatusCode, ServerError),
    #[error("Got an unexpected response type: {0}")]
    UnexpectedResponse(String),
}

pub type ClientResult<T> = Result<T, ClientError>;

pub async fn send_locally<Request, Response>(
    tx: Sender<ComponentRequestAndResponseSender<Request, Response>>,
    request: Request,
) -> Response
where
    Request: Send + Sync,
    Response: Send + Sync,
{
    let (res_tx, mut res_rx) = channel::<Response>(1);
    let request_and_res_tx = ComponentRequestAndResponseSender { request, tx: res_tx };
    tx.send(request_and_res_tx).await.expect("Outbound connection should be open.");
    res_rx.recv().await.expect("Inbound connection should be open.")
}

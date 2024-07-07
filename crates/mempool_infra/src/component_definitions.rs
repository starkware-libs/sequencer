use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::mpsc::Sender;

#[async_trait]
pub trait ComponentRequestHandler<Request, Response> {
    async fn handle_request(&mut self, request: Request) -> Response;
}

pub struct ComponentRequestAndResponseSender<Request, Response>
where
    Request: Send + Sync,
    Response: Send + Sync,
{
    pub request: Request,
    pub tx: Sender<Response>,
}

pub const APPLICATION_OCTET_STREAM: &str = "application/octet-stream";

#[derive(Debug, Error, Deserialize, Serialize)]
pub enum ServerError {
    #[error("Could not deserialize client request: {0}")]
    RequestDeserializationFailure(String),
}

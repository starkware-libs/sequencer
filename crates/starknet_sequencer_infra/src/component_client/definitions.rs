use hyper::StatusCode;
use serde::de::DeserializeOwned;
use serde::Serialize;
use thiserror::Error;

use super::{LocalComponentClient, RemoteComponentClient};
use crate::component_definitions::ServerError;

#[derive(Clone, Debug, Error)]
pub enum ClientError {
    #[error("Communication error: {0}")]
    CommunicationFailure(String),
    #[error("Could not deserialize server response: {0}")]
    ResponseDeserializationFailure(String),
    #[error("Could not parse the response: {0}")]
    ResponseParsingFailure(String),
    #[error("Got status code: {0}, with server error: {1}")]
    ResponseError(StatusCode, ServerError),
    #[error("Got an unexpected response type: {0}")]
    UnexpectedResponse(String),
}

pub type ClientResult<T> = Result<T, ClientError>;

pub struct Client<Request, Response>
where
    Request: Send + Serialize,
    Response: Send + DeserializeOwned,
{
    local_client: Option<LocalComponentClient<Request, Response>>,
    remote_client: Option<RemoteComponentClient<Request, Response>>,
}

impl<Request, Response> Client<Request, Response>
where
    Request: Send + Serialize,
    Response: Send + DeserializeOwned,
{
    pub fn new(
        local_client: Option<LocalComponentClient<Request, Response>>,
        remote_client: Option<RemoteComponentClient<Request, Response>>,
    ) -> Self {
        if local_client.is_some() && remote_client.is_some() {
            panic!("Cannot have both local_client and remote_client simultaneously.");
        }
        Self { local_client, remote_client }
    }

    pub fn get_local_client(&self) -> Option<LocalComponentClient<Request, Response>> {
        self.local_client.clone()
    }

    pub fn get_remote_client(&self) -> Option<RemoteComponentClient<Request, Response>> {
        self.remote_client.clone()
    }
}

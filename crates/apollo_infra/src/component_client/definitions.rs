// TODO(victork): finalise migration to hyper 1.x
use http_1::StatusCode;
use serde::de::DeserializeOwned;
use serde::Serialize;
use thiserror::Error;

use super::{LocalComponentClient, NoopComponentClient, RemoteComponentClient};
use crate::component_definitions::ServerError;

#[derive(Clone, Debug, Error, PartialEq, Eq)]
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
    #[error("Noop client error")]
    Noop,
}

pub type ClientResult<T> = Result<T, ClientError>;

pub struct Client<Request, Response>
where
    Request: Send + Serialize,
    Response: Send + DeserializeOwned,
{
    local_client: Option<LocalComponentClient<Request, Response>>,
    remote_client: Option<RemoteComponentClient<Request, Response>>,
    noop_client: Option<NoopComponentClient<Request, Response>>,
}

impl<Request, Response> Client<Request, Response>
where
    Request: Send + Serialize,
    Response: Send + DeserializeOwned,
{
    pub fn new(
        local_client: Option<LocalComponentClient<Request, Response>>,
        remote_client: Option<RemoteComponentClient<Request, Response>>,
        noop_client: Option<NoopComponentClient<Request, Response>>,
    ) -> Self {
        let client_count = [local_client.is_some(), remote_client.is_some(), noop_client.is_some()]
            .iter()
            .filter(|&&is_present| is_present)
            .count();
        assert!(
            client_count <= 1,
            "Cannot have multiple client types (local, remote, noop) simultaneously."
        );
        Self { local_client, remote_client, noop_client }
    }

    pub fn get_local_client(&self) -> Option<LocalComponentClient<Request, Response>> {
        self.local_client.clone()
    }

    pub fn get_remote_client(&self) -> Option<RemoteComponentClient<Request, Response>> {
        self.remote_client.clone()
    }

    pub fn get_noop_client(&self) -> Option<NoopComponentClient<Request, Response>> {
        self.noop_client.clone()
    }
}

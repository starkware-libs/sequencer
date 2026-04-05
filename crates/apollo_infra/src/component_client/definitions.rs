use http::StatusCode;
use serde::de::DeserializeOwned;
use serde::Serialize;
use thiserror::Error;

use super::{LocalComponentClient, RemoteComponentClient};
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
}

pub type ClientResult<T> = Result<T, ClientError>;

pub enum Client<Request, Response>
where
    Request: Send + Serialize,
    Response: Send + DeserializeOwned,
{
    Local(LocalComponentClient<Request, Response>),
    Remote(RemoteComponentClient<Request, Response>),
    Disabled,
}

impl<Request, Response> Client<Request, Response>
where
    Request: Send + Serialize,
    Response: Send + DeserializeOwned,
{
    pub fn get_local_client(&self) -> LocalComponentClient<Request, Response> {
        match self {
            Client::Local(client) => client.clone(),
            Client::Remote(_) | Client::Disabled => {
                panic!("Expected a local client, but got a remote or disabled client.")
            }
        }
    }
}

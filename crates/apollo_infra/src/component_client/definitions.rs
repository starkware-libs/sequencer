use http::StatusCode;
use serde::de::DeserializeOwned;
use serde::Serialize;
use thiserror::Error;

use super::{LocalComponentClient, RemoteComponentClient};
use crate::component_client::LocalComponentReaderClient;
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

/// A client that communicates via request/response (local mpsc or remote HTTP). The standard
/// client type for components that support both local and remote deployment.
pub type RpcClient<Request, Response> = Client<Request, Response, ()>;

/// A client that reads the latest value from a watch channel. Used for components that only
/// support local read-only access with no remote deployment option.
pub type ReaderClient<T> = Client<(), (), T>;

pub enum Client<Request, Response, T>
where
    Request: Send + Serialize,
    Response: Send + DeserializeOwned,
    T: Send + Sync + Clone,
{
    Local(LocalComponentClient<Request, Response>),
    LocalReadOnlyClient(LocalComponentReaderClient<T>),
    Remote(RemoteComponentClient<Request, Response>),
    Disabled,
}

impl<Request, Response, T> Client<Request, Response, T>
where
    Request: Send + Serialize,
    Response: Send + DeserializeOwned,
    T: Send + Sync + Clone,
{
    pub fn get_local_client(&self) -> LocalComponentClient<Request, Response> {
        match self {
            Client::Local(client) => client.clone(),
            Client::LocalReadOnlyClient(_) | Client::Remote(_) | Client::Disabled => {
                panic!("Local client should be set for inbound remote connections.");
            }
        }
    }

    pub fn get_local_read_only_client(&self) -> LocalComponentReaderClient<T> {
        match self {
            Client::LocalReadOnlyClient(client) => client.clone(),
            Client::Local(_) | Client::Remote(_) | Client::Disabled => {
                panic!("Expected LocalReadOnlyClient, got a different client variant.");
            }
        }
    }
}

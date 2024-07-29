use std::sync::Arc;

use hyper::StatusCode;
use thiserror::Error;

use crate::component_definitions::ServerError;

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

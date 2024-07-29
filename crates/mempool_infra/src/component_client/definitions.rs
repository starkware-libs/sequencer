use bincode::ErrorKind;
use hyper::{Error as HyperError, StatusCode};
use thiserror::Error;

use crate::component_definitions::ServerError;

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("Communication error: {0}")]
    CommunicationFailure(HyperError),
    #[error("Could not deserialize server response: {0}")]
    ResponseDeserializationFailure(Box<ErrorKind>),
    #[error("Could not parse the response: {0}")]
    ResponseParsingFailure(HyperError),
    #[error("Got status code: {0}, with server error: {1}")]
    ResponseError(StatusCode, ServerError),
    #[error("Got an unexpected response type: {0}")]
    UnexpectedResponse(String),
}

pub type ClientResult<T> = Result<T, ClientError>;

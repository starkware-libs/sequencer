use apollo_gateway_types::communication::GatewayClientError;
use apollo_gateway_types::errors::GatewayError;
use axum::response::{IntoResponse, Response};
use hyper::StatusCode;
use jsonrpsee::types::error::ErrorCode;
use starknet_api::compression_utils::CompressionError;
use thiserror::Error;
use tracing::{debug, error};

/// Errors originating from `[`HttpServer::run`]` command.
#[derive(Debug, Error)]
pub enum HttpServerRunError {
    #[error(transparent)]
    ServerStartupError(#[from] hyper::Error),
}

/// Errors that may occur during the runtime of the HTTP server.
#[derive(Error, Debug)]
pub enum HttpServerError {
    #[error(transparent)]
    GatewayClientError(#[from] GatewayClientError),
    #[error(transparent)]
    DeserializationError(#[from] serde_json::Error),
    #[error(transparent)]
    DecompressionError(#[from] CompressionError),
}

impl IntoResponse for HttpServerError {
    fn into_response(self) -> Response {
        match self {
            HttpServerError::GatewayClientError(e) => gw_client_err_into_response(e),
            HttpServerError::DeserializationError(e) => serde_error_into_response(e),
            HttpServerError::DecompressionError(e) => compression_error_into_response(e),
        }
    }
}

fn compression_error_into_response(err: CompressionError) -> Response {
    debug!("Failed to decompress the transaction: {}", err);
    let parse_error = jsonrpsee::types::ErrorObject::owned(
        ErrorCode::InvalidParams.code(),
        "Failed to decompress the provided Sierra program.",
        None::<()>,
    );
    serialize_error(parse_error)
}

fn serde_error_into_response(err: serde_json::Error) -> Response {
    debug!("Failed to deserialize transaction: {}", err);
    let parse_error = jsonrpsee::types::ErrorObject::owned(
        ErrorCode::ParseError.code(),
        "Failed to parse the request body.",
        None::<()>,
    );
    serialize_error(parse_error)
}

fn gw_client_err_into_response(err: GatewayClientError) -> Response {
    let (response_code, general_rpc_error) = match err {
        GatewayClientError::ClientError(e) => {
            error!("Encountered a ClientError: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                jsonrpsee::types::ErrorObject::owned(
                    ErrorCode::InternalError.code(),
                    "Internal error",
                    None::<()>,
                ),
            )
        }
        GatewayClientError::GatewayError(GatewayError::GatewaySpecError {
            source,
            p2p_message_metadata: _,
        }) => {
            // TODO(yair): Find out what is the p2p_message_metadata and whether it needs to be
            // added to the error response.
            (StatusCode::BAD_REQUEST, {
                let rpc_spec_error = source.into_rpc();
                jsonrpsee::types::ErrorObject::owned(
                    ErrorCode::ServerError(rpc_spec_error.code).code(),
                    rpc_spec_error.message,
                    rpc_spec_error.data,
                )
            })
        }
    };

    let response_body = serialize_error(general_rpc_error);

    (response_code, response_body).into_response()
}

fn serialize_error<T: serde::Serialize>(body: T) -> Response {
    serde_json::to_vec(&body).expect("Expecting a serializable error.").into_response()
}

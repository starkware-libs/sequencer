use apollo_gateway_types::communication::GatewayClientError;
use apollo_gateway_types::errors::GatewayError;
use axum::response::{IntoResponse, Response};
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
    serde_json::to_vec(&parse_error).expect("Expecting a serializable error.").into_response()
}

fn serde_error_into_response(err: serde_json::Error) -> Response {
    debug!("Failed to deserialize transaction: {}", err);
    let parse_error = jsonrpsee::types::ErrorObject::owned(
        ErrorCode::ParseError.code(),
        "Failed to parse the request body.",
        None::<()>,
    );
    serde_json::to_vec(&parse_error).expect("Expecting a serializable error.").into_response()
}

fn gw_client_err_into_response(err: GatewayClientError) -> Response {
    let general_rpc_error = match err {
        GatewayClientError::ClientError(e) => {
            error!("Encountered a ClientError: {}", e);
            jsonrpsee::types::ErrorObject::owned(
                ErrorCode::InternalError.code(),
                "Internal error",
                None::<()>,
            )
        }
        GatewayClientError::GatewayError(GatewayError::DeprecatedGWError {
            source,
            p2p_message_metadata: _,
        }) => {
            // TODO(yair): Find out what is the p2p_message_metadata and whether it needs to be
            // added to the error response.
            // TODO(yair,noamsp): Find out what to return here.
            jsonrpsee::types::ErrorObject::owned(400, source.message, None::<()>)
        }
    };

    serde_json::to_vec(&general_rpc_error).expect("Expecting a serializable error.").into_response()
}

use apollo_gateway_types::communication::GatewayClientError;
use apollo_gateway_types::deprecated_gateway_error::{
    KnownStarknetErrorCode,
    StarknetError,
    StarknetErrorCode,
};
use apollo_gateway_types::errors::GatewayError;
use axum::response::{IntoResponse, Response};
use hyper::StatusCode;
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
    let (response_code, deprecated_gateway_error) = (
        StatusCode::BAD_REQUEST,
        StarknetError {
            code: StarknetErrorCode::UnknownErrorCode(
                "StarknetErrorCode.INVALID_PROGRAM".to_string(),
            ),
            message: "Invalid compressed program.".to_string(),
        },
    );
    let response_body = serialize_error(&deprecated_gateway_error);
    (response_code, response_body).into_response()
}

fn serde_error_into_response(err: serde_json::Error) -> Response {
    debug!("Failed to deserialize transaction: {}", err);
    let (response_code, deprecated_gateway_error) = (
        StatusCode::BAD_REQUEST,
        StarknetError {
            code: StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::MalformedRequest),
            message: err.to_string(),
        },
    );
    let response_body = serialize_error(&deprecated_gateway_error);
    (response_code, response_body).into_response()
}

fn gw_client_err_into_response(err: GatewayClientError) -> Response {
    let (response_code, deprecated_gateway_error) = match err {
        GatewayClientError::ClientError(e) => {
            error!("Encountered a ClientError: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, StarknetError::internal("Internal error"))
        }
        GatewayClientError::GatewayError(GatewayError::DeprecatedGatewayError {
            source,
            p2p_message_metadata: _,
        }) => {
            // TODO(yair): Find out what is the p2p_message_metadata and whether it needs to be
            // added to the error response.
            (StatusCode::BAD_REQUEST, source)
        }
    };

    let response_body = serialize_error(&deprecated_gateway_error);

    (response_code, response_body).into_response()
}

fn serialize_error<T: serde::Serialize>(body: &T) -> Response {
    serde_json::to_vec(body).expect("Expecting a serializable error.").into_response()
}

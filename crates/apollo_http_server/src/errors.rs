use apollo_gateway_types::communication::GatewayClientError;
use apollo_gateway_types::errors::GatewayError;
use axum::response::{IntoResponse, Response};
use hyper::StatusCode;
use jsonrpsee::types::error::ErrorCode;
use thiserror::Error;
use tracing::error;

/// Errors originating from `[`HttpServer::run`]` command.
#[derive(Debug, Error)]
pub enum HttpServerRunError {
    #[error(transparent)]
    ServerStartupError(#[from] hyper::Error),
}

/// Errors that may occure during the runtime of the HTTP server.
#[derive(Error, Debug)]
pub enum HttpServerError {
    #[error(transparent)]
    GatewayClientError(#[from] GatewayClientError),
}

impl IntoResponse for HttpServerError {
    fn into_response(self) -> Response {
        match self {
            HttpServerError::GatewayClientError(e) => gw_client_err_into_response(e),
        }
    }
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
        GatewayClientError::GatewayError(GatewayError::GatewaySpecError {
            source,
            p2p_message_metadata: _,
        }) => {
            // TODO(yair): Find out what is the p2p_message_metadata and whether it needs to be
            // added to the error response.
            let rpc_spec_error = source.into_rpc();
            jsonrpsee::types::ErrorObject::owned(
                ErrorCode::ServerError(rpc_spec_error.code).code(),
                rpc_spec_error.message,
                rpc_spec_error.data,
            )
        }
    };

    let response_code = StatusCode::BAD_REQUEST;
    let response_body = serde_json::to_vec(&general_rpc_error)
        .expect("Expecting a serializable error.")
        .into_response();

    (response_code, response_body).into_response()
}

use axum::response::{IntoResponse, Response};
use jsonrpsee::types::error::ErrorCode;
use starknet_gateway_types::communication::GatewayClientError;
use starknet_gateway_types::errors::GatewayError;
use thiserror::Error;
use tracing::error;

/// Errors originating from `[`HttpServer::run`]` command.
#[derive(Debug, Error)]
pub enum HttpServerRunError {
    #[error(transparent)]
    ServerStartupError(#[from] hyper::Error),
}

/// Wraps the `GatewayClientError` in order to implement Axum's `IntoResponse` trait.
#[derive(Error, Debug)]
#[error(transparent)]
pub struct GatewayClientErrorWrapper(#[from] GatewayClientError);

impl IntoResponse for GatewayClientErrorWrapper {
    fn into_response(self) -> Response {
        let general_rpc_error = match self.0 {
            GatewayClientError::ClientError(e) => {
                error!("Got a gateway client: {}", e);
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
                let rpc_spec_error = source.into_rpc();
                jsonrpsee::types::ErrorObject::owned(
                    ErrorCode::ServerError(rpc_spec_error.code).code(),
                    rpc_spec_error.message,
                    rpc_spec_error.data,
                )
            }
        };

        serde_json::to_vec(&general_rpc_error)
            .expect("Expecting a serializable error.")
            .into_response()
    }
}

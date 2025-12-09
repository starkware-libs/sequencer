use std::collections::BTreeMap;
use std::io;
use std::marker::PhantomData;
use std::net::{Ipv4Addr, SocketAddr};

use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use async_trait::async_trait;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tracing::{error, info};

use crate::{StorageError, StorageReader};

#[cfg(test)]
#[path = "storage_reader_server_test.rs"]
mod storage_reader_server_test;

/// Configuration for the storage reader server.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ServerConfig {
    /// The socket address to bind the server to.
    pub socket: SocketAddr,
    /// Whether the server is enabled.
    pub enable: bool,
}

impl ServerConfig {
    /// Creates a new server configuration.
    pub fn new(socket: SocketAddr, enable: bool) -> Self {
        Self { socket, enable }
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self { socket: (Ipv4Addr::UNSPECIFIED, 8080).into(), enable: false }
    }
}

impl SerializeConfig for ServerConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "socket",
                &self.socket.to_string(),
                "The socket address for the storage reader HTTP server.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "enable",
                &self.enable,
                "Whether to enable the storage reader HTTP server.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

#[async_trait]
/// Handler trait for processing storage reader requests.
pub trait StorageReaderServerHandler<Request, Response> {
    /// Handles an incoming request and returns a response.
    async fn handle_request(
        storage_reader: &StorageReader,
        request: Request,
    ) -> Result<Response, StorageError>;
}

// TODO(Nadin): Remove #[allow(dead_code)] once the fields are used in the implementation.
#[allow(dead_code)]
/// A server for handling remote storage reader queries via a configurable request handler.
pub struct StorageReaderServer<RequestHandler, Request, Response>
where
    RequestHandler: StorageReaderServerHandler<Request, Response>,
    Request: Serialize + DeserializeOwned + Send + 'static,
    Response: Serialize + DeserializeOwned + Send + 'static,
{
    app_state: AppState<RequestHandler, Request, Response>,
    config: ServerConfig,
}

struct AppState<RequestHandler, Request, Response>
where
    RequestHandler: StorageReaderServerHandler<Request, Response>,
{
    storage_reader: StorageReader,
    _phantom: PhantomData<(RequestHandler, Request, Response)>,
}

impl<RequestHandler, Request, Response> Clone for AppState<RequestHandler, Request, Response>
where
    RequestHandler: StorageReaderServerHandler<Request, Response>,
{
    fn clone(&self) -> Self {
        Self { storage_reader: self.storage_reader.clone(), _phantom: PhantomData }
    }
}

impl<RequestHandler, Request, Response> StorageReaderServer<RequestHandler, Request, Response>
where
    RequestHandler: StorageReaderServerHandler<Request, Response>,
    Request: Serialize + DeserializeOwned + Send + 'static,
    Response: Serialize + DeserializeOwned + Send + 'static,
{
    /// Creates a new storage reader server with the given handler and configuration.
    pub fn new(storage_reader: StorageReader, config: ServerConfig) -> Self {
        let app_state = AppState { storage_reader, _phantom: PhantomData };
        Self { app_state, config }
    }

    /// Creates the axum router with configured routes and state.
    pub fn app(&self) -> Router
    where
        RequestHandler: Send + Sync + 'static,
        Request: Send + Sync + 'static,
        Response: Send + Sync + 'static,
    {
        Router::new()
            .route(
                "/storage/query",
                post(handle_request_endpoint::<RequestHandler, Request, Response>),
            )
            .with_state(self.app_state.clone())
    }

    /// Runs the server to handle incoming requests.
    pub async fn run(self) -> Result<(), StorageError>
    where
        RequestHandler: Send + Sync + 'static,
        Request: Send + Sync + 'static,
        Response: Send + Sync + 'static,
    {
        if !self.config.enable {
            info!("Storage reader server is disabled, not starting");
            return Ok(());
        }
        info!("Starting storage reader server on {}", self.config.socket);
        let app = self.app();
        info!("Storage reader server listening on {}", self.config.socket);

        // Start the server
        axum::Server::bind(&self.config.socket).serve(app.into_make_service()).await.map_err(|e| {
            error!("Storage reader server error: {}", e);
            StorageError::IOError(io::Error::other(e))
        })
    }
}

/// Axum handler for storage query requests.
async fn handle_request_endpoint<RequestHandler, Request, Response>(
    State(app_state): State<AppState<RequestHandler, Request, Response>>,
    Json(request): Json<Request>,
) -> Result<Json<Response>, StorageServerError>
where
    RequestHandler: StorageReaderServerHandler<Request, Response>,
    Request: Send + Sync + 'static,
    Response: Send + Sync + 'static,
{
    let response = RequestHandler::handle_request(&app_state.storage_reader, request).await?;

    Ok(Json(response))
}

/// Error type for HTTP responses.
#[derive(Debug)]
struct StorageServerError(StorageError);

impl From<StorageError> for StorageServerError {
    fn from(error: StorageError) -> Self {
        StorageServerError(error)
    }
}

impl IntoResponse for StorageServerError {
    fn into_response(self) -> Response {
        let error_message = format!("Storage error: {}", self.0);
        error!("{}", error_message);
        (StatusCode::INTERNAL_SERVER_ERROR, error_message).into_response()
    }
}

/// Creates and returns an optional StorageReaderServer based on the enable flag.
pub fn create_storage_reader_server<RequestHandler, Request, Response>(
    storage_reader: StorageReader,
    socket: SocketAddr,
    enable: bool,
) -> Option<StorageReaderServer<RequestHandler, Request, Response>>
where
    RequestHandler: StorageReaderServerHandler<Request, Response>,
    Request: Serialize + DeserializeOwned + Send + 'static,
    Response: Serialize + DeserializeOwned + Send + 'static,
{
    if enable {
        let config = ServerConfig::new(socket, enable);
        Some(StorageReaderServer::new(storage_reader, config))
    } else {
        None
    }
}

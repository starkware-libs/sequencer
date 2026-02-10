use std::collections::BTreeMap;
use std::io;
use std::marker::PhantomData;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use apollo_config::dumping::{prepend_sub_config_name, ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use async_trait::async_trait;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{serve, Json, Router};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tokio::task::AbortHandle;
use tokio::time;
use tracing::{error, info};
use validator::Validate;

use crate::{StorageError, StorageReader};

#[cfg(test)]
#[path = "storage_reader_server_test.rs"]
mod storage_reader_server_test;

/// Static configuration for the storage reader server.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Validate)]
pub struct StorageReaderServerStaticConfig {
    /// The socket address for the server.
    pub ip: IpAddr,
    /// The port for the server.
    pub port: u16,
}

impl Default for StorageReaderServerStaticConfig {
    fn default() -> Self {
        Self { ip: Ipv4Addr::UNSPECIFIED.into(), port: 8091 }
    }
}

impl SerializeConfig for StorageReaderServerStaticConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "ip",
                &self.ip.to_string(),
                "The IP address for the storage reader HTTP server.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "port",
                &self.port,
                "The port for the storage reader HTTP server.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

/// Dynamic configuration for the storage reader server.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Validate)]
pub struct StorageReaderServerDynamicConfig {
    /// Whether the server is enabled.
    pub enable: bool,
}

impl SerializeConfig for StorageReaderServerDynamicConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([ser_param(
            "enable",
            &self.enable,
            "Whether to enable the storage reader HTTP server.",
            ParamPrivacyInput::Public,
        )])
    }
}

/// Configuration for the storage reader server (wrapper of static + dynamic).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Validate)]
pub struct ServerConfig {
    /// Static configuration.
    #[validate(nested)]
    pub static_config: StorageReaderServerStaticConfig,
    /// Dynamic configuration.
    #[validate(nested)]
    pub dynamic_config: StorageReaderServerDynamicConfig,
}

impl ServerConfig {
    /// Creates a new server configuration.
    pub fn new(ip: IpAddr, port: u16, enable: bool) -> Self {
        Self {
            static_config: StorageReaderServerStaticConfig { ip, port },
            dynamic_config: StorageReaderServerDynamicConfig { enable },
        }
    }

    /// Returns the server IP.
    pub fn ip(&self) -> IpAddr {
        self.static_config.ip
    }

    /// Returns the server port.
    pub fn port(&self) -> u16 {
        self.static_config.port
    }

    /// Returns whether the server is enabled.
    pub fn is_enabled(&self) -> bool {
        self.dynamic_config.enable
    }
}

impl SerializeConfig for ServerConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut dump = prepend_sub_config_name(self.static_config.dump(), "static_config");
        dump.append(&mut prepend_sub_config_name(self.dynamic_config.dump(), "dynamic_config"));
        dump
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

/// A server for handling remote storage reader queries via a configurable request handler.
/// The server monitors dynamic configuration updates and can be started/stopped at runtime.
pub struct StorageReaderServer<RequestHandler, Request, Response>
where
    RequestHandler: StorageReaderServerHandler<Request, Response>,
    Request: Serialize + DeserializeOwned + Send + 'static,
    Response: Serialize + DeserializeOwned + Send + 'static,
{
    app_state: AppState<RequestHandler, Request, Response>,
    config: ServerConfig,
    /// Shared dynamic configuration that can be updated externally.
    /// The server monitors the enable flag and can be started/stopped dynamically.
    dynamic_config: Arc<RwLock<StorageReaderServerDynamicConfig>>,
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
    /// The server will monitor the enable flag via the shared config and can be
    /// started/stopped at runtime.
    pub fn new(
        storage_reader: StorageReader,
        config: ServerConfig,
        dynamic_config: Arc<RwLock<StorageReaderServerDynamicConfig>>,
    ) -> Self {
        let app_state = AppState { storage_reader, _phantom: PhantomData };
        Self { app_state, config, dynamic_config }
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
    /// Monitors the enable flag and gracefully starts/stops the server as needed.
    pub async fn run(self) -> Result<(), StorageError>
    where
        RequestHandler: Send + Sync + 'static,
        Request: Send + Sync + 'static,
        Response: Send + Sync + 'static,
    {
        let dynamic_config = self.dynamic_config;
        let ip = self.config.ip();
        let port = self.config.port();
        let app_state = self.app_state;
        let check_interval = Duration::from_millis(500); // Check config every 500ms

        info!("Storage reader server running with dynamic configuration");
        loop {
            // Get current enable state
            let is_enabled = dynamic_config.read().unwrap().enable;

            if is_enabled {
                info!("Storage reader server is enabled, starting server");
                let socket = SocketAddr::from((ip, port));
                info!("Storage reader server listening on {}", socket);

                let router = Router::new()
                    .route(
                        "/storage/query",
                        post(handle_request_endpoint::<RequestHandler, Request, Response>),
                    )
                    .with_state(app_state.clone());
                let listener = TcpListener::bind(&socket).await.map_err(|e| {
                    error!("Storage reader server bind error: {}", e);
                    StorageError::IOError(io::Error::other(e))
                })?;

                // Run server and monitor for config changes
                let config_clone = dynamic_config.clone();
                tokio::select! {
                    result = serve(listener, router) => {
                        error!("Storage reader server stopped unexpectedly");
                        return result.map_err(|e| {
                            StorageError::IOError(io::Error::other(e))
                        });
                    }
                    _ = async {
                        loop {
                            time::sleep(check_interval).await;
                            if !config_clone.read().unwrap().enable {
                                info!("Storage reader server disabled via config update, shutting down");
                                break;
                            }
                        }
                    } => {
                        // Config changed to disabled, loop will restart and wait for enable
                        continue;
                    }
                }
            } else {
                info!("Storage reader server is disabled, waiting for enable");
                // Wait for enable flag to become true
                loop {
                    time::sleep(check_interval).await;
                    if dynamic_config.read().unwrap().enable {
                        break;
                    }
                }
            }
        }
    }

    /// Spawns the storage reader server in a background task if it's enabled.
    pub fn spawn_if_enabled(server: Option<Self>) -> Option<AbortHandle>
    where
        RequestHandler: Send + Sync + 'static,
        Request: Send + Sync + 'static,
        Response: Send + Sync + 'static,
    {
        server.map(|server| {
            tokio::spawn(async move {
                if let Err(e) = server.run().await {
                    tracing::error!("Storage reader server error: {:?}", e);
                }
            })
            .abort_handle()
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

/// Creates a storage reader server with shared dynamic configuration.
/// The server will always be created (even if initially disabled) and will monitor the
/// enable flag via the shared config reference.
/// Returns the server and a shared reference to the config that can be updated externally.
pub fn create_storage_reader_server<RequestHandler, Request, Response>(
    storage_reader: StorageReader,
    storage_reader_server_config: ServerConfig,
) -> (
    StorageReaderServer<RequestHandler, Request, Response>,
    Arc<RwLock<StorageReaderServerDynamicConfig>>,
)
where
    RequestHandler: StorageReaderServerHandler<Request, Response>,
    Request: Serialize + DeserializeOwned + Send + 'static,
    Response: Serialize + DeserializeOwned + Send + 'static,
{
    let dynamic_config = Arc::new(RwLock::new(storage_reader_server_config.dynamic_config.clone()));
    let server = StorageReaderServer::new(
        storage_reader,
        storage_reader_server_config,
        dynamic_config.clone(),
    );
    (server, dynamic_config)
}

/// Generic polling function for updating storage reader server dynamic configuration.
/// The `extract_config` function should fetch the parent config and extract the
/// storage reader server dynamic config from it.
pub async fn poll_dynamic_config<F, Fut>(
    dynamic_config: Arc<RwLock<StorageReaderServerDynamicConfig>>,
    extract_config: F,
    poll_interval: Duration,
) where
    F: Fn() -> Fut + Send,
    Fut: std::future::Future<Output = Option<StorageReaderServerDynamicConfig>> + Send,
{
    let mut interval = time::interval(poll_interval);
    loop {
        interval.tick().await;
        if let Some(new_config) = extract_config().await {
            *dynamic_config.write().unwrap() = new_config;
        }
    }
}

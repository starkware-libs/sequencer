use std::collections::BTreeMap;
use std::io;
use std::marker::PhantomData;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

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
    /// The component that owns this storage reader server.
    /// This field is not serialized/deserialized as it's set programmatically.
    #[serde(skip)]
    pub component: StorageReaderComponent,
}

impl Default for StorageReaderServerStaticConfig {
    fn default() -> Self {
        Self {
            ip: Ipv4Addr::UNSPECIFIED.into(),
            port: 8091,
            component: StorageReaderComponent::Batcher,
        }
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
    pub fn new(ip: IpAddr, port: u16, enable: bool, component: StorageReaderComponent) -> Self {
        Self {
            static_config: StorageReaderServerStaticConfig { ip, port, component },
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

    /// Returns the component that owns this server.
    pub fn component(&self) -> StorageReaderComponent {
        self.static_config.component
    }
}

impl SerializeConfig for ServerConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut dump = prepend_sub_config_name(self.static_config.dump(), "static_config");
        dump.append(&mut prepend_sub_config_name(self.dynamic_config.dump(), "dynamic_config"));
        dump
    }
}

/// Identifies which component owns a storage reader server instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum StorageReaderComponent {
    /// Batcher component's storage reader server.
    #[default]
    Batcher,
    /// State sync component's storage reader server.
    StateSync,
    /// Class manager component's storage reader server.
    ClassManager,
}

/// Trait for config clients that can provide storage reader dynamic config.
#[async_trait]
pub trait StorageReaderConfigClient: Send + Sync {
    /// Gets the storage reader dynamic configuration.
    async fn get_storage_reader_dynamic_config(
        &self,
    ) -> Result<StorageReaderServerDynamicConfig, Box<dyn std::error::Error + Send + Sync>>;
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
    config_manager_client: std::sync::Arc<dyn StorageReaderConfigClient>,
    _phantom: PhantomData<(RequestHandler, Request, Response)>,
}

impl<RequestHandler, Request, Response> Clone for AppState<RequestHandler, Request, Response>
where
    RequestHandler: StorageReaderServerHandler<Request, Response>,
{
    fn clone(&self) -> Self {
        Self {
            storage_reader: self.storage_reader.clone(),
            config_manager_client: self.config_manager_client.clone(),
            _phantom: PhantomData,
        }
    }
}

impl<RequestHandler, Request, Response> StorageReaderServer<RequestHandler, Request, Response>
where
    RequestHandler: StorageReaderServerHandler<Request, Response>,
    Request: Serialize + DeserializeOwned + Send + 'static,
    Response: Serialize + DeserializeOwned + Send + 'static,
{
    /// Creates a new storage reader server with the given handler and configuration.
    pub fn new(
        storage_reader: StorageReader,
        config: ServerConfig,
        config_manager_client: std::sync::Arc<dyn StorageReaderConfigClient>,
    ) -> Self {
        let app_state = AppState { storage_reader, config_manager_client, _phantom: PhantomData };
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
        if !self.config.is_enabled() {
            info!("Storage reader server is disabled, not starting");
            return Ok(());
        }
        let socket = SocketAddr::from((self.config.ip(), self.config.port()));
        info!("Starting storage reader server on {}", socket);
        let app = self.app();
        info!("Storage reader server listening on {}", socket);

        // Start the server
        let listener = TcpListener::bind(&socket).await.map_err(|e| {
            error!("Storage reader server error: {}", e);
            StorageError::IOError(io::Error::other(e))
        })?;
        serve(listener, app).await.map_err(|e| {
            error!("Storage reader server error: {}", e);
            StorageError::IOError(io::Error::other(e))
        })
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

/// Fetches and validates the storage reader dynamic configuration.
async fn get_and_validate_config(
    config_client: &std::sync::Arc<dyn StorageReaderConfigClient>,
) -> Result<StorageReaderServerDynamicConfig, StorageServerError> {
    let dynamic_config = config_client
        .get_storage_reader_dynamic_config()
        .await
        .expect("Should be able to get storage reader dynamic config");

    if !dynamic_config.enable {
        return Err(StorageServerError(StorageError::ServerDisabled));
    }

    Ok(dynamic_config)
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
    // Get and validate dynamic config before each request
    get_and_validate_config(&app_state.config_manager_client).await?;

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
    storage_reader_server_config: ServerConfig,
    config_manager_client: std::sync::Arc<dyn StorageReaderConfigClient>,
) -> Option<StorageReaderServer<RequestHandler, Request, Response>>
where
    RequestHandler: StorageReaderServerHandler<Request, Response>,
    Request: Serialize + DeserializeOwned + Send + 'static,
    Response: Serialize + DeserializeOwned + Send + 'static,
{
    if storage_reader_server_config.is_enabled() {
        Some(StorageReaderServer::new(
            storage_reader,
            storage_reader_server_config,
            config_manager_client,
        ))
    } else {
        None
    }
}

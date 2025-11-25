use std::marker::PhantomData;
use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::{StorageError, StorageReader};

// TODO(Nadin): Remove #[allow(dead_code)] once the fields are used in the implementation.
#[allow(dead_code)]
/// Configuration for the storage reader server.
pub struct ServerConfig {
    /// The socket address to bind the server to.
    socket: SocketAddr,
    /// Maximum number of concurrent requests the server can handle.
    max_concurrency: usize,
    /// Whether the server is enabled.
    enable: bool,
}

impl ServerConfig {
    /// Creates a new server configuration.
    pub fn new(socket: SocketAddr, max_concurrency: usize, enable: bool) -> Self {
        Self { socket, max_concurrency, enable }
    }
}

#[async_trait]
/// Handler trait for processing storage reader requests.
pub trait StorageReaderServerHandler<Request, Response> {
    /// Handles an incoming request and returns a response.
    async fn handle_request(
        &self,
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

/// Application state shared across request handlers.
struct AppState<RequestHandler, Request, Response>
where
    RequestHandler: StorageReaderServerHandler<Request, Response>,
{
    storage_reader: Arc<StorageReader>,
    request_handler: Arc<RequestHandler>,
    semaphore: Arc<Semaphore>,
    _req_res: PhantomData<(Request, Response)>,
}

impl<RequestHandler, Request, Response> Clone for AppState<RequestHandler, Request, Response>
where
    RequestHandler: StorageReaderServerHandler<Request, Response>,
{
    fn clone(&self) -> Self {
        Self {
            storage_reader: Arc::clone(&self.storage_reader),
            request_handler: Arc::clone(&self.request_handler),
            semaphore: Arc::clone(&self.semaphore),
            _req_res: PhantomData,
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
        storage_reader: Arc<StorageReader>,
        request_handler: Arc<RequestHandler>,
        config: ServerConfig,
    ) -> Self {
        let app_state = AppState {
            storage_reader,
            request_handler,
            semaphore: Arc::new(Semaphore::new(config.max_concurrency)),
            _req_res: PhantomData,
        };
        Self { app_state, config }
    }

    /// Starts the server to handle incoming requests.
    pub fn start(&mut self) {
        unimplemented!()
    }
}

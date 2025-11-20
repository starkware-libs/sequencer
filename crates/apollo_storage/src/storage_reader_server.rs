use std::marker::PhantomData;
use std::net::SocketAddr;

use async_trait::async_trait;
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::StorageReader;

// TODO(Nadin): Remove #[allow(dead_code)] once the fields are used in the implementation.
#[allow(dead_code)]
/// Configuration for the storage reader server.
pub struct ServerConfig {
    /// The socket address to bind the server to.
    pub socket: SocketAddr,
    /// Maximum number of concurrent requests the server can handle.
    pub max_concurrency: usize,
    /// Whether the server is enabled.
    pub enable: bool,
}

#[async_trait]
/// Handler trait for processing storage reader requests.
pub trait StorageReaderServerHandler<Request, Response> {
    /// Handles an incoming request and returns a response.
    async fn handle_request(
        &self,
        storage_reader: &StorageReader,
        request: Request,
    ) -> Result<Response, Box<dyn std::error::Error>>;
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
    storage_reader: StorageReader,
    request_handler: RequestHandler,
    config: ServerConfig,
    _req_res: PhantomData<(Request, Response)>,
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
        request_handler: RequestHandler,
        config: ServerConfig,
    ) -> Self {
        Self { storage_reader, request_handler, config, _req_res: PhantomData }
    }

    /// Starts the server to handle incoming requests.
    pub fn start(&mut self) {
        unimplemented!()
    }
}

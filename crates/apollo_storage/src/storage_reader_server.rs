use std::marker::PhantomData;
use std::net::SocketAddr;

use async_trait::async_trait;
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::StorageReader;

// TODO(Nadin): Remove #[allow(dead_code)] once the fields are used in the implementation.
#[allow(dead_code)]
struct ServerConfig {
    socket: SocketAddr,
    max_concurrency: usize,
    enable: bool,
}

#[async_trait]
pub trait StorageReaderServerHandler<Request, Response> {
    async fn handle_request(
        &self,
        storage_reader: &StorageReader,
        request: Request,
    ) -> Result<Response, Box<dyn std::error::Error>>;
}

// TODO(Nadin): Remove #[allow(dead_code)] once the fields are used in the implementation.
#[allow(dead_code)]
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
    pub fn new(
        storage_reader: StorageReader,
        request_handler: RequestHandler,
        config: ServerConfig,
    ) -> Self {
        Self { storage_reader, request_handler, config, _req_res: PhantomData }
    }

    pub fn start(&mut self) {
        unimplemented!()
    }
}

use std::io;
use std::net::SocketAddr;
use std::sync::Arc;

use apollo_batcher_types::bootstrap_types::{BootstrapRequest, BootstrapResponse};
use apollo_storage::storage_reader_server::{ServerConfig, StorageReaderServerHandler};
use apollo_storage::storage_reader_types::{
    GenericStorageReaderServerHandler,
    StorageReaderRequest,
    StorageReaderResponse,
};
use apollo_storage::{StorageError, StorageReader};
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{serve, Json, Router};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tokio::task::AbortHandle;
use tracing::{error, info};

use crate::bootstrap::{
    bootstrap_transactions_for_state,
    current_bootstrap_state,
    BootstrapConfig,
};

pub type SharedBootstrapConfig = Arc<BootstrapConfig>;

/// Batcher-specific request type: storage queries and bootstrap queries over the same endpoint.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum BatcherStorageReaderRequest {
    Storage(StorageReaderRequest),
    Bootstrap(BootstrapRequest),
}

/// Batcher-specific response type: storage responses and bootstrap responses.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum BatcherStorageReaderResponse {
    Storage(StorageReaderResponse),
    Bootstrap(BootstrapResponse),
}

/// Handler that encapsulates both generic storage reader logic and bootstrap-specific logic.
#[derive(Clone)]
pub struct BatcherStorageReaderHandler {
    storage_reader: StorageReader,
    bootstrap_config: SharedBootstrapConfig,
}

impl BatcherStorageReaderHandler {
    pub fn new(storage_reader: StorageReader, bootstrap_config: SharedBootstrapConfig) -> Self {
        Self { storage_reader, bootstrap_config }
    }

    pub async fn handle_request(
        &self,
        request: BatcherStorageReaderRequest,
    ) -> Result<BatcherStorageReaderResponse, StorageError> {
        match request {
            BatcherStorageReaderRequest::Storage(req) => {
                let resp =
                    GenericStorageReaderServerHandler::handle_request(&self.storage_reader, req)
                        .await?;
                Ok(BatcherStorageReaderResponse::Storage(resp))
            }
            BatcherStorageReaderRequest::Bootstrap(req) => {
                let resp = match req {
                    BootstrapRequest::GetBootstrapState => BootstrapResponse::BootstrapState(
                        current_bootstrap_state(&self.bootstrap_config, &self.storage_reader),
                    ),
                    BootstrapRequest::GetBootstrapTransactions => {
                        let state =
                            current_bootstrap_state(&self.bootstrap_config, &self.storage_reader);
                        BootstrapResponse::BootstrapTransactions(bootstrap_transactions_for_state(
                            &self.bootstrap_config,
                            state,
                        ))
                    }
                };
                Ok(BatcherStorageReaderResponse::Bootstrap(resp))
            }
        }
    }
}

/// Batcher storage reader HTTP server that handles both storage and bootstrap queries.
pub struct BatcherStorageReaderServer {
    handler: BatcherStorageReaderHandler,
    config: ServerConfig,
}

impl BatcherStorageReaderServer {
    pub fn new(
        storage_reader: StorageReader,
        config: ServerConfig,
        bootstrap_config: SharedBootstrapConfig,
    ) -> Self {
        let handler = BatcherStorageReaderHandler::new(storage_reader, bootstrap_config);
        Self { handler, config }
    }

    pub async fn run(self) -> Result<(), StorageError> {
        if !self.config.is_enabled() {
            info!("Batcher storage reader server is disabled, not starting");
            return Ok(());
        }
        let socket = SocketAddr::from((self.config.ip(), self.config.port()));
        info!("Starting batcher storage reader server on {}", socket);

        let app = Router::new()
            .route("/storage/query", post(handle_request_endpoint))
            .with_state(self.handler);

        let listener = TcpListener::bind(&socket).await.map_err(|e| {
            error!("Batcher storage reader server error: {}", e);
            StorageError::IOError(io::Error::other(e))
        })?;
        serve(listener, app).await.map_err(|e| {
            error!("Batcher storage reader server error: {}", e);
            StorageError::IOError(io::Error::other(e))
        })
    }

    pub fn spawn_if_enabled(server: Option<Self>) -> Option<AbortHandle> {
        server.map(|server| {
            tokio::spawn(async move {
                if let Err(e) = server.run().await {
                    error!("Batcher storage reader server error: {:?}", e);
                }
            })
            .abort_handle()
        })
    }
}

/// Creates an optional batcher storage reader server based on the config.
pub fn create_bootstrap_storage_reader_server(
    storage_reader: StorageReader,
    config: ServerConfig,
    bootstrap_config: SharedBootstrapConfig,
) -> Option<BatcherStorageReaderServer> {
    if config.is_enabled() {
        Some(BatcherStorageReaderServer::new(storage_reader, config, bootstrap_config))
    } else {
        None
    }
}

async fn handle_request_endpoint(
    State(handler): State<BatcherStorageReaderHandler>,
    Json(request): Json<BatcherStorageReaderRequest>,
) -> Result<Json<BatcherStorageReaderResponse>, BatcherServerError> {
    let response = handler.handle_request(request).await?;
    Ok(Json(response))
}

#[derive(Debug)]
struct BatcherServerError(StorageError);

impl From<StorageError> for BatcherServerError {
    fn from(error: StorageError) -> Self {
        BatcherServerError(error)
    }
}

impl IntoResponse for BatcherServerError {
    fn into_response(self) -> Response {
        let error_message = format!("Storage error: {}", self.0);
        error!("{}", error_message);
        (StatusCode::INTERNAL_SERVER_ERROR, error_message).into_response()
    }
}

#[cfg(test)]
mod tests {
    use apollo_batcher_types::bootstrap_types::BootstrapState;
    use apollo_storage::test_utils::get_test_storage;

    use super::*;
    use crate::bootstrap::BootstrapConfig;

    fn make_handler(bootstrap_enabled: bool) -> (BatcherStorageReaderHandler, StorageReader) {
        let ((reader, _writer), _temp_dir) = get_test_storage();
        let cfg = Arc::new(BootstrapConfig { bootstrap_enabled });
        let handler = BatcherStorageReaderHandler::new(reader.clone(), cfg);
        (handler, reader)
    }

    #[tokio::test]
    async fn get_state_when_bootstrapping() {
        let (handler, _reader) = make_handler(true);
        let request = BatcherStorageReaderRequest::Bootstrap(BootstrapRequest::GetBootstrapState);
        let response = handler.handle_request(request).await.unwrap();
        assert!(matches!(
            response,
            BatcherStorageReaderResponse::Bootstrap(BootstrapResponse::BootstrapState(
                BootstrapState::DeclareContracts
            ))
        ));
    }

    #[tokio::test]
    async fn get_state_when_not_bootstrapping() {
        let (handler, _reader) = make_handler(false);
        let request = BatcherStorageReaderRequest::Bootstrap(BootstrapRequest::GetBootstrapState);
        let response = handler.handle_request(request).await.unwrap();
        assert!(matches!(
            response,
            BatcherStorageReaderResponse::Bootstrap(BootstrapResponse::BootstrapState(
                BootstrapState::NotInBootstrap
            ))
        ));
    }

    #[tokio::test]
    async fn get_transactions_returns_declares_in_declare_state() {
        let (handler, _reader) = make_handler(true);
        let request =
            BatcherStorageReaderRequest::Bootstrap(BootstrapRequest::GetBootstrapTransactions);
        let response = handler.handle_request(request).await.unwrap();
        if let BatcherStorageReaderResponse::Bootstrap(BootstrapResponse::BootstrapTransactions(
            txs,
        )) = response
        {
            assert_eq!(txs.len(), 2);
            use starknet_api::rpc_transaction::RpcTransaction;
            assert!(matches!(txs[0], RpcTransaction::Declare(_)));
            assert!(matches!(txs[1], RpcTransaction::Declare(_)));
        } else {
            panic!("Expected BootstrapTransactions response");
        }
    }

    #[tokio::test]
    async fn get_transactions_returns_empty_when_not_bootstrapping() {
        let (handler, _reader) = make_handler(false);
        let request =
            BatcherStorageReaderRequest::Bootstrap(BootstrapRequest::GetBootstrapTransactions);
        let response = handler.handle_request(request).await.unwrap();
        if let BatcherStorageReaderResponse::Bootstrap(BootstrapResponse::BootstrapTransactions(
            txs,
        )) = response
        {
            assert!(txs.is_empty());
        } else {
            panic!("Expected BootstrapTransactions response");
        }
    }
}

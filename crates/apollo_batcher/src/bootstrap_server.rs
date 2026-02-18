use std::sync::Arc;

use apollo_batcher_types::bootstrap_types::{BootstrapRequest, BootstrapResponse};
use apollo_storage::storage_reader_server::{StorageReaderServer, StorageReaderServerHandler};
use apollo_storage::storage_reader_types::{
    GenericStorageReaderServerHandler,
    StorageReaderRequest,
    StorageReaderResponse,
};
use apollo_storage::{StorageError, StorageReader};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::bootstrap::BootstrapStateMachine;

pub type SharedBootstrapStateMachine = Arc<BootstrapStateMachine>;

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

pub struct BatcherStorageReaderHandler;

#[async_trait]
impl
    StorageReaderServerHandler<
        BatcherStorageReaderRequest,
        BatcherStorageReaderResponse,
        SharedBootstrapStateMachine,
    > for BatcherStorageReaderHandler
{
    async fn handle_request(
        storage_reader: &StorageReader,
        extra_state: &SharedBootstrapStateMachine,
        request: BatcherStorageReaderRequest,
    ) -> Result<BatcherStorageReaderResponse, StorageError> {
        match request {
            BatcherStorageReaderRequest::Storage(req) => {
                let resp =
                    GenericStorageReaderServerHandler::handle_request(storage_reader, &(), req)
                        .await?;
                Ok(BatcherStorageReaderResponse::Storage(resp))
            }
            BatcherStorageReaderRequest::Bootstrap(req) => {
                let resp = match req {
                    BootstrapRequest::GetBootstrapState => {
                        BootstrapResponse::BootstrapState(extra_state.current_state(storage_reader))
                    }
                    BootstrapRequest::GetBootstrapTransactions => {
                        let state = extra_state.current_state(storage_reader);
                        BootstrapResponse::BootstrapTransactions(
                            extra_state.transactions_for_state(state),
                        )
                    }
                };
                Ok(BatcherStorageReaderResponse::Bootstrap(resp))
            }
        }
    }
}

pub type BatcherStorageReaderServer = StorageReaderServer<
    BatcherStorageReaderHandler,
    BatcherStorageReaderRequest,
    BatcherStorageReaderResponse,
    SharedBootstrapStateMachine,
>;

#[cfg(test)]
mod tests {
    use apollo_batcher_types::bootstrap_types::BootstrapState;
    use apollo_storage::test_utils::get_test_storage;

    use super::*;

    #[tokio::test]
    async fn get_state_when_bootstrapping() {
        let ((reader, _writer), _temp_dir) = get_test_storage();
        let sm = Arc::new(BootstrapStateMachine::new(true));
        let request = BatcherStorageReaderRequest::Bootstrap(BootstrapRequest::GetBootstrapState);
        let response =
            BatcherStorageReaderHandler::handle_request(&reader, &sm, request).await.unwrap();
        assert!(matches!(
            response,
            BatcherStorageReaderResponse::Bootstrap(BootstrapResponse::BootstrapState(
                BootstrapState::DeclareContracts
            ))
        ));
    }

    #[tokio::test]
    async fn get_state_when_not_bootstrapping() {
        let ((reader, _writer), _temp_dir) = get_test_storage();
        let sm = Arc::new(BootstrapStateMachine::new(false));
        let request = BatcherStorageReaderRequest::Bootstrap(BootstrapRequest::GetBootstrapState);
        let response =
            BatcherStorageReaderHandler::handle_request(&reader, &sm, request).await.unwrap();
        assert!(matches!(
            response,
            BatcherStorageReaderResponse::Bootstrap(BootstrapResponse::BootstrapState(
                BootstrapState::NotInBootstrap
            ))
        ));
    }

    #[tokio::test]
    async fn get_transactions_returns_declares_in_declare_state() {
        let ((reader, _writer), _temp_dir) = get_test_storage();
        let sm = Arc::new(BootstrapStateMachine::new(true));
        let request =
            BatcherStorageReaderRequest::Bootstrap(BootstrapRequest::GetBootstrapTransactions);
        let response =
            BatcherStorageReaderHandler::handle_request(&reader, &sm, request).await.unwrap();
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
        let ((reader, _writer), _temp_dir) = get_test_storage();
        let sm = Arc::new(BootstrapStateMachine::new(false));
        let request =
            BatcherStorageReaderRequest::Bootstrap(BootstrapRequest::GetBootstrapTransactions);
        let response =
            BatcherStorageReaderHandler::handle_request(&reader, &sm, request).await.unwrap();
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

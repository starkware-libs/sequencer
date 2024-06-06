use std::sync::Arc;

use async_trait::async_trait;
use starknet_mempool_infra::component_client::{ClientError, ComponentClient};
use starknet_mempool_infra::component_definitions::ComponentRequestAndResponseSender;
use thiserror::Error;

use crate::errors::MempoolError;
use crate::mempool_types::{MempoolInput, ThinTransaction};

pub type MempoolClientImpl = ComponentClient<MempoolRequest, MempoolResponse>;
pub type MempoolResult<T> = Result<T, MempoolError>;
pub type MempoolClientResult<T> = Result<T, MempoolClientError>;
pub type MempoolRequestAndResponseSender =
    ComponentRequestAndResponseSender<MempoolRequest, MempoolResponse>;
pub type SharedMempoolClient = Arc<dyn MempoolClient>;

/// Serves as the mempool's shared interface. Requires `Send + Sync` to allow transferring and
/// sharing resources (inputs, futures) across threads.
#[async_trait]
pub trait MempoolClient: Send + Sync {
    async fn add_tx(&self, mempool_input: MempoolInput) -> MempoolClientResult<()>;
    async fn get_txs(&self, n_txs: usize) -> MempoolClientResult<Vec<ThinTransaction>>;
}

#[derive(Debug)]
pub enum MempoolRequest {
    AddTransaction(MempoolInput),
    GetTransactions(usize),
}

#[derive(Debug)]
pub enum MempoolResponse {
    AddTransaction(MempoolResult<()>),
    GetTransactions(MempoolResult<Vec<ThinTransaction>>),
}

#[derive(Debug, Error)]
pub enum MempoolClientError {
    #[error(transparent)]
    ClientError(#[from] ClientError),
    #[error(transparent)]
    MempoolError(#[from] MempoolError),
}

#[async_trait]
impl MempoolClient for MempoolClientImpl {
    async fn add_tx(&self, mempool_input: MempoolInput) -> MempoolClientResult<()> {
        let request = MempoolRequest::AddTransaction(mempool_input);
        let response = self.send(request).await;
        match response {
            MempoolResponse::AddTransaction(Ok(response)) => Ok(response),
            MempoolResponse::AddTransaction(Err(response)) => {
                Err(MempoolClientError::MempoolError(response))
            }
            _ => Err(MempoolClientError::ClientError(ClientError::UnexpectedResponse)),
        }
    }

    async fn get_txs(&self, n_txs: usize) -> MempoolClientResult<Vec<ThinTransaction>> {
        let request = MempoolRequest::GetTransactions(n_txs);
        let response = self.send(request).await;
        match response {
            MempoolResponse::GetTransactions(Ok(response)) => Ok(response),
            MempoolResponse::GetTransactions(Err(response)) => {
                Err(MempoolClientError::MempoolError(response))
            }
            _ => Err(MempoolClientError::ClientError(ClientError::UnexpectedResponse)),
        }
    }
}

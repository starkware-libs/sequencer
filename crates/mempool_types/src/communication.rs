use std::sync::Arc;

use async_trait::async_trait;
use mockall::predicate::*;
use mockall::*;
use papyrus_network_types::network_types::BroadcastedMessageManager;
use papyrus_proc_macros::handle_response_variants;
use serde::{Deserialize, Serialize};
use starknet_api::executable_transaction::Transaction;
use starknet_mempool_infra::component_client::{
    ClientError,
    LocalComponentClient,
    RemoteComponentClient,
};
use starknet_mempool_infra::component_definitions::ComponentRequestAndResponseSender;
use thiserror::Error;

use crate::errors::MempoolError;
use crate::mempool_types::MempoolInput;

pub type LocalMempoolClientImpl = LocalComponentClient<MempoolRequest, MempoolResponse>;
pub type RemoteMempoolClientImpl = RemoteComponentClient<MempoolRequest, MempoolResponse>;
pub type MempoolResult<T> = Result<T, MempoolError>;
pub type MempoolClientResult<T> = Result<T, MempoolClientError>;
pub type MempoolRequestAndResponseSender =
    ComponentRequestAndResponseSender<MempoolRequest, MempoolResponse>;
pub type SharedMempoolClient = Arc<dyn MempoolClient>;

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct MempoolWrapperInput {
    pub mempool_input: MempoolInput,
    pub message_metadata: Option<BroadcastedMessageManager>,
}

/// Serves as the mempool's shared interface. Requires `Send + Sync` to allow transferring and
/// sharing resources (inputs, futures) across threads.
#[automock]
#[async_trait]
pub trait MempoolClient: Send + Sync {
    // TODO: Add Option<BroadcastedMessageManager> as an argument for add_transaction
    // TODO: Rename tx to transaction
    async fn add_tx(&self, mempool_input: MempoolWrapperInput) -> MempoolClientResult<()>;
    async fn get_txs(&self, n_txs: usize) -> MempoolClientResult<Vec<Transaction>>;
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum MempoolRequest {
    AddTransaction(MempoolWrapperInput),
    GetTransactions(usize),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum MempoolResponse {
    AddTransaction(MempoolResult<()>),
    GetTransactions(MempoolResult<Vec<Transaction>>),
}

#[derive(Clone, Debug, Error)]
pub enum MempoolClientError {
    #[error(transparent)]
    ClientError(#[from] ClientError),
    #[error(transparent)]
    MempoolError(#[from] MempoolError),
}

#[async_trait]
impl MempoolClient for LocalMempoolClientImpl {
    async fn add_tx(&self, mempool_wrapper_input: MempoolWrapperInput) -> MempoolClientResult<()> {
        let request = MempoolRequest::AddTransaction(mempool_wrapper_input);
        let response = self.send(request).await;
        handle_response_variants!(MempoolResponse, AddTransaction, MempoolClientError, MempoolError)
    }

    async fn get_txs(&self, n_txs: usize) -> MempoolClientResult<Vec<Transaction>> {
        let request = MempoolRequest::GetTransactions(n_txs);
        let response = self.send(request).await;
        handle_response_variants!(
            MempoolResponse,
            GetTransactions,
            MempoolClientError,
            MempoolError
        )
    }
}

#[async_trait]
impl MempoolClient for RemoteMempoolClientImpl {
    async fn add_tx(&self, mempool_wrapper_input: MempoolWrapperInput) -> MempoolClientResult<()> {
        let request = MempoolRequest::AddTransaction(mempool_wrapper_input);
        let response = self.send(request).await?;
        handle_response_variants!(MempoolResponse, AddTransaction, MempoolClientError, MempoolError)
    }

    async fn get_txs(&self, n_txs: usize) -> MempoolClientResult<Vec<Transaction>> {
        let request = MempoolRequest::GetTransactions(n_txs);
        let response = self.send(request).await?;
        handle_response_variants!(
            MempoolResponse,
            GetTransactions,
            MempoolClientError,
            MempoolError
        )
    }
}

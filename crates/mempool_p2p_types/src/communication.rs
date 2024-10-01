use std::sync::Arc;

use async_trait::async_trait;
use papyrus_network_types::network_types::BroadcastedMessageMetadata;
use papyrus_proc_macros::handle_response_variants;
use serde::{Deserialize, Serialize};
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_mempool_infra::component_client::{ClientError, LocalComponentClient};
use thiserror::Error;

use crate::errors::MempoolP2pSenderError;
use crate::mempool_p2p_types::MempoolP2pSenderResult;

#[async_trait]
pub trait MempoolP2pSenderClient: Send + Sync {
    /// Adds a transaction to be propagated to other peers. This should only be called on a new
    /// transaction coming from the user and not from another peer. To handle transactions coming
    /// from other peers, use `continue_propagation`.
    async fn add_transaction(
        &self,
        transaction: RpcTransaction,
    ) -> MempoolP2pSenderClientResult<()>;

    /// Continues the propagation of a transaction we've received from another peer.
    async fn continue_propagation(
        &self,
        propagation_metadata: BroadcastedMessageMetadata,
    ) -> MempoolP2pSenderClientResult<()>;
}

// TODO: Implement remote MempoolP2pSenderClient.
pub type LocalMempoolP2pSenderClientImpl =
    LocalComponentClient<MempoolP2pSenderRequest, MempoolP2pSenderResponse>;
pub type SharedMempoolP2pSenderClient = Arc<dyn MempoolP2pSenderClient>;
pub type MempoolP2pSenderClientResult<T> = Result<T, MempoolP2pSenderClientError>;

#[derive(Debug, Serialize, Deserialize)]
pub enum MempoolP2pSenderRequest {
    AddTransaction(RpcTransaction),
    ContinuePropagation(BroadcastedMessageMetadata),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum MempoolP2pSenderResponse {
    AddTransaction(MempoolP2pSenderResult<()>),
    ContinuePropagation(MempoolP2pSenderResult<()>),
}

#[derive(Clone, Debug, Error)]
pub enum MempoolP2pSenderClientError {
    #[error(transparent)]
    ClientError(#[from] ClientError),
    #[error(transparent)]
    MempoolP2pSenderError(#[from] MempoolP2pSenderError),
}

#[async_trait]
impl MempoolP2pSenderClient for LocalMempoolP2pSenderClientImpl {
    async fn add_transaction(
        &self,
        transaction: RpcTransaction,
    ) -> MempoolP2pSenderClientResult<()> {
        let request = MempoolP2pSenderRequest::AddTransaction(transaction);
        let response = self.send(request).await;
        handle_response_variants!(
            MempoolP2pSenderResponse,
            AddTransaction,
            MempoolP2pSenderClientError,
            MempoolP2pSenderError
        )
    }

    async fn continue_propagation(
        &self,
        propagation_metadata: BroadcastedMessageMetadata,
    ) -> MempoolP2pSenderClientResult<()> {
        let request = MempoolP2pSenderRequest::ContinuePropagation(propagation_metadata);
        let response = self.send(request).await;
        handle_response_variants!(
            MempoolP2pSenderResponse,
            ContinuePropagation,
            MempoolP2pSenderClientError,
            MempoolP2pSenderError
        )
    }
}

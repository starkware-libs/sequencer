use std::sync::Arc;

use async_trait::async_trait;
pub use papyrus_network::network_manager::BroadcastedMessageManager;
use papyrus_proc_macros::handle_response_variants;
use serde::{Deserialize, Serialize};
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_mempool_infra::component_client::{ClientError, LocalComponentClient};
use starknet_mempool_infra::component_definitions::ComponentRequestHandler;
use thiserror::Error;

pub struct MempoolP2pSender;

// This error is defined even though it's empty to be compatible with the other components.
#[derive(Debug, Error, Serialize, Deserialize, Clone)]
pub enum MempoolP2pSenderError {}

pub type MempoolP2pSenderResult<T> = Result<T, MempoolP2pSenderError>;

#[derive(Clone, Debug, Error)]
pub enum MempoolP2pSenderClientError {
    #[error(transparent)]
    ClientError(#[from] ClientError),
    #[error(transparent)]
    MempoolP2pSenderError(#[from] MempoolP2pSenderError),
}

pub type MempoolP2pSenderClientResult<T> = Result<T, MempoolP2pSenderClientError>;

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
        propagation_manager: BroadcastedMessageManager,
    ) -> MempoolP2pSenderClientResult<()>;
}

pub type SharedMempoolP2pSenderClient = Arc<dyn MempoolP2pSenderClient>;

#[derive(Debug, Serialize, Deserialize)]
pub enum MempoolP2pSenderRequest {
    AddTransaction(RpcTransaction),
    ContinuePropagation(BroadcastedMessageManager),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum MempoolP2pSenderResponse {
    AddTransaction(MempoolP2pSenderResult<()>),
    ContinuePropagation(MempoolP2pSenderResult<()>),
}

#[async_trait]
impl ComponentRequestHandler<MempoolP2pSenderRequest, MempoolP2pSenderResponse>
    for MempoolP2pSender
{
    async fn handle_request(
        &mut self,
        _request: MempoolP2pSenderRequest,
    ) -> MempoolP2pSenderResponse {
        unimplemented!()
    }
}

#[async_trait]
impl MempoolP2pSenderClient
    for LocalComponentClient<MempoolP2pSenderRequest, MempoolP2pSenderResponse>
{
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
        propagation_manager: BroadcastedMessageManager,
    ) -> MempoolP2pSenderClientResult<()> {
        let request = MempoolP2pSenderRequest::ContinuePropagation(propagation_manager);
        let response = self.send(request).await;
        handle_response_variants!(
            MempoolP2pSenderResponse,
            ContinuePropagation,
            MempoolP2pSenderClientError,
            MempoolP2pSenderError
        )
    }
}

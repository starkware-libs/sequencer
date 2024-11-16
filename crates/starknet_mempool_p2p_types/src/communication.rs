use std::sync::Arc;

use async_trait::async_trait;
use papyrus_network_types::network_types::BroadcastedMessageMetadata;
use papyrus_proc_macros::handle_response_variants;
use serde::{Deserialize, Serialize};
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_sequencer_infra::component_client::{
    ClientError,
    LocalComponentClient,
    RemoteComponentClient,
};
use starknet_sequencer_infra::component_definitions::ComponentRequestAndResponseSender;
use thiserror::Error;

use crate::errors::MempoolP2pPropagatorError;
use crate::mempool_p2p_types::MempoolP2pPropagatorResult;

#[async_trait]
pub trait MempoolP2pPropagatorClient: Send + Sync {
    /// Adds a transaction to be propagated to other peers. This should only be called on a new
    /// transaction coming from the user and not from another peer. To handle transactions coming
    /// from other peers, use `continue_propagation`.
    async fn add_transaction(
        &self,
        transaction: RpcTransaction,
    ) -> MempoolP2pPropagatorClientResult<()>;

    /// Continues the propagation of a transaction we've received from another peer.
    async fn continue_propagation(
        &self,
        propagation_metadata: BroadcastedMessageMetadata,
    ) -> MempoolP2pPropagatorClientResult<()>;
}

pub type LocalMempoolP2pPropagatorClient =
    LocalComponentClient<MempoolP2pPropagatorRequest, MempoolP2pPropagatorResponse>;
pub type RemoteMempoolP2pPropagatorClient =
    RemoteComponentClient<MempoolP2pPropagatorRequest, MempoolP2pPropagatorResponse>;
pub type SharedMempoolP2pPropagatorClient = Arc<dyn MempoolP2pPropagatorClient>;
pub type MempoolP2pPropagatorClientResult<T> = Result<T, MempoolP2pPropagatorClientError>;
pub type MempoolP2pPropagatorRequestAndResponseSender =
    ComponentRequestAndResponseSender<MempoolP2pPropagatorRequest, MempoolP2pPropagatorResponse>;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum MempoolP2pPropagatorRequest {
    AddTransaction(RpcTransaction),
    ContinuePropagation(BroadcastedMessageMetadata),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum MempoolP2pPropagatorResponse {
    AddTransaction(MempoolP2pPropagatorResult<()>),
    ContinuePropagation(MempoolP2pPropagatorResult<()>),
}

#[derive(Clone, Debug, Error)]
pub enum MempoolP2pPropagatorClientError {
    #[error(transparent)]
    ClientError(#[from] ClientError),
    #[error(transparent)]
    MempoolP2pPropagatorError(#[from] MempoolP2pPropagatorError),
}

#[async_trait]
impl MempoolP2pPropagatorClient for LocalMempoolP2pPropagatorClient {
    async fn add_transaction(
        &self,
        transaction: RpcTransaction,
    ) -> MempoolP2pPropagatorClientResult<()> {
        let request = MempoolP2pPropagatorRequest::AddTransaction(transaction);
        let response = self.send(request).await?;
        handle_response_variants!(
            MempoolP2pPropagatorResponse,
            AddTransaction,
            MempoolP2pPropagatorClientError,
            MempoolP2pPropagatorError
        )
    }

    async fn continue_propagation(
        &self,
        propagation_metadata: BroadcastedMessageMetadata,
    ) -> MempoolP2pPropagatorClientResult<()> {
        let request = MempoolP2pPropagatorRequest::ContinuePropagation(propagation_metadata);
        let response = self.send(request).await?;
        handle_response_variants!(
            MempoolP2pPropagatorResponse,
            ContinuePropagation,
            MempoolP2pPropagatorClientError,
            MempoolP2pPropagatorError
        )
    }
}

#[async_trait]
impl MempoolP2pPropagatorClient for RemoteMempoolP2pPropagatorClient {
    async fn add_transaction(
        &self,
        transaction: RpcTransaction,
    ) -> MempoolP2pPropagatorClientResult<()> {
        let request = MempoolP2pPropagatorRequest::AddTransaction(transaction);
        let response = self.send(request).await?;
        handle_response_variants!(
            MempoolP2pPropagatorResponse,
            AddTransaction,
            MempoolP2pPropagatorClientError,
            MempoolP2pPropagatorError
        )
    }

    async fn continue_propagation(
        &self,
        propagation_metadata: BroadcastedMessageMetadata,
    ) -> MempoolP2pPropagatorClientResult<()> {
        let request = MempoolP2pPropagatorRequest::ContinuePropagation(propagation_metadata);
        let response = self.send(request).await?;
        handle_response_variants!(
            MempoolP2pPropagatorResponse,
            ContinuePropagation,
            MempoolP2pPropagatorClientError,
            MempoolP2pPropagatorError
        )
    }
}

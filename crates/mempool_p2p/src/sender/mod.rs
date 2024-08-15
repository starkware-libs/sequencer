use std::sync::Arc;

use async_trait::async_trait;
pub use papyrus_network::network_manager::BroadcastedMessageManager;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_mempool_infra::component_client::{ClientResult, LocalComponentClient};
use starknet_mempool_infra::component_definitions::ComponentRequestHandler;

pub struct MempoolP2PSender;

#[async_trait]
pub trait MempoolP2PSenderClient: Send + Sync {
    /// Adds a transaction to be propagated to other peers. This should only be called on a new
    /// transaction coming from the user and not from another peer. To handle transactions coming
    /// from other peers, use `continue_propagation`.
    async fn add_transaction(&self, transaction: RpcTransaction) -> ClientResult<()>;

    /// Continues the propagation of a transaction we've received from another peer.
    async fn continue_propagation(
        &self,
        propagation_manager: BroadcastedMessageManager,
    ) -> ClientResult<()>;
}

pub type SharedMempoolP2PSenderClient = Arc<dyn MempoolP2PSenderClient>;

pub enum MempoolP2PSenderRequest {
    AddTransaction(RpcTransaction),
    ContinuePropagation(BroadcastedMessageManager),
}

pub struct MempoolP2PSenderResponse;

#[async_trait]
impl ComponentRequestHandler<MempoolP2PSenderRequest, MempoolP2PSenderResponse>
    for MempoolP2PSender
{
    async fn handle_request(
        &mut self,
        _request: MempoolP2PSenderRequest,
    ) -> MempoolP2PSenderResponse {
        unimplemented!()
    }
}

#[async_trait]
impl MempoolP2PSenderClient
    for LocalComponentClient<MempoolP2PSenderRequest, MempoolP2PSenderResponse>
{
    async fn add_transaction(&self, transaction: RpcTransaction) -> ClientResult<()> {
        self.send(MempoolP2PSenderRequest::AddTransaction(transaction)).await;
        Ok(())
    }

    async fn continue_propagation(
        &self,
        propagation_manager: BroadcastedMessageManager,
    ) -> ClientResult<()> {
        self.send(MempoolP2PSenderRequest::ContinuePropagation(propagation_manager)).await;
        Ok(())
    }
}

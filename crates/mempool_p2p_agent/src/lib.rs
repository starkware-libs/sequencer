use std::future::pending;
use std::sync::Arc;

use async_trait::async_trait;
pub use papyrus_network::network_manager::BroadcastedMessageManager;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_mempool_infra::component_client::{ClientResult, LocalComponentClient};
use starknet_mempool_infra::component_definitions::ComponentRequestHandler;
use starknet_mempool_infra::component_runner::{ComponentStartError, ComponentStarter};

pub struct _MempoolP2PAgent;

#[async_trait]
pub trait MempoolP2PAgentClient: Send + Sync {
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

pub type SharedMempoolP2PAgentClient = Arc<dyn MempoolP2PAgentClient>;

#[async_trait]
impl ComponentStarter for _MempoolP2PAgent {
    async fn start(&mut self) -> Result<(), ComponentStartError> {
        // TODO: implement this and remove the pending.
        let () = pending().await;
        Ok(())
    }
}

pub enum MempoolP2PAgentRequest {
    AddTransaction(RpcTransaction),
    ContinuePropagation(BroadcastedMessageManager),
}

pub struct MempoolP2PAgentResponse;

#[async_trait]
impl ComponentRequestHandler<MempoolP2PAgentRequest, MempoolP2PAgentResponse> for _MempoolP2PAgent {
    async fn handle_request(
        &mut self,
        _request: MempoolP2PAgentRequest,
    ) -> MempoolP2PAgentResponse {
        // TODO: implement this
        MempoolP2PAgentResponse
    }
}

#[async_trait]
impl MempoolP2PAgentClient
    for LocalComponentClient<MempoolP2PAgentRequest, MempoolP2PAgentResponse>
{
    async fn add_transaction(&self, transaction: RpcTransaction) -> ClientResult<()> {
        self.send(MempoolP2PAgentRequest::AddTransaction(transaction)).await;
        Ok(())
    }

    async fn continue_propagation(
        &self,
        propagation_manager: BroadcastedMessageManager,
    ) -> ClientResult<()> {
        self.send(MempoolP2PAgentRequest::ContinuePropagation(propagation_manager)).await;
        Ok(())
    }
}

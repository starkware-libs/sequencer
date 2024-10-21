#[cfg(test)]
mod test;

use async_trait::async_trait;
use papyrus_network::network_manager::{BroadcastTopicClient, BroadcastTopicClientTrait};
use papyrus_network_types::network_types::BroadcastedMessageMetadata;
use papyrus_protobuf::mempool::RpcTransactionWrapper;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_mempool_p2p_types::communication::{
    MempoolP2pPropagatorClient,
    MempoolP2pPropagatorClientResult,
    MempoolP2pPropagatorRequest,
    MempoolP2pPropagatorResponse,
};
use starknet_mempool_p2p_types::errors::MempoolP2pPropagatorError;
use starknet_sequencer_infra::component_definitions::ComponentRequestHandler;

pub struct MempoolP2pPropagator {
    broadcast_topic_client: BroadcastTopicClient<RpcTransactionWrapper>,
}

impl MempoolP2pPropagator {
    pub fn new(broadcast_topic_client: BroadcastTopicClient<RpcTransactionWrapper>) -> Self {
        Self { broadcast_topic_client }
    }
}

#[async_trait]
impl ComponentRequestHandler<MempoolP2pPropagatorRequest, MempoolP2pPropagatorResponse>
    for MempoolP2pPropagator
{
    async fn handle_request(
        &mut self,
        request: MempoolP2pPropagatorRequest,
    ) -> MempoolP2pPropagatorResponse {
        match request {
            MempoolP2pPropagatorRequest::AddTransaction(transaction) => {
                let result = self
                    .broadcast_topic_client
                    .broadcast_message(RpcTransactionWrapper(transaction))
                    .await
                    .map_err(|_| MempoolP2pPropagatorError::NetworkSendError);
                MempoolP2pPropagatorResponse::AddTransaction(result)
            }
            MempoolP2pPropagatorRequest::ContinuePropagation(propagation_manager) => {
                let result = self
                    .broadcast_topic_client
                    .continue_propagation(&propagation_manager)
                    .await
                    .map_err(|_| MempoolP2pPropagatorError::NetworkSendError);
                MempoolP2pPropagatorResponse::ContinuePropagation(result)
            }
        }
    }
}

pub struct EmptyMempoolP2pPropagatorClient;

#[async_trait]
impl MempoolP2pPropagatorClient for EmptyMempoolP2pPropagatorClient {
    async fn add_transaction(
        &self,
        _transaction: RpcTransaction,
    ) -> MempoolP2pPropagatorClientResult<()> {
        Ok(())
    }

    async fn continue_propagation(
        &self,
        _propagation_manager: BroadcastedMessageMetadata,
    ) -> MempoolP2pPropagatorClientResult<()> {
        Ok(())
    }
}

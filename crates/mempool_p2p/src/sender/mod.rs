#[cfg(test)]
mod test;

use async_trait::async_trait;
use papyrus_network::network_manager::{BroadcastTopicClient, BroadcastTopicClientTrait};
use papyrus_protobuf::mempool::RpcTransactionWrapper;
use starknet_mempool_infra::component_definitions::ComponentRequestHandler;
use starknet_mempool_p2p_types::communication::{
    MempoolP2pSenderRequest,
    MempoolP2pSenderResponse,
};
use starknet_mempool_p2p_types::errors::MempoolP2pSenderError;

pub struct MempoolP2pSender {
    broadcast_topic_client: BroadcastTopicClient<RpcTransactionWrapper>,
}

impl MempoolP2pSender {
    pub fn new(broadcast_topic_client: BroadcastTopicClient<RpcTransactionWrapper>) -> Self {
        Self { broadcast_topic_client }
    }
}

#[async_trait]
impl ComponentRequestHandler<MempoolP2pSenderRequest, MempoolP2pSenderResponse>
    for MempoolP2pSender
{
    async fn handle_request(
        &mut self,
        request: MempoolP2pSenderRequest,
    ) -> MempoolP2pSenderResponse {
        match request {
            MempoolP2pSenderRequest::AddTransaction(transaction) => {
                let result = self
                    .broadcast_topic_client
                    .broadcast_message(RpcTransactionWrapper(transaction))
                    .await
                    .map_err(|_| MempoolP2pSenderError::NetworkSendError);
                MempoolP2pSenderResponse::AddTransaction(result)
            }
            MempoolP2pSenderRequest::ContinuePropagation(propagation_manager) => {
                let result = self
                    .broadcast_topic_client
                    .continue_propagation(&propagation_manager)
                    .await
                    .map_err(|_| MempoolP2pSenderError::NetworkSendError);
                MempoolP2pSenderResponse::ContinuePropagation(result)
            }
        }
    }
}

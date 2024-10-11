use async_trait::async_trait;
use papyrus_network::network_manager::BroadcastTopicClient;
use papyrus_protobuf::mempool::RpcTransactionWrapper;
use starknet_mempool_infra::component_definitions::ComponentRequestHandler;
use starknet_mempool_p2p_types::communication::{
    MempoolP2pSenderRequest,
    MempoolP2pSenderResponse,
};

pub struct MempoolP2pSender {
    #[allow(dead_code)]
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
        _request: MempoolP2pSenderRequest,
    ) -> MempoolP2pSenderResponse {
        unimplemented!()
    }
}

use papyrus_network::network_manager::{
    BroadcastTopicClient,
    BroadcastTopicServer,
    NetworkManager,
};
use papyrus_protobuf::mempool::RpcTransactionWrapper;
use starknet_gateway_types::communication::SharedGatewayClient;
use starknet_mempool_infra::component_definitions::ComponentStarter;

pub struct MempoolP2pReceiver {
    #[allow(dead_code)]
    network_manager: Option<NetworkManager>,
    #[allow(dead_code)]
    broadcasted_messages_server: BroadcastTopicServer<RpcTransactionWrapper>,
    #[allow(dead_code)]
    broadcast_messages_client: BroadcastTopicClient<RpcTransactionWrapper>,
    #[allow(dead_code)]
    gateway_client: SharedGatewayClient,
}

impl MempoolP2pReceiver {
    pub fn new(
        network_manager: Option<NetworkManager>,
        broadcasted_messages_server: BroadcastTopicServer<RpcTransactionWrapper>,
        broadcast_messages_client: BroadcastTopicClient<RpcTransactionWrapper>,
        gateway_client: SharedGatewayClient,
    ) -> Self {
        Self {
            network_manager,
            broadcasted_messages_server,
            broadcast_messages_client,
            gateway_client,
        }
    }
}

impl ComponentStarter for MempoolP2pReceiver {}

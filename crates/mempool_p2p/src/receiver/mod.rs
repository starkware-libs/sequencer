use async_trait::async_trait;
use futures::StreamExt;
use papyrus_network::network_manager::{
    BroadcastTopicClient,
    BroadcastTopicClientTrait,
    BroadcastTopicServer,
    NetworkManager,
};
use papyrus_protobuf::mempool::RpcTransactionWrapper;
use starknet_mempool_infra::component_definitions::ComponentStarter;
use starknet_mempool_infra::errors::ComponentError;

pub struct MempoolP2pReceiver {
    #[allow(dead_code)]
    network_manager: Option<NetworkManager>,
    #[allow(dead_code)]
    broadcasted_messages_server: BroadcastTopicServer<RpcTransactionWrapper>,
    #[allow(dead_code)]
    broadcast_messages_client: BroadcastTopicClient<RpcTransactionWrapper>,
}

impl MempoolP2pReceiver {
    pub fn new(
        network_manager: Option<NetworkManager>,
        broadcasted_messages_server: BroadcastTopicServer<RpcTransactionWrapper>,
        broadcast_messages_client: BroadcastTopicClient<RpcTransactionWrapper>,
    ) -> Self {
        Self { network_manager, broadcasted_messages_server, broadcast_messages_client }
    }
}

#[async_trait]
impl ComponentStarter for MempoolP2pReceiver {
    async fn start(&mut self) -> Result<(), ComponentError> {
        loop {
            let network = self.network_manager.take().expect("Network manager is missing");
            tokio::select! {
                _ = network.run() => panic!("Mempool Network Manager has unexpectedly ended"),
                Some((maybe_message, broadcasted_message_manager)) = self.broadcasted_messages_server.next() => {
                    match maybe_message {
                        Ok(_message) => {},
                        Err(_e) => {
                            let _ = self.broadcast_messages_client.report_peer(broadcasted_message_manager).await;
                        }
                    }
                }
            }
        }
    }
}

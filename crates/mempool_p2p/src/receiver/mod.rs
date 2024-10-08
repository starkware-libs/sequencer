#[cfg(test)]
mod test;

use async_trait::async_trait;
use futures::{StreamExt, TryFutureExt};
use papyrus_network::network_manager::{
    BroadcastTopicClient,
    BroadcastTopicClientTrait,
    BroadcastTopicServer,
    NetworkManager,
};
use papyrus_protobuf::mempool::RpcTransactionWrapper;
use starknet_gateway_types::communication::SharedGatewayClient;
use starknet_gateway_types::gateway_types::GatewayInput;
use starknet_mempool_infra::component_definitions::ComponentStarter;
use starknet_mempool_infra::errors::ComponentError;
use tracing::warn;

pub struct MempoolP2pReceiver {
    network_manager: Option<NetworkManager>,
    broadcasted_messages_server: BroadcastTopicServer<RpcTransactionWrapper>,
    broadcast_messages_client: BroadcastTopicClient<RpcTransactionWrapper>,
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

#[async_trait]
impl ComponentStarter for MempoolP2pReceiver {
    async fn start(&mut self) -> Result<(), ComponentError> {
        tokio::spawn(
            self.network_manager
                .take()
                .expect("Network manager not configured")
                .run()
                .map_err(|e| panic!("Network manager failed: {:?}", e)),
        );
        loop {
            tokio::select! {
                Some((maybe_message, broadcasted_message_manager)) = self.broadcasted_messages_server.next() => {
                    match maybe_message {
                        Ok(message) => {
                            //TODO(eitan): Add message metadata.
                            match self.gateway_client.add_tx(GatewayInput { rpc_tx: message.0, message_metadata: None }).await {
                                Ok(_tx_hash) => {}
                                Err(e) => {
                                    warn!("Failed to forward transaction from p2p receiver to gateway: {:?}", e);
                                    if let Err(e) = self.broadcast_messages_client.report_peer(broadcasted_message_manager).await {
                                        warn!("Failed to report peer: {:?}", e);
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            warn!("Failed to receive transaction from network: {:?}", e);
                            if let Err(e) = self.broadcast_messages_client.report_peer(broadcasted_message_manager).await {
                                warn!("Failed to report peer: {:?}", e);
                            }
                        }
                    }
                }
            }
        }
    }
}

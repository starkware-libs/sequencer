#[cfg(test)]
mod test;

use async_trait::async_trait;
use futures::{pin_mut, FutureExt, StreamExt, TryFutureExt};
use papyrus_network::network_manager::{
    BroadcastTopicClient,
    BroadcastTopicClientTrait,
    BroadcastTopicServer,
    NetworkManager,
};
use papyrus_protobuf::mempool::RpcTransactionWrapper;
use starknet_gateway_types::communication::SharedGatewayClient;
use starknet_gateway_types::gateway_types::GatewayInput;
use starknet_sequencer_infra::component_definitions::ComponentStarter;
use starknet_sequencer_infra::errors::ComponentError;
use tracing::warn;

pub struct MempoolP2pRunner {
    network_manager: Option<NetworkManager>,
    broadcasted_topic_server: BroadcastTopicServer<RpcTransactionWrapper>,
    broadcast_topic_client: BroadcastTopicClient<RpcTransactionWrapper>,
    gateway_client: SharedGatewayClient,
}

impl MempoolP2pRunner {
    pub fn new(
        network_manager: Option<NetworkManager>,
        broadcasted_topic_server: BroadcastTopicServer<RpcTransactionWrapper>,
        broadcast_topic_client: BroadcastTopicClient<RpcTransactionWrapper>,
        gateway_client: SharedGatewayClient,
    ) -> Self {
        Self { network_manager, broadcasted_topic_server, broadcast_topic_client, gateway_client }
    }
}

#[async_trait]
impl ComponentStarter for MempoolP2pRunner {
    async fn start(&mut self) -> Result<(), ComponentError> {
        let network_future = self
            .network_manager
            .take()
            .expect("Network manager not found")
            .run()
            .map_err(|_| ComponentError::InternalComponentError);
        pin_mut!(network_future);
        loop {
            tokio::select! {
                // tokio::select! takes ownership of the futures, so we need to wrap with poll_fn
                result = futures::future::poll_fn(|cx| network_future.poll_unpin(cx)) => {
                    result?;
                    panic!("Network stopped unexpectedly");
                }
                // TODO(eitan): Extract the logic into a handle_broadcasted_message method
                Some((message_result, broadcasted_message_manager)) = self.broadcasted_topic_server.next() => {
                    match message_result {
                        Ok(message) => {
                            // TODO(eitan): Add message metadata.
                            // TODO(eitan): make this call non blocking by adding this future to a
                            // FuturesUnordered that will be polled in the select.
                            match self.gateway_client.add_tx(
                                GatewayInput { rpc_tx: message.0, message_metadata: None }
                            ).await {
                                Ok(_tx_hash) => {}
                                Err(e) => {
                                    warn!(
                                        "Failed to forward transaction from MempoolP2pRunner to gateway: {:?}", e);
                                    if let Err(e) = self.broadcast_topic_client.report_peer(broadcasted_message_manager).await {
                                        warn!("Failed to report peer: {:?}", e);
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            warn!("Received a faulty transaction from network: {:?}. Attempting to report the sending peer", e);
                            if let Err(e) = self.broadcast_topic_client.report_peer(broadcasted_message_manager).await {
                                warn!("Failed to report peer: {:?}", e);
                            }
                        }
                    }
                }
            }
        }
    }
}

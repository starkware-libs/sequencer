#[cfg(test)]
mod test;

use std::time::Duration;

use async_trait::async_trait;
use futures::future::BoxFuture;
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use papyrus_network::network_manager::{
    BroadcastTopicClient,
    BroadcastTopicClientTrait,
    BroadcastTopicServer,
    NetworkError,
};
use papyrus_protobuf::mempool::RpcTransactionBatch;
use starknet_gateway_types::communication::{GatewayClientError, SharedGatewayClient};
use starknet_gateway_types::errors::GatewayError;
use starknet_gateway_types::gateway_types::GatewayInput;
use starknet_mempool_p2p_types::communication::SharedMempoolP2pPropagatorClient;
use starknet_sequencer_infra::component_definitions::ComponentStarter;
use starknet_sequencer_infra::component_server::WrapperServer;
use tokio::time::MissedTickBehavior::Delay;
use tracing::{debug, info, warn};

pub struct MempoolP2pRunner {
    network_future: BoxFuture<'static, Result<(), NetworkError>>,
    broadcasted_topic_server: BroadcastTopicServer<RpcTransactionBatch>,
    broadcast_topic_client: BroadcastTopicClient<RpcTransactionBatch>,
    gateway_client: SharedGatewayClient,
    _mempool_p2p_propagator_client: SharedMempoolP2pPropagatorClient,
    transaction_batch_rate_millis: Duration,
}

impl MempoolP2pRunner {
    pub fn new(
        network_future: BoxFuture<'static, Result<(), NetworkError>>,
        broadcasted_topic_server: BroadcastTopicServer<RpcTransactionBatch>,
        broadcast_topic_client: BroadcastTopicClient<RpcTransactionBatch>,
        gateway_client: SharedGatewayClient,
        mempool_p2p_propagator_client: SharedMempoolP2pPropagatorClient,
        transaction_batch_rate_millis: Duration,
    ) -> Self {
        Self {
            network_future,
            broadcasted_topic_server,
            broadcast_topic_client,
            gateway_client,
            _mempool_p2p_propagator_client: mempool_p2p_propagator_client,
            transaction_batch_rate_millis,
        }
    }
}

#[async_trait]
impl ComponentStarter for MempoolP2pRunner {
    async fn start(&mut self) {
        let mut gateway_futures = FuturesUnordered::new();
        let mut transaction_batch_broadcast_interval =
            tokio::time::interval(self.transaction_batch_rate_millis);
        transaction_batch_broadcast_interval.set_missed_tick_behavior(Delay);
        loop {
            tokio::select! {
                _ = &mut self.network_future => {
                    panic!("MempoolP2pRunner failed - network stopped unexpectedly");
                }
                _ = transaction_batch_broadcast_interval.tick() => {
                    if (self._mempool_p2p_propagator_client.broadcast_queued_transactions().await).is_err() {
                        warn!("MempoolP2pPropagatorClient denied BroadcastQueuedTransactions request");
                    };
                }
                Some(result) = gateway_futures.next() => {
                    match result {
                        Ok(_) => {}
                        Err(gateway_client_error) => {
                            // TODO(shahak): Analyze the error to see if it's the tx's fault or an
                            // internal error. Widen GatewayError's variants if necessary.
                            if let GatewayClientError::GatewayError(
                                GatewayError::GatewaySpecError{p2p_message_metadata: Some(p2p_message_metadata), ..}
                            ) = gateway_client_error {
                                warn!(
                                    "Gateway rejected transaction we received from another peer. Reporting peer."
                                );
                                if let Err(e) = self.broadcast_topic_client.report_peer(p2p_message_metadata.clone()).await {
                                    warn!("Failed to report peer: {:?}", e);
                                }
                            } else {
                                warn!(
                                    "Failed sending transaction to gateway. {:?}",
                                    gateway_client_error
                                );
                            }
                        }
                    }
                }
                Some((message_result, broadcasted_message_metadata)) = self.broadcasted_topic_server.next() => {
                    match message_result {
                        Ok(message) => {
                            // TODO(alonl): consider calculating the tx_hash and pringing it instead of the entire tx.
                            info!("Received transaction from network, forwarding to gateway");
                            debug!("received transaction: {:?}", message.0);
                            gateway_futures.push(self.gateway_client.add_txs(
                                GatewayInput { transactions: message.0, message_metadata: Some(broadcasted_message_metadata) }
                            ))
                        }
                        Err(e) => {
                            warn!("Received a faulty transaction from network: {:?}. Attempting to report the sending peer", e);
                            if let Err(e) = self.broadcast_topic_client.report_peer(broadcasted_message_metadata).await {
                                warn!("Failed to report peer: {:?}", e);
                            }
                        }
                    }
                }
            }
        }
    }
}

pub type MempoolP2pRunnerServer = WrapperServer<MempoolP2pRunner>;

#[cfg(test)]
mod test;

use std::time::Duration;

use apollo_gateway_types::communication::{
    GatewayClientError,
    GatewayClientResult,
    SharedGatewayClient,
};
use apollo_gateway_types::errors::GatewayError;
use apollo_gateway_types::gateway_types::{GatewayInput, GatewayOutput};
use apollo_infra::component_definitions::ComponentStarter;
use apollo_infra::component_server::WrapperServer;
use apollo_mempool_p2p_types::communication::SharedMempoolP2pPropagatorClient;
use apollo_network::network_manager::{
    BroadcastTopicClient,
    BroadcastTopicClientTrait,
    BroadcastTopicServer,
    NetworkError,
};
use apollo_protobuf::mempool::RpcTransactionBatch;
use async_trait::async_trait;
use futures::future::BoxFuture;
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use tokio::time::MissedTickBehavior::Delay;
use tracing::{debug, warn};

pub struct MempoolP2pRunner {
    network_handle: tokio::task::JoinHandle<Result<(), NetworkError>>,
    broadcasted_topic_server: BroadcastTopicServer<RpcTransactionBatch>,
    broadcast_topic_client: BroadcastTopicClient<RpcTransactionBatch>,
    gateway_client: SharedGatewayClient,
    mempool_p2p_propagator_client: SharedMempoolP2pPropagatorClient,
    transaction_batch_rate_millis: Duration,
    max_concurrent_gateway_requests: usize,
}

impl MempoolP2pRunner {
    pub fn new(
        network_future: BoxFuture<'static, Result<(), NetworkError>>,
        broadcasted_topic_server: BroadcastTopicServer<RpcTransactionBatch>,
        broadcast_topic_client: BroadcastTopicClient<RpcTransactionBatch>,
        gateway_client: SharedGatewayClient,
        mempool_p2p_propagator_client: SharedMempoolP2pPropagatorClient,
        transaction_batch_rate_millis: Duration,
        max_concurrent_gateway_requests: usize,
    ) -> Self {
        Self {
            // Wrap network_future in spawn to make it cancel-safe.
            network_handle: tokio::task::spawn(network_future),
            broadcasted_topic_server,
            broadcast_topic_client,
            gateway_client,
            mempool_p2p_propagator_client,
            transaction_batch_rate_millis,
            max_concurrent_gateway_requests,
        }
    }
}

#[async_trait]
impl ComponentStarter for MempoolP2pRunner {
    async fn start(&mut self) {
        let mut gateway_futures: FuturesUnordered<
            tokio::task::JoinHandle<GatewayClientResult<GatewayOutput>>,
        > = FuturesUnordered::new();
        let mut concurrent_gateway_requests = 0;
        let mut transaction_batch_broadcast_interval =
            tokio::time::interval(self.transaction_batch_rate_millis);
        transaction_batch_broadcast_interval.set_missed_tick_behavior(Delay);
        transaction_batch_broadcast_interval.tick().await; // The first tick is ready immediately so we consume it.
        loop {
            tokio::select! {
                // Cancel-safe: JoinHandle::poll is cancel-safe.
                _ = &mut self.network_handle => {
                    panic!("MempoolP2pRunner failed - network stopped unexpectedly");
                }
                // Cancel-safe: Interval::tick() is cancel-safe per tokio docs.
                _ = transaction_batch_broadcast_interval.tick() => {
                    let result = self.mempool_p2p_propagator_client.broadcast_queued_transactions().await;
                    if result.is_err() {
                        warn!("MempoolP2pPropagatorClient denied BroadcastQueuedTransactions request: {result:?}");
                    };
                }
                // Cancel-safe: FuturesUnordered::next() is cancel-safe as long as the futures
                // inside it are cancel-safe. The futures inside are wrapped in a tokio task which
                // makes them cancel-safe as well.
                Some(join_result) = gateway_futures.next() => {
                    concurrent_gateway_requests -= 1;
                    let result = join_result
                        .expect("Gateway client add_tx task should not panic");
                    match result {
                        Ok(_) => {}
                        Err(gateway_client_error) => {
                            // TODO(shahak): Analyze the error to see if it's the tx's fault or an
                            // internal error. Widen GatewayError's variants if necessary.
                            if let GatewayClientError::GatewayError(
                                GatewayError::DeprecatedGatewayError{p2p_message_metadata: Some(p2p_message_metadata), ..}
                            ) = gateway_client_error {
                                warn!(
                                    "Gateway rejected transaction we received from another peer. Reporting peer: {:?}.", p2p_message_metadata
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
                // Cancel-safe: mpsc Receiver::recv() is cancel-safe per tokio docs.
                Some((message_result, broadcasted_message_metadata)) = self.broadcasted_topic_server.next() => {
                    match message_result {
                        Ok(message) => {
                            // TODO(alonl): consider calculating the tx_hash and printing it instead of the entire tx.
                            debug!("Received transaction batch from network, forwarding to gateway. Batch: {:?}", message.0);
                            for rpc_tx in message.0 {
                                if concurrent_gateway_requests == self.max_concurrent_gateway_requests {
                                    warn!("Rejecting transaction due to backpressure. Transaction: {:?}", rpc_tx);
                                    continue;
                                }

                                // Wrap in tokio::spawn to make the gateway request cancel-safe.
                                // Without this, the tokio::select! can cancel the add_tx future
                                // mid-HTTP-request, causing the gateway to have a dead connection.
                                let gateway_client = self.gateway_client.clone();
                                let message_metadata = Some(broadcasted_message_metadata.clone());
                                gateway_futures.push(tokio::spawn(async move {
                                    gateway_client.add_tx(
                                        GatewayInput { rpc_tx, message_metadata }
                                    ).await
                                }));
                                concurrent_gateway_requests += 1;
                            }
                        }
                        Err(e) => {
                            warn!("Received a faulty transaction from network: {:?}. Attempting to report the sending peer {:?}", e, broadcasted_message_metadata);
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

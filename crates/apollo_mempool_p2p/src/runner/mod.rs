#[cfg(test)]
mod test;

use std::sync::Arc;
use std::time::Duration;

use apollo_gateway_types::communication::{GatewayClientError, SharedGatewayClient};
use apollo_gateway_types::errors::GatewayError;
use apollo_gateway_types::gateway_types::GatewayInput;
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
use tokio::sync::Semaphore;
use tokio::time::MissedTickBehavior::Delay;
use tracing::{debug, warn};

pub struct MempoolP2pRunner {
    network_future: BoxFuture<'static, Result<(), NetworkError>>,
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
            network_future,
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
        let gateway_semaphore = Arc::new(Semaphore::new(self.max_concurrent_gateway_requests));
        let mut gateway_futures = FuturesUnordered::new();
        let mut broadcast_queued_txs_handle =
            tokio::spawn(broadcast_queued_transactions_every_tick(
                self.mempool_p2p_propagator_client.clone(),
                self.transaction_batch_rate_millis,
            ));
        loop {
            tokio::select! {
                res = &mut self.network_future => {
                    res.expect("Mempool P2P network failed");
                    unreachable!("Network manager's run should never return");
                }
                res = &mut broadcast_queued_txs_handle => {
                    res.expect("Broadcast task panicked");
                    unreachable!("The broadcast task runs an infinite loop, so it should never return");
                }
                Some(result) = gateway_futures.next() => {
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
                Some((message_result, broadcasted_message_metadata)) = self.broadcasted_topic_server.next() => {
                    match message_result {
                        Ok(message) => {
                            // TODO(alonl): consider calculating the tx_hash and printing it instead of the entire tx.
                            debug!("Received transaction batch from network, forwarding to gateway. Batch: {:?}", message.0);
                            for rpc_tx in message.0 {
                                let permit = match gateway_semaphore.clone().try_acquire_owned() {
                                    Ok(permit) => permit,
                                    Err(_) => {
                                        warn!(
                                            "Rejecting transaction due to backpressure. \
                                             Transaction: {rpc_tx:?}"
                                        );
                                        continue;
                                    }
                                };
                                let gateway_client = self.gateway_client.clone();
                                let message_metadata = Some(broadcasted_message_metadata.clone());
                                gateway_futures.push(async move {
                                    let _permit = permit;
                                    gateway_client.add_tx(
                                        GatewayInput { rpc_tx, message_metadata }
                                    ).await
                                });
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

async fn broadcast_queued_transactions_every_tick(
    client: SharedMempoolP2pPropagatorClient,
    rate: Duration,
) {
    let mut interval = tokio::time::interval(rate);
    interval.set_missed_tick_behavior(Delay);
    interval.tick().await; // The first tick is ready immediately so we consume it.
    loop {
        interval.tick().await;
        let result = client.broadcast_queued_transactions().await;
        if let Err(err) = result {
            warn!(
                "MempoolP2pPropagatorClient denied BroadcastQueuedTransactions request. Will \
                 retry at the next interval. Error: {err:?}"
            );
        }
    }
}

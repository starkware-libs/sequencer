#[cfg(test)]
mod test;

use std::sync::Arc;
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
use apollo_network_types::network_types::BroadcastedMessageMetadata;
use apollo_protobuf::mempool::RpcTransactionBatch;
use starknet_api::rpc_transaction::RpcTransaction;
use async_trait::async_trait;
use futures::future::{BoxFuture, Future};
use futures::StreamExt;
use tokio::sync::Semaphore;
use tokio::time::MissedTickBehavior::Delay;
use tracing::{debug, warn};

pub struct MempoolP2pRunner {
    network_future: Option<BoxFuture<'static, Result<(), NetworkError>>>,
    broadcasted_topic_server: Option<BroadcastTopicServer<RpcTransactionBatch>>,
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
            network_future: Some(network_future),
            broadcasted_topic_server: Some(broadcasted_topic_server),
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
        let network_future =
            self.network_future.take().expect("start() should only be called once");
        let network_handle = tokio::task::spawn(network_future);

        let broadcast_queued_txs_handle = tokio::spawn(broadcast_queued_transactions_every_tick(
            self.mempool_p2p_propagator_client.clone(),
            self.transaction_batch_rate_millis,
        ));

        let broadcasted_topic_server =
            self.broadcasted_topic_server.take().expect("start() should only be called once");
        let incoming_messages_handle = tokio::spawn(listen_and_handle_incoming_p2p_messages(
            broadcasted_topic_server,
            self.gateway_client.clone(),
            self.broadcast_topic_client.clone(),
            self.max_concurrent_gateway_requests,
        ));

        tokio::select! {
            res = network_handle => {
                let _ = res.expect("Network future panicked");
                panic!("MempoolP2pRunner failed - network stopped unexpectedly");
            }
            res = broadcast_queued_txs_handle => {
                let _ = res.expect("Broadcast task panicked");
                panic!("broadcast_queued_transactions_every_tick unexpectedly stopped");
            }
            res = incoming_messages_handle => {
                let _ = res.expect("Messages task panicked");
                panic!("listen_and_handle_incoming_p2p_messages unexpectedly stopped");
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

async fn listen_and_handle_incoming_p2p_messages(
    mut broadcasted_topic_server: BroadcastTopicServer<RpcTransactionBatch>,
    gateway_client: SharedGatewayClient,
    mut broadcast_topic_client: BroadcastTopicClient<RpcTransactionBatch>,
    max_concurrent_gateway_requests: usize,
) {
    let gateway_semaphore = Arc::new(Semaphore::new(max_concurrent_gateway_requests));
    let mut gateway_futures = tokio::task::JoinSet::new();

    while let Some((message_result, broadcasted_message_metadata)) =
        broadcasted_topic_server.next().await
    {
        while let Some(join_result) = gateway_futures.try_join_next() {
            join_result.expect("Gateway client add_tx task should not panic");
        }

        match message_result {
            Ok(message) => {
                // TODO(alonl): consider calculating the tx_hash and printing it instead of the
                // entire tx.
                debug!(
                    "Received transaction batch from network, forwarding to gateway. Batch: {:?}",
                    message.0
                );
                for rpc_tx in message.0 {
                    if let Some(future) = create_gateway_future_for_incoming_tx(
                        rpc_tx,
                        gateway_client.clone(),
                        &gateway_semaphore,
                        broadcast_topic_client.clone(),
                        broadcasted_message_metadata.clone(),
                    ) {
                        gateway_futures.spawn(future);
                    }
                }
            }
            Err(e) => {
                warn!(
                    "Received a faulty transaction from network: {:?}. Attempting to report the \
                     sending peer {:?}",
                    e, broadcasted_message_metadata
                );
                if let Err(e) =
                    broadcast_topic_client.report_peer(broadcasted_message_metadata).await
                {
                    warn!("Failed to report peer: {:?}", e);
                }
            }
        }
    }
}

fn create_gateway_future_for_incoming_tx(
    rpc_tx: RpcTransaction,
    gateway_client: SharedGatewayClient,
    gateway_semaphore: &Arc<Semaphore>,
    broadcast_topic_client: BroadcastTopicClient<RpcTransactionBatch>,
    broadcasted_message_metadata: BroadcastedMessageMetadata,
) -> Option<impl Future<Output = ()>> {
    let permit = match gateway_semaphore.clone().try_acquire_owned() {
        Ok(permit) => permit,
        Err(_) => {
            warn!("Rejecting transaction due to backpressure. Transaction: {:?}", rpc_tx);
            return None;
        }
    };
    let message_metadata = Some(broadcasted_message_metadata);
    Some(async move {
        let _permit = permit;
        let result = gateway_client.add_tx(GatewayInput { rpc_tx, message_metadata }).await;
        handle_gateway_result(result, broadcast_topic_client).await;
    })
}

async fn handle_gateway_result(
    result: GatewayClientResult<GatewayOutput>,
    mut broadcast_topic_client: BroadcastTopicClient<RpcTransactionBatch>,
) {
    match result {
        Ok(_) => {}
        Err(gateway_client_error) => {
            // TODO(shahak): Analyze the error to see if it's the tx's fault or an
            // internal error. Widen GatewayError's variants if necessary.
            if let GatewayClientError::GatewayError(GatewayError::DeprecatedGatewayError {
                p2p_message_metadata: Some(p2p_message_metadata),
                ..
            }) = gateway_client_error
            {
                warn!(
                    "Gateway rejected transaction we received from another peer. Reporting peer: \
                     {:?}.",
                    p2p_message_metadata
                );
                if let Err(e) =
                    broadcast_topic_client.report_peer(p2p_message_metadata.clone()).await
                {
                    warn!("Failed to report peer: {:?}", e);
                }
            } else {
                warn!("Failed sending transaction to gateway. {:?}", gateway_client_error);
            }
        }
    }
}

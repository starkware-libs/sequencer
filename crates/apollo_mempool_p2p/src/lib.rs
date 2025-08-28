pub mod config;
pub mod metrics;
pub mod propagator;
pub mod runner;

use std::collections::HashMap;

use apollo_class_manager_types::transaction_converter::TransactionConverter;
use apollo_class_manager_types::SharedClassManagerClient;
use apollo_gateway_types::communication::SharedGatewayClient;
use apollo_mempool_p2p_types::communication::SharedMempoolP2pPropagatorClient;
use apollo_network::gossipsub_impl::Topic;
use apollo_network::network_manager::metrics::{
    BroadcastNetworkErrorMetrics,
    BroadcastNetworkMetrics,
    NetworkMetrics,
};
use apollo_network::network_manager::{BroadcastTopicChannels, NetworkManager};
use futures::FutureExt;
use metrics::MEMPOOL_P2P_NUM_BLACKLISTED_PEERS;
use tracing::{info_span, Instrument};

use crate::config::MempoolP2pConfig;
use crate::metrics::{
    MEMPOOL_P2P_NUM_CONNECTED_PEERS,
    MEMPOOL_P2P_NUM_NO_PEERS_SUBSCRIBED_ERRORS,
    MEMPOOL_P2P_NUM_RECEIVED_MESSAGES,
    MEMPOOL_P2P_NUM_SENT_MESSAGES,
};
use crate::propagator::MempoolP2pPropagator;
use crate::runner::MempoolP2pRunner;

pub const MEMPOOL_TOPIC: &str = "apollo_mempool_transaction_propagation/0.1.0";

pub fn create_p2p_propagator_and_runner(
    mempool_p2p_config: MempoolP2pConfig,
    gateway_client: SharedGatewayClient,
    class_manager_client: SharedClassManagerClient,
    mempool_p2p_propagator_client: SharedMempoolP2pPropagatorClient,
) -> (MempoolP2pPropagator, MempoolP2pRunner) {
    let transaction_converter = TransactionConverter::new(
        class_manager_client.clone(),
        mempool_p2p_config.network_config.chain_id.clone(),
    );
    let mut broadcast_metrics_by_topic = HashMap::new();
    broadcast_metrics_by_topic.insert(
        Topic::new(MEMPOOL_TOPIC).hash(),
        BroadcastNetworkMetrics {
            num_sent_broadcast_messages: MEMPOOL_P2P_NUM_SENT_MESSAGES,
            num_received_broadcast_messages: MEMPOOL_P2P_NUM_RECEIVED_MESSAGES,
            error_metrics: BroadcastNetworkErrorMetrics {
                num_no_peers_subscribed_errors: MEMPOOL_P2P_NUM_NO_PEERS_SUBSCRIBED_ERRORS,
            },
        },
    );
    let network_manager_metrics = Some(NetworkMetrics {
        num_connected_peers: MEMPOOL_P2P_NUM_CONNECTED_PEERS,
        num_blacklisted_peers: MEMPOOL_P2P_NUM_BLACKLISTED_PEERS,
        broadcast_metrics_by_topic: Some(broadcast_metrics_by_topic),
        sqmr_metrics: None,
    });
    let mut network_manager = NetworkManager::new(
        mempool_p2p_config.network_config,
        // TODO(Shahak): Consider filling this once the sequencer node has a name.
        None,
        network_manager_metrics,
    );
    let BroadcastTopicChannels { broadcasted_messages_receiver, broadcast_topic_client } =
        network_manager
            .register_broadcast_topic(
                Topic::new(MEMPOOL_TOPIC),
                mempool_p2p_config.network_buffer_size,
            )
            .expect("Failed to register broadcast topic");
    let network_future = network_manager.run().instrument(info_span!("[Mempool network]"));
    let mempool_p2p_propagator = MempoolP2pPropagator::new(
        broadcast_topic_client.clone(),
        Box::new(transaction_converter),
        mempool_p2p_config.max_transaction_batch_size,
    );
    let mempool_p2p_runner = MempoolP2pRunner::new(
        network_future.boxed(),
        broadcasted_messages_receiver,
        broadcast_topic_client,
        gateway_client,
        mempool_p2p_propagator_client,
        mempool_p2p_config.transaction_batch_rate_millis,
    );
    (mempool_p2p_propagator, mempool_p2p_runner)
}

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
    BroadcastNetworkMetrics,
    EventMetrics,
    NetworkMetrics,
};
use apollo_network::network_manager::{BroadcastTopicChannels, NetworkManager};
use futures::FutureExt;
use metrics::MEMPOOL_P2P_NUM_BLACKLISTED_PEERS;
use tracing::{info_span, Instrument};

use crate::config::MempoolP2pConfig;
use crate::metrics::{
    MEMPOOL_P2P_ADDRESS_CHANGE,
    MEMPOOL_P2P_CONNECTIONS_CLOSED,
    MEMPOOL_P2P_CONNECTIONS_ESTABLISHED,
    MEMPOOL_P2P_CONNECTION_HANDLER_EVENTS,
    MEMPOOL_P2P_DIAL_FAILURE,
    MEMPOOL_P2P_EXPIRED_LISTEN_ADDRS,
    MEMPOOL_P2P_EXTERNAL_ADDR_CONFIRMED,
    MEMPOOL_P2P_EXTERNAL_ADDR_EXPIRED,
    MEMPOOL_P2P_INBOUND_CONNECTIONS_HANDLED,
    MEMPOOL_P2P_LISTENER_CLOSED,
    MEMPOOL_P2P_LISTEN_ERROR,
    MEMPOOL_P2P_LISTEN_FAILURE,
    MEMPOOL_P2P_NEW_EXTERNAL_ADDR_CANDIDATE,
    MEMPOOL_P2P_NEW_EXTERNAL_ADDR_OF_PEER,
    MEMPOOL_P2P_NEW_LISTENERS,
    MEMPOOL_P2P_NEW_LISTEN_ADDRS,
    MEMPOOL_P2P_NUM_CONNECTED_PEERS,
    MEMPOOL_P2P_NUM_DROPPED_MESSAGES,
    MEMPOOL_P2P_NUM_RECEIVED_MESSAGES,
    MEMPOOL_P2P_NUM_SENT_MESSAGES,
    MEMPOOL_P2P_OUTBOUND_CONNECTIONS_HANDLED,
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
            num_dropped_broadcast_messages: MEMPOOL_P2P_NUM_DROPPED_MESSAGES,
        },
    );
    let network_manager_metrics = Some(NetworkMetrics {
        num_connected_peers: MEMPOOL_P2P_NUM_CONNECTED_PEERS,
        num_blacklisted_peers: MEMPOOL_P2P_NUM_BLACKLISTED_PEERS,
        broadcast_metrics_by_topic: Some(broadcast_metrics_by_topic),
        sqmr_metrics: None,
        event_metrics: Some(EventMetrics {
            connections_established: MEMPOOL_P2P_CONNECTIONS_ESTABLISHED,
            connections_closed: MEMPOOL_P2P_CONNECTIONS_CLOSED,
            dial_failure: MEMPOOL_P2P_DIAL_FAILURE,
            listen_failure: MEMPOOL_P2P_LISTEN_FAILURE,
            listen_error: MEMPOOL_P2P_LISTEN_ERROR,
            address_change: MEMPOOL_P2P_ADDRESS_CHANGE,
            new_listeners: MEMPOOL_P2P_NEW_LISTENERS,
            new_listen_addrs: MEMPOOL_P2P_NEW_LISTEN_ADDRS,
            expired_listen_addrs: MEMPOOL_P2P_EXPIRED_LISTEN_ADDRS,
            listener_closed: MEMPOOL_P2P_LISTENER_CLOSED,
            new_external_addr_candidate: MEMPOOL_P2P_NEW_EXTERNAL_ADDR_CANDIDATE,
            external_addr_confirmed: MEMPOOL_P2P_EXTERNAL_ADDR_CONFIRMED,
            external_addr_expired: MEMPOOL_P2P_EXTERNAL_ADDR_EXPIRED,
            new_external_addr_of_peer: MEMPOOL_P2P_NEW_EXTERNAL_ADDR_OF_PEER,
            inbound_connections_handled: MEMPOOL_P2P_INBOUND_CONNECTIONS_HANDLED,
            outbound_connections_handled: MEMPOOL_P2P_OUTBOUND_CONNECTIONS_HANDLED,
            connection_handler_events: MEMPOOL_P2P_CONNECTION_HANDLER_EVENTS,
        }),
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

pub mod config;
pub mod propagator;
pub mod runner;

use futures::FutureExt;
use papyrus_network::gossipsub_impl::Topic;
use papyrus_network::network_manager::{BroadcastTopicChannels, NetworkManager};
use starknet_class_manager_types::transaction_converter::TransactionConverter;
use starknet_class_manager_types::SharedClassManagerClient;
use starknet_gateway_types::communication::SharedGatewayClient;

use crate::config::MempoolP2pConfig;
use crate::propagator::MempoolP2pPropagator;
use crate::runner::MempoolP2pRunner;

pub const MEMPOOL_TOPIC: &str = "starknet_mempool_transaction_propagation/0.1.0";
const MEMPOOL_P2P_METRICS_PREFIX: &str = "mempool_p2p";

pub fn create_p2p_propagator_and_runner(
    mempool_p2p_config: MempoolP2pConfig,
    gateway_client: SharedGatewayClient,
    class_manager_client: SharedClassManagerClient,
) -> (MempoolP2pPropagator, MempoolP2pRunner) {
    let transaction_converter = TransactionConverter::new(
        class_manager_client.clone(),
        mempool_p2p_config.network_config.chain_id.clone(),
    );
    let mut network_manager = NetworkManager::new(
        mempool_p2p_config.network_config,
        // TODO(Shahak): Consider filling this once the sequencer node has a name.
        None,
        MEMPOOL_P2P_METRICS_PREFIX,
    );
    let BroadcastTopicChannels { broadcasted_messages_receiver, broadcast_topic_client } =
        network_manager
            .register_broadcast_topic(
                Topic::new(MEMPOOL_TOPIC),
                mempool_p2p_config.network_buffer_size,
            )
            .expect("Failed to register broadcast topic");
    let network_future = network_manager.run();
    let mempool_p2p_propagator =
        MempoolP2pPropagator::new(broadcast_topic_client.clone(), transaction_converter);
    let mempool_p2p_runner = MempoolP2pRunner::new(
        network_future.boxed(),
        broadcasted_messages_receiver,
        broadcast_topic_client,
        gateway_client,
    );
    (mempool_p2p_propagator, mempool_p2p_runner)
}

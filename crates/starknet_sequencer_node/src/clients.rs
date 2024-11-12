use std::sync::Arc;

use starknet_batcher_types::communication::{LocalBatcherClient, SharedBatcherClient};
use starknet_gateway_types::communication::{LocalGatewayClient, SharedGatewayClient};
use starknet_mempool_p2p_types::communication::{
    LocalMempoolP2pPropagatorClient,
    SharedMempoolP2pPropagatorClient,
};
use starknet_mempool_types::communication::{LocalMempoolClient, SharedMempoolClient};

use crate::communication::SequencerNodeCommunication;
use crate::config::{ComponentExecutionMode, SequencerNodeConfig};

pub struct SequencerNodeClients {
    batcher_client: Option<SharedBatcherClient>,
    mempool_client: Option<SharedMempoolClient>,
    gateway_client: Option<SharedGatewayClient>,
    // TODO (Lev): Change to Option<Box<dyn MemPoolClient>>.
    mempool_p2p_propagator_client: Option<SharedMempoolP2pPropagatorClient>,
}

impl SequencerNodeClients {
    pub fn get_batcher_client(&self) -> Option<SharedBatcherClient> {
        self.batcher_client.clone()
    }

    pub fn get_mempool_client(&self) -> Option<SharedMempoolClient> {
        self.mempool_client.clone()
    }

    pub fn get_gateway_client(&self) -> Option<SharedGatewayClient> {
        self.gateway_client.clone()
    }

    pub fn get_mempool_p2p_propagator_client(&self) -> Option<SharedMempoolP2pPropagatorClient> {
        self.mempool_p2p_propagator_client.clone()
    }
}

/// A macro for creating a component client, determined by the component's execution mode. Returns a
/// local client if the component is run locally, otherwise None.
///
/// # Arguments
///
/// * $execution_mode - A reference to the component's execution mode, i.e., type
///   &ComponentExecutionMode.
/// * $channel_expr - Sender side for the client.
/// * $client_type - The client type to create, e.g., LocalBatcherClient. The client type should
///   have a function $client_type::new(tx : $channel_expr).
///
/// # Returns
///
/// An `Option<Arc<dyn ClientType>>` containing the client if the execution mode is enabled
/// (LocalExecutionWithRemoteDisabled / LocalExecutionWithRemoteEnabled), or None if the execution
/// mode is Disabled.
///
/// # Example
///
/// ```rust,ignore
/// // Assuming ComponentExecutionMode and channels are defined, and LocalBatcherClient
/// // has a new method that accepts a channel.
/// let batcher_client: Option<SharedBatcherClient> = create_client!(
///     &config.components.batcher.execution_mode,
///     LocalBatcherClient,
///     channels.take_batcher_tx()
/// );
///
/// match batcher_client {
///     Some(client) => println!("Client created: {:?}", client),
///     None => println!("Client not created because the execution mode is disabled."),
/// }
/// ```
macro_rules! create_client {
    ($execution_mode:expr, $client_type:ty, $channel_expr:expr) => {
        match *$execution_mode {
            ComponentExecutionMode::LocalExecutionWithRemoteDisabled
            | ComponentExecutionMode::LocalExecutionWithRemoteEnabled => {
                Some(Arc::new(<$client_type>::new($channel_expr)))
            }
            ComponentExecutionMode::Disabled | ComponentExecutionMode::Remote => None,
        }
    };
}

pub fn create_node_clients(
    config: &SequencerNodeConfig,
    channels: &mut SequencerNodeCommunication,
) -> SequencerNodeClients {
    let batcher_client: Option<SharedBatcherClient> = create_client!(
        &config.components.batcher.execution_mode,
        LocalBatcherClient,
        channels.take_batcher_tx()
    );
    let mempool_client: Option<SharedMempoolClient> = create_client!(
        &config.components.mempool.execution_mode,
        LocalMempoolClient,
        channels.take_mempool_tx()
    );
    let gateway_client: Option<SharedGatewayClient> = create_client!(
        &config.components.gateway.execution_mode,
        LocalGatewayClient,
        channels.take_gateway_tx()
    );

    let mempool_p2p_propagator_client: Option<SharedMempoolP2pPropagatorClient> = create_client!(
        &config.components.mempool_p2p.execution_mode,
        LocalMempoolP2pPropagatorClient,
        channels.take_mempool_p2p_propagator_tx()
    );
    SequencerNodeClients {
        batcher_client,
        mempool_client,
        gateway_client,
        mempool_p2p_propagator_client,
    }
}

use std::sync::Arc;

use starknet_batcher_types::communication::{
    LocalBatcherClient,
    RemoteBatcherClient,
    SharedBatcherClient,
};
use starknet_gateway_types::communication::{
    LocalGatewayClient,
    RemoteGatewayClient,
    SharedGatewayClient,
};
use starknet_mempool_p2p_types::communication::{
    LocalMempoolP2pPropagatorClient,
    RemoteMempoolP2pPropagatorClient,
    SharedMempoolP2pPropagatorClient,
};
use starknet_mempool_types::communication::{
    LocalMempoolClient,
    RemoteMempoolClient,
    SharedMempoolClient,
};

use crate::communication::SequencerNodeCommunication;
use crate::config::component_execution_config::ComponentExecutionMode;
use crate::config::node_config::SequencerNodeConfig;

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
/// local client if the component is run locally, or a remote client if the execution mode is
/// Remote, otherwise None.
///
/// # Arguments
///
/// * $execution_mode - A reference to the component's execution mode, i.e., type
///   &ComponentExecutionMode.
/// * $local_client_type - The type for the local client to create, e.g., LocalBatcherClient. The
///   client type should have a function $local_client_type::new(tx: $channel_expr).
/// * $remote_client_type - The type for the remote client to create, e.g., RemoteBatcherClient. The
///   client type should have a function $remote_client_type::new(config).
/// * $channel_expr - Sender side for the local client.
/// * $remote_client_config - Configuration for the remote client, passed as Some(config) when
///   available.
///
/// # Returns
///
/// An `Option<Arc<dyn ClientType>>` containing the client if the execution mode is enabled
/// (LocalExecutionWithRemoteDisabled / LocalExecutionWithRemoteEnabled for local clients, Remote
/// for remote clients), or None if the execution mode is Disabled.
///
/// # Example
///
/// ```rust,ignore
/// // Assuming ComponentExecutionMode, channels, and remote client configuration are defined, and
/// // LocalBatcherClient and RemoteBatcherClient have new methods that accept a channel and config,
/// // respectively.
/// let batcher_client: Option<SharedBatcherClient> = create_client!(
///     &config.components.batcher.execution_mode,
///     LocalBatcherClient,
///     RemoteBatcherClient,
///     channels.take_batcher_tx(),
///     config.components.batcher.remote_client_config
/// );
///
/// match batcher_client {
///     Some(client) => println!("Client created: {:?}", client),
///     None => println!("Client not created because the execution mode is disabled."),
/// }
/// ```
macro_rules! create_client {
    (
        $execution_mode:expr,
        $local_client_type:ty,
        $remote_client_type:ty,
        $channel_expr:expr,
        $remote_client_config:expr
    ) => {
        match *$execution_mode {
            ComponentExecutionMode::LocalExecutionWithRemoteDisabled
            | ComponentExecutionMode::LocalExecutionWithRemoteEnabled => {
                Some(Arc::new(<$local_client_type>::new($channel_expr)))
            }
            ComponentExecutionMode::Remote => match $remote_client_config {
                Some(ref config) => Some(Arc::new(<$remote_client_type>::new(config.clone()))),
                None => None,
            },
            ComponentExecutionMode::Disabled => None,
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
        RemoteBatcherClient,
        channels.take_batcher_tx(),
        config.components.batcher.remote_client_config
    );
    let mempool_client: Option<SharedMempoolClient> = create_client!(
        &config.components.mempool.execution_mode,
        LocalMempoolClient,
        RemoteMempoolClient,
        channels.take_mempool_tx(),
        config.components.mempool.remote_client_config
    );
    let gateway_client: Option<SharedGatewayClient> = create_client!(
        &config.components.gateway.execution_mode,
        LocalGatewayClient,
        RemoteGatewayClient,
        channels.take_gateway_tx(),
        config.components.gateway.remote_client_config
    );

    let mempool_p2p_propagator_client: Option<SharedMempoolP2pPropagatorClient> = create_client!(
        &config.components.mempool_p2p.execution_mode,
        LocalMempoolP2pPropagatorClient,
        RemoteMempoolP2pPropagatorClient,
        channels.take_mempool_p2p_propagator_tx(),
        config.components.mempool_p2p.remote_client_config
    );
    SequencerNodeClients {
        batcher_client,
        mempool_client,
        gateway_client,
        mempool_p2p_propagator_client,
    }
}

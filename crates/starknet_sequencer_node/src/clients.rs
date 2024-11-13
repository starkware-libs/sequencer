use std::sync::Arc;

use starknet_batcher_types::communication::{
    BatcherRequest,
    BatcherResponse,
    LocalBatcherClient,
    RemoteBatcherClient,
    SharedBatcherClient,
};
use starknet_gateway_types::communication::{
    GatewayRequest,
    GatewayResponse,
    LocalGatewayClient,
    RemoteGatewayClient,
    SharedGatewayClient,
};
use starknet_mempool_p2p_types::communication::{
    LocalMempoolP2pPropagatorClient,
    MempoolP2pPropagatorRequest,
    MempoolP2pPropagatorResponse,
    RemoteMempoolP2pPropagatorClient,
    SharedMempoolP2pPropagatorClient,
};
use starknet_mempool_types::communication::{
    LocalMempoolClient,
    MempoolRequest,
    MempoolResponse,
    RemoteMempoolClient,
    SharedMempoolClient,
};
use starknet_sequencer_infra::component_client::Client;

use crate::communication::SequencerNodeCommunication;
use crate::config::component_execution_config::ComponentExecutionMode;
use crate::config::node_config::SequencerNodeConfig;

pub struct SequencerNodeClients {
    batcher_client: Option<Client<BatcherRequest, BatcherResponse>>,
    mempool_client: Option<Client<MempoolRequest, MempoolResponse>>,
    gateway_client: Option<Client<GatewayRequest, GatewayResponse>>,
    // TODO (Lev): Change to Option<Box<dyn MemPoolClient>>.
    mempool_p2p_propagator_client:
        Option<Client<MempoolP2pPropagatorRequest, MempoolP2pPropagatorResponse>>,
}

/// A macro to retrieve a shared client (either local or remote) from a specified field in a struct,
/// returning it wrapped in an `Arc`. This macro simplifies access to a client by checking if a
/// This macro simplifies client access by checking the specified client field and returning the
/// existing client, either local_client or remote_client. Only one will exist at a time.
///
/// # Arguments
///
/// * `$self` - The `self` reference to the struct that contains the client field.
/// * `$client_field` - The field name (within `self`) representing the client, which has both
///   `local_client` and `remote_client` as options.
///
/// # Returns
///
/// An Option<Arc<dyn Trait>> containing the available client (local_client or remote_client),
/// wrapped in Arc. If neither client exists, it returns None.
///
/// # Example
///
/// ```rust,ignore
/// // Assuming `SequencerNodeClients` struct has fields `batcher_client` and `mempool_client.
/// impl SequencerNodeClients {
///     pub fn get_batcher_shared_client(&self) -> Option<Arc<dyn BatcherClient>> {
///         get_shared_client!(self, batcher_client)
///     }
///
///     pub fn get_mempool_shared_client(&self) -> Option<Arc<dyn MempoolClient>> {
///         get_shared_client!(self, mempool_client)
///     }
/// }
/// ```
///
/// In this example, `get_shared_client!` checks if `batcher_client` has a local or remote client
/// available. If a local client exists, it returns `Some(Arc::new(local_client))`; otherwise,
/// it checks for a remote client and returns `Some(Arc::new(remote_client))` if available.
/// If neither client is available, it returns `None`.
#[macro_export]
macro_rules! get_shared_client {
    ($self:ident, $client_field:ident) => {{
        if let Some(client) = &$self.$client_field {
            if let Some(local_client) = client.get_local_client() {
                return Some(Arc::new(local_client));
            } else if let Some(remote_client) = client.get_remote_client() {
                return Some(Arc::new(remote_client));
            }
        }
        None
    }};
}

impl SequencerNodeClients {
    pub fn get_batcher_shared_client(&self) -> Option<SharedBatcherClient> {
        get_shared_client!(self, batcher_client)
    }

    pub fn get_mempool_shared_client(&self) -> Option<SharedMempoolClient> {
        get_shared_client!(self, mempool_client)
    }

    pub fn get_gateway_shared_client(&self) -> Option<SharedGatewayClient> {
        get_shared_client!(self, gateway_client)
    }

    pub fn get_mempool_p2p_propagator_shared_client(
        &self,
    ) -> Option<SharedMempoolP2pPropagatorClient> {
        get_shared_client!(self, mempool_p2p_propagator_client)
    }
}

/// A macro for creating a component client, determined by the component's execution mode. Returns a
/// `Client` containing either a local client if the component is run locally, or a remote client if
/// the execution mode is Remote. Returns None if the execution mode is Disabled.
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
/// An `Option<Client<...>>` containing either a local or remote client based on the execution mode
/// (LocalExecutionWithRemoteDisabled / LocalExecutionWithRemoteEnabled for local clients, Remote
/// for remote clients), or None if the execution mode is Disabled.
///
/// # Example
///
/// ```rust,ignore
/// // Assuming ComponentExecutionMode, channels, and remote client configuration are defined, and
/// // LocalBatcherClient and RemoteBatcherClient have new methods that accept a channel and config,
/// // respectively.
/// let batcher_client: Option<Client<BatcherRequest, BatcherResponse>> = create_client!(
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
                let local_client = Some(<$local_client_type>::new($channel_expr));
                Some(Client::new(local_client, None))
            }
            ComponentExecutionMode::Remote => match $remote_client_config {
                Some(ref config) => {
                    let remote_client = Some(<$remote_client_type>::new(config.clone()));
                    Some(Client::new(None, remote_client))
                }
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
    let batcher_client = create_client!(
        &config.components.batcher.execution_mode,
        LocalBatcherClient,
        RemoteBatcherClient,
        channels.take_batcher_tx(),
        config.components.batcher.remote_client_config
    );
    let mempool_client = create_client!(
        &config.components.mempool.execution_mode,
        LocalMempoolClient,
        RemoteMempoolClient,
        channels.take_mempool_tx(),
        config.components.mempool.remote_client_config
    );
    let gateway_client = create_client!(
        &config.components.gateway.execution_mode,
        LocalGatewayClient,
        RemoteGatewayClient,
        channels.take_gateway_tx(),
        config.components.gateway.remote_client_config
    );

    let mempool_p2p_propagator_client = create_client!(
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

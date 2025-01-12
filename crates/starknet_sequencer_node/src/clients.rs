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
use starknet_l1_provider::communication::{LocalL1ProviderClient, RemoteL1ProviderClient};
use starknet_l1_provider_types::{L1ProviderRequest, L1ProviderResponse, SharedL1ProviderClient};
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
use starknet_sequencer_infra::component_client::{Client, LocalComponentClient};
use starknet_state_sync_types::communication::{
    LocalStateSyncClient,
    RemoteStateSyncClient,
    SharedStateSyncClient,
    StateSyncRequest,
    StateSyncResponse,
};

use crate::communication::SequencerNodeCommunication;
use crate::config::component_execution_config::ReactiveComponentExecutionMode;
use crate::config::node_config::SequencerNodeConfig;

pub struct SequencerNodeClients {
    batcher_client: Client<BatcherRequest, BatcherResponse>,
    mempool_client: Client<MempoolRequest, MempoolResponse>,
    gateway_client: Client<GatewayRequest, GatewayResponse>,
    mempool_p2p_propagator_client:
        Client<MempoolP2pPropagatorRequest, MempoolP2pPropagatorResponse>,
    state_sync_client: Client<StateSyncRequest, StateSyncResponse>,
    l1_provider_client: Client<L1ProviderRequest, L1ProviderResponse>,
}

/// A macro to retrieve a shared client wrapped in an `Arc`. The returned client is either the local
/// or the remote, based on the provided execution mode. If the execution mode is `Disabled` or
/// neither client exists, it returns `None`.
///
/// # Arguments
///
/// * `$self` - The `self` reference to the struct that contains the client field.
/// * `$client_field` - The field name (within `self`) representing the client, which has both
///   `local_client` and `remote_client` as options.
/// * `$execution_mode` - A reference to the `ReactiveComponentExecutionMode` that determines which
///   client to return (`local_client` or `remote_client`).
///
/// # Returns
///
/// An `Option<Arc<dyn ClientTrait>>` containing the available client (local_client or
/// remote_client), wrapped in `Arc`. If the execution mode is `Disabled` or neither client exists,
/// returns `None`.
///
/// # Example
///
/// ```rust,ignore
/// // Assuming `SequencerNodeClients` struct has fields `batcher_client` and `mempool_client`.
/// impl SequencerNodeClients {
///     pub fn get_batcher_shared_client(
///         &self,
///         execution_mode: &ReactiveComponentExecutionMode,
///     ) -> Option<Arc<dyn BatcherClient>> {
///         get_shared_client!(self, batcher_client, execution_mode)
///     }
///
///     pub fn get_mempool_shared_client(
///         &self,
///         execution_mode: &ReactiveComponentExecutionMode,
///     ) -> Option<Arc<dyn MempoolClient>> {
///         get_shared_client!(self, mempool_client, execution_mode)
///     }
/// }
/// ```
#[macro_export]
macro_rules! get_shared_client {
    ($self:ident, $client_field:ident) => {{
        let client = &$self.$client_field;
        match client.get_use_remote_client() {
            true => Some(Arc::new(client.get_remote_client())),
            false => Some(Arc::new(client.get_local_client())),
        }
    }};
}

// TODO(Nadin): Refactor getters to remove code duplication.
impl SequencerNodeClients {
    pub fn get_batcher_shared_client(&self) -> Option<SharedBatcherClient> {
        get_shared_client!(self, batcher_client)
    }

    pub fn get_batcher_local_client(
        &self,
    ) -> LocalComponentClient<BatcherRequest, BatcherResponse> {
        self.batcher_client.get_local_client()
    }

    pub fn get_mempool_shared_client(&self) -> Option<SharedMempoolClient> {
        get_shared_client!(self, mempool_client)
    }

    pub fn get_mempool_local_client(
        &self,
    ) -> LocalComponentClient<MempoolRequest, MempoolResponse> {
        self.mempool_client.get_local_client()
    }

    pub fn get_gateway_shared_client(&self) -> Option<SharedGatewayClient> {
        get_shared_client!(self, gateway_client)
    }

    pub fn get_gateway_local_client(
        &self,
    ) -> LocalComponentClient<GatewayRequest, GatewayResponse> {
        self.gateway_client.get_local_client()
    }

    pub fn get_l1_provider_local_client(
        &self,
    ) -> LocalComponentClient<L1ProviderRequest, L1ProviderResponse> {
        self.l1_provider_client.get_local_client()
    }

    pub fn get_l1_provider_shared_client(&self) -> Option<SharedL1ProviderClient> {
        get_shared_client!(self, l1_provider_client)
    }

    pub fn get_mempool_p2p_propagator_shared_client(
        &self,
    ) -> Option<SharedMempoolP2pPropagatorClient> {
        get_shared_client!(self, mempool_p2p_propagator_client)
    }

    pub fn get_mempool_p2p_propagator_local_client(
        &self,
    ) -> LocalComponentClient<MempoolP2pPropagatorRequest, MempoolP2pPropagatorResponse> {
        self.mempool_p2p_propagator_client.get_local_client()
    }

    pub fn get_state_sync_shared_client(&self) -> Option<SharedStateSyncClient> {
        get_shared_client!(self, state_sync_client)
    }

    pub fn get_state_sync_local_client(
        &self,
    ) -> LocalComponentClient<StateSyncRequest, StateSyncResponse> {
        self.state_sync_client.get_local_client()
    }
}

/// /// Creates a new client instance by initializing both a local and a remote client.
///
/// # Arguments
///
/// * $local_client_type - The type for the local client to create, e.g., LocalBatcherClient. The
///   client type should have a function $local_client_type::new(tx: $channel_expr).
/// * $remote_client_type - The type for the remote client to create, e.g., RemoteBatcherClient. The
///   client type should have a function $remote_client_type::new(config).
/// * $channel_expr - Sender side for the local client.
/// * $remote_client_config - Configuration for the remote client, passed as Option(config).
/// * $execution_mode - Determines whether to enable the remote client
///   (ReactiveComponentExecutionMode::Remote) or to use the local client.
///
/// # Example
///
/// ```rust,ignore
/// // Assuming ReactiveComponentExecutionMode, channels, and remote client configuration are defined, and
/// // LocalBatcherClient and RemoteBatcherClient have new methods that accept a channel and config,
/// // respectively.
/// let batcher_client: Option<Client<BatcherRequest, BatcherResponse>> = create_client!(
///     LocalBatcherClient,
///     RemoteBatcherClient,
///     channels.take_batcher_tx(),
///     config.components.batcher.remote_client_config,
///     ReactiveComponentExecutionMode::Remote
/// );
/// ```
///
/// This macro returns an instance of `Client` with both local and remote clients initialized.
/// The local client uses the provided channel, and the remote client uses the configuration and
/// socket. The use_remote_client field is set based on the value of $execution_mode.
#[macro_export]
macro_rules! create_client {
    (
        $local_client_type:ty,
        $remote_client_type:ty,
        $channel_expr:expr,
        $remote_client_config:expr,
        $socket:expr,
        $execution_mode:expr
    ) => {
        Client::new(
            <$local_client_type>::new($channel_expr),
            <$remote_client_type>::new($remote_client_config.clone(), $socket),
            matches!($execution_mode, ReactiveComponentExecutionMode::Remote),
        )
    };
}

pub fn create_node_clients(
    config: &SequencerNodeConfig,
    channels: &mut SequencerNodeCommunication,
) -> SequencerNodeClients {
    let batcher_client = create_client!(
        LocalBatcherClient,
        RemoteBatcherClient,
        channels.take_batcher_tx(),
        &config.components.batcher.remote_client_config,
        config.components.batcher.socket,
        config.components.batcher.execution_mode
    );
    let mempool_client = create_client!(
        LocalMempoolClient,
        RemoteMempoolClient,
        channels.take_mempool_tx(),
        &config.components.mempool.remote_client_config,
        config.components.mempool.socket,
        config.components.mempool.execution_mode
    );
    let gateway_client = create_client!(
        LocalGatewayClient,
        RemoteGatewayClient,
        channels.take_gateway_tx(),
        &config.components.gateway.remote_client_config,
        config.components.gateway.socket,
        config.components.gateway.execution_mode
    );

    let mempool_p2p_propagator_client = create_client!(
        LocalMempoolP2pPropagatorClient,
        RemoteMempoolP2pPropagatorClient,
        channels.take_mempool_p2p_propagator_tx(),
        &config.components.mempool_p2p.remote_client_config,
        config.components.mempool_p2p.socket,
        config.components.mempool_p2p.execution_mode
    );

    let state_sync_client = create_client!(
        LocalStateSyncClient,
        RemoteStateSyncClient,
        channels.take_state_sync_tx(),
        &config.components.state_sync.remote_client_config,
        config.components.state_sync.socket,
        config.components.state_sync.execution_mode
    );

    let l1_provider_client = create_client!(
        LocalL1ProviderClient,
        RemoteL1ProviderClient,
        channels.take_l1_provider_tx(),
        &config.components.l1_provider.remote_client_config,
        config.components.l1_provider.socket,
        config.components.l1_provider.execution_mode
    );

    SequencerNodeClients {
        batcher_client,
        mempool_client,
        gateway_client,
        mempool_p2p_propagator_client,
        state_sync_client,
        l1_provider_client,
    }
}

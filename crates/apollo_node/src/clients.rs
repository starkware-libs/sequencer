use std::sync::Arc;

use apollo_batcher::metrics::{
    BATCHER_LABELED_LOCAL_RESPONSE_TIMES_SECS,
    BATCHER_LABELED_REMOTE_CLIENT_COMMUNICATION_FAILURE_TIMES_SECS,
    BATCHER_LABELED_REMOTE_RESPONSE_TIMES_SECS,
};
use apollo_batcher_types::communication::{
    BatcherRequest,
    BatcherResponse,
    LocalBatcherClient,
    RemoteBatcherClient,
    SharedBatcherClient,
};
use apollo_class_manager::metrics::{
    CLASS_MANAGER_LABELED_LOCAL_RESPONSE_TIMES_SECS,
    CLASS_MANAGER_LABELED_REMOTE_CLIENT_COMMUNICATION_FAILURE_TIMES_SECS,
    CLASS_MANAGER_LABELED_REMOTE_RESPONSE_TIMES_SECS,
};
use apollo_class_manager_types::{
    ClassManagerRequest,
    ClassManagerResponse,
    LocalClassManagerClient,
    RemoteClassManagerClient,
    SharedClassManagerClient,
};
use apollo_compile_to_casm::metrics::{
    SIERRA_COMPILER_LABELED_LOCAL_RESPONSE_TIMES_SECS,
    SIERRA_COMPILER_LABELED_REMOTE_CLIENT_COMMUNICATION_FAILURE_TIMES_SECS,
    SIERRA_COMPILER_LABELED_REMOTE_RESPONSE_TIMES_SECS,
};
use apollo_compile_to_casm_types::{
    LocalSierraCompilerClient,
    RemoteSierraCompilerClient,
    SharedSierraCompilerClient,
    SierraCompilerRequest,
    SierraCompilerResponse,
};
use apollo_gateway::metrics::{
    GATEWAY_LABELED_LOCAL_RESPONSE_TIMES_SECS,
    GATEWAY_LABELED_REMOTE_CLIENT_COMMUNICATION_FAILURE_TIMES_SECS,
    GATEWAY_LABELED_REMOTE_RESPONSE_TIMES_SECS,
};
use apollo_gateway_types::communication::{
    GatewayRequest,
    GatewayResponse,
    LocalGatewayClient,
    RemoteGatewayClient,
    SharedGatewayClient,
};
use apollo_infra::component_client::{Client, LocalComponentClient};
use apollo_infra::metrics::{
    LocalClientMetrics,
    RemoteClientMetrics,
    BATCHER_REMOTE_CLIENT_SEND_ATTEMPTS,
    CLASS_MANAGER_REMOTE_CLIENT_SEND_ATTEMPTS,
    GATEWAY_REMOTE_CLIENT_SEND_ATTEMPTS,
    L1_ENDPOINT_MONITOR_SEND_ATTEMPTS,
    L1_GAS_PRICE_PROVIDER_REMOTE_CLIENT_SEND_ATTEMPTS,
    L1_PROVIDER_REMOTE_CLIENT_SEND_ATTEMPTS,
    MEMPOOL_P2P_REMOTE_CLIENT_SEND_ATTEMPTS,
    MEMPOOL_REMOTE_CLIENT_SEND_ATTEMPTS,
    SIERRA_COMPILER_REMOTE_CLIENT_SEND_ATTEMPTS,
    STATE_SYNC_REMOTE_CLIENT_SEND_ATTEMPTS,
};
use apollo_l1_endpoint_monitor::communication::{
    LocalL1EndpointMonitorClient,
    RemoteL1EndpointMonitorClient,
};
use apollo_l1_endpoint_monitor_types::{
    L1EndpointMonitorRequest,
    L1EndpointMonitorResponse,
    SharedL1EndpointMonitorClient,
    L1_ENDPOINT_MONITOR_LOCAL_RESPONSE_TIMES_SECS,
    L1_ENDPOINT_MONITOR_REMOTE_CLIENT_COMMUNICATION_FAILURE_TIMES_SECS,
    L1_ENDPOINT_MONITOR_REMOTE_RESPONSE_TIMES_SECS,
};
use apollo_l1_gas_price::communication::{LocalL1GasPriceClient, RemoteL1GasPriceClient};
use apollo_l1_gas_price::metrics::{
    L1_GAS_PRICE_LABELED_LOCAL_RESPONSE_TIMES_SECS,
    L1_GAS_PRICE_LABELED_REMOTE_CLIENT_COMMUNICATION_FAILURE_TIMES_SECS,
    L1_GAS_PRICE_LABELED_REMOTE_RESPONSE_TIMES_SECS,
};
use apollo_l1_gas_price_types::{L1GasPriceRequest, L1GasPriceResponse, SharedL1GasPriceClient};
use apollo_l1_provider::communication::{LocalL1ProviderClient, RemoteL1ProviderClient};
use apollo_l1_provider::metrics::{
    L1_PROVIDER_LABELED_LOCAL_RESPONSE_TIMES_SECS,
    L1_PROVIDER_LABELED_REMOTE_CLIENT_COMMUNICATION_FAILURE_TIMES_SECS,
    L1_PROVIDER_LABELED_REMOTE_RESPONSE_TIMES_SECS,
};
use apollo_l1_provider_types::{L1ProviderRequest, L1ProviderResponse, SharedL1ProviderClient};
use apollo_mempool::metrics::{
    MEMPOOL_LABELED_LOCAL_RESPONSE_TIMES_SECS,
    MEMPOOL_LABELED_REMOTE_CLIENT_COMMUNICATION_FAILURE_TIMES_SECS,
    MEMPOOL_LABELED_REMOTE_RESPONSE_TIMES_SECS,
};
use apollo_mempool_p2p::metrics::{
    MEMPOOL_P2P_LABELED_LOCAL_RESPONSE_TIMES_SECS,
    MEMPOOL_P2P_LABELED_REMOTE_CLIENT_COMMUNICATION_FAILURE_TIMES_SECS,
    MEMPOOL_P2P_LABELED_REMOTE_RESPONSE_TIMES_SECS,
};
use apollo_mempool_p2p_types::communication::{
    LocalMempoolP2pPropagatorClient,
    MempoolP2pPropagatorRequest,
    MempoolP2pPropagatorResponse,
    RemoteMempoolP2pPropagatorClient,
    SharedMempoolP2pPropagatorClient,
};
use apollo_mempool_types::communication::{
    LocalMempoolClient,
    MempoolRequest,
    MempoolResponse,
    RemoteMempoolClient,
    SharedMempoolClient,
};
use apollo_state_sync_metrics::metrics::{
    STATE_SYNC_LABELED_LOCAL_RESPONSE_TIMES_SECS,
    STATE_SYNC_LABELED_REMOTE_CLIENT_COMMUNICATION_FAILURE_TIMES_SECS,
    STATE_SYNC_LABELED_REMOTE_RESPONSE_TIMES_SECS,
};
use apollo_state_sync_types::communication::{
    LocalStateSyncClient,
    RemoteStateSyncClient,
    SharedStateSyncClient,
    StateSyncRequest,
    StateSyncResponse,
};
use tracing::info;

use crate::communication::SequencerNodeCommunication;
use crate::config::component_execution_config::ReactiveComponentExecutionMode;
use crate::config::node_config::SequencerNodeConfig;

// Local client metrics per component (static references are required by LocalComponentClient::new)
const BATCHER_LOCAL_CLIENT_METRICS: LocalClientMetrics =
    LocalClientMetrics::new(&BATCHER_LABELED_LOCAL_RESPONSE_TIMES_SECS);
const CLASS_MANAGER_LOCAL_CLIENT_METRICS: LocalClientMetrics =
    LocalClientMetrics::new(&CLASS_MANAGER_LABELED_LOCAL_RESPONSE_TIMES_SECS);
const GATEWAY_LOCAL_CLIENT_METRICS: LocalClientMetrics =
    LocalClientMetrics::new(&GATEWAY_LABELED_LOCAL_RESPONSE_TIMES_SECS);
const L1_ENDPOINT_MONITOR_LOCAL_CLIENT_METRICS: LocalClientMetrics =
    LocalClientMetrics::new(&L1_ENDPOINT_MONITOR_LOCAL_RESPONSE_TIMES_SECS);
const L1_PROVIDER_LOCAL_CLIENT_METRICS: LocalClientMetrics =
    LocalClientMetrics::new(&L1_PROVIDER_LABELED_LOCAL_RESPONSE_TIMES_SECS);
const L1_GAS_PRICE_LOCAL_CLIENT_METRICS: LocalClientMetrics =
    LocalClientMetrics::new(&L1_GAS_PRICE_LABELED_LOCAL_RESPONSE_TIMES_SECS);
const MEMPOOL_LOCAL_CLIENT_METRICS: LocalClientMetrics =
    LocalClientMetrics::new(&MEMPOOL_LABELED_LOCAL_RESPONSE_TIMES_SECS);
const MEMPOOL_P2P_LOCAL_CLIENT_METRICS: LocalClientMetrics =
    LocalClientMetrics::new(&MEMPOOL_P2P_LABELED_LOCAL_RESPONSE_TIMES_SECS);
const SIERRA_COMPILER_LOCAL_CLIENT_METRICS: LocalClientMetrics =
    LocalClientMetrics::new(&SIERRA_COMPILER_LABELED_LOCAL_RESPONSE_TIMES_SECS);
const STATE_SYNC_LOCAL_CLIENT_METRICS: LocalClientMetrics =
    LocalClientMetrics::new(&STATE_SYNC_LABELED_LOCAL_RESPONSE_TIMES_SECS);

// Remote client metrics per component (static references are required by
// RemoteComponentClient::new)
const BATCHER_REMOTE_CLIENT_METRICS: RemoteClientMetrics = RemoteClientMetrics::new(
    &BATCHER_REMOTE_CLIENT_SEND_ATTEMPTS,
    &BATCHER_LABELED_REMOTE_RESPONSE_TIMES_SECS,
    &BATCHER_LABELED_REMOTE_CLIENT_COMMUNICATION_FAILURE_TIMES_SECS,
);
const CLASS_MANAGER_REMOTE_CLIENT_METRICS: RemoteClientMetrics = RemoteClientMetrics::new(
    &CLASS_MANAGER_REMOTE_CLIENT_SEND_ATTEMPTS,
    &CLASS_MANAGER_LABELED_REMOTE_RESPONSE_TIMES_SECS,
    &CLASS_MANAGER_LABELED_REMOTE_CLIENT_COMMUNICATION_FAILURE_TIMES_SECS,
);
const GATEWAY_REMOTE_CLIENT_METRICS: RemoteClientMetrics = RemoteClientMetrics::new(
    &GATEWAY_REMOTE_CLIENT_SEND_ATTEMPTS,
    &GATEWAY_LABELED_REMOTE_RESPONSE_TIMES_SECS,
    &GATEWAY_LABELED_REMOTE_CLIENT_COMMUNICATION_FAILURE_TIMES_SECS,
);
const L1_ENDPOINT_MONITOR_REMOTE_CLIENT_METRICS: RemoteClientMetrics = RemoteClientMetrics::new(
    &L1_ENDPOINT_MONITOR_SEND_ATTEMPTS,
    &L1_ENDPOINT_MONITOR_REMOTE_RESPONSE_TIMES_SECS,
    &L1_ENDPOINT_MONITOR_REMOTE_CLIENT_COMMUNICATION_FAILURE_TIMES_SECS,
);
const L1_PROVIDER_REMOTE_CLIENT_METRICS: RemoteClientMetrics = RemoteClientMetrics::new(
    &L1_PROVIDER_REMOTE_CLIENT_SEND_ATTEMPTS,
    &L1_PROVIDER_LABELED_REMOTE_RESPONSE_TIMES_SECS,
    &L1_PROVIDER_LABELED_REMOTE_CLIENT_COMMUNICATION_FAILURE_TIMES_SECS,
);
const L1_GAS_PRICE_PROVIDER_REMOTE_CLIENT_METRICS: RemoteClientMetrics = RemoteClientMetrics::new(
    &L1_GAS_PRICE_PROVIDER_REMOTE_CLIENT_SEND_ATTEMPTS,
    &L1_GAS_PRICE_LABELED_REMOTE_RESPONSE_TIMES_SECS,
    &L1_GAS_PRICE_LABELED_REMOTE_CLIENT_COMMUNICATION_FAILURE_TIMES_SECS,
);
const MEMPOOL_REMOTE_CLIENT_METRICS: RemoteClientMetrics = RemoteClientMetrics::new(
    &MEMPOOL_REMOTE_CLIENT_SEND_ATTEMPTS,
    &MEMPOOL_LABELED_REMOTE_RESPONSE_TIMES_SECS,
    &MEMPOOL_LABELED_REMOTE_CLIENT_COMMUNICATION_FAILURE_TIMES_SECS,
);
const MEMPOOL_P2P_REMOTE_CLIENT_METRICS: RemoteClientMetrics = RemoteClientMetrics::new(
    &MEMPOOL_P2P_REMOTE_CLIENT_SEND_ATTEMPTS,
    &MEMPOOL_P2P_LABELED_REMOTE_RESPONSE_TIMES_SECS,
    &MEMPOOL_P2P_LABELED_REMOTE_CLIENT_COMMUNICATION_FAILURE_TIMES_SECS,
);
const SIERRA_COMPILER_REMOTE_CLIENT_METRICS: RemoteClientMetrics = RemoteClientMetrics::new(
    &SIERRA_COMPILER_REMOTE_CLIENT_SEND_ATTEMPTS,
    &SIERRA_COMPILER_LABELED_REMOTE_RESPONSE_TIMES_SECS,
    &SIERRA_COMPILER_LABELED_REMOTE_CLIENT_COMMUNICATION_FAILURE_TIMES_SECS,
);
const STATE_SYNC_REMOTE_CLIENT_METRICS: RemoteClientMetrics = RemoteClientMetrics::new(
    &STATE_SYNC_REMOTE_CLIENT_SEND_ATTEMPTS,
    &STATE_SYNC_LABELED_REMOTE_RESPONSE_TIMES_SECS,
    &STATE_SYNC_LABELED_REMOTE_CLIENT_COMMUNICATION_FAILURE_TIMES_SECS,
);

pub struct SequencerNodeClients {
    batcher_client: Client<BatcherRequest, BatcherResponse>,
    class_manager_client: Client<ClassManagerRequest, ClassManagerResponse>,
    gateway_client: Client<GatewayRequest, GatewayResponse>,
    l1_endpoint_monitor_client: Client<L1EndpointMonitorRequest, L1EndpointMonitorResponse>,
    l1_provider_client: Client<L1ProviderRequest, L1ProviderResponse>,
    l1_gas_price_client: Client<L1GasPriceRequest, L1GasPriceResponse>,
    mempool_client: Client<MempoolRequest, MempoolResponse>,
    mempool_p2p_propagator_client:
        Client<MempoolP2pPropagatorRequest, MempoolP2pPropagatorResponse>,
    sierra_compiler_client: Client<SierraCompilerRequest, SierraCompilerResponse>,
    state_sync_client: Client<StateSyncRequest, StateSyncResponse>,
}

/// A macro to retrieve a shared client wrapped in an `Arc`. The returned client is either the local
/// or the remote, as at most one of them exists. If neither, it returns `None`.
///
/// # Arguments
///
/// * `$self` - The `self` reference to the struct that contains the client field.
/// * `$client_field` - The field name (within `self`) representing the client, which has both
///   `local_client` and `remote_client` as options.
///
/// # Returns
///
/// An `Option<Arc<dyn ClientTrait>>` containing the available client (local_client or
/// remote_client), wrapped in Arc. If neither exists, returns None.
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
#[macro_export]
macro_rules! get_shared_client {
    ($self:ident, $client_field:ident) => {{
        let client = &$self.$client_field;
        if let Some(local_client) = client.get_local_client() {
            return Some(Arc::new(local_client));
        } else if let Some(remote_client) = client.get_remote_client() {
            return Some(Arc::new(remote_client));
        }
        None
    }};
}

// TODO(Nadin): Refactor getters to remove code duplication.
impl SequencerNodeClients {
    pub fn get_batcher_local_client(
        &self,
    ) -> Option<LocalComponentClient<BatcherRequest, BatcherResponse>> {
        self.batcher_client.get_local_client()
    }

    pub fn get_batcher_shared_client(&self) -> Option<SharedBatcherClient> {
        get_shared_client!(self, batcher_client)
    }

    pub fn get_class_manager_local_client(
        &self,
    ) -> Option<LocalComponentClient<ClassManagerRequest, ClassManagerResponse>> {
        self.class_manager_client.get_local_client()
    }

    pub fn get_class_manager_shared_client(&self) -> Option<SharedClassManagerClient> {
        get_shared_client!(self, class_manager_client)
    }

    pub fn get_gateway_local_client(
        &self,
    ) -> Option<LocalComponentClient<GatewayRequest, GatewayResponse>> {
        self.gateway_client.get_local_client()
    }

    pub fn get_gateway_shared_client(&self) -> Option<SharedGatewayClient> {
        get_shared_client!(self, gateway_client)
    }

    pub fn get_l1_endpoint_monitor_local_client(
        &self,
    ) -> Option<LocalComponentClient<L1EndpointMonitorRequest, L1EndpointMonitorResponse>> {
        self.l1_endpoint_monitor_client.get_local_client()
    }

    pub fn get_l1_endpoint_monitor_shared_client(&self) -> Option<SharedL1EndpointMonitorClient> {
        get_shared_client!(self, l1_endpoint_monitor_client)
    }

    pub fn get_l1_provider_local_client(
        &self,
    ) -> Option<LocalComponentClient<L1ProviderRequest, L1ProviderResponse>> {
        self.l1_provider_client.get_local_client()
    }

    pub fn get_l1_gas_price_provider_local_client(
        &self,
    ) -> Option<LocalComponentClient<L1GasPriceRequest, L1GasPriceResponse>> {
        self.l1_gas_price_client.get_local_client()
    }

    pub fn get_l1_provider_shared_client(&self) -> Option<SharedL1ProviderClient> {
        get_shared_client!(self, l1_provider_client)
    }

    pub fn get_l1_gas_price_shared_client(&self) -> Option<SharedL1GasPriceClient> {
        get_shared_client!(self, l1_gas_price_client)
    }

    pub fn get_mempool_local_client(
        &self,
    ) -> Option<LocalComponentClient<MempoolRequest, MempoolResponse>> {
        self.mempool_client.get_local_client()
    }

    pub fn get_mempool_shared_client(&self) -> Option<SharedMempoolClient> {
        get_shared_client!(self, mempool_client)
    }

    pub fn get_mempool_p2p_propagator_local_client(
        &self,
    ) -> Option<LocalComponentClient<MempoolP2pPropagatorRequest, MempoolP2pPropagatorResponse>>
    {
        self.mempool_p2p_propagator_client.get_local_client()
    }

    pub fn get_mempool_p2p_propagator_shared_client(
        &self,
    ) -> Option<SharedMempoolP2pPropagatorClient> {
        get_shared_client!(self, mempool_p2p_propagator_client)
    }

    pub fn get_sierra_compiler_local_client(
        &self,
    ) -> Option<LocalComponentClient<SierraCompilerRequest, SierraCompilerResponse>> {
        self.sierra_compiler_client.get_local_client()
    }

    pub fn get_sierra_compiler_shared_client(&self) -> Option<SharedSierraCompilerClient> {
        get_shared_client!(self, sierra_compiler_client)
    }

    pub fn get_state_sync_local_client(
        &self,
    ) -> Option<LocalComponentClient<StateSyncRequest, StateSyncResponse>> {
        self.state_sync_client.get_local_client()
    }

    pub fn get_state_sync_shared_client(&self) -> Option<SharedStateSyncClient> {
        get_shared_client!(self, state_sync_client)
    }
}

/// A macro for creating a component client fitting the component's execution mode. Returns a
/// `Client` containing: a local client if the component is run locally, a remote client if
/// the component is run remotely, and neither if the component is disabled.
///
/// # Arguments
///
/// * $execution_mode - A reference to the component's execution mode, i.e., type
///   &ReactiveComponentExecutionMode.
/// * $local_client_type - The type for the local client to create, e.g., LocalBatcherClient. The
///   client type should have a function $local_client_type::new(tx: $channel_expr, metrics:
///   &LocalClientMetrics).
/// * $remote_client_type - The type for the remote client to create, e.g., RemoteBatcherClient. The
///   client type should have a function $remote_client_type::new(config, url, port,
///   remote_client_metrics).
/// * $channel_expr - Sender side for the local client.
/// * $remote_client_config - Configuration for the remote client, passed as Option(config).
/// * $url - URL of the remote component server.
/// * $port - Listening port of the remote component server.
/// * $local_client_metrics - Local client metrics reference to pass to the local client.
/// * $remote_client_metrics - Remote client metrics instance to pass to the remote client.
///
/// # Example
///
/// ```rust,ignore
/// // Assuming ReactiveComponentExecutionMode, channels, and remote client configuration are defined, and
/// // LocalBatcherClient and RemoteBatcherClient have new methods that accept a channel and config,
/// // respectively.
/// let batcher_client: Option<Client<BatcherRequest, BatcherResponse>> = create_client!(
///     &config.components.batcher.execution_mode,
///     LocalBatcherClient,
///     RemoteBatcherClient,
///     channels.take_batcher_tx(),
///     config.components.batcher.remote_client_config,
///     config.components.batcher.url,
///     config.components.batcher.port,
///     &BATCHER_LOCAL_CLIENT_METRICS,
///     &BATCHER_REMOTE_CLIENT_METRICS
/// );
/// ```
macro_rules! create_client {
    (
        $execution_mode:expr,
        $local_client_type:ty,
        $remote_client_type:ty,
        $channel_expr:expr,
        $remote_client_config:expr,
        $url:expr,
        $port:expr,
        $local_client_metrics:expr,
        $remote_client_metrics:expr
    ) => {
        match *$execution_mode {
            ReactiveComponentExecutionMode::LocalExecutionWithRemoteDisabled
            | ReactiveComponentExecutionMode::LocalExecutionWithRemoteEnabled => {
                let local_client =
                    Some(<$local_client_type>::new($channel_expr, $local_client_metrics));
                Client::new(local_client, None)
            }
            ReactiveComponentExecutionMode::Remote => {
                let remote_client = Some(<$remote_client_type>::new(
                    $remote_client_config
                        .as_ref()
                        .expect("Remote client config should be available")
                        .clone(),
                    $url,
                    $port,
                    $remote_client_metrics,
                ));
                Client::new(None, remote_client)
            }
            ReactiveComponentExecutionMode::Disabled => Client::new(None, None),
        }
    };
}

// TODO(alonl): move these client creations to the component crates.
pub fn create_node_clients(
    config: &SequencerNodeConfig,
    channels: &mut SequencerNodeCommunication,
) -> SequencerNodeClients {
    info!("Creating node clients.");

    let batcher_client = create_client!(
        &config.components.batcher.execution_mode,
        LocalBatcherClient,
        RemoteBatcherClient,
        channels.take_batcher_tx(),
        &config.components.batcher.remote_client_config,
        &config.components.batcher.url,
        config.components.batcher.port,
        &BATCHER_LOCAL_CLIENT_METRICS,
        &BATCHER_REMOTE_CLIENT_METRICS
    );

    let class_manager_client = create_client!(
        &config.components.class_manager.execution_mode,
        LocalClassManagerClient,
        RemoteClassManagerClient,
        channels.take_class_manager_tx(),
        &config.components.class_manager.remote_client_config,
        &config.components.class_manager.url,
        config.components.class_manager.port,
        &CLASS_MANAGER_LOCAL_CLIENT_METRICS,
        &CLASS_MANAGER_REMOTE_CLIENT_METRICS
    );

    let gateway_client = create_client!(
        &config.components.gateway.execution_mode,
        LocalGatewayClient,
        RemoteGatewayClient,
        channels.take_gateway_tx(),
        &config.components.gateway.remote_client_config,
        &config.components.gateway.url,
        config.components.gateway.port,
        &GATEWAY_LOCAL_CLIENT_METRICS,
        &GATEWAY_REMOTE_CLIENT_METRICS
    );

    let l1_endpoint_monitor_client = create_client!(
        &config.components.l1_endpoint_monitor.execution_mode,
        LocalL1EndpointMonitorClient,
        RemoteL1EndpointMonitorClient,
        channels.take_l1_endpoint_monitor_tx(),
        &config.components.l1_endpoint_monitor.remote_client_config,
        &config.components.l1_endpoint_monitor.url,
        config.components.l1_endpoint_monitor.port,
        &L1_ENDPOINT_MONITOR_LOCAL_CLIENT_METRICS,
        &L1_ENDPOINT_MONITOR_REMOTE_CLIENT_METRICS
    );

    let l1_provider_client = create_client!(
        &config.components.l1_provider.execution_mode,
        LocalL1ProviderClient,
        RemoteL1ProviderClient,
        channels.take_l1_provider_tx(),
        &config.components.l1_provider.remote_client_config,
        &config.components.l1_provider.url,
        config.components.l1_provider.port,
        &L1_PROVIDER_LOCAL_CLIENT_METRICS,
        &L1_PROVIDER_REMOTE_CLIENT_METRICS
    );

    let l1_gas_price_client = create_client!(
        &config.components.l1_gas_price_provider.execution_mode,
        LocalL1GasPriceClient,
        RemoteL1GasPriceClient,
        channels.take_l1_gas_price_tx(),
        &config.components.l1_gas_price_provider.remote_client_config,
        &config.components.l1_gas_price_provider.url,
        config.components.l1_gas_price_provider.port,
        &L1_GAS_PRICE_LOCAL_CLIENT_METRICS,
        &L1_GAS_PRICE_PROVIDER_REMOTE_CLIENT_METRICS
    );

    let mempool_client = create_client!(
        &config.components.mempool.execution_mode,
        LocalMempoolClient,
        RemoteMempoolClient,
        channels.take_mempool_tx(),
        &config.components.mempool.remote_client_config,
        &config.components.mempool.url,
        config.components.mempool.port,
        &MEMPOOL_LOCAL_CLIENT_METRICS,
        &MEMPOOL_REMOTE_CLIENT_METRICS
    );

    let mempool_p2p_propagator_client = create_client!(
        &config.components.mempool_p2p.execution_mode,
        LocalMempoolP2pPropagatorClient,
        RemoteMempoolP2pPropagatorClient,
        channels.take_mempool_p2p_propagator_tx(),
        &config.components.mempool_p2p.remote_client_config,
        &config.components.mempool_p2p.url,
        config.components.mempool_p2p.port,
        &MEMPOOL_P2P_LOCAL_CLIENT_METRICS,
        &MEMPOOL_P2P_REMOTE_CLIENT_METRICS
    );

    let sierra_compiler_client = create_client!(
        &config.components.sierra_compiler.execution_mode,
        LocalSierraCompilerClient,
        RemoteSierraCompilerClient,
        channels.take_sierra_compiler_tx(),
        &config.components.sierra_compiler.remote_client_config,
        &config.components.sierra_compiler.url,
        config.components.sierra_compiler.port,
        &SIERRA_COMPILER_LOCAL_CLIENT_METRICS,
        &SIERRA_COMPILER_REMOTE_CLIENT_METRICS
    );

    let state_sync_client = create_client!(
        &config.components.state_sync.execution_mode,
        LocalStateSyncClient,
        RemoteStateSyncClient,
        channels.take_state_sync_tx(),
        &config.components.state_sync.remote_client_config,
        &config.components.state_sync.url,
        config.components.state_sync.port,
        &STATE_SYNC_LOCAL_CLIENT_METRICS,
        &STATE_SYNC_REMOTE_CLIENT_METRICS
    );

    SequencerNodeClients {
        batcher_client,
        class_manager_client,
        gateway_client,
        l1_endpoint_monitor_client,
        l1_provider_client,
        l1_gas_price_client,
        mempool_client,
        mempool_p2p_propagator_client,
        sierra_compiler_client,
        state_sync_client,
    }
}

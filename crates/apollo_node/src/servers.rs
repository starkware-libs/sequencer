use std::future::pending;
use std::pin::Pin;

use apollo_batcher::communication::{LocalBatcherServer, RemoteBatcherServer};
use apollo_batcher::metrics::{
    BATCHER_LABELED_PROCESSING_TIMES_SECS,
    BATCHER_LABELED_QUEUEING_TIMES_SECS,
    BATCHER_LOCAL_MSGS_PROCESSED,
    BATCHER_LOCAL_MSGS_RECEIVED,
    BATCHER_LOCAL_QUEUE_DEPTH,
    BATCHER_REMOTE_MSGS_PROCESSED,
    BATCHER_REMOTE_MSGS_RECEIVED,
    BATCHER_REMOTE_NUMBER_OF_CONNECTIONS,
    BATCHER_REMOTE_VALID_MSGS_RECEIVED,
};
use apollo_class_manager::communication::{LocalClassManagerServer, RemoteClassManagerServer};
use apollo_class_manager::metrics::{
    CLASS_MANAGER_LABELED_PROCESSING_TIMES_SECS,
    CLASS_MANAGER_LABELED_QUEUEING_TIMES_SECS,
    CLASS_MANAGER_LOCAL_MSGS_PROCESSED,
    CLASS_MANAGER_LOCAL_MSGS_RECEIVED,
    CLASS_MANAGER_LOCAL_QUEUE_DEPTH,
    CLASS_MANAGER_REMOTE_MSGS_PROCESSED,
    CLASS_MANAGER_REMOTE_MSGS_RECEIVED,
    CLASS_MANAGER_REMOTE_NUMBER_OF_CONNECTIONS,
    CLASS_MANAGER_REMOTE_VALID_MSGS_RECEIVED,
};
use apollo_compile_to_casm::communication::{
    LocalSierraCompilerServer,
    RemoteSierraCompilerServer,
};
use apollo_compile_to_casm::metrics::{
    SIERRA_COMPILER_LABELED_PROCESSING_TIMES_SECS,
    SIERRA_COMPILER_LABELED_QUEUEING_TIMES_SECS,
    SIERRA_COMPILER_LOCAL_MSGS_PROCESSED,
    SIERRA_COMPILER_LOCAL_MSGS_RECEIVED,
    SIERRA_COMPILER_LOCAL_QUEUE_DEPTH,
    SIERRA_COMPILER_REMOTE_MSGS_PROCESSED,
    SIERRA_COMPILER_REMOTE_MSGS_RECEIVED,
    SIERRA_COMPILER_REMOTE_NUMBER_OF_CONNECTIONS,
    SIERRA_COMPILER_REMOTE_VALID_MSGS_RECEIVED,
};
use apollo_consensus_manager::communication::ConsensusManagerServer;
use apollo_gateway::communication::{LocalGatewayServer, RemoteGatewayServer};
use apollo_gateway::metrics::{
    GATEWAY_LABELED_PROCESSING_TIMES_SECS,
    GATEWAY_LABELED_QUEUEING_TIMES_SECS,
    GATEWAY_LOCAL_MSGS_PROCESSED,
    GATEWAY_LOCAL_MSGS_RECEIVED,
    GATEWAY_LOCAL_QUEUE_DEPTH,
    GATEWAY_REMOTE_MSGS_PROCESSED,
    GATEWAY_REMOTE_MSGS_RECEIVED,
    GATEWAY_REMOTE_NUMBER_OF_CONNECTIONS,
    GATEWAY_REMOTE_VALID_MSGS_RECEIVED,
};
use apollo_http_server::communication::HttpServer;
use apollo_infra::component_server::{
    ComponentServerStarter,
    ConcurrentLocalComponentServer,
    LocalComponentServer,
    RemoteComponentServer,
    WrapperServer,
};
use apollo_infra::metrics::{LocalServerMetrics, RemoteServerMetrics};
use apollo_l1_endpoint_monitor::communication::{
    LocalL1EndpointMonitorServer,
    RemoteL1EndpointMonitorServer,
};
use apollo_l1_endpoint_monitor_types::{
    L1_ENDPOINT_MONITOR_LABELED_PROCESSING_TIMES_SECS,
    L1_ENDPOINT_MONITOR_LABELED_QUEUEING_TIMES_SECS,
    L1_ENDPOINT_MONITOR_LOCAL_MSGS_PROCESSED,
    L1_ENDPOINT_MONITOR_LOCAL_MSGS_RECEIVED,
    L1_ENDPOINT_MONITOR_LOCAL_QUEUE_DEPTH,
    L1_ENDPOINT_MONITOR_REMOTE_MSGS_PROCESSED,
    L1_ENDPOINT_MONITOR_REMOTE_MSGS_RECEIVED,
    L1_ENDPOINT_MONITOR_REMOTE_NUMBER_OF_CONNECTIONS,
    L1_ENDPOINT_MONITOR_REMOTE_VALID_MSGS_RECEIVED,
};
use apollo_l1_gas_price::communication::{
    L1GasPriceScraperServer,
    LocalL1GasPriceServer,
    RemoteL1GasPriceServer,
};
use apollo_l1_gas_price::metrics::{
    L1_GAS_PRICE_LABELED_PROCESSING_TIMES_SECS,
    L1_GAS_PRICE_LABELED_QUEUEING_TIMES_SECS,
    L1_GAS_PRICE_LOCAL_MSGS_PROCESSED,
    L1_GAS_PRICE_LOCAL_MSGS_RECEIVED,
    L1_GAS_PRICE_LOCAL_QUEUE_DEPTH,
    L1_GAS_PRICE_REMOTE_MSGS_PROCESSED,
    L1_GAS_PRICE_REMOTE_MSGS_RECEIVED,
    L1_GAS_PRICE_REMOTE_NUMBER_OF_CONNECTIONS,
    L1_GAS_PRICE_REMOTE_VALID_MSGS_RECEIVED,
};
use apollo_l1_provider::communication::{
    L1ScraperServer,
    LocalL1ProviderServer,
    RemoteL1ProviderServer,
};
use apollo_l1_provider::metrics::{
    L1_PROVIDER_LABELED_PROCESSING_TIMES_SECS,
    L1_PROVIDER_LABELED_QUEUEING_TIMES_SECS,
    L1_PROVIDER_LOCAL_MSGS_PROCESSED,
    L1_PROVIDER_LOCAL_MSGS_RECEIVED,
    L1_PROVIDER_LOCAL_QUEUE_DEPTH,
    L1_PROVIDER_REMOTE_MSGS_PROCESSED,
    L1_PROVIDER_REMOTE_MSGS_RECEIVED,
    L1_PROVIDER_REMOTE_NUMBER_OF_CONNECTIONS,
    L1_PROVIDER_REMOTE_VALID_MSGS_RECEIVED,
};
use apollo_mempool::communication::{LocalMempoolServer, RemoteMempoolServer};
use apollo_mempool::metrics::{
    MEMPOOL_LABELED_PROCESSING_TIMES_SECS,
    MEMPOOL_LABELED_QUEUEING_TIMES_SECS,
    MEMPOOL_LOCAL_MSGS_PROCESSED,
    MEMPOOL_LOCAL_MSGS_RECEIVED,
    MEMPOOL_LOCAL_QUEUE_DEPTH,
    MEMPOOL_REMOTE_MSGS_PROCESSED,
    MEMPOOL_REMOTE_MSGS_RECEIVED,
    MEMPOOL_REMOTE_NUMBER_OF_CONNECTIONS,
    MEMPOOL_REMOTE_VALID_MSGS_RECEIVED,
};
use apollo_mempool_p2p::metrics::{
    MEMPOOL_P2P_LABELED_PROCESSING_TIMES_SECS,
    MEMPOOL_P2P_LABELED_QUEUEING_TIMES_SECS,
    MEMPOOL_P2P_LOCAL_MSGS_PROCESSED,
    MEMPOOL_P2P_LOCAL_MSGS_RECEIVED,
    MEMPOOL_P2P_LOCAL_QUEUE_DEPTH,
    MEMPOOL_P2P_REMOTE_MSGS_PROCESSED,
    MEMPOOL_P2P_REMOTE_MSGS_RECEIVED,
    MEMPOOL_P2P_REMOTE_NUMBER_OF_CONNECTIONS,
    MEMPOOL_P2P_REMOTE_VALID_MSGS_RECEIVED,
};
use apollo_mempool_p2p::propagator::{
    LocalMempoolP2pPropagatorServer,
    RemoteMempoolP2pPropagatorServer,
};
use apollo_mempool_p2p::runner::MempoolP2pRunnerServer;
use apollo_monitoring_endpoint::communication::MonitoringEndpointServer;
use apollo_state_sync::runner::StateSyncRunnerServer;
use apollo_state_sync::{LocalStateSyncServer, RemoteStateSyncServer};
use apollo_state_sync_metrics::metrics::{
    STATE_SYNC_LABELED_PROCESSING_TIMES_SECS,
    STATE_SYNC_LABELED_QUEUEING_TIMES_SECS,
    STATE_SYNC_LOCAL_MSGS_PROCESSED,
    STATE_SYNC_LOCAL_MSGS_RECEIVED,
    STATE_SYNC_LOCAL_QUEUE_DEPTH,
    STATE_SYNC_REMOTE_MSGS_PROCESSED,
    STATE_SYNC_REMOTE_MSGS_RECEIVED,
    STATE_SYNC_REMOTE_NUMBER_OF_CONNECTIONS,
    STATE_SYNC_REMOTE_VALID_MSGS_RECEIVED,
};
use futures::stream::FuturesUnordered;
use futures::{Future, FutureExt, StreamExt};
use papyrus_base_layer::ethereum_base_layer_contract::EthereumBaseLayerContract;
use tracing::info;

use crate::clients::SequencerNodeClients;
use crate::communication::SequencerNodeCommunication;
use crate::components::SequencerNodeComponents;
use crate::config::component_execution_config::{
    ActiveComponentExecutionMode,
    ReactiveComponentExecutionMode,
};
use crate::config::node_config::SequencerNodeConfig;

// Component servers that can run locally.
struct LocalServers {
    pub(crate) batcher: Option<Box<LocalBatcherServer>>,
    pub(crate) class_manager: Option<Box<LocalClassManagerServer>>,
    pub(crate) gateway: Option<Box<LocalGatewayServer>>,
    pub(crate) l1_endpoint_monitor: Option<Box<LocalL1EndpointMonitorServer>>,
    pub(crate) l1_provider: Option<Box<LocalL1ProviderServer>>,
    pub(crate) l1_gas_price_provider: Option<Box<LocalL1GasPriceServer>>,
    pub(crate) mempool: Option<Box<LocalMempoolServer>>,
    pub(crate) mempool_p2p_propagator: Option<Box<LocalMempoolP2pPropagatorServer>>,
    pub(crate) sierra_compiler: Option<Box<LocalSierraCompilerServer>>,
    pub(crate) state_sync: Option<Box<LocalStateSyncServer>>,
}

// Component servers that wrap a component without a server.
struct WrapperServers {
    pub(crate) consensus_manager: Option<Box<ConsensusManagerServer>>,
    pub(crate) http_server: Option<Box<HttpServer>>,
    pub(crate) l1_scraper_server: Option<Box<L1ScraperServer<EthereumBaseLayerContract>>>,
    pub(crate) l1_gas_price_scraper_server:
        Option<Box<L1GasPriceScraperServer<EthereumBaseLayerContract>>>,
    pub(crate) monitoring_endpoint: Option<Box<MonitoringEndpointServer>>,
    pub(crate) mempool_p2p_runner: Option<Box<MempoolP2pRunnerServer>>,
    pub(crate) state_sync_runner: Option<Box<StateSyncRunnerServer>>,
}

// Component servers that can run remotely.
// TODO(Nadin): Remove pub from the struct and update the fields to be pub(crate).
pub struct RemoteServers {
    pub batcher: Option<Box<RemoteBatcherServer>>,
    pub class_manager: Option<Box<RemoteClassManagerServer>>,
    pub gateway: Option<Box<RemoteGatewayServer>>,
    pub l1_endpoint_monitor: Option<Box<RemoteL1EndpointMonitorServer>>,
    pub l1_provider: Option<Box<RemoteL1ProviderServer>>,
    pub l1_gas_price_provider: Option<Box<RemoteL1GasPriceServer>>,
    pub mempool: Option<Box<RemoteMempoolServer>>,
    pub mempool_p2p_propagator: Option<Box<RemoteMempoolP2pPropagatorServer>>,
    pub sierra_compiler: Option<Box<RemoteSierraCompilerServer>>,
    pub state_sync: Option<Box<RemoteStateSyncServer>>,
}

pub struct SequencerNodeServers {
    local_servers: LocalServers,
    remote_servers: RemoteServers,
    wrapper_servers: WrapperServers,
}

/// A macro for creating a remote component server based on the component's execution mode.
/// Returns a remote server if the component is configured with Remote execution mode; otherwise,
/// returns None.
///
/// # Arguments
///
/// * `$execution_mode` - Component execution mode reference.
/// * `$local_client_getter` - Local client getter function, used for the remote server
///   initialization if needed.
/// * `$ip` - Remote component server binding address, default "0.0.0.0".
/// * `$port` - Remote component server listening port.
/// * `$max_concurrency` - the maximum number of concurrent connections the server will handle.
///
/// # Returns
///
/// An `Option<Box<RemoteComponentServer<LocalClientType, RequestType, ResponseType>>>` containing
/// the remote server if the execution mode is Remote, or None if the execution mode is Disabled,
/// LocalExecutionWithRemoteEnabled, or LocalExecutionWithRemoteDisabled.
///
/// # Example
///
/// ```rust,ignore
/// let batcher_remote_server = create_remote_server!(
///     &config.components.batcher.execution_mode,
///     || {clients.get_gateway_local_client()},
///     config.components.batcher.ip,
///     config.components.batcher.port,
///     config.components.batcher.max_concurrency
/// );
/// match batcher_remote_server {
///     Some(server) => println!("Remote server created: {:?}", server),
///     None => println!("Remote server not created because the execution mode is not remote."),
/// }
/// ```
#[macro_export]
macro_rules! create_remote_server {
    (
        $execution_mode:expr,
        $local_client_getter:expr,
        $url:expr,
        $port:expr,
        $max_concurrency:expr,
        $metrics:expr
    ) => {
        match *$execution_mode {
            ReactiveComponentExecutionMode::LocalExecutionWithRemoteEnabled => {
                let local_client = $local_client_getter()
                    .expect("Local client should be set for inbound remote connections.");

                Some(Box::new(RemoteComponentServer::new(
                    local_client,
                    $url,
                    $port,
                    $max_concurrency,
                    $metrics,
                )))
            }
            ReactiveComponentExecutionMode::LocalExecutionWithRemoteDisabled
            | ReactiveComponentExecutionMode::Remote
            | ReactiveComponentExecutionMode::Disabled => None,
        }
    };
}

/// A macro for creating a local component server or a concurrent local component server, determined
/// by the component's execution mode. Returns a [concurrent/regular] local server if the component
/// is run locally, otherwise None.
///
/// # Arguments
///
/// * $server_type - the type of the server, one of string literals REGULAR_LOCAL_SERVER or
///   CONCURRENT_LOCAL_SERVER.
/// * $execution_mode - A reference to the component's execution mode, i.e., type
///   &ReactiveComponentExecutionMode.
/// * $component - The component that will be taken to initialize the server if the execution mode
///   is enabled(LocalExecutionWithRemoteDisabled / LocalExecutionWithRemoteEnabled).
/// * $local_server_config - The component's local server configuration.
/// * $receiver - receiver side for the server.
/// * $server_metrics - The metrics for the server.
/// * $max_concurrency - The maximum number of concurrent requests the server will handle. Only
///   required for the CONCURRENT_LOCAL_SERVER.
///
/// # Returns
///
/// An Option<Box<LocalComponentServer<ComponentType, RequestType, ResponseType>>> or
/// an Option<Box<ConcurrentLocalComponentServer<ComponentType, RequestType, ResponseType>>>
/// containing the server if the execution mode is enabled(LocalExecutionWithRemoteDisabled /
/// LocalExecutionWithRemoteEnabled), or None if the execution mode is Disabled.
///
/// # Example
///
/// ```rust,ignore
/// let batcher_server = create_local_server!(
///     REGULAR_LOCAL_SERVER,
///     &config.components.batcher.execution_mode,
///     components.batcher,
///     &config.components.batcher.local_server_config,
///     communication.take_batcher_rx(),
///     batcher_metrics
/// );
/// match batcher_server {
///     Some(server) => println!("Server created: {:?}", server),
///     None => println!("Server not created because the execution mode is disabled."),
/// }
/// ```
macro_rules! create_local_server {
    (
        $server_type:tt,
        $execution_mode:expr,
        $component:expr,
        $local_server_config:expr,
        $receiver:expr,
        $server_metrics:expr
        $(, $max_concurrency:expr)?
    ) => {
        match *$execution_mode {
            ReactiveComponentExecutionMode::LocalExecutionWithRemoteDisabled
            | ReactiveComponentExecutionMode::LocalExecutionWithRemoteEnabled => {
                Some(Box::new(create_local_server!(@create $server_type)(
                    $component
                        .take()
                        .expect(concat!(stringify!($component), " is not initialized.")),
                    $local_server_config,
                    $receiver,
                    $( $max_concurrency,)?
                    $server_metrics,
                )))
            }
            ReactiveComponentExecutionMode::Disabled | ReactiveComponentExecutionMode::Remote => {
                None
            }
        }
    };
    (@create REGULAR_LOCAL_SERVER) => {
        LocalComponentServer::new
    };
    (@create CONCURRENT_LOCAL_SERVER) => {
        ConcurrentLocalComponentServer::new
    };
}

/// A macro for creating a WrapperServer, determined by the component's execution mode. Returns a
/// wrapper server if the component is run locally, otherwise None.
///
/// # Arguments
///
/// * $execution_mode - A reference to the component's execution mode, i.e., type
///   &ReactiveComponentExecutionMode.
/// * $component - The component that will be taken to initialize the server if the execution mode
///   is enabled(LocalExecutionWithRemoteDisabled / LocalExecutionWithRemoteEnabled).
///
/// # Returns
///
/// An `Option<Box<WrapperServer<ComponentType>>>` containing the server if the execution mode is
/// enabled(LocalExecutionWithRemoteDisabled / LocalExecutionWithRemoteEnabled), or `None` if the
/// execution mode is `Disabled`.
///
/// # Example
///
/// ```rust, ignore
/// // Assuming ReactiveComponentExecutionMode and components are defined, and WrapperServer
/// // has a new method that accepts a component.
/// let consensus_manager_server = create_wrapper_server!(
///     &config.components.consensus_manager.execution_mode,
///     components.consensus_manager
/// );
///
/// match consensus_manager_server {
///     Some(server) => println!("Server created: {:?}", server),
///     None => println!("Server not created because the execution mode is disabled."),
/// }
/// ```
macro_rules! create_wrapper_server {
    ($execution_mode:expr, $component:expr) => {
        match *$execution_mode {
            ActiveComponentExecutionMode::Enabled => Some(Box::new(WrapperServer::new(
                $component.take().expect(concat!(stringify!($component), " is not initialized.")),
            ))),
            ActiveComponentExecutionMode::Disabled => None,
        }
    };
}

// TODO(alonl): use the InfraMetrics consts instead of referencing the metrics directly (same for
// remote servers).

fn create_local_servers(
    config: &SequencerNodeConfig,
    communication: &mut SequencerNodeCommunication,
    components: &mut SequencerNodeComponents,
) -> LocalServers {
    const BATCHER_METRICS: LocalServerMetrics = LocalServerMetrics::new(
        &BATCHER_LOCAL_MSGS_RECEIVED,
        &BATCHER_LOCAL_MSGS_PROCESSED,
        &BATCHER_LOCAL_QUEUE_DEPTH,
        &BATCHER_LABELED_PROCESSING_TIMES_SECS,
        &BATCHER_LABELED_QUEUEING_TIMES_SECS,
    );
    let batcher_server = create_local_server!(
        REGULAR_LOCAL_SERVER,
        &config.components.batcher.execution_mode,
        &mut components.batcher,
        &config
            .components
            .batcher
            .local_server_config
            .as_ref()
            .expect("Batcher local server config should be available."),
        communication.take_batcher_rx(),
        &BATCHER_METRICS
    );

    const CLASS_MANAGER_METRICS: LocalServerMetrics = LocalServerMetrics::new(
        &CLASS_MANAGER_LOCAL_MSGS_RECEIVED,
        &CLASS_MANAGER_LOCAL_MSGS_PROCESSED,
        &CLASS_MANAGER_LOCAL_QUEUE_DEPTH,
        &CLASS_MANAGER_LABELED_PROCESSING_TIMES_SECS,
        &CLASS_MANAGER_LABELED_QUEUEING_TIMES_SECS,
    );
    let class_manager_server = create_local_server!(
        CONCURRENT_LOCAL_SERVER,
        &config.components.class_manager.execution_mode,
        &mut components.class_manager,
        &config
            .components
            .class_manager
            .local_server_config
            .as_ref()
            .expect("Class manager local server config should be available."),
        communication.take_class_manager_rx(),
        &CLASS_MANAGER_METRICS,
        config.components.class_manager.max_concurrency
    );

    const GATEWAY_METRICS: LocalServerMetrics = LocalServerMetrics::new(
        &GATEWAY_LOCAL_MSGS_RECEIVED,
        &GATEWAY_LOCAL_MSGS_PROCESSED,
        &GATEWAY_LOCAL_QUEUE_DEPTH,
        &GATEWAY_LABELED_PROCESSING_TIMES_SECS,
        &GATEWAY_LABELED_QUEUEING_TIMES_SECS,
    );
    let gateway_server = create_local_server!(
        CONCURRENT_LOCAL_SERVER,
        &config.components.gateway.execution_mode,
        &mut components.gateway,
        &config
            .components
            .gateway
            .local_server_config
            .as_ref()
            .expect("Gateway local server config should be available."),
        communication.take_gateway_rx(),
        &GATEWAY_METRICS,
        config.components.gateway.max_concurrency
    );

    const L1_ENDPOINT_MONITOR_METRICS: LocalServerMetrics = LocalServerMetrics::new(
        &L1_ENDPOINT_MONITOR_LOCAL_MSGS_RECEIVED,
        &L1_ENDPOINT_MONITOR_LOCAL_MSGS_PROCESSED,
        &L1_ENDPOINT_MONITOR_LOCAL_QUEUE_DEPTH,
        &L1_ENDPOINT_MONITOR_LABELED_PROCESSING_TIMES_SECS,
        &L1_ENDPOINT_MONITOR_LABELED_QUEUEING_TIMES_SECS,
    );
    let l1_endpoint_monitor_server = create_local_server!(
        REGULAR_LOCAL_SERVER,
        &config.components.l1_endpoint_monitor.execution_mode,
        &mut components.l1_endpoint_monitor,
        &config
            .components
            .l1_endpoint_monitor
            .local_server_config
            .as_ref()
            .expect("L1 endpoint monitor local server config should be available."),
        communication.take_l1_endpoint_monitor_rx(),
        &L1_ENDPOINT_MONITOR_METRICS
    );

    const L1_GAS_PRICE_PROVIDER_METRICS: LocalServerMetrics = LocalServerMetrics::new(
        &L1_GAS_PRICE_LOCAL_MSGS_RECEIVED,
        &L1_GAS_PRICE_LOCAL_MSGS_PROCESSED,
        &L1_GAS_PRICE_LOCAL_QUEUE_DEPTH,
        &L1_GAS_PRICE_LABELED_PROCESSING_TIMES_SECS,
        &L1_GAS_PRICE_LABELED_QUEUEING_TIMES_SECS,
    );
    let l1_gas_price_provider_server = create_local_server!(
        REGULAR_LOCAL_SERVER,
        &config.components.l1_gas_price_provider.execution_mode,
        &mut components.l1_gas_price_provider,
        &config
            .components
            .l1_gas_price_provider
            .local_server_config
            .as_ref()
            .expect("L1 gas price provider local server config should be available."),
        communication.take_l1_gas_price_rx(),
        &L1_GAS_PRICE_PROVIDER_METRICS
    );

    const L1_PROVIDER_METRICS: LocalServerMetrics = LocalServerMetrics::new(
        &L1_PROVIDER_LOCAL_MSGS_RECEIVED,
        &L1_PROVIDER_LOCAL_MSGS_PROCESSED,
        &L1_PROVIDER_LOCAL_QUEUE_DEPTH,
        &L1_PROVIDER_LABELED_PROCESSING_TIMES_SECS,
        &L1_PROVIDER_LABELED_QUEUEING_TIMES_SECS,
    );
    let l1_provider_server = create_local_server!(
        REGULAR_LOCAL_SERVER,
        &config.components.l1_provider.execution_mode,
        &mut components.l1_provider,
        &config
            .components
            .l1_provider
            .local_server_config
            .as_ref()
            .expect("L1 provider local server config should be available."),
        communication.take_l1_provider_rx(),
        &L1_PROVIDER_METRICS
    );

    const MEMPOOL_METRICS: LocalServerMetrics = LocalServerMetrics::new(
        &MEMPOOL_LOCAL_MSGS_RECEIVED,
        &MEMPOOL_LOCAL_MSGS_PROCESSED,
        &MEMPOOL_LOCAL_QUEUE_DEPTH,
        &MEMPOOL_LABELED_PROCESSING_TIMES_SECS,
        &MEMPOOL_LABELED_QUEUEING_TIMES_SECS,
    );
    let mempool_server = create_local_server!(
        REGULAR_LOCAL_SERVER,
        &config.components.mempool.execution_mode,
        &mut components.mempool,
        &config
            .components
            .mempool
            .local_server_config
            .as_ref()
            .expect("Mempool local server config should be available."),
        communication.take_mempool_rx(),
        &MEMPOOL_METRICS
    );

    const MEMPOOL_P2P_METRICS: LocalServerMetrics = LocalServerMetrics::new(
        &MEMPOOL_P2P_LOCAL_MSGS_RECEIVED,
        &MEMPOOL_P2P_LOCAL_MSGS_PROCESSED,
        &MEMPOOL_P2P_LOCAL_QUEUE_DEPTH,
        &MEMPOOL_P2P_LABELED_PROCESSING_TIMES_SECS,
        &MEMPOOL_P2P_LABELED_QUEUEING_TIMES_SECS,
    );
    let mempool_p2p_propagator_server = create_local_server!(
        REGULAR_LOCAL_SERVER,
        &config.components.mempool_p2p.execution_mode,
        &mut components.mempool_p2p_propagator,
        &config
            .components
            .mempool_p2p
            .local_server_config
            .as_ref()
            .expect("Mempool p2p local server config should be available."),
        communication.take_mempool_p2p_propagator_rx(),
        &MEMPOOL_P2P_METRICS
    );

    const SIERRA_COMPILER_METRICS: LocalServerMetrics = LocalServerMetrics::new(
        &SIERRA_COMPILER_LOCAL_MSGS_RECEIVED,
        &SIERRA_COMPILER_LOCAL_MSGS_PROCESSED,
        &SIERRA_COMPILER_LOCAL_QUEUE_DEPTH,
        &SIERRA_COMPILER_LABELED_PROCESSING_TIMES_SECS,
        &SIERRA_COMPILER_LABELED_QUEUEING_TIMES_SECS,
    );
    let sierra_compiler_server = create_local_server!(
        CONCURRENT_LOCAL_SERVER,
        &config.components.sierra_compiler.execution_mode,
        &mut components.sierra_compiler,
        &config
            .components
            .sierra_compiler
            .local_server_config
            .as_ref()
            .expect("Sierra compiler local server config should be available."),
        communication.take_sierra_compiler_rx(),
        &SIERRA_COMPILER_METRICS,
        config.components.sierra_compiler.max_concurrency
    );

    const STATE_SYNC_METRICS: LocalServerMetrics = LocalServerMetrics::new(
        &STATE_SYNC_LOCAL_MSGS_RECEIVED,
        &STATE_SYNC_LOCAL_MSGS_PROCESSED,
        &STATE_SYNC_LOCAL_QUEUE_DEPTH,
        &STATE_SYNC_LABELED_PROCESSING_TIMES_SECS,
        &STATE_SYNC_LABELED_QUEUEING_TIMES_SECS,
    );
    let state_sync_server = create_local_server!(
        CONCURRENT_LOCAL_SERVER,
        &config.components.state_sync.execution_mode,
        &mut components.state_sync,
        &config
            .components
            .state_sync
            .local_server_config
            .as_ref()
            .expect("State sync local server config should be available."),
        communication.take_state_sync_rx(),
        &STATE_SYNC_METRICS,
        config.components.state_sync.max_concurrency
    );

    LocalServers {
        batcher: batcher_server,
        class_manager: class_manager_server,
        gateway: gateway_server,
        l1_endpoint_monitor: l1_endpoint_monitor_server,
        l1_provider: l1_provider_server,
        l1_gas_price_provider: l1_gas_price_provider_server,
        mempool: mempool_server,
        mempool_p2p_propagator: mempool_p2p_propagator_server,
        sierra_compiler: sierra_compiler_server,
        state_sync: state_sync_server,
    }
}

async fn create_servers(
    labeled_futures: Vec<(impl Future<Output = ()> + Send + 'static, String)>,
) -> FuturesUnordered<Pin<Box<dyn Future<Output = String> + Send>>> {
    let tasks = FuturesUnordered::new();
    for (future, label) in labeled_futures.into_iter() {
        tasks.push(future.map(move |_| label).boxed());
    }
    tasks
}

impl LocalServers {
    async fn run(self) -> FuturesUnordered<Pin<Box<dyn Future<Output = String> + Send>>> {
        create_servers(vec![
            server_future_and_label(self.batcher, "Local Batcher"),
            server_future_and_label(self.class_manager, "Local Class Manager"),
            server_future_and_label(self.gateway, "Local Gateway"),
            server_future_and_label(self.l1_endpoint_monitor, "Local L1 Endpoint Monitor"),
            server_future_and_label(self.l1_provider, "Local L1 Provider"),
            server_future_and_label(self.l1_gas_price_provider, "Local L1 Gas Price Provider"),
            server_future_and_label(self.mempool, "Local Mempool"),
            server_future_and_label(self.mempool_p2p_propagator, "Local Mempool P2p Propagator"),
            server_future_and_label(self.sierra_compiler, "Concurrent Local Sierra Compiler"),
            server_future_and_label(self.state_sync, "Local State Sync"),
        ])
        .await
    }
}

pub fn create_remote_servers(
    config: &SequencerNodeConfig,
    clients: &SequencerNodeClients,
) -> RemoteServers {
    let batcher_metrics = RemoteServerMetrics::new(
        &BATCHER_REMOTE_MSGS_RECEIVED,
        &BATCHER_REMOTE_VALID_MSGS_RECEIVED,
        &BATCHER_REMOTE_MSGS_PROCESSED,
        &BATCHER_REMOTE_NUMBER_OF_CONNECTIONS,
    );
    let batcher_server = create_remote_server!(
        &config.components.batcher.execution_mode,
        || { clients.get_batcher_local_client() },
        config.components.batcher.ip,
        config.components.batcher.port,
        config.components.batcher.max_concurrency,
        batcher_metrics
    );

    let class_manager_metrics = RemoteServerMetrics::new(
        &CLASS_MANAGER_REMOTE_MSGS_RECEIVED,
        &CLASS_MANAGER_REMOTE_VALID_MSGS_RECEIVED,
        &CLASS_MANAGER_REMOTE_MSGS_PROCESSED,
        &CLASS_MANAGER_REMOTE_NUMBER_OF_CONNECTIONS,
    );
    let class_manager_server = create_remote_server!(
        &config.components.class_manager.execution_mode,
        || { clients.get_class_manager_local_client() },
        config.components.class_manager.ip,
        config.components.class_manager.port,
        config.components.class_manager.max_concurrency,
        class_manager_metrics
    );

    let gateway_metrics = RemoteServerMetrics::new(
        &GATEWAY_REMOTE_MSGS_RECEIVED,
        &GATEWAY_REMOTE_VALID_MSGS_RECEIVED,
        &GATEWAY_REMOTE_MSGS_PROCESSED,
        &GATEWAY_REMOTE_NUMBER_OF_CONNECTIONS,
    );
    let gateway_server = create_remote_server!(
        &config.components.gateway.execution_mode,
        || { clients.get_gateway_local_client() },
        config.components.gateway.ip,
        config.components.gateway.port,
        config.components.gateway.max_concurrency,
        gateway_metrics
    );

    let l1_endpoint_monitor_metrics = RemoteServerMetrics::new(
        &L1_ENDPOINT_MONITOR_REMOTE_MSGS_RECEIVED,
        &L1_ENDPOINT_MONITOR_REMOTE_VALID_MSGS_RECEIVED,
        &L1_ENDPOINT_MONITOR_REMOTE_MSGS_PROCESSED,
        &L1_ENDPOINT_MONITOR_REMOTE_NUMBER_OF_CONNECTIONS,
    );
    let l1_endpoint_monitor_server = create_remote_server!(
        &config.components.l1_endpoint_monitor.execution_mode,
        || { clients.get_l1_endpoint_monitor_local_client() },
        config.components.l1_endpoint_monitor.ip,
        config.components.l1_endpoint_monitor.port,
        config.components.l1_endpoint_monitor.max_concurrency,
        l1_endpoint_monitor_metrics
    );

    let l1_provider_metrics = RemoteServerMetrics::new(
        &L1_PROVIDER_REMOTE_MSGS_RECEIVED,
        &L1_PROVIDER_REMOTE_VALID_MSGS_RECEIVED,
        &L1_PROVIDER_REMOTE_MSGS_PROCESSED,
        &L1_PROVIDER_REMOTE_NUMBER_OF_CONNECTIONS,
    );
    let l1_provider_server = create_remote_server!(
        &config.components.l1_provider.execution_mode,
        || { clients.get_l1_provider_local_client() },
        config.components.l1_provider.ip,
        config.components.l1_provider.port,
        config.components.l1_provider.max_concurrency,
        l1_provider_metrics
    );
    let l1_gas_price_provider_metrics = RemoteServerMetrics::new(
        &L1_GAS_PRICE_REMOTE_MSGS_RECEIVED,
        &L1_GAS_PRICE_REMOTE_VALID_MSGS_RECEIVED,
        &L1_GAS_PRICE_REMOTE_MSGS_PROCESSED,
        &L1_GAS_PRICE_REMOTE_NUMBER_OF_CONNECTIONS,
    );
    let l1_gas_price_provider_server = create_remote_server!(
        &config.components.l1_gas_price_provider.execution_mode,
        || { clients.get_l1_gas_price_provider_local_client() },
        config.components.l1_gas_price_provider.ip,
        config.components.l1_gas_price_provider.port,
        config.components.l1_gas_price_provider.max_concurrency,
        l1_gas_price_provider_metrics
    );

    let mempool_metrics = RemoteServerMetrics::new(
        &MEMPOOL_REMOTE_MSGS_RECEIVED,
        &MEMPOOL_REMOTE_VALID_MSGS_RECEIVED,
        &MEMPOOL_REMOTE_MSGS_PROCESSED,
        &MEMPOOL_REMOTE_NUMBER_OF_CONNECTIONS,
    );

    let mempool_server = create_remote_server!(
        &config.components.mempool.execution_mode,
        || { clients.get_mempool_local_client() },
        config.components.mempool.ip,
        config.components.mempool.port,
        config.components.mempool.max_concurrency,
        mempool_metrics
    );

    let mempool_p2p_metrics = RemoteServerMetrics::new(
        &MEMPOOL_P2P_REMOTE_MSGS_RECEIVED,
        &MEMPOOL_P2P_REMOTE_VALID_MSGS_RECEIVED,
        &MEMPOOL_P2P_REMOTE_MSGS_PROCESSED,
        &MEMPOOL_P2P_REMOTE_NUMBER_OF_CONNECTIONS,
    );
    let mempool_p2p_propagator_server = create_remote_server!(
        &config.components.mempool_p2p.execution_mode,
        || { clients.get_mempool_p2p_propagator_local_client() },
        config.components.mempool_p2p.ip,
        config.components.mempool_p2p.port,
        config.components.mempool_p2p.max_concurrency,
        mempool_p2p_metrics
    );

    let sierra_compiler_metrics = RemoteServerMetrics::new(
        &SIERRA_COMPILER_REMOTE_MSGS_RECEIVED,
        &SIERRA_COMPILER_REMOTE_VALID_MSGS_RECEIVED,
        &SIERRA_COMPILER_REMOTE_MSGS_PROCESSED,
        &SIERRA_COMPILER_REMOTE_NUMBER_OF_CONNECTIONS,
    );
    let sierra_compiler_server = create_remote_server!(
        &config.components.sierra_compiler.execution_mode,
        || { clients.get_sierra_compiler_local_client() },
        config.components.sierra_compiler.ip,
        config.components.sierra_compiler.port,
        config.components.sierra_compiler.max_concurrency,
        sierra_compiler_metrics
    );

    let state_sync_metrics = RemoteServerMetrics::new(
        &STATE_SYNC_REMOTE_MSGS_RECEIVED,
        &STATE_SYNC_REMOTE_VALID_MSGS_RECEIVED,
        &STATE_SYNC_REMOTE_MSGS_PROCESSED,
        &STATE_SYNC_REMOTE_NUMBER_OF_CONNECTIONS,
    );
    let state_sync_server = create_remote_server!(
        &config.components.state_sync.execution_mode,
        || { clients.get_state_sync_local_client() },
        config.components.state_sync.ip,
        config.components.state_sync.port,
        config.components.state_sync.max_concurrency,
        state_sync_metrics
    );

    RemoteServers {
        batcher: batcher_server,
        class_manager: class_manager_server,
        gateway: gateway_server,
        l1_endpoint_monitor: l1_endpoint_monitor_server,
        l1_provider: l1_provider_server,
        l1_gas_price_provider: l1_gas_price_provider_server,
        mempool: mempool_server,
        mempool_p2p_propagator: mempool_p2p_propagator_server,
        sierra_compiler: sierra_compiler_server,
        state_sync: state_sync_server,
    }
}

impl RemoteServers {
    async fn run(self) -> FuturesUnordered<Pin<Box<dyn Future<Output = String> + Send>>> {
        create_servers(vec![
            server_future_and_label(self.batcher, "Remote Batcher"),
            server_future_and_label(self.class_manager, "Remote Class Manager"),
            server_future_and_label(self.gateway, "Remote Gateway"),
            server_future_and_label(self.l1_endpoint_monitor, "Remote L1 Endpoint Monitor"),
            server_future_and_label(self.l1_provider, "Remote L1 Provider"),
            server_future_and_label(self.l1_gas_price_provider, "Remote L1 Gas Price Provider"),
            server_future_and_label(self.mempool, "Remote Mempool"),
            server_future_and_label(self.mempool_p2p_propagator, "Remote Mempool P2p Propagator"),
            server_future_and_label(self.sierra_compiler, "Remote Sierra Compiler"),
            server_future_and_label(self.state_sync, "Remote State Sync"),
        ])
        .await
    }
}

fn create_wrapper_servers(
    config: &SequencerNodeConfig,
    components: &mut SequencerNodeComponents,
) -> WrapperServers {
    let consensus_manager_server = create_wrapper_server!(
        &config.components.consensus_manager.execution_mode,
        components.consensus_manager
    );

    let http_server = create_wrapper_server!(
        &config.components.http_server.execution_mode,
        components.http_server
    );

    let l1_scraper_server =
        create_wrapper_server!(&config.components.l1_scraper.execution_mode, components.l1_scraper);

    let l1_gas_price_scraper_server = create_wrapper_server!(
        &config.components.l1_gas_price_scraper.execution_mode,
        components.l1_gas_price_scraper
    );

    let monitoring_endpoint_server = create_wrapper_server!(
        &config.components.monitoring_endpoint.execution_mode,
        components.monitoring_endpoint
    );

    let mempool_p2p_runner_server = create_wrapper_server!(
        &config.components.mempool_p2p.execution_mode.clone().into(),
        components.mempool_p2p_runner
    );
    let state_sync_runner_server = create_wrapper_server!(
        &config.components.state_sync.execution_mode.clone().into(),
        components.state_sync_runner
    );

    WrapperServers {
        consensus_manager: consensus_manager_server,
        http_server,
        l1_scraper_server,
        l1_gas_price_scraper_server,
        monitoring_endpoint: monitoring_endpoint_server,
        mempool_p2p_runner: mempool_p2p_runner_server,
        state_sync_runner: state_sync_runner_server,
    }
}

impl WrapperServers {
    async fn run(self) -> FuturesUnordered<Pin<Box<dyn Future<Output = String> + Send>>> {
        create_servers(vec![
            server_future_and_label(self.consensus_manager, "Consensus Manager"),
            server_future_and_label(self.http_server, "Http"),
            server_future_and_label(self.l1_scraper_server, "L1 Scraper"),
            server_future_and_label(self.l1_gas_price_scraper_server, "L1 Gas Price Scraper"),
            server_future_and_label(self.monitoring_endpoint, "Monitoring Endpoint"),
            server_future_and_label(self.mempool_p2p_runner, "Mempool P2p Runner"),
            server_future_and_label(self.state_sync_runner, "State Sync Runner"),
        ])
        .await
    }
}

pub fn create_node_servers(
    config: &SequencerNodeConfig,
    communication: &mut SequencerNodeCommunication,
    components: SequencerNodeComponents,
    clients: &SequencerNodeClients,
) -> SequencerNodeServers {
    info!("Creating node servers.");
    let mut components = components;
    let local_servers = create_local_servers(config, communication, &mut components);
    let remote_servers = create_remote_servers(config, clients);
    let wrapper_servers = create_wrapper_servers(config, &mut components);

    SequencerNodeServers { local_servers, remote_servers, wrapper_servers }
}

pub async fn run_component_servers(servers: SequencerNodeServers) {
    // TODO(alonl): check if we can use create_servers instead of extending a new
    // FuturesUnordered.
    let mut all_servers = FuturesUnordered::new();
    all_servers.extend(servers.local_servers.run().await);
    all_servers.extend(servers.remote_servers.run().await);
    all_servers.extend(servers.wrapper_servers.run().await);

    if let Some(servers_type) = all_servers.next().await {
        // TODO(alonl): check all tasks are exited properly in case of a server failure before
        // panicing.
        panic!("{} Servers ended unexpectedly.", servers_type);
    } else {
        unreachable!("all_servers is never empty");
    }
}

type ComponentServerFuture = Pin<Box<dyn Future<Output = ()> + Send>>;

fn get_server_future(
    server: Option<Box<impl ComponentServerStarter + Send + 'static>>,
) -> ComponentServerFuture {
    match server {
        Some(mut server) => async move { server.start().await }.boxed(),
        None => pending().boxed(),
    }
}

pub fn server_future_and_label(
    server: Option<Box<impl ComponentServerStarter + Send + 'static>>,
    label: &str,
) -> (ComponentServerFuture, String) {
    (get_server_future(server), label.to_string())
}

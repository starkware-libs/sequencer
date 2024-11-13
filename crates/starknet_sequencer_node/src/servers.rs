use std::future::pending;
use std::pin::Pin;

use futures::{Future, FutureExt};
use starknet_batcher::communication::LocalBatcherServer;
use starknet_consensus_manager::communication::ConsensusManagerServer;
use starknet_gateway::communication::LocalGatewayServer;
use starknet_http_server::communication::HttpServer;
use starknet_mempool::communication::LocalMempoolServer;
use starknet_mempool_p2p::propagator::LocalMempoolP2pPropagatorServer;
use starknet_mempool_p2p::runner::MempoolP2pRunnerServer;
use starknet_monitoring_endpoint::communication::MonitoringEndpointServer;
use starknet_sequencer_infra::component_server::{
    ComponentServerStarter,
    LocalComponentServer,
    WrapperServer,
};
use starknet_sequencer_infra::errors::ComponentServerError;
use tracing::error;

use crate::communication::SequencerNodeCommunication;
use crate::components::SequencerNodeComponents;
use crate::config::component_execution_config::ComponentExecutionMode;
use crate::config::node_config::SequencerNodeConfig;

// Component servers that can run locally.
struct LocalServers {
    pub(crate) batcher: Option<Box<LocalBatcherServer>>,
    pub(crate) gateway: Option<Box<LocalGatewayServer>>,
    pub(crate) mempool: Option<Box<LocalMempoolServer>>,
    pub(crate) mempool_p2p_propagator: Option<Box<LocalMempoolP2pPropagatorServer>>,
}

// Component servers that wrap a component without a server.
struct WrapperServers {
    pub(crate) consensus_manager: Option<Box<ConsensusManagerServer>>,
    pub(crate) http_server: Option<Box<HttpServer>>,
    pub(crate) monitoring_endpoint: Option<Box<MonitoringEndpointServer>>,
    pub(crate) mempool_p2p_runner: Option<Box<MempoolP2pRunnerServer>>,
}

pub struct SequencerNodeServers {
    local_servers: LocalServers,
    wrapper_servers: WrapperServers,
}

/// A macro for creating a remote component server based on the component's execution mode.
/// Returns a remote server if the component is configured with Remote execution mode; otherwise,
/// returns None.
///
/// # Arguments
///
/// * `$execution_mode` - A reference to the component's execution mode, of type
///   `&ComponentExecutionMode`.
/// * `$client` - The client to be used for the remote server initialization if the execution mode
///   is `Remote`.
/// * `$config` - The configuration for the remote server.
///
/// # Returns
///
/// An `Option<Box<RemoteComponentServer<LocalClientType, RequestType, ResponseType>>>` containing
/// the server if the execution mode is Remote, or None if the execution mode is Disabled,
/// LocalExecutionWithRemoteEnabled or LocalExecutionWithRemoteDisabled.
///
/// # Example
///
/// ```rust,ignore
/// let batcher_remote_server = create_remote_server!(
///     &config.components.batcher.execution_mode,
///     client,
///     config.remote_server_config
/// );
/// match batcher_remote_server {
///     Some(server) => println!("Remote server created: {:?}", server),
///     None => println!("Remote server not created because the execution mode is not remote."),
/// }
/// ```
#[macro_export]
macro_rules! create_remote_server {
    ($execution_mode:expr, $client:expr, $config:expr) => {
        match *$execution_mode {
            ComponentExecutionMode::Remote => {
                let client = $client
                    .as_ref()
                    .expect("Error: client must be initialized in Remote execution mode.");
                let config = $config
                    .as_ref()
                    .expect("Error: config must be initialized in Remote execution mode.");

                if let Some(local_client) = client.get_local_client() {
                    Some(Box::new(RemoteComponentServer::new(local_client, config.clone())))
                } else {
                    None
                }
            }
            ComponentExecutionMode::LocalExecutionWithRemoteDisabled
            | ComponentExecutionMode::LocalExecutionWithRemoteEnabled
            | ComponentExecutionMode::Disabled => None,
        }
    };
}

/// A macro for creating a component server, determined by the component's execution mode. Returns a
/// local server if the component is run locally, otherwise None.
///
/// # Arguments
///
/// * $execution_mode - A reference to the component's execution mode, i.e., type
///   &ComponentExecutionMode.
/// * $component - The component that will be taken to initialize the server if the execution mode
///   is enabled(LocalExecutionWithRemoteDisabled / LocalExecutionWithRemoteEnabled).
/// * $Receiver - receiver side for the server.
///
/// # Returns
///
/// An Option<Box<LocalComponentServer<ComponentType, RequestType, ResponseType>>> containing the
/// server if the execution mode is enabled(LocalExecutionWithRemoteDisabled /
/// LocalExecutionWithRemoteEnabled), or None if the execution mode is Disabled.
///
/// # Example
///
/// ```rust,ignore
/// let batcher_server = create_local_server!(
///     &config.components.batcher.execution_mode,
///     components.batcher,
///     communication.take_batcher_rx()
/// );
/// match batcher_server {
///     Some(server) => println!("Server created: {:?}", server),
///     None => println!("Server not created because the execution mode is disabled."),
/// }
/// ```
macro_rules! create_local_server {
    ($execution_mode:expr, $component:expr, $receiver:expr) => {
        match *$execution_mode {
            ComponentExecutionMode::LocalExecutionWithRemoteDisabled
            | ComponentExecutionMode::LocalExecutionWithRemoteEnabled => {
                Some(Box::new(LocalComponentServer::new(
                    $component
                        .take()
                        .expect(concat!(stringify!($component), " is not initialized.")),
                    $receiver,
                )))
            }
            ComponentExecutionMode::Disabled | ComponentExecutionMode::Remote => None,
        }
    };
}

/// A macro for creating a WrapperServer, determined by the component's execution mode. Returns a
/// wrapper server if the component is run locally, otherwise None.
///
/// # Arguments
///
/// * $execution_mode - A reference to the component's execution mode, i.e., type
///   &ComponentExecutionMode.
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
/// // Assuming ComponentExecutionMode and components are defined, and WrapperServer
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
            ComponentExecutionMode::LocalExecutionWithRemoteDisabled
            | ComponentExecutionMode::LocalExecutionWithRemoteEnabled => {
                Some(Box::new(WrapperServer::new(
                    $component
                        .take()
                        .expect(concat!(stringify!($component), " is not initialized.")),
                )))
            }
            ComponentExecutionMode::Disabled | ComponentExecutionMode::Remote => None,
        }
    };
}

fn create_local_servers(
    config: &SequencerNodeConfig,
    communication: &mut SequencerNodeCommunication,
    components: &mut SequencerNodeComponents,
) -> LocalServers {
    let batcher_server = create_local_server!(
        &config.components.batcher.execution_mode,
        components.batcher,
        communication.take_batcher_rx()
    );
    let gateway_server = create_local_server!(
        &config.components.gateway.execution_mode,
        components.gateway,
        communication.take_gateway_rx()
    );
    let mempool_server = create_local_server!(
        &config.components.mempool.execution_mode,
        components.mempool,
        communication.take_mempool_rx()
    );
    let mempool_p2p_propagator_server = create_local_server!(
        &config.components.mempool_p2p.execution_mode,
        components.mempool_p2p_propagator,
        communication.take_mempool_p2p_propagator_rx()
    );
    LocalServers {
        batcher: batcher_server,
        gateway: gateway_server,
        mempool: mempool_server,
        mempool_p2p_propagator: mempool_p2p_propagator_server,
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

    let monitoring_endpoint_server = create_wrapper_server!(
        &config.components.monitoring_endpoint.execution_mode,
        components.monitoring_endpoint
    );

    let mempool_p2p_runner_server = create_wrapper_server!(
        &config.components.mempool_p2p.execution_mode,
        components.mempool_p2p_runner
    );
    WrapperServers {
        consensus_manager: consensus_manager_server,
        http_server,
        monitoring_endpoint: monitoring_endpoint_server,
        mempool_p2p_runner: mempool_p2p_runner_server,
    }
}

pub fn create_node_servers(
    config: &SequencerNodeConfig,
    communication: &mut SequencerNodeCommunication,
    components: SequencerNodeComponents,
) -> SequencerNodeServers {
    let mut components = components;
    let local_servers = create_local_servers(config, communication, &mut components);
    let wrapper_servers = create_wrapper_servers(config, &mut components);

    SequencerNodeServers { local_servers, wrapper_servers }
}

pub async fn run_component_servers(servers: SequencerNodeServers) -> anyhow::Result<()> {
    // Batcher server.
    let batcher_future = get_server_future(servers.local_servers.batcher);

    // Consensus Manager server.
    let consensus_manager_future = get_server_future(servers.wrapper_servers.consensus_manager);

    // Gateway server.
    let gateway_future = get_server_future(servers.local_servers.gateway);

    // HttpServer server.
    let http_server_future = get_server_future(servers.wrapper_servers.http_server);

    // Mempool server.
    let mempool_future = get_server_future(servers.local_servers.mempool);

    // Sequencer Monitoring server.
    let monitoring_endpoint_future = get_server_future(servers.wrapper_servers.monitoring_endpoint);

    // MempoolP2pPropagator server.
    let mempool_p2p_propagator_future =
        get_server_future(servers.local_servers.mempool_p2p_propagator);

    // MempoolP2pRunner server.
    let mempool_p2p_runner_future = get_server_future(servers.wrapper_servers.mempool_p2p_runner);

    // Start servers.
    let batcher_handle = tokio::spawn(batcher_future);
    let consensus_manager_handle = tokio::spawn(consensus_manager_future);
    let gateway_handle = tokio::spawn(gateway_future);
    let http_server_handle = tokio::spawn(http_server_future);
    let mempool_handle = tokio::spawn(mempool_future);
    let monitoring_endpoint_handle = tokio::spawn(monitoring_endpoint_future);
    let mempool_p2p_propagator_handle = tokio::spawn(mempool_p2p_propagator_future);
    let mempool_p2p_runner_handle = tokio::spawn(mempool_p2p_runner_future);

    let result = tokio::select! {
        res = batcher_handle => {
            error!("Batcher Server stopped.");
            res?
        }
        res = consensus_manager_handle => {
            error!("Consensus Manager Server stopped.");
            res?
        }
        res = gateway_handle => {
            error!("Gateway Server stopped.");
            res?
        }
        res = http_server_handle => {
            error!("Http Server stopped.");
            res?
        }
        res = mempool_handle => {
            error!("Mempool Server stopped.");
            res?
        }
        res = monitoring_endpoint_handle => {
            error!("Monitoring Endpoint Server stopped.");
            res?
        }
        res = mempool_p2p_propagator_handle => {
            error!("Mempool P2P Propagator Server stopped.");
            res?
        }
        res = mempool_p2p_runner_handle => {
            error!("Mempool P2P Runner Server stopped.");
            res?
        }
    };
    error!("Servers ended with unexpected Ok.");

    Ok(result?)
}

pub fn get_server_future(
    server: Option<Box<impl ComponentServerStarter + Send + 'static>>,
) -> Pin<Box<dyn Future<Output = Result<(), ComponentServerError>> + Send>> {
    match server {
        Some(mut server) => async move { server.start().await }.boxed(),
        None => pending().boxed(),
    }
}

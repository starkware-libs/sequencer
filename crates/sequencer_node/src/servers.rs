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
use crate::config::{ComponentExecutionMode, SequencerNodeConfig};

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

/// A macro to create a LocalComponentServer based on the component's execution mode.
///
/// This macro conditionally creates a LocalComponentServer instance for a given component
/// and channel receiver, based on whether the execution mode is enabled or disabled.
///
/// # Arguments
///
/// * $execution_mode - A reference to the execution mode to evaluate, expected to be of type
///   &ComponentExecutionMode.
/// * $component - The optional component that will be taken to initialize the server if the mode is
///   enabled.
/// * $receiver - An expression to retrieve the channel receiver required for the server.
///
/// # Returns
///
/// An Option<Box<LocalComponentServer<ComponentType, RequestType, ResponseType>>> containing the
/// server if the execution mode is enabled, or None if the execution mode is Disabled.
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
            ComponentExecutionMode::Disabled => None,
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
    let consensus_manager_server = match config.components.consensus_manager.execution_mode {
        ComponentExecutionMode::LocalExecutionWithRemoteDisabled
        | ComponentExecutionMode::LocalExecutionWithRemoteEnabled => {
            Some(Box::new(WrapperServer::new(
                components.consensus_manager.take().expect("Consensus Manager is not initialized."),
            )))
        }
        ComponentExecutionMode::Disabled => None,
    };
    let http_server = match config.components.http_server.execution_mode {
        ComponentExecutionMode::LocalExecutionWithRemoteDisabled
        | ComponentExecutionMode::LocalExecutionWithRemoteEnabled => {
            Some(Box::new(WrapperServer::new(
                components.http_server.take().expect("Http Server is not initialized."),
            )))
        }
        ComponentExecutionMode::Disabled => None,
    };
    let monitoring_endpoint_server = match config.components.monitoring_endpoint.execution_mode {
        ComponentExecutionMode::LocalExecutionWithRemoteDisabled
        | ComponentExecutionMode::LocalExecutionWithRemoteEnabled => {
            Some(Box::new(WrapperServer::new(
                components
                    .monitoring_endpoint
                    .take()
                    .expect("Monitoring Endpoint is not initialized."),
            )))
        }
        ComponentExecutionMode::Disabled => None,
    };

    let mempool_p2p_runner_server = match config.components.mempool_p2p.execution_mode {
        ComponentExecutionMode::LocalExecutionWithRemoteDisabled
        | ComponentExecutionMode::LocalExecutionWithRemoteEnabled => {
            Some(Box::new(MempoolP2pRunnerServer::new(
                components
                    .mempool_p2p_runner
                    .take()
                    .expect("Mempool P2P Runner is not initialized."),
            )))
        }
        ComponentExecutionMode::Disabled => None,
    };
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

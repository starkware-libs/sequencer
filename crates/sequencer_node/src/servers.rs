use std::future::pending;
use std::pin::Pin;

use futures::{Future, FutureExt};
use starknet_batcher::communication::LocalBatcherServer;
use starknet_consensus_manager::communication::ConsensusManagerServer;
use starknet_gateway::communication::LocalGatewayServer;
use starknet_http_server::communication::HttpServer;
use starknet_mempool::communication::LocalMempoolServer;
use starknet_mempool_p2p::propagator::{
    create_mempool_p2p_propagator_server,
    LocalMempoolP2pPropagatorServer,
};
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

pub fn create_node_servers(
    config: &SequencerNodeConfig,
    communication: &mut SequencerNodeCommunication,
    components: SequencerNodeComponents,
) -> SequencerNodeServers {
    let batcher_server = match config.components.batcher.execution_mode {
        ComponentExecutionMode::LocalExecutionWithRemoteDisabled
        | ComponentExecutionMode::LocalExecutionWithRemoteEnabled => {
            Some(Box::new(LocalComponentServer::new(
                components.batcher.expect("Batcher is not initialized."),
                communication.take_batcher_rx(),
            )))
        }
        ComponentExecutionMode::Disabled => None,
    };
    let consensus_manager_server = match config.components.consensus_manager.execution_mode {
        ComponentExecutionMode::LocalExecutionWithRemoteDisabled
        | ComponentExecutionMode::LocalExecutionWithRemoteEnabled => {
            Some(Box::new(WrapperServer::new(
                components.consensus_manager.expect("Consensus Manager is not initialized."),
            )))
        }
        ComponentExecutionMode::Disabled => None,
    };
    let gateway_server = match config.components.gateway.execution_mode {
        ComponentExecutionMode::LocalExecutionWithRemoteDisabled
        | ComponentExecutionMode::LocalExecutionWithRemoteEnabled => {
            Some(Box::new(LocalComponentServer::new(
                components.gateway.expect("Gateway is not initialized."),
                communication.take_gateway_rx(),
            )))
        }
        ComponentExecutionMode::Disabled => None,
    };
    let http_server = match config.components.http_server.execution_mode {
        ComponentExecutionMode::LocalExecutionWithRemoteDisabled
        | ComponentExecutionMode::LocalExecutionWithRemoteEnabled => Some(Box::new(
            WrapperServer::new(components.http_server.expect("Http Server is not initialized.")),
        )),
        ComponentExecutionMode::Disabled => None,
    };
    let monitoring_endpoint_server = match config.components.monitoring_endpoint.execution_mode {
        ComponentExecutionMode::LocalExecutionWithRemoteDisabled
        | ComponentExecutionMode::LocalExecutionWithRemoteEnabled => {
            Some(Box::new(WrapperServer::new(
                components.monitoring_endpoint.expect("Monitoring Endpoint is not initialized."),
            )))
        }
        ComponentExecutionMode::Disabled => None,
    };
    let mempool_server = match config.components.mempool.execution_mode {
        ComponentExecutionMode::LocalExecutionWithRemoteDisabled
        | ComponentExecutionMode::LocalExecutionWithRemoteEnabled => {
            Some(Box::new(LocalComponentServer::new(
                components.mempool.expect("Mempool is not initialized."),
                communication.take_mempool_rx(),
            )))
        }
        ComponentExecutionMode::Disabled => None,
    };

    let mempool_p2p_propagator_server = match config.components.mempool_p2p.execution_mode {
        ComponentExecutionMode::LocalExecutionWithRemoteDisabled
        | ComponentExecutionMode::LocalExecutionWithRemoteEnabled => {
            Some(Box::new(create_mempool_p2p_propagator_server(
                components
                    .mempool_p2p_propagator
                    .expect("Mempool P2P Propagator is not initialized."),
                communication.take_mempool_p2p_propagator_rx(),
            )))
        }
        ComponentExecutionMode::Disabled => None,
    };

    let mempool_p2p_runner_server = match config.components.mempool_p2p.execution_mode {
        ComponentExecutionMode::LocalExecutionWithRemoteDisabled
        | ComponentExecutionMode::LocalExecutionWithRemoteEnabled => {
            Some(Box::new(MempoolP2pRunnerServer::new(
                components.mempool_p2p_runner.expect("Mempool P2P Runner is not initialized."),
            )))
        }
        ComponentExecutionMode::Disabled => None,
    };

    let local_servers = LocalServers {
        batcher: batcher_server,
        gateway: gateway_server,
        mempool: mempool_server,
        mempool_p2p_propagator: mempool_p2p_propagator_server,
    };

    let wrapper_servers = WrapperServers {
        consensus_manager: consensus_manager_server,
        http_server,
        monitoring_endpoint: monitoring_endpoint_server,
        mempool_p2p_runner: mempool_p2p_runner_server,
    };

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

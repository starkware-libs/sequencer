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
use starknet_sequencer_infra::component_definitions::{
    ComponentRequestAndResponseSender,
    ComponentRequestHandler,
};
use starknet_sequencer_infra::component_server::{
    ComponentServerStarter,
    LocalComponentServer,
    WrapperServer,
};
use starknet_sequencer_infra::errors::ComponentServerError;
use tokio::sync::mpsc::Receiver;
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

fn create_local_server<Component, Request, Response>(
    execution_mode: &ComponentExecutionMode,
    component: Component,
    receiver: Receiver<ComponentRequestAndResponseSender<Request, Response>>,
) -> Option<Box<LocalComponentServer<Component, Request, Response>>>
where
    Component: ComponentRequestHandler<Request, Response> + Send,
    Request: Send + Sync,
    Response: Send + Sync,
{
    match execution_mode {
        ComponentExecutionMode::LocalExecutionWithRemoteDisabled
        | ComponentExecutionMode::LocalExecutionWithRemoteEnabled => {
            Some(Box::new(LocalComponentServer::new(component, receiver)))
        }
        ComponentExecutionMode::Disabled => None,
    }
}

fn create_wrapper_server<Component>(
    execution_mode: &ComponentExecutionMode,
    component: Component,
) -> Option<Box<WrapperServer<Component>>> {
    match execution_mode {
        ComponentExecutionMode::LocalExecutionWithRemoteDisabled
        | ComponentExecutionMode::LocalExecutionWithRemoteEnabled => {
            Some(Box::new(WrapperServer::new(component)))
        }
        ComponentExecutionMode::Disabled => None,
    }
}

fn create_local_servers(
    config: &SequencerNodeConfig,
    communication: &mut SequencerNodeCommunication,
    components: &mut SequencerNodeComponents,
) -> LocalServers {
    let batcher_server = create_local_server(
        &config.components.batcher.execution_mode,
        components.batcher.take().expect("Batcher is not initialized."),
        communication.take_batcher_rx(),
    );
    let gateway_server = create_local_server(
        &config.components.gateway.execution_mode,
        components.gateway.take().expect("Gateway is not initialized."),
        communication.take_gateway_rx(),
    );
    let mempool_server = create_local_server(
        &config.components.mempool.execution_mode,
        components.mempool.take().expect("Mempool is not initialized."),
        communication.take_mempool_rx(),
    );
    let mempool_p2p_propagator_server = create_local_server(
        &config.components.mempool_p2p.execution_mode,
        components
            .mempool_p2p_propagator
            .take()
            .expect("Mempool P2P Propagator is not initialized."),
        communication.take_mempool_p2p_propagator_rx(),
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
    let consensus_manager_server = create_wrapper_server(
        &config.components.consensus_manager.execution_mode,
        components.consensus_manager.take().expect("Consensus Manager is not initialized."),
    );
    let http_server = create_wrapper_server(
        &config.components.http_server.execution_mode,
        components.http_server.take().expect("Http Server is not initialized."),
    );
    let monitoring_endpoint_server = create_wrapper_server(
        &config.components.monitoring_endpoint.execution_mode,
        components.monitoring_endpoint.take().expect("Monitoring Endpoint is not initialized."),
    );

    let mempool_p2p_runner_server = create_wrapper_server(
        &config.components.mempool_p2p.execution_mode,
        components.mempool_p2p_runner.take().expect("Mempool P2P Runner is not initialized."),
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

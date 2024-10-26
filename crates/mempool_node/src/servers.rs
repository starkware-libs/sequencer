use std::future::pending;
use std::pin::Pin;

use futures::{Future, FutureExt};
use starknet_batcher::communication::{create_local_batcher_server, LocalBatcherServer};
use starknet_consensus_manager::communication::{
    create_consensus_manager_server,
    ConsensusManagerServer,
};
use starknet_gateway::communication::{create_gateway_server, LocalGatewayServer};
use starknet_http_server::communication::{create_http_server, HttpServer};
use starknet_mempool::communication::{create_mempool_server, LocalMempoolServer};
use starknet_monitoring_endpoint::communication::{
    create_monitoring_endpoint_server,
    MonitoringEndpointServer,
};
use starknet_sequencer_infra::component_server::ComponentServerStarter;
use starknet_sequencer_infra::errors::ComponentServerError;
use tracing::error;

use crate::communication::SequencerNodeCommunication;
use crate::components::SequencerNodeComponents;
use crate::config::SequencerNodeConfig;

// Component servers that can run locally.
struct LocalServers {
    pub(crate) batcher: Option<Box<LocalBatcherServer>>,
    pub(crate) gateway: Option<Box<LocalGatewayServer>>,
    pub(crate) mempool: Option<Box<LocalMempoolServer>>,
    // propagator server
}

// Component servers that wrap a component without a server.
struct WrapperServers {
    pub(crate) consensus_manager: Option<Box<ConsensusManagerServer>>,
    pub(crate) http_server: Option<Box<HttpServer>>,
    pub(crate) monitoring_endpoint: Option<Box<MonitoringEndpointServer>>,
    // runner server
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
    let batcher_server = if config.components.batcher.execute {
        Some(Box::new(create_local_batcher_server(
            components.batcher.expect("Batcher is not initialized."),
            communication.take_batcher_rx(),
        )))
    } else {
        None
    };
    let consensus_manager_server = if config.components.consensus_manager.execute {
        Some(Box::new(create_consensus_manager_server(
            components.consensus_manager.expect("Consensus Manager is not initialized."),
        )))
    } else {
        None
    };
    let gateway_server = if config.components.gateway.execute {
        Some(Box::new(create_gateway_server(
            components.gateway.expect("Gateway is not initialized."),
            communication.take_gateway_rx(),
        )))
    } else {
        None
    };
    let http_server = if config.components.http_server.execute {
        Some(Box::new(create_http_server(
            components.http_server.expect("Http Server is not initialized."),
        )))
    } else {
        None
    };
    let mempool_server = if config.components.mempool.execute {
        Some(Box::new(create_mempool_server(
            components.mempool.expect("Mempool is not initialized."),
            communication.take_mempool_rx(),
        )))
    } else {
        None
    };

    let monitoring_endpoint_server = if config.components.monitoring_endpoint.execute {
        Some(Box::new(create_monitoring_endpoint_server(
            components.monitoring_endpoint.expect("Monitoring Endpoint is not initialized."),
        )))
    } else {
        None
    };

    // let propagator_server = if config.components.mempool_p2p.execute {
    //     Some(Box::new(create_mempool_p2p_propagator_server(
    //         components.mempool_propagator.expect("Mempool Propagator is not initialized."),
    //         communication.take_propagator_rx(),
    //     )))
    // } else {
    //     None
    // };

    let local_servers =
        LocalServers { batcher: batcher_server, gateway: gateway_server, mempool: mempool_server };

    let wrapper_servers = WrapperServers {
        consensus_manager: consensus_manager_server,
        http_server,
        monitoring_endpoint: monitoring_endpoint_server,
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

    // Start servers.
    let batcher_handle = tokio::spawn(batcher_future);
    let consensus_manager_handle = tokio::spawn(consensus_manager_future);
    let gateway_handle = tokio::spawn(gateway_future);
    let http_server_handle = tokio::spawn(http_server_future);
    let mempool_handle = tokio::spawn(mempool_future);
    let monitoring_endpoint_handle = tokio::spawn(monitoring_endpoint_future);

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

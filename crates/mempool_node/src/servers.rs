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
use starknet_mempool_infra::errors::ComponentServerError;
use starknet_mempool_infra::starters::Startable;
use tracing::error;

use crate::communication::MempoolNodeCommunication;
use crate::components::Components;
use crate::config::MempoolNodeConfig;

// Component servers that can run locally.
pub struct LocalServers {
    pub batcher: Option<Box<LocalBatcherServer>>,
    pub gateway: Option<Box<LocalGatewayServer>>,
    pub mempool: Option<Box<LocalMempoolServer>>,
}

/// TODO(Tsabary): rename empty server to wrapper server.

// Component servers that wrap a component without a server.
pub struct WrapperServers {
    pub consensus_manager: Option<Box<ConsensusManagerServer>>,
    pub http_server: Option<Box<HttpServer>>,
}

pub struct Servers {
    local_servers: LocalServers,
    wrapper_servers: WrapperServers,
}

impl Servers {
    pub fn take_batcher(&mut self) -> Option<Box<LocalBatcherServer>> {
        self.local_servers.batcher.take()
    }

    pub fn take_gateway(&mut self) -> Option<Box<LocalGatewayServer>> {
        self.local_servers.gateway.take()
    }

    pub fn take_mempool(&mut self) -> Option<Box<LocalMempoolServer>> {
        self.local_servers.mempool.take()
    }

    pub fn take_consensus_manager(&mut self) -> Option<Box<ConsensusManagerServer>> {
        self.wrapper_servers.consensus_manager.take()
    }

    pub fn take_http_server(&mut self) -> Option<Box<HttpServer>> {
        self.wrapper_servers.http_server.take()
    }
}

pub fn create_servers(
    config: &MempoolNodeConfig,
    communication: &mut MempoolNodeCommunication,
    components: Components,
) -> Servers {
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

    let local_servers =
        LocalServers { batcher: batcher_server, gateway: gateway_server, mempool: mempool_server };

    let wrapper_servers =
        WrapperServers { consensus_manager: consensus_manager_server, http_server };

    Servers { local_servers, wrapper_servers }
}

pub async fn run_component_servers(
    config: &MempoolNodeConfig,
    servers: Servers,
) -> anyhow::Result<()> {
    // Batcher server.
    let batcher_future = get_server_future(
        "Batcher",
        config.components.batcher.execute,
        servers.local_servers.batcher,
    );

    // Consensus Manager server.
    let consensus_manager_future = get_server_future(
        "Consensus Manager",
        config.components.consensus_manager.execute,
        servers.wrapper_servers.consensus_manager,
    );

    // Gateway server.
    let gateway_future = get_server_future(
        "Gateway",
        config.components.gateway.execute,
        servers.local_servers.gateway,
    );

    // HttpServer server.
    let http_server_future = get_server_future(
        "HttpServer",
        config.components.http_server.execute,
        servers.wrapper_servers.http_server,
    );

    // Mempool server.
    let mempool_future = get_server_future(
        "Mempool",
        config.components.mempool.execute,
        servers.local_servers.mempool,
    );

    // Start servers.
    let batcher_handle = tokio::spawn(batcher_future);
    let consensus_manager_handle = tokio::spawn(consensus_manager_future);
    let gateway_handle = tokio::spawn(gateway_future);
    let http_server_handle = tokio::spawn(http_server_future);
    let mempool_handle = tokio::spawn(mempool_future);

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
    };
    error!("Servers ended with unexpected Ok.");

    Ok(result?)
}

pub fn get_server_future(
    name: &str,
    execute_flag: bool,
    server: Option<Box<impl Startable<ComponentServerError> + Send + 'static>>,
) -> Pin<Box<dyn Future<Output = Result<(), ComponentServerError>> + Send>> {
    let server_future = match execute_flag {
        true => {
            let mut server = match server {
                Some(server) => server,
                _ => panic!("{} component is not initialized.", name),
            };
            async move { server.start().await }.boxed()
        }
        false => pending().boxed(),
    };
    server_future
}

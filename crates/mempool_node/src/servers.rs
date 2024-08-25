use std::future::pending;
use std::pin::Pin;

use futures::{Future, FutureExt};
use starknet_batcher::communication::create_local_batcher_server;
use starknet_consensus_manager::communication::create_local_consensus_manager_server;
use starknet_gateway::communication::create_gateway_server;
use starknet_mempool::communication::{create_mempool_server, create_remote_mempool_server};
use starknet_mempool_infra::component_definitions::RemoteComponentCommunicationConfig;
use starknet_mempool_infra::component_server::ComponentServerStarter;
use tracing::error;

use crate::communication::MempoolNodeCommunication;
use crate::components::Components;
use crate::config::{LocationType, MempoolNodeConfig};

pub struct Servers {
    pub batcher: Option<Box<dyn ComponentServerStarter>>,
    pub consensus_manager: Option<Box<dyn ComponentServerStarter>>,
    pub gateway: Option<Box<dyn ComponentServerStarter>>,
    pub mempool: Option<Box<dyn ComponentServerStarter>>,
}

pub fn create_servers(
    config: &MempoolNodeConfig,
    communication: &mut MempoolNodeCommunication,
    components: Components,
) -> Servers {
    let batcher_server: Option<Box<dyn ComponentServerStarter>> =
        if config.components.batcher.execute {
            Some(Box::new(create_local_batcher_server(
                components.batcher.expect("Batcher is not initialized."),
                communication.take_batcher_rx(),
            )))
        } else {
            None
        };
    let consensus_manager_server: Option<Box<dyn ComponentServerStarter>> =
        if config.components.consensus_manager.execute {
            Some(Box::new(create_local_consensus_manager_server(
                components.consensus_manager.expect("Consensus Manager is not initialized."),
                communication.take_consensus_manager_rx(),
            )))
        } else {
            None
        };
    let gateway_server: Option<Box<dyn ComponentServerStarter>> =
        if config.components.gateway.execute {
            Some(Box::new(create_gateway_server(
                components.gateway.expect("Gateway is not initialized."),
            )))
        } else {
            None
        };

    let mempool_server = if config.components.mempool.execute {
        let mempool_server: Box<dyn ComponentServerStarter> =
            match config.components.mempool.location {
                LocationType::Local => Box::new(create_mempool_server(
                    components.mempool.expect("Mempool is not initialized."),
                    communication.take_mempool_rx(),
                )),
                LocationType::Remote => {
                    let RemoteComponentCommunicationConfig { ip, port, retries: _ } =
                        config.components.mempool.remote_config.clone().unwrap();

                    Box::new(create_remote_mempool_server(
                        components.mempool.expect("Mempool is not initialized."),
                        ip,
                        port,
                    ))
                }
            };
        Some(mempool_server)
    } else {
        None
    };

    Servers {
        batcher: batcher_server,
        consensus_manager: consensus_manager_server,
        gateway: gateway_server,
        mempool: mempool_server,
    }
}

pub async fn run_component_servers(
    config: &MempoolNodeConfig,
    servers: Servers,
) -> anyhow::Result<()> {
    // Batcher server.
    let batcher_future =
        get_server_future("Batcher", config.components.batcher.execute, servers.batcher);

    // Consensus Manager server.
    let consensus_manager_future = get_server_future(
        "Consensus Manager",
        config.components.consensus_manager.execute,
        servers.consensus_manager,
    );

    // Gateway server.
    let gateway_future =
        get_server_future("Gateway", config.components.gateway.execute, servers.gateway);

    // Mempool server.
    let mempool_future =
        get_server_future("Mempool", config.components.mempool.execute, servers.mempool);

    // Start servers.
    let batcher_handle = tokio::spawn(batcher_future);
    let consensus_manager_handle = tokio::spawn(consensus_manager_future);
    let gateway_handle = tokio::spawn(gateway_future);
    let mempool_handle = tokio::spawn(mempool_future);

    tokio::select! {
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
        res = mempool_handle => {
            error!("Mempool Server stopped.");
            res?
        }
    };
    error!("Servers ended with unexpected Ok.");

    Ok(())
}

pub fn get_server_future(
    name: &str,
    execute_flag: bool,
    server: Option<Box<dyn ComponentServerStarter>>,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    let server_future = if execute_flag {
        let mut server = match server {
            Some(server) => server,
            _ => panic!("{} component is not initialized.", name),
        };
        async move { server.start().await }.boxed()
    } else {
        pending().boxed()
    };
    server_future
}

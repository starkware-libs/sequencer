use std::future::pending;
use std::pin::Pin;

use futures::{Future, FutureExt};
use starknet_gateway::communication::{create_gateway_server, GatewayServer};
use starknet_mempool::communication::{create_mempool_server, MempoolServer};
use starknet_mempool_infra::component_server::ComponentServerStarter;
use tracing::error;

use crate::communication::MempoolNodeCommunication;
use crate::components::Components;
use crate::config::MempoolNodeConfig;

pub struct Servers {
    pub gateway: Option<Box<GatewayServer>>,
    pub mempool: Option<Box<MempoolServer>>,
}

pub fn create_servers(
    config: &MempoolNodeConfig,
    communication: &mut MempoolNodeCommunication,
    components: Components,
) -> Servers {
    let gateway_server = if config.components.gateway.execute {
        Some(Box::new(create_gateway_server(
            components.gateway.expect("Gateway is not initialized."),
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

    Servers { gateway: gateway_server, mempool: mempool_server }
}

pub async fn run_component_servers(
    config: &MempoolNodeConfig,
    servers: Servers,
) -> anyhow::Result<()> {
    // Gateway server.
    let gateway_future =
        get_server_future("Gateway", config.components.gateway.execute, servers.gateway);

    // Mempool server.
    let mempool_future =
        get_server_future("Mempool", config.components.mempool.execute, servers.mempool);

    // Start servers.
    let gateway_handle = tokio::spawn(gateway_future);
    let mempool_handle = tokio::spawn(mempool_future);

    tokio::select! {
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
    server: Option<Box<impl ComponentServerStarter + 'static>>,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
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

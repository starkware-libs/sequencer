use std::future::pending;
use std::pin::Pin;

use futures::{Future, FutureExt};
use starknet_gateway::communication::{create_gateway_server, GatewayServer};
use starknet_mempool::communication::{create_mempool_server, MempoolServer};
use starknet_mempool_infra::component_server::ComponentServerStarter;

use crate::communication::MempoolNodeCommunication;
use crate::components::Components;
use crate::config::MempoolNodeConfig;

pub struct Servers {
    pub gateway: Option<Box<GatewayServer>>,
    pub mempool: Option<Box<MempoolServer>>,
}

pub fn create_servers(
    config: &MempoolNodeConfig,
    mut communication: MempoolNodeCommunication,
    components: Components,
) -> Servers {
    let gateway_server = if config.components.gateway_component.execute {
        Some(Box::new(create_gateway_server(
            components.gateway.expect("Gateway is not initialized."),
        )))
    } else {
        None
    };

    let mempool_server = if config.components.mempool_component.execute {
        Some(Box::new(create_mempool_server(
            components.mempool.expect("Mempool is not initialized."),
            communication.take_mempool_rx(),
        )))
    } else {
        None
    };

    Servers { gateway: gateway_server, mempool: mempool_server }
}

pub async fn run_server_components(
    config: &MempoolNodeConfig,
    servers: Servers,
) -> anyhow::Result<()> {
    // Gateway component.
    let gateway_future =
        get_server_future("Gateway", config.components.gateway_component.execute, servers.gateway);

    // Mempool component.
    let mempool_future =
        get_server_future("Mempool", config.components.mempool_component.execute, servers.mempool);

    let gateway_handle = tokio::spawn(gateway_future);
    let mempool_handle = tokio::spawn(mempool_future);

    tokio::select! {
        res = gateway_handle => {
            println!("Error: Gateway Server stopped.");
            res?
        }
        res = mempool_handle => {
            println!("Error: Mempool Server stopped.");
            res?
        }
    };
    println!("Error: Servers ended with unexpected Ok.");

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

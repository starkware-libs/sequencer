use starknet_mempool_infra::component_server::empty_component_server::{
    create_empty_server,
    EmptyServer,
};

use crate::gateway::Gateway;

pub type GatewayServer = EmptyServer<Gateway>;

pub fn create_gateway_server(gateway: Gateway) -> GatewayServer {
    create_empty_server(gateway)
}

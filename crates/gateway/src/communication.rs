use starknet_mempool_infra::component_server::{create_empty_server, EmptyServer};

use crate::gateway::Gateway;

pub fn create_gateway_server(gateway: Gateway) -> EmptyServer<Gateway> {
    create_empty_server(gateway)
}

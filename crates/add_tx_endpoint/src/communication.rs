use starknet_mempool_infra::component_server::{create_empty_server, WrapperServer};

use crate::add_tx_endpoint::AddTxEndpoint;

pub type AddTxEndpointServer = WrapperServer<AddTxEndpoint>;

pub fn create_add_tx_endpoint(add_tx_endpoint: AddTxEndpoint) -> AddTxEndpointServer {
    create_empty_server(add_tx_endpoint)
}

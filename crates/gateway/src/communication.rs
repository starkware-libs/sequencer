use async_trait::async_trait;
use starknet_mempool_infra::component_server::{create_empty_server, EmptyServer};

use crate::gateway::Gateway;

pub type GatewayServer = EmptyServer<Gateway>;

pub fn create_gateway_server(gateway: Gateway) -> GatewayServer {
    create_empty_server(gateway)
}
use starknet_gateway_types::communication::{GatewayRequest, GatewayResponse};
use starknet_gateway_types::errors::GatewayError;
use starknet_mempool_infra::component_definitions::ComponentRequestHandler;

#[async_trait]
impl ComponentRequestHandler<GatewayRequest, GatewayResponse> for Gateway {
    async fn handle_request(&mut self, request: GatewayRequest) -> GatewayResponse {
        match request {
            GatewayRequest::AddTransaction(gateway_input) => GatewayResponse::AddTransaction(
                self.add_tx(gateway_input.rpc_tx).await.map_err(GatewayError::GatewaySpecError),
            ),
        }
    }
}

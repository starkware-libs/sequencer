use async_trait::async_trait;
use starknet_gateway_types::communication::{
    GatewayRequest,
    GatewayRequestAndResponseSender,
    GatewayResponse,
};
use starknet_gateway_types::errors::GatewayError;
use starknet_sequencer_infra::component_definitions::ComponentRequestHandler;
use starknet_sequencer_infra::component_server::LocalComponentServer;
use tokio::sync::mpsc::Receiver;
use tracing::instrument;

use crate::gateway::Gateway;

pub type LocalGatewayServer = LocalComponentServer<Gateway, GatewayRequest, GatewayResponse>;

pub fn create_gateway_server(
    gateway: Gateway,
    rx_gateway: Receiver<GatewayRequestAndResponseSender>,
) -> LocalGatewayServer {
    LocalComponentServer::new(gateway, rx_gateway)
}

#[async_trait]
impl ComponentRequestHandler<GatewayRequest, GatewayResponse> for Gateway {
    #[instrument(skip(self))]
    async fn handle_request(&mut self, request: GatewayRequest) -> GatewayResponse {
        match request {
            GatewayRequest::AddTransaction(gateway_input) => {
                let p2p_message_metadata = gateway_input.message_metadata.clone();
                GatewayResponse::AddTransaction(
                    self.add_tx(gateway_input.rpc_tx, gateway_input.message_metadata)
                        .await
                        .map_err(|source| GatewayError::GatewaySpecError {
                            source,
                            p2p_message_metadata,
                        }),
                )
            }
        }
    }
}

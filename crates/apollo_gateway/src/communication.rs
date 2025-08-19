use apollo_gateway_types::communication::{
    GatewayRequest,
    GatewayRequestLabelValue,
    GatewayResponse,
};
use apollo_gateway_types::errors::GatewayError;
use apollo_infra::component_definitions::ComponentRequestHandler;
use apollo_infra::component_server::{ConcurrentLocalComponentServer, RemoteComponentServer};
use apollo_infra::requests::LABEL_NAME_REQUEST_VARIANT;
use apollo_metrics::generate_permutation_labels;
use async_trait::async_trait;
use strum::VariantNames;

use crate::gateway::Gateway;

pub type LocalGatewayServer =
    ConcurrentLocalComponentServer<Gateway, GatewayRequest, GatewayResponse>;
pub type RemoteGatewayServer = RemoteComponentServer<GatewayRequest, GatewayResponse>;

#[async_trait]
impl ComponentRequestHandler<GatewayRequest, GatewayResponse> for Gateway {
    async fn handle_request(&mut self, request: GatewayRequest) -> GatewayResponse {
        match request {
            GatewayRequest::AddTransaction(gateway_input) => {
                let p2p_message_metadata = gateway_input.message_metadata.clone();
                GatewayResponse::AddTransaction(
                    self.add_tx(gateway_input.rpc_tx, gateway_input.message_metadata)
                        .await
                        .map_err(|source| GatewayError::DeprecatedGatewayError {
                            source,
                            p2p_message_metadata,
                        }),
                )
            }
        }
    }
}

generate_permutation_labels! {
    GATEWAY_REQUEST_LABELS,
    (LABEL_NAME_REQUEST_VARIANT, GatewayRequestLabelValue),
}

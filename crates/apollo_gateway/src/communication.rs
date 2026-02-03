use apollo_config_manager_types::communication::SharedConfigManagerClient;
use apollo_gateway_types::communication::{
    GatewayRequest,
    GatewayRequestLabelValue,
    GatewayResponse,
};
use apollo_gateway_types::errors::GatewayError;
use apollo_infra::component_definitions::{ComponentRequestHandler, ComponentStarter};
use apollo_infra::component_server::{ConcurrentLocalComponentServer, RemoteComponentServer};
use apollo_infra::requests::LABEL_NAME_REQUEST_VARIANT;
use apollo_metrics::generate_permutation_labels;
use async_trait::async_trait;
use strum::VariantNames;

use crate::gateway::Gateway;
use crate::metrics::register_metrics;

pub type LocalGatewayServer =
    ConcurrentLocalComponentServer<GatewayCommunicationWrapper, GatewayRequest, GatewayResponse>;
pub type RemoteGatewayServer = RemoteComponentServer<GatewayRequest, GatewayResponse>;

/// Wraps the gateway to enable inbound async communication from other components.
#[derive(Clone)]
pub struct GatewayCommunicationWrapper {
    gateway: Gateway,
    config_manager_client: SharedConfigManagerClient,
}

impl GatewayCommunicationWrapper {
    pub fn new(gateway: Gateway, config_manager_client: SharedConfigManagerClient) -> Self {
        GatewayCommunicationWrapper { gateway, config_manager_client }
    }

    async fn update_dynamic_config(&mut self) {
        let gateway_dynamic_config = self
            .config_manager_client
            .get_gateway_dynamic_config()
            .await
            .expect("Should be able to get gateway dynamic config");
        self.gateway.update_dynamic_config(gateway_dynamic_config);
    }
}

#[async_trait]
impl ComponentRequestHandler<GatewayRequest, GatewayResponse> for GatewayCommunicationWrapper {
    async fn handle_request(&mut self, request: GatewayRequest) -> GatewayResponse {
        // Update the dynamic config before handling the request.
        self.update_dynamic_config().await;
        match request {
            GatewayRequest::AddTransaction(gateway_input) => {
                let p2p_message_metadata = gateway_input.message_metadata.clone();
                GatewayResponse::AddTransaction(
                    self.gateway
                        .add_tx(gateway_input.rpc_tx, gateway_input.message_metadata)
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

#[async_trait]
impl ComponentStarter for GatewayCommunicationWrapper {
    async fn start(&mut self) {
        register_metrics();
    }
}

generate_permutation_labels! {
    GATEWAY_REQUEST_LABELS,
    (LABEL_NAME_REQUEST_VARIANT, GatewayRequestLabelValue),
}

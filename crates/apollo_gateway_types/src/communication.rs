use std::sync::Arc;

use apollo_infra::component_client::{ClientError, LocalComponentClient, RemoteComponentClient};
use apollo_infra::component_definitions::{
    ComponentClient,
    ComponentRequestAndResponseSender,
    PrioritizedRequest,
};
use apollo_infra::{impl_debug_for_infra_requests_and_responses, impl_labeled_request};
use apollo_proc_macros::handle_all_response_variants;
use async_trait::async_trait;
#[cfg(any(feature = "testing", test))]
use mockall::automock;
use serde::{Deserialize, Serialize};
use strum::EnumVariantNames;
use strum_macros::{AsRefStr, EnumDiscriminants, EnumIter, IntoStaticStr};
use thiserror::Error;

use crate::errors::GatewayError;
use crate::gateway_types::{GatewayInput, GatewayOutput, GatewayResult};

pub type LocalGatewayClient = LocalComponentClient<GatewayRequest, GatewayResponse>;
pub type RemoteGatewayClient = RemoteComponentClient<GatewayRequest, GatewayResponse>;
pub type GatewayClientResult<T> = Result<T, GatewayClientError>;
pub type GatewayRequestAndResponseSender =
    ComponentRequestAndResponseSender<GatewayRequest, GatewayResponse>;
pub type SharedGatewayClient = Arc<dyn GatewayClient>;
use tracing::{error, instrument};

/// Serves as the gateway's shared interface. Requires `Send + Sync` to allow transferring
/// and sharing resources (inputs, futures) across threads.
#[cfg_attr(any(feature = "testing", test), automock)]
#[async_trait]
pub trait GatewayClient: Send + Sync {
    async fn add_tx(&self, gateway_input: GatewayInput) -> GatewayClientResult<GatewayOutput>;
}

#[derive(Serialize, Deserialize, Clone, AsRefStr, EnumDiscriminants)]
#[strum_discriminants(
    name(GatewayRequestLabelValue),
    derive(IntoStaticStr, EnumIter, EnumVariantNames),
    strum(serialize_all = "snake_case")
)]
pub enum GatewayRequest {
    AddTransaction(GatewayInput),
}
impl_debug_for_infra_requests_and_responses!(GatewayRequest);
impl_labeled_request!(GatewayRequest, GatewayRequestLabelValue);
impl PrioritizedRequest for GatewayRequest {}

#[derive(Clone, Serialize, Deserialize, AsRefStr)]
pub enum GatewayResponse {
    AddTransaction(GatewayResult<GatewayOutput>),
}
impl_debug_for_infra_requests_and_responses!(GatewayResponse);

#[derive(Clone, Debug, Error)]
pub enum GatewayClientError {
    #[error(transparent)]
    ClientError(#[from] ClientError),
    #[error(transparent)]
    GatewayError(#[from] GatewayError),
}

#[async_trait]
impl<ComponentClientType> GatewayClient for ComponentClientType
where
    ComponentClientType: Send + Sync + ComponentClient<GatewayRequest, GatewayResponse>,
{
    #[instrument(skip(self))]
    async fn add_tx(&self, gateway_input: GatewayInput) -> GatewayClientResult<GatewayOutput> {
        let request = GatewayRequest::AddTransaction(gateway_input);
        handle_all_response_variants!(
            GatewayResponse,
            AddTransaction,
            GatewayClientError,
            GatewayError,
            Direct
        )
    }
}

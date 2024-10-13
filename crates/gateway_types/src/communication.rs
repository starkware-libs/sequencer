use std::sync::Arc;

use async_trait::async_trait;
use mockall::predicate::*;
use mockall::*;
use serde::{Deserialize, Serialize};
use starknet_api::transaction::fields::TransactionHash;
use starknet_mempool_infra::component_client::{
    ClientError,
    LocalComponentClient,
    RemoteComponentClient,
};
use starknet_mempool_infra::component_definitions::ComponentRequestAndResponseSender;
use thiserror::Error;

use crate::errors::GatewayError;
use crate::gateway_types::{GatewayInput, GatewayResult};

pub type LocalGatewayClientImpl = LocalComponentClient<GatewayRequest, GatewayResponse>;
pub type RemoteGatewayClientImpl = RemoteComponentClient<GatewayRequest, GatewayResponse>;
pub type GatewayClientResult<T> = Result<T, GatewayClientError>;
pub type GatewayRequestAndResponseSender =
    ComponentRequestAndResponseSender<GatewayRequest, GatewayResponse>;
pub type SharedGatewayClient = Arc<dyn GatewayClient>;
use tracing::{error, instrument};

/// Serves as the gateway's shared interface. Requires `Send + Sync` to allow transferring
/// and sharing resources (inputs, futures) across threads.
#[automock]
#[async_trait]
pub trait GatewayClient: Send + Sync {
    async fn add_tx(&self, gateway_input: GatewayInput) -> GatewayClientResult<TransactionHash>;
}

#[derive(Debug, Serialize, Deserialize)]
pub enum GatewayRequest {
    AddTransaction(GatewayInput),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum GatewayResponse {
    AddTransaction(GatewayResult<TransactionHash>),
}

#[derive(Clone, Debug, Error)]
pub enum GatewayClientError {
    #[error(transparent)]
    ClientError(#[from] ClientError),
    #[error(transparent)]
    GatewayError(#[from] GatewayError),
}

#[async_trait]
impl GatewayClient for LocalGatewayClientImpl {
    #[instrument(skip(self))]
    async fn add_tx(&self, gateway_input: GatewayInput) -> GatewayClientResult<TransactionHash> {
        let request = GatewayRequest::AddTransaction(gateway_input);
        let response = self.send(request).await;
        match response {
            GatewayResponse::AddTransaction(Ok(response)) => Ok(response),
            GatewayResponse::AddTransaction(Err(response)) => {
                Err(GatewayClientError::GatewayError(response))
            }
        }
    }
}

use std::sync::Arc;

use apollo_infra::component_client::{ClientError, LocalComponentClient, RemoteComponentClient};
use apollo_infra::component_definitions::{ComponentClient, ComponentRequestAndResponseSender};
use apollo_infra::impl_debug_for_infra_requests_and_responses;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use strum_macros::AsRefStr;
use thiserror::Error;

use crate::errors::BlockHashCalculatorError;

pub type LocalBlockHashCalculatorClient =
    LocalComponentClient<BlockHashCalculatorRequest, BlockHashCalculatorResponse>;
pub type RemoteBlockHashCalculatorClient =
    RemoteComponentClient<BlockHashCalculatorRequest, BlockHashCalculatorResponse>;
pub type BlockHashCalculatorClientResult<T> = Result<T, BlockHashCalculatorClientError>;
pub type BlockHashCalculatorRequestAndResponseSender =
    ComponentRequestAndResponseSender<BlockHashCalculatorRequest, BlockHashCalculatorResponse>;
pub type SharedBlockHashCalculatorClient = Arc<dyn BlockHashCalculatorClient>;

/// Client trait for communicating with the block hash calculator component.
#[cfg_attr(any(test, feature = "testing"), mockall::automock)]
#[async_trait]
pub trait BlockHashCalculatorClient: Send + Sync {
    // Empty for now - placeholder for future methods
}

/// Requests that can be sent to the block hash calculator component.
#[derive(Debug, Serialize, Deserialize, Clone, AsRefStr)]
pub enum BlockHashCalculatorRequest {
    // Empty for now - placeholder for future requests
}

/// Responses from the block hash calculator component.
#[derive(Serialize, Deserialize, AsRefStr)]
pub enum BlockHashCalculatorResponse {
    // Empty for now - placeholder for future responses
}

impl_debug_for_infra_requests_and_responses!(BlockHashCalculatorResponse);

#[derive(Debug, Error)]
pub enum BlockHashCalculatorClientError {
    #[error(transparent)]
    ClientError(#[from] ClientError),
    #[error(transparent)]
    BlockHashCalculator(#[from] BlockHashCalculatorError),
}

#[async_trait]
impl<ComponentClientType> BlockHashCalculatorClient for ComponentClientType
where
    ComponentClientType:
        Send + Sync + ComponentClient<BlockHashCalculatorRequest, BlockHashCalculatorResponse>,
{
    // Empty for now - implementations will be added when methods are defined
}

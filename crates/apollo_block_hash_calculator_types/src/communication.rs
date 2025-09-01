use std::sync::Arc;

use apollo_infra::component_client::{ClientError, LocalComponentClient, RemoteComponentClient};
use apollo_infra::component_definitions::{ComponentClient, ComponentRequestAndResponseSender};
use apollo_infra::impl_debug_for_infra_requests_and_responses;
use apollo_proc_macros::handle_all_response_variants;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockHash;
use starknet_types_core::felt::Felt;
use strum_macros::AsRefStr;
use thiserror::Error;

use crate::errors::{BlockHashCalculatorError, BlockHashCalculatorResult};

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
    /// Start computing the block hash without the new state roots.
    async fn initialize_block_hash(
        &self,
        input: InitializeBlockHashInput,
    ) -> BlockHashCalculatorClientResult<Felt>;

    /// Finalize the block hash computation with the new state roots.
    async fn finalize_block_hash(
        &self,
        input: FinalizeBlockHashInput,
    ) -> BlockHashCalculatorClientResult<BlockHash>;
}

// TODO(Nimrod): Add relevant fields here.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InitializeBlockHashInput {}

// TODO(Nimrod): Add relevant fields here.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FinalizeBlockHashInput {}

/// Requests that can be sent to the block hash calculator component.
#[derive(Debug, Serialize, Deserialize, Clone, AsRefStr)]
pub enum BlockHashCalculatorRequest {
    InitializeBlockHash(InitializeBlockHashInput),
    FinalizeBlockHash(FinalizeBlockHashInput),
}

/// Responses from the block hash calculator component.
#[derive(Serialize, Deserialize, AsRefStr)]
pub enum BlockHashCalculatorResponse {
    InitializeBlockHash(BlockHashCalculatorResult<Felt>),
    FinalizeBlockHash(BlockHashCalculatorResult<BlockHash>),
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
    async fn initialize_block_hash(
        &self,
        input: InitializeBlockHashInput,
    ) -> BlockHashCalculatorClientResult<Felt> {
        let request = BlockHashCalculatorRequest::InitializeBlockHash(input);
        handle_all_response_variants!(
            BlockHashCalculatorResponse,
            InitializeBlockHash,
            BlockHashCalculatorClientError,
            BlockHashCalculator,
            Direct
        )
    }

    async fn finalize_block_hash(
        &self,
        input: FinalizeBlockHashInput,
    ) -> BlockHashCalculatorClientResult<BlockHash> {
        let request = BlockHashCalculatorRequest::FinalizeBlockHash(input);
        handle_all_response_variants!(
            BlockHashCalculatorResponse,
            FinalizeBlockHash,
            BlockHashCalculatorClientError,
            BlockHashCalculator,
            Direct
        )
    }
}

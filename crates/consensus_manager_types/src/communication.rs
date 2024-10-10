use std::fmt::Debug;
use std::sync::Arc;

use async_trait::async_trait;
use mockall::predicate::*;
use mockall::*;
use papyrus_proc_macros::handle_response_variants;
use serde::{Deserialize, Serialize};
use starknet_mempool_infra::component_client::{
    ClientError,
    LocalComponentClient,
    RemoteComponentClient,
};
use starknet_mempool_infra::component_definitions::ComponentRequestAndResponseSender;
use thiserror::Error;

use crate::consensus_manager_types::{
    ConsensusManagerFnOneInput,
    ConsensusManagerFnOneReturnValue,
    ConsensusManagerFnTwoInput,
    ConsensusManagerFnTwoReturnValue,
    ConsensusManagerResult,
};
use crate::errors::ConsensusManagerError;

pub type LocalConsensusManagerClient =
    LocalComponentClient<ConsensusManagerRequest, ConsensusManagerResponse>;
pub type RemoteConsensusManagerClient =
    RemoteComponentClient<ConsensusManagerRequest, ConsensusManagerResponse>;
pub type ConsensusManagerClientResult<T> = Result<T, ConsensusManagerClientError>;
pub type ConsensusManagerRequestAndResponseSender =
    ComponentRequestAndResponseSender<ConsensusManagerRequest, ConsensusManagerResponse>;
pub type SharedConsensusManagerClient = Arc<dyn ConsensusManagerClient>;

/// Serves as the consensus manager's shared interface. Requires `Send + Sync` to allow transferring
/// and sharing resources (inputs, futures) across threads.
#[automock]
#[async_trait]
pub trait ConsensusManagerClient: Send + Sync {
    async fn consensus_manager_fn_one(
        &self,
        consensus_manager_fn_one_input: ConsensusManagerFnOneInput,
    ) -> ConsensusManagerClientResult<ConsensusManagerFnOneReturnValue>;

    async fn consensus_manager_fn_two(
        &self,
        consensus_manager_fn_two_input: ConsensusManagerFnTwoInput,
    ) -> ConsensusManagerClientResult<ConsensusManagerFnTwoReturnValue>;
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum ConsensusManagerRequest {
    ConsensusManagerFnOne(ConsensusManagerFnOneInput),
    ConsensusManagerFnTwo(ConsensusManagerFnTwoInput),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum ConsensusManagerResponse {
    ConsensusManagerFnOne(ConsensusManagerResult<ConsensusManagerFnOneReturnValue>),
    ConsensusManagerFnTwo(ConsensusManagerResult<ConsensusManagerFnTwoReturnValue>),
}

#[derive(Clone, Debug, Error)]
pub enum ConsensusManagerClientError {
    #[error(transparent)]
    ClientError(#[from] ClientError),
    #[error(transparent)]
    ConsensusManagerError(#[from] ConsensusManagerError),
}

#[async_trait]
impl ConsensusManagerClient for LocalConsensusManagerClient {
    async fn consensus_manager_fn_one(
        &self,
        consensus_manager_fn_one_input: ConsensusManagerFnOneInput,
    ) -> ConsensusManagerClientResult<ConsensusManagerFnOneReturnValue> {
        let request =
            ConsensusManagerRequest::ConsensusManagerFnOne(consensus_manager_fn_one_input);
        let response = self.send(request).await;
        handle_response_variants!(
            ConsensusManagerResponse,
            ConsensusManagerFnOne,
            ConsensusManagerClientError,
            ConsensusManagerError
        )
    }

    async fn consensus_manager_fn_two(
        &self,
        consensus_manager_fn_two_input: ConsensusManagerFnTwoInput,
    ) -> ConsensusManagerClientResult<ConsensusManagerFnTwoReturnValue> {
        let request =
            ConsensusManagerRequest::ConsensusManagerFnTwo(consensus_manager_fn_two_input);
        let response = self.send(request).await;
        handle_response_variants!(
            ConsensusManagerResponse,
            ConsensusManagerFnTwo,
            ConsensusManagerClientError,
            ConsensusManagerError
        )
    }
}

#[async_trait]
impl ConsensusManagerClient for RemoteConsensusManagerClient {
    async fn consensus_manager_fn_one(
        &self,
        consensus_manager_fn_one_input: ConsensusManagerFnOneInput,
    ) -> ConsensusManagerClientResult<ConsensusManagerFnOneReturnValue> {
        let request =
            ConsensusManagerRequest::ConsensusManagerFnOne(consensus_manager_fn_one_input);
        let response = self.send(request).await?;
        handle_response_variants!(
            ConsensusManagerResponse,
            ConsensusManagerFnOne,
            ConsensusManagerClientError,
            ConsensusManagerError
        )
    }

    async fn consensus_manager_fn_two(
        &self,
        consensus_manager_fn_two_input: ConsensusManagerFnTwoInput,
    ) -> ConsensusManagerClientResult<ConsensusManagerFnTwoReturnValue> {
        let request =
            ConsensusManagerRequest::ConsensusManagerFnTwo(consensus_manager_fn_two_input);
        let response = self.send(request).await?;
        handle_response_variants!(
            ConsensusManagerResponse,
            ConsensusManagerFnTwo,
            ConsensusManagerClientError,
            ConsensusManagerError
        )
    }
}

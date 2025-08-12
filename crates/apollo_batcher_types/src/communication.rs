use std::sync::Arc;

use apollo_infra::component_client::{ClientError, LocalComponentClient, RemoteComponentClient};
use apollo_infra::component_definitions::{
    ComponentClient,
    ComponentRequestAndResponseSender,
    PrioritizedRequest,
};
use apollo_infra::impl_debug_for_infra_requests_and_responses;
use apollo_proc_macros::handle_all_response_variants;
use apollo_state_sync_types::state_sync_types::SyncBlock;
use async_trait::async_trait;
#[cfg(any(feature = "testing", test))]
use mockall::automock;
use serde::{Deserialize, Serialize};
use strum_macros::AsRefStr;
use thiserror::Error;

use crate::batcher_types::{
    BatcherResult,
    DecisionReachedInput,
    DecisionReachedResponse,
    GetHeightResponse,
    GetProposalContentInput,
    GetProposalContentResponse,
    ProposalId,
    ProposeBlockInput,
    RevertBlockInput,
    SendProposalContentInput,
    SendProposalContentResponse,
    StartHeightInput,
    ValidateBlockInput,
};
use crate::errors::BatcherError;

pub type LocalBatcherClient = LocalComponentClient<BatcherRequest, BatcherResponse>;
pub type RemoteBatcherClient = RemoteComponentClient<BatcherRequest, BatcherResponse>;
pub type BatcherClientResult<T> = Result<T, BatcherClientError>;
pub type BatcherRequestAndResponseSender =
    ComponentRequestAndResponseSender<BatcherRequest, BatcherResponse>;
pub type SharedBatcherClient = Arc<dyn BatcherClient>;

/// Serves as the batcher's shared interface. Requires `Send + Sync` to allow transferring and
/// sharing resources (inputs, futures) across threads.
#[cfg_attr(any(test, feature = "testing"), automock)]
#[async_trait]
pub trait BatcherClient: Send + Sync {
    /// Starts the process of building a proposal.
    async fn propose_block(&self, input: ProposeBlockInput) -> BatcherClientResult<()>;
    /// Gets the first height that is not written in the storage yet.
    async fn get_height(&self) -> BatcherClientResult<GetHeightResponse>;
    /// Gets the next available content from the proposal stream (only relevant when building a
    /// proposal).
    async fn get_proposal_content(
        &self,
        input: GetProposalContentInput,
    ) -> BatcherClientResult<GetProposalContentResponse>;
    /// Starts the process of validating a proposal.
    async fn validate_block(&self, input: ValidateBlockInput) -> BatcherClientResult<()>;
    /// Sends the content of a proposal. Only relevant when validating a proposal.
    /// Note:
    ///   * This call can be blocking if the batcher has too many unprocessed transactions.
    ///   * The next send might receive an `InvalidProposal` response for the previous send.
    ///   * If this marks the end of the content, i.e. `SendProposalContent::Finish` is received,
    ///     the batcher will block until the proposal has finished processing before responding.
    async fn send_proposal_content(
        &self,
        input: SendProposalContentInput,
    ) -> BatcherClientResult<SendProposalContentResponse>;
    /// Starts the process of a new height.
    /// From this point onwards, the batcher will accept requests only for proposals associated
    /// with this height.
    async fn start_height(&self, input: StartHeightInput) -> BatcherClientResult<()>;
    /// Adds a block from the state sync. Updates the batcher's state and commits the
    /// transactions to the mempool.
    async fn add_sync_block(&self, sync_block: SyncBlock) -> BatcherClientResult<()>;
    /// Notifies the batcher that a decision has been reached.
    /// This closes the process of the given height, and the accepted proposal is committed.
    async fn decision_reached(
        &self,
        input: DecisionReachedInput,
    ) -> BatcherClientResult<DecisionReachedResponse>;
    /// Reverts the block with the given block number, only if it is the last in the storage.
    async fn revert_block(&self, input: RevertBlockInput) -> BatcherClientResult<()>;
    /// Dumps the block for the given proposal_id to the logs at DEBUG level.
    async fn dump_block(&self, proposal_id: ProposalId) -> BatcherClientResult<()>;
}

#[derive(Serialize, Deserialize, Clone, AsRefStr)]
pub enum BatcherRequest {
    ProposeBlock(ProposeBlockInput),
    GetProposalContent(GetProposalContentInput),
    ValidateBlock(ValidateBlockInput),
    SendProposalContent(SendProposalContentInput),
    StartHeight(StartHeightInput),
    GetCurrentHeight,
    DecisionReached(DecisionReachedInput),
    AddSyncBlock(SyncBlock),
    RevertBlock(RevertBlockInput),
    DumpBlock(ProposalId),
}
impl_debug_for_infra_requests_and_responses!(BatcherRequest);
impl PrioritizedRequest for BatcherRequest {}

#[derive(Serialize, Deserialize, AsRefStr)]
pub enum BatcherResponse {
    ProposeBlock(BatcherResult<()>),
    GetCurrentHeight(BatcherResult<GetHeightResponse>),
    GetProposalContent(BatcherResult<GetProposalContentResponse>),
    ValidateBlock(BatcherResult<()>),
    SendProposalContent(BatcherResult<SendProposalContentResponse>),
    StartHeight(BatcherResult<()>),
    DecisionReached(BatcherResult<Box<DecisionReachedResponse>>),
    AddSyncBlock(BatcherResult<()>),
    RevertBlock(BatcherResult<()>),
    DumpBlock(BatcherResult<()>),
}
impl_debug_for_infra_requests_and_responses!(BatcherResponse);

#[derive(Clone, Debug, Error)]
pub enum BatcherClientError {
    #[error(transparent)]
    ClientError(#[from] ClientError),
    #[error(transparent)]
    BatcherError(#[from] BatcherError),
}

#[async_trait]
impl<ComponentClientType> BatcherClient for ComponentClientType
where
    ComponentClientType: Send + Sync + ComponentClient<BatcherRequest, BatcherResponse>,
{
    async fn propose_block(&self, input: ProposeBlockInput) -> BatcherClientResult<()> {
        let request = BatcherRequest::ProposeBlock(input);
        handle_all_response_variants!(
            BatcherResponse,
            ProposeBlock,
            BatcherClientError,
            BatcherError,
            Direct
        )
    }

    async fn get_proposal_content(
        &self,
        input: GetProposalContentInput,
    ) -> BatcherClientResult<GetProposalContentResponse> {
        let request = BatcherRequest::GetProposalContent(input);
        handle_all_response_variants!(
            BatcherResponse,
            GetProposalContent,
            BatcherClientError,
            BatcherError,
            Direct
        )
    }

    async fn validate_block(&self, input: ValidateBlockInput) -> BatcherClientResult<()> {
        let request = BatcherRequest::ValidateBlock(input);
        handle_all_response_variants!(
            BatcherResponse,
            ValidateBlock,
            BatcherClientError,
            BatcherError,
            Direct
        )
    }

    async fn send_proposal_content(
        &self,
        input: SendProposalContentInput,
    ) -> BatcherClientResult<SendProposalContentResponse> {
        let request = BatcherRequest::SendProposalContent(input);
        handle_all_response_variants!(
            BatcherResponse,
            SendProposalContent,
            BatcherClientError,
            BatcherError,
            Direct
        )
    }

    async fn start_height(&self, input: StartHeightInput) -> BatcherClientResult<()> {
        let request = BatcherRequest::StartHeight(input);
        handle_all_response_variants!(
            BatcherResponse,
            StartHeight,
            BatcherClientError,
            BatcherError,
            Direct
        )
    }

    async fn get_height(&self) -> BatcherClientResult<GetHeightResponse> {
        let request = BatcherRequest::GetCurrentHeight;
        handle_all_response_variants!(
            BatcherResponse,
            GetCurrentHeight,
            BatcherClientError,
            BatcherError,
            Direct
        )
    }

    async fn decision_reached(
        &self,
        input: DecisionReachedInput,
    ) -> BatcherClientResult<DecisionReachedResponse> {
        let request = BatcherRequest::DecisionReached(input);
        handle_all_response_variants!(
            BatcherResponse,
            DecisionReached,
            BatcherClientError,
            BatcherError,
            Boxed
        )
    }

    async fn add_sync_block(&self, sync_block: SyncBlock) -> BatcherClientResult<()> {
        let request = BatcherRequest::AddSyncBlock(sync_block);
        handle_all_response_variants!(
            BatcherResponse,
            AddSyncBlock,
            BatcherClientError,
            BatcherError,
            Direct
        )
    }

    async fn revert_block(&self, input: RevertBlockInput) -> BatcherClientResult<()> {
        let request = BatcherRequest::RevertBlock(input);
        handle_all_response_variants!(
            BatcherResponse,
            RevertBlock,
            BatcherClientError,
            BatcherError,
            Direct
        )
    }

    async fn dump_block(&self, proposal_id: ProposalId) -> BatcherClientResult<()> {
        let request = BatcherRequest::DumpBlock(proposal_id);
        handle_all_response_variants!(
            BatcherResponse,
            DumpBlock,
            BatcherClientError,
            BatcherError,
            Direct
        )
    }
}

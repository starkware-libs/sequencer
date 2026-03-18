use std::sync::Arc;

use apollo_infra::component_client::{ClientError, LocalComponentClient, RemoteComponentClient};
use apollo_infra::component_definitions::{ComponentClient, PrioritizedRequest, RequestWrapper};
use apollo_infra::requests::LABEL_NAME_REQUEST_VARIANT;
use apollo_infra::{
    handle_all_response_variants,
    impl_debug_for_infra_requests_and_responses,
    impl_labeled_request,
};
use apollo_metrics::generate_permutation_labels;
use apollo_state_sync_types::state_sync_types::SyncBlock;
use async_trait::async_trait;
#[cfg(any(feature = "testing", test))]
use mockall::automock;
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHash, BlockNumber, UnixTimestamp};
use strum::{AsRefStr, EnumDiscriminants, EnumIter, IntoStaticStr, VariantNames};
use thiserror::Error;

use crate::batcher_types::{
    BatcherResult,
    CallContractInput,
    CallContractOutput,
    DecisionReachedInput,
    DecisionReachedResponse,
    FinishProposalInput,
    FinishProposalStatus,
    GetHeightResponse,
    GetProposalContentInput,
    GetProposalContentResponse,
    ProposalId,
    ProposeBlockInput,
    RevertBlockInput,
    SendTxsForProposalInput,
    SendTxsForProposalStatus,
    StartHeightInput,
    ValidateBlockInput,
};
use crate::errors::BatcherError;

pub type LocalBatcherClient = LocalComponentClient<BatcherRequest, BatcherResponse>;
pub type RemoteBatcherClient = RemoteComponentClient<BatcherRequest, BatcherResponse>;
pub type BatcherClientResult<T> = Result<T, BatcherClientError>;
pub type BatcherRequestWrapper = RequestWrapper<BatcherRequest, BatcherResponse>;
pub type SharedBatcherClient = Arc<dyn BatcherClient>;

/// Serves as the batcher's shared interface. Requires `Send + Sync` to allow transferring and
/// sharing resources (inputs, futures) across threads.
#[cfg_attr(any(test, feature = "testing"), automock)]
#[async_trait]
pub trait BatcherClient: Send + Sync {
    /// Starts the process of building a proposal.
    async fn propose_block(&self, input: ProposeBlockInput) -> BatcherClientResult<()>;
    /// Gets the block hash for a given block number.
    async fn get_block_hash(&self, block_number: BlockNumber) -> BatcherClientResult<BlockHash>;
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
    /// Aborts a proposal that is currently being validated.
    async fn abort_proposal(&self, proposal_id: ProposalId) -> BatcherClientResult<()>;
    /// Signals that validation stream content is complete and waits for finalization.
    async fn finish_proposal(
        &self,
        input: FinishProposalInput,
    ) -> BatcherClientResult<FinishProposalStatus>;
    /// Sends transactions for a proposal being validated.
    async fn send_txs_for_proposal(
        &self,
        input: SendTxsForProposalInput,
    ) -> BatcherClientResult<SendTxsForProposalStatus>;
    /// Reverts the block with the given block number, only if it is the last in the storage.
    async fn revert_block(&self, input: RevertBlockInput) -> BatcherClientResult<()>;
    async fn get_batch_timestamp(&self) -> BatcherClientResult<UnixTimestamp>;
    /// Executes a view (read-only) entry point on a contract against the latest committed batcher
    /// state and returns the retdata.
    async fn call_contract(
        &self,
        input: CallContractInput,
    ) -> BatcherClientResult<CallContractOutput>;
}

#[derive(Serialize, Deserialize, AsRefStr, EnumDiscriminants)]
#[strum_discriminants(
    name(BatcherRequestLabelValue),
    derive(IntoStaticStr, EnumIter, VariantNames),
    strum(serialize_all = "snake_case")
)]
#[allow(clippy::large_enum_variant)]
pub enum BatcherRequest {
    ProposeBlock(ProposeBlockInput),
    GetBlockHash(BlockNumber),
    GetProposalContent(GetProposalContentInput),
    ValidateBlock(ValidateBlockInput),
    AbortProposal(ProposalId),
    FinishProposal(FinishProposalInput),
    SendTxsForProposal(SendTxsForProposalInput),
    StartHeight(StartHeightInput),
    GetCurrentHeight,
    DecisionReached(DecisionReachedInput),
    AddSyncBlock(SyncBlock),
    RevertBlock(RevertBlockInput),
    GetBatchTimestamp,
    CallContract(CallContractInput),
}
impl_debug_for_infra_requests_and_responses!(BatcherRequest);
impl_labeled_request!(BatcherRequest, BatcherRequestLabelValue);
impl PrioritizedRequest for BatcherRequest {}

generate_permutation_labels! {
    BATCHER_REQUEST_LABELS,
    (LABEL_NAME_REQUEST_VARIANT, BatcherRequestLabelValue),
}

#[derive(Serialize, Deserialize, AsRefStr)]
pub enum BatcherResponse {
    ProposeBlock(BatcherResult<()>),
    GetBlockHash(BatcherResult<BlockHash>),
    GetCurrentHeight(BatcherResult<GetHeightResponse>),
    GetProposalContent(BatcherResult<GetProposalContentResponse>),
    ValidateBlock(BatcherResult<()>),
    SendTxsForProposal(BatcherResult<SendTxsForProposalStatus>),
    AbortProposal(BatcherResult<()>),
    FinishProposal(BatcherResult<FinishProposalStatus>),
    StartHeight(BatcherResult<()>),
    DecisionReached(BatcherResult<Box<DecisionReachedResponse>>),
    AddSyncBlock(BatcherResult<()>),
    RevertBlock(BatcherResult<()>),
    GetBatchTimestamp(BatcherResult<u64>),
    CallContract(BatcherResult<CallContractOutput>),
}
impl_debug_for_infra_requests_and_responses!(BatcherResponse);

#[derive(Clone, Debug, Error, PartialEq)]
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
            self,
            request,
            BatcherResponse,
            ProposeBlock,
            BatcherClientError,
            BatcherError,
            Direct
        )
    }

    async fn get_block_hash(&self, block_number: BlockNumber) -> BatcherClientResult<BlockHash> {
        let request = BatcherRequest::GetBlockHash(block_number);
        handle_all_response_variants!(
            self,
            request,
            BatcherResponse,
            GetBlockHash,
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
            self,
            request,
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
            self,
            request,
            BatcherResponse,
            ValidateBlock,
            BatcherClientError,
            BatcherError,
            Direct
        )
    }

    async fn send_txs_for_proposal(
        &self,
        input: SendTxsForProposalInput,
    ) -> BatcherClientResult<SendTxsForProposalStatus> {
        let request = BatcherRequest::SendTxsForProposal(input);
        handle_all_response_variants!(
            self,
            request,
            BatcherResponse,
            SendTxsForProposal,
            BatcherClientError,
            BatcherError,
            Direct
        )
    }

    async fn finish_proposal(
        &self,
        input: FinishProposalInput,
    ) -> BatcherClientResult<FinishProposalStatus> {
        let request = BatcherRequest::FinishProposal(input);
        handle_all_response_variants!(
            self,
            request,
            BatcherResponse,
            FinishProposal,
            BatcherClientError,
            BatcherError,
            Direct
        )
    }

    async fn start_height(&self, input: StartHeightInput) -> BatcherClientResult<()> {
        let request = BatcherRequest::StartHeight(input);
        handle_all_response_variants!(
            self,
            request,
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
            self,
            request,
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
            self,
            request,
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
            self,
            request,
            BatcherResponse,
            AddSyncBlock,
            BatcherClientError,
            BatcherError,
            Direct
        )
    }

    async fn abort_proposal(&self, proposal_id: ProposalId) -> BatcherClientResult<()> {
        let request = BatcherRequest::AbortProposal(proposal_id);
        handle_all_response_variants!(
            self,
            request,
            BatcherResponse,
            AbortProposal,
            BatcherClientError,
            BatcherError,
            Direct
        )
    }

    async fn revert_block(&self, input: RevertBlockInput) -> BatcherClientResult<()> {
        let request = BatcherRequest::RevertBlock(input);
        handle_all_response_variants!(
            self,
            request,
            BatcherResponse,
            RevertBlock,
            BatcherClientError,
            BatcherError,
            Direct
        )
    }

    async fn get_batch_timestamp(&self) -> BatcherClientResult<UnixTimestamp> {
        let request = BatcherRequest::GetBatchTimestamp;
        handle_all_response_variants!(
            self,
            request,
            BatcherResponse,
            GetBatchTimestamp,
            BatcherClientError,
            BatcherError,
            Direct
        )
    }

    async fn call_contract(
        &self,
        input: CallContractInput,
    ) -> BatcherClientResult<CallContractOutput> {
        let request = BatcherRequest::CallContract(input);
        handle_all_response_variants!(
            self,
            request,
            BatcherResponse,
            CallContract,
            BatcherClientError,
            BatcherError,
            Direct
        )
    }
}

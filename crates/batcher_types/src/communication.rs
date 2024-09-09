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

use crate::batcher_types::{
    BatcherResult,
    BuildProposalInput,
    DecisionReachedInput,
    GetStreamContentInput,
    SendContentResponse,
    SendStreamContentInput,
    StartHeightInput,
    StreamContent,
    ValidateProposalInput,
};
use crate::errors::BatcherError;

pub type LocalBatcherClientImpl = LocalComponentClient<BatcherRequest, BatcherResponse>;
pub type RemoteBatcherClientImpl = RemoteComponentClient<BatcherRequest, BatcherResponse>;
pub type BatcherClientResult<T> = Result<T, BatcherClientError>;
pub type BatcherRequestAndResponseSender =
    ComponentRequestAndResponseSender<BatcherRequest, BatcherResponse>;
pub type SharedBatcherClient = Arc<dyn BatcherClient>;

/// Serves as the batcher's shared interface. Requires `Send + Sync` to allow transferring and
/// sharing resources (inputs, futures) across threads.
#[automock]
#[async_trait]
pub trait BatcherClient: Send + Sync {
    async fn build_proposal(&self, input: BuildProposalInput) -> BatcherClientResult<()>;
    async fn get_stream_content(
        &self,
        input: GetStreamContentInput,
    ) -> BatcherClientResult<StreamContent>;
    async fn validate_proposal(&self, input: ValidateProposalInput) -> BatcherClientResult<()>;
    async fn send_stream_content(
        &self,
        input: SendStreamContentInput,
    ) -> BatcherClientResult<SendContentResponse>;
    async fn start_height(&self, input: StartHeightInput) -> BatcherClientResult<()>;
    async fn decision_reached(&self, input: DecisionReachedInput) -> BatcherClientResult<()>;
}

#[derive(Debug, Serialize, Deserialize)]
pub enum BatcherRequest {
    BuildProposal(BuildProposalInput),
    GetStreamContent(GetStreamContentInput),
    ValidateProposal(ValidateProposalInput),
    SendStreamContent(SendStreamContentInput),
    StartHeight(StartHeightInput),
    DecisionReached(DecisionReachedInput),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum BatcherResponse {
    BuildProposal(BatcherResult<()>),
    GetStreamContent(BatcherResult<StreamContent>),
    ValidateProposal(BatcherResult<()>),
    SendStreamContent(BatcherResult<SendContentResponse>),
    StartHeight(BatcherResult<()>),
    DecisionReached(BatcherResult<()>),
}

#[derive(Clone, Debug, Error)]
pub enum BatcherClientError {
    #[error(transparent)]
    ClientError(#[from] ClientError),
    #[error(transparent)]
    BatcherError(#[from] BatcherError),
}

#[async_trait]
impl BatcherClient for LocalBatcherClientImpl {
    async fn build_proposal(&self, input: BuildProposalInput) -> BatcherClientResult<()> {
        let request = BatcherRequest::BuildProposal(input);
        let response = self.send(request).await;
        handle_response_variants!(BatcherResponse, BuildProposal, BatcherClientError, BatcherError)
    }

    async fn get_stream_content(
        &self,
        input: GetStreamContentInput,
    ) -> BatcherClientResult<StreamContent> {
        let request = BatcherRequest::GetStreamContent(input);
        let response = self.send(request).await;
        handle_response_variants!(
            BatcherResponse,
            GetStreamContent,
            BatcherClientError,
            BatcherError
        )
    }

    async fn validate_proposal(&self, input: ValidateProposalInput) -> BatcherClientResult<()> {
        let request = BatcherRequest::ValidateProposal(input);
        let response = self.send(request).await;
        handle_response_variants!(
            BatcherResponse,
            ValidateProposal,
            BatcherClientError,
            BatcherError
        )
    }

    async fn send_stream_content(
        &self,
        input: SendStreamContentInput,
    ) -> BatcherClientResult<SendContentResponse> {
        let request = BatcherRequest::SendStreamContent(input);
        let response = self.send(request).await;
        handle_response_variants!(
            BatcherResponse,
            SendStreamContent,
            BatcherClientError,
            BatcherError
        )
    }

    async fn start_height(&self, input: StartHeightInput) -> BatcherClientResult<()> {
        let request = BatcherRequest::StartHeight(input);
        let response = self.send(request).await;
        handle_response_variants!(BatcherResponse, StartHeight, BatcherClientError, BatcherError)
    }

    async fn decision_reached(&self, input: DecisionReachedInput) -> BatcherClientResult<()> {
        let request = BatcherRequest::DecisionReached(input);
        let response = self.send(request).await;
        handle_response_variants!(
            BatcherResponse,
            DecisionReached,
            BatcherClientError,
            BatcherError
        )
    }
}

#[async_trait]
impl BatcherClient for RemoteBatcherClientImpl {
    async fn build_proposal(&self, input: BuildProposalInput) -> BatcherClientResult<()> {
        let request = BatcherRequest::BuildProposal(input);
        let response = self.send(request).await?;
        handle_response_variants!(BatcherResponse, BuildProposal, BatcherClientError, BatcherError)
    }

    async fn get_stream_content(
        &self,
        input: GetStreamContentInput,
    ) -> BatcherClientResult<StreamContent> {
        let request = BatcherRequest::GetStreamContent(input);
        let response = self.send(request).await?;
        handle_response_variants!(
            BatcherResponse,
            GetStreamContent,
            BatcherClientError,
            BatcherError
        )
    }

    async fn validate_proposal(&self, input: ValidateProposalInput) -> BatcherClientResult<()> {
        let request = BatcherRequest::ValidateProposal(input);
        let response = self.send(request).await?;
        handle_response_variants!(
            BatcherResponse,
            ValidateProposal,
            BatcherClientError,
            BatcherError
        )
    }

    async fn send_stream_content(
        &self,
        input: SendStreamContentInput,
    ) -> BatcherClientResult<SendContentResponse> {
        let request = BatcherRequest::SendStreamContent(input);
        let response = self.send(request).await?;
        handle_response_variants!(
            BatcherResponse,
            SendStreamContent,
            BatcherClientError,
            BatcherError
        )
    }

    async fn start_height(&self, input: StartHeightInput) -> BatcherClientResult<()> {
        let request = BatcherRequest::StartHeight(input);
        let response = self.send(request).await?;
        handle_response_variants!(BatcherResponse, StartHeight, BatcherClientError, BatcherError)
    }

    async fn decision_reached(&self, input: DecisionReachedInput) -> BatcherClientResult<()> {
        let request = BatcherRequest::DecisionReached(input);
        let response = self.send(request).await?;
        handle_response_variants!(
            BatcherResponse,
            DecisionReached,
            BatcherClientError,
            BatcherError
        )
    }
}

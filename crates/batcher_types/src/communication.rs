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
    BatcherFnOneInput,
    BatcherFnOneReturnValue,
    BatcherFnTwoInput,
    BatcherFnTwoReturnValue,
    BatcherResult,
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
    async fn batcher_fn_one(
        &self,
        batcher_fn_one_input: BatcherFnOneInput,
    ) -> BatcherClientResult<BatcherFnOneReturnValue>;

    async fn batcher_fn_two(
        &self,
        batcher_fn_two_input: BatcherFnTwoInput,
    ) -> BatcherClientResult<BatcherFnTwoReturnValue>;
}

#[derive(Debug, Serialize, Deserialize)]
pub enum BatcherRequest {
    BatcherFnOne(BatcherFnOneInput),
    BatcherFnTwo(BatcherFnTwoInput),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum BatcherResponse {
    BatcherFnOne(BatcherResult<BatcherFnOneReturnValue>),
    BatcherFnTwo(BatcherResult<BatcherFnTwoReturnValue>),
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
    async fn batcher_fn_one(
        &self,
        batcher_fn_one_input: BatcherFnOneInput,
    ) -> BatcherClientResult<BatcherFnOneReturnValue> {
        let request = BatcherRequest::BatcherFnOne(batcher_fn_one_input);
        let response = self.send(request).await;
        handle_response_variants!(BatcherResponse, BatcherFnOne, BatcherClientError, BatcherError)
    }

    async fn batcher_fn_two(
        &self,
        batcher_fn_two_input: BatcherFnTwoInput,
    ) -> BatcherClientResult<BatcherFnTwoReturnValue> {
        let request = BatcherRequest::BatcherFnTwo(batcher_fn_two_input);
        let response = self.send(request).await;
        handle_response_variants!(BatcherResponse, BatcherFnTwo, BatcherClientError, BatcherError)
    }
}

#[async_trait]
impl BatcherClient for RemoteBatcherClientImpl {
    async fn batcher_fn_one(
        &self,
        batcher_fn_one_input: BatcherFnOneInput,
    ) -> BatcherClientResult<BatcherFnOneReturnValue> {
        let request = BatcherRequest::BatcherFnOne(batcher_fn_one_input);
        let response = self.send(request).await?;
        handle_response_variants!(BatcherResponse, BatcherFnOne, BatcherClientError, BatcherError)
    }

    async fn batcher_fn_two(
        &self,
        batcher_fn_two_input: BatcherFnTwoInput,
    ) -> BatcherClientResult<BatcherFnTwoReturnValue> {
        let request = BatcherRequest::BatcherFnTwo(batcher_fn_two_input);
        let response = self.send(request).await?;
        handle_response_variants!(BatcherResponse, BatcherFnTwo, BatcherClientError, BatcherError)
    }
}

use std::sync::Arc;

use async_trait::async_trait;
use mockall::predicate::*;
use mockall::*;
use serde::{Deserialize, Serialize};
use starknet_mempool_infra::component_client::{
    ClientError,
    LocalComponentClient,
    RemoteComponentClient,
};
use starknet_mempool_infra::component_definitions::ComponentRequestAndResponseSender;
use thiserror::Error;

use crate::batcher_types::{BatcherInput, BatcherResult};
use crate::errors::BatcherError;
pub type BatcherClientImpl = LocalComponentClient<BatcherRequest, BatcherResponse>;
pub type RemoteBatcherClientImpl = RemoteComponentClient<BatcherRequest, BatcherResponse>;
pub type BatcherClientResult<T> = Result<T, BatcherClientError>;
pub type BatcherRequestAndResponseSender =
    ComponentRequestAndResponseSender<BatcherRequest, BatcherResponse>;
pub type SharedBatcherClient = Arc<dyn BatcherClient>;

// TODO(Tsabary/Yael/Dafna): Replace with the actual return type of the batcher function.
pub type PlaceholderReturnType = ();

/// Serves as the batcher's shared interface. Requires `Send + Sync` to allow transferring and
/// sharing resources (inputs, futures) across threads.
#[automock]
#[async_trait]
pub trait BatcherClient: Send + Sync {
    async fn batcher_placeholder_fn_name(
        &self,
        batcher_input: BatcherInput,
    ) -> BatcherClientResult<PlaceholderReturnType>;
}

#[derive(Debug, Serialize, Deserialize)]
pub enum BatcherRequest {
    PlaceholderBatcherRequest(BatcherInput),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum BatcherResponse {
    PlaceholderBatcherResponse(BatcherResult<PlaceholderReturnType>),
}

#[derive(Clone, Debug, Error)]
pub enum BatcherClientError {
    #[error(transparent)]
    ClientError(#[from] ClientError),
    #[error(transparent)]
    BatcherError(#[from] BatcherError),
}

#[async_trait]
impl BatcherClient for BatcherClientImpl {
    async fn batcher_placeholder_fn_name(
        &self,
        batcher_input: BatcherInput,
    ) -> BatcherClientResult<PlaceholderReturnType> {
        let request = BatcherRequest::PlaceholderBatcherRequest(batcher_input);
        let response = self.send(request).await;
        match response {
            BatcherResponse::PlaceholderBatcherResponse(Ok(response)) => Ok(response),
            BatcherResponse::PlaceholderBatcherResponse(Err(response)) => {
                Err(BatcherClientError::BatcherError(response))
            }
        }
    }
}

#[async_trait]
impl BatcherClient for RemoteBatcherClientImpl {
    async fn batcher_placeholder_fn_name(
        &self,
        batcher_input: BatcherInput,
    ) -> BatcherClientResult<PlaceholderReturnType> {
        let request = BatcherRequest::PlaceholderBatcherRequest(batcher_input);
        let response = self.send(request).await?;
        match response {
            BatcherResponse::PlaceholderBatcherResponse(Ok(response)) => Ok(response),
            BatcherResponse::PlaceholderBatcherResponse(Err(response)) => {
                Err(BatcherClientError::BatcherError(response))
            }
        }
    }
}

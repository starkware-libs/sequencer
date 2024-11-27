use std::sync::Arc;

use async_trait::async_trait;
use papyrus_proc_macros::handle_response_variants;
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use starknet_sequencer_infra::component_client::{
    ClientError,
    LocalComponentClient,
    RemoteComponentClient,
};
use starknet_sequencer_infra::component_definitions::{
    ComponentClient,
    ComponentRequestAndResponseSender,
};
use thiserror::Error;

use crate::errors::StateSyncError;
use crate::state_sync_types::SyncBlock;

#[async_trait]
pub trait StateSyncClient: Send + Sync {
    /// Request for a block at a specific height.
    /// If the block doesn't exist, or if the sync didn't download it yet, returns None.
    async fn get_block(
        &self,
        block_number: BlockNumber,
    ) -> StateSyncClientResult<Option<SyncBlock>>;

    // TODO: Add state reader methods for gateway.
}

pub type StateSyncResult<T> = Result<T, StateSyncError>;

#[derive(Clone, Debug, Error)]
pub enum StateSyncClientError {
    #[error(transparent)]
    ClientError(#[from] ClientError),
    #[error(transparent)]
    StateSyncError(#[from] StateSyncError),
}
pub type StateSyncClientResult<T> = Result<T, StateSyncClientError>;

pub type LocalStateSyncClient = LocalComponentClient<StateSyncRequest, StateSyncResponse>;
pub type RemoteStateSyncClient = RemoteComponentClient<StateSyncRequest, StateSyncResponse>;
pub type SharedStateSyncClient = Arc<dyn StateSyncClient>;
pub type StateSyncRequestAndResponseSender =
    ComponentRequestAndResponseSender<StateSyncRequest, StateSyncResponse>;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum StateSyncRequest {
    GetBlock(BlockNumber),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum StateSyncResponse {
    GetBlock(StateSyncResult<Option<SyncBlock>>),
}

#[async_trait]
impl StateSyncClient for LocalStateSyncClient {
    async fn get_block(
        &self,
        block_number: BlockNumber,
    ) -> StateSyncClientResult<Option<SyncBlock>> {
        let request = StateSyncRequest::GetBlock(block_number);
        let response = self.send(request).await;
        handle_response_variants!(StateSyncResponse, GetBlock, StateSyncClientError, StateSyncError)
    }
}

#[async_trait]
impl StateSyncClient for RemoteStateSyncClient {
    async fn get_block(
        &self,
        block_number: BlockNumber,
    ) -> StateSyncClientResult<Option<SyncBlock>> {
        let request = StateSyncRequest::GetBlock(block_number);
        let response = self.send(request).await;
        handle_response_variants!(StateSyncResponse, GetBlock, StateSyncClientError, StateSyncError)
    }
}

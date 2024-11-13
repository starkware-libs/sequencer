use async_trait::async_trait;
use starknet_api::block::BlockNumber;
use starknet_sequencer_infra::component_client::ClientError;
use thiserror::Error;

use crate::errors::StateSyncError;
use crate::state_sync_types::SyncBlock;

#[async_trait]
pub trait StateSyncClient: Send + Sync {
    async fn get_block(
        &self,
        block_number: BlockNumber,
    ) -> StateSyncClientResult<Option<SyncBlock>>;

    // TODO: Add state reader methods for gateway.
}

#[derive(Clone, Debug, Error)]
pub enum StateSyncClientError {
    #[error(transparent)]
    ClientError(#[from] ClientError),
    #[error(transparent)]
    StateSyncError(#[from] StateSyncError),
}
pub type StateSyncClientResult<T> = Result<T, StateSyncClientError>;

// TODO: Add client types and request/response enums

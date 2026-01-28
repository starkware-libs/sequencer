use std::sync::Arc;

use async_trait::async_trait;
use starknet_api::block::BlockNumber;
use tokio::sync::Mutex;

use crate::committer_types::{
    CommitBlockRequest,
    CommitBlockResponse,
    RevertBlockRequest,
    RevertBlockResponse,
};
use crate::communication::{CommitterClient, MockCommitterClient};
use crate::errors::CommitterClientResult;

/// A wrapper around MockCommitterClient that tracks an offset field.
/// The offset is set to the input height on commit_block and set to the input height on
/// revert_block.
pub struct MockCommitterClientWithOffset {
    pub inner: MockCommitterClient,
    offset: Arc<Mutex<BlockNumber>>,
}

#[async_trait]
#[cfg(any(feature = "testing", test))]
impl CommitterClient for MockCommitterClientWithOffset {
    async fn commit_block(
        &self,
        input: CommitBlockRequest,
    ) -> CommitterClientResult<CommitBlockResponse> {
        self.set_offset(input.height).await;
        self.inner.commit_block(input).await
    }

    async fn revert_block(
        &self,
        input: RevertBlockRequest,
    ) -> CommitterClientResult<RevertBlockResponse> {
        self.set_offset(input.height).await;
        self.inner.revert_block(input).await
    }
}

impl MockCommitterClientWithOffset {
    pub fn new(inner: MockCommitterClient, initial_offset: Option<BlockNumber>) -> Self {
        let offset = initial_offset.unwrap_or_default();
        Self { inner, offset: Arc::new(Mutex::new(offset)) }
    }
    pub async fn set_offset(&self, offset: BlockNumber) {
        *self.offset.lock().await = offset;
    }

    pub fn get_offset(&self) -> Arc<Mutex<BlockNumber>> {
        Arc::clone(&self.offset)
    }
}

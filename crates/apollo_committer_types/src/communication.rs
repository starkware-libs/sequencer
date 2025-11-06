use std::sync::Arc;

use async_trait::async_trait;
#[cfg(any(feature = "testing", test))]
use mockall::automock;

use crate::committer_types::{CommitBlockRequest, CommitBlockResponse};
use crate::errors::CommitterClientResult;

pub type SharedCommitterClient = Arc<dyn CommitterClient>;

#[async_trait]
#[cfg_attr(any(feature = "testing", test), automock)]
pub trait CommitterClient: Send + Sync {
    /// Applies the state diff on the state trees and computes the new state roots.
    async fn commit_block(
        &self,
        input: CommitBlockRequest,
    ) -> CommitterClientResult<CommitBlockResponse>;
}

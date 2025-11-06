use std::sync::Arc;

use async_trait::async_trait;

use crate::committer_types::{CommitBlockInput, CommitBlockResponse};
use crate::errors::CommitterClientResult;

#[async_trait]
#[cfg_attr(any(feature = "testing", test), mockall::automock)]
pub trait CommitterClient: Send + Sync {
    /// Applies the state diff on the state trees and computes the new state roots.
    async fn commit_block(
        &self,
        input: CommitBlockInput,
    ) -> CommitterClientResult<CommitBlockResponse>;
}

pub type SharedCommitterClient = Arc<dyn CommitterClient>;
